use std::os::fd::FromRawFd;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow};
use smithay::output::{Mode, Output, OutputModeSource, PhysicalProperties, Subpixel};
use smithay_drm_extras::display_info;
use std::result::Result::Ok;
use nix::libc::dev_t;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::GbmAllocator;
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Bind, ImportEgl};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::GbmDevice;
use smithay::backend::session::Session;
use smithay::backend::udev::{self, UdevBackend};
use smithay::desktop::space::SpaceRenderElements;
use smithay::reexports::calloop::timer::Timer;
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::DeviceFd;
use smithay::reexports::drm::control::{Device, ModeTypeFlags};
use smithay::reexports::drm::control::connector::{
    Interface as ConnectorInterface, State as ConnectorState
};
use smithay::backend::allocator::gbm::GbmBufferFlags;

use crate::{CalloopData, Tsuki};

use super::Backend;

const SUPPORTED_COLOR_FORMATS: &[Fourcc] = &[Fourcc::Argb8888, Fourcc::Abgr8888];

pub struct Tty {
    session: LibSeatSession,
    primary_gpu_path: PathBuf,
    output_device: Option<OutputDevice>
}

type GbmDrmCompositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmDevice<DrmDeviceFd>, (), DrmDeviceFd>;

struct OutputDevice {
    id: dev_t,
    path: PathBuf,
    token: RegistrationToken,
    drm: DrmDevice,
    gles: GlesRenderer,
    drm_compositor: GbmDrmCompositor
}

impl Backend for Tty {
    fn seat_name(&self) -> String {
        self.session.seat()
    }

    fn renderer(&mut self) -> &mut GlesRenderer {
        &mut self.output_device.as_mut().unwrap().gles
    }

    fn render(
        &mut self,
        tsuki: &mut crate::Tsuki,
        elements: &[SpaceRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    ) {
        let output_device = self.output_device.as_mut().unwrap();

        let res = output_device
            .drm_compositor
            .render_frame(
                &mut output_device.gles,
                elements,
                [0.1, 0.1, 0.1, 1.],
                FrameFlags::empty()
            )
            .unwrap();

        assert!(!res.needs_sync());
        
        if !res.is_empty {
            output_device
                .drm_compositor
                .queue_frame(())
                .expect("Failed to queue frame");
        } else {
            tsuki.event_loop.insert_source(
                Timer::from_duration(Duration::from_millis(6)),
                |_, _, data| {
                    data.tsuki.redraw(data.tty.as_mut().unwrap());
                    smithay::reexports::calloop::timer::TimeoutAction::Drop
                }).unwrap();
        }

    }
}

impl Tty {
    pub fn new(event_loop: LoopHandle<'static, CalloopData>) -> Self {
        let (session, notifier) = LibSeatSession::new().unwrap();
        let seat_name = session.seat();

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput.udev_assign_seat(&seat_name).unwrap();

        let input_backend = LibinputInputBackend::new(libinput.clone());
        event_loop
            .insert_source(input_backend, |event, _, data| {
                data.tsuki.process_input_event(event);
            }).unwrap();

        event_loop
            .insert_source(notifier, move |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let tsuki = &mut data.tsuki;

                match event {
                    smithay::backend::session::Event::PauseSession => {
                        libinput.suspend();

                        if let Some(output_device) = tty.output_device.as_mut() {
                            output_device.drm.pause();
                        }
                    },
                    smithay::backend::session::Event::ActivateSession => {
                        if libinput.resume().is_err() {
                            println!("error resuming libinput");
                        }

                        if let Some(output_device) = &tty.output_device {
                            tty.device_changed(output_device.id, tsuki);
                        }

                        tsuki.redraw(tty);
                    }
                }
            }).unwrap();
        
        let primary_gpu_path = udev::primary_gpu(&seat_name).unwrap().unwrap();

        Self {
            session,
            primary_gpu_path,
            output_device: None
        }
    }

    pub fn init(&mut self, tsuki: &mut Tsuki) {
        let backend = UdevBackend::new(&self.session.seat()).unwrap();
        for (device_id, path) in backend.device_list() {
            if let Err(err) = self.device_added(device_id, path.to_owned(), tsuki) {
                println!("error adding device: {err:?}");
            }
        }

        tsuki.event_loop
            .insert_source(backend, move |event, _, data| {
                let tty = data.tty.as_mut().unwrap();
                let tsuki = &mut data.tsuki;

                match event {
                    udev::UdevEvent::Added { device_id, path } => {
                        if let Err(err) = tty.device_added(device_id, path, tsuki) {
                            println!("error adding device: {err:?}");
                        }
                        tsuki.redraw(tty);
                    },
                    udev::UdevEvent::Changed { device_id } => tty.device_changed(device_id, tsuki),
                    udev::UdevEvent::Removed { device_id } => tty.device_removed(device_id, tsuki)
                }
            }).unwrap();

        tsuki.redraw(self);
    }

    fn device_added(
        &mut self,
        device_id: dev_t,
        path: PathBuf,
        tsuki: &mut Tsuki
    ) -> anyhow::Result<()> {
        if path != self.primary_gpu_path {
            println!("skipping non-primary device {path:?}");
            return Ok(())
        }

        println!("adding device {path:?}");
        assert!(self.output_device.is_none());

        let open_flags = OFlags::RWMODE | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
        let fd = self.session.open(&path, open_flags)?;
        let device_fd =  DrmDeviceFd::new(DeviceFd::from(fd));

        let (mut drm, drm_notifier) = DrmDevice::new(device_fd.clone(), true)?;
        let gbm = GbmDevice::new(device_fd)?;

        let display = unsafe { EGLDisplay::new(gbm.clone()) }?;
        let egl_context = EGLContext::new(&display)?;

        let mut gles = unsafe { GlesRenderer::new(egl_context)? };
        gles.bind_wl_display(&tsuki.display_handle);

        let drm_compositor = self.create_drm_compositor(&mut drm, &gbm, &gles, tsuki)?;

        let token = tsuki
            .event_loop
            .insert_source(drm_notifier, move |event, metadata, data| {
                let tty = data.tty.as_mut().unwrap();
                match event {
                    DrmEvent::VBlank(_crtc) => {
                        let output_device = tty.output_device.as_mut().unwrap();

                        if let Err(err) = output_device.drm_compositor.frame_submitted() {
                            // print error message 
                        }

                        data.tsuki.redraw(tty);
                    },
                    DrmEvent::Error(error) => {println!("DRM error: {error}")}
                }
            }).unwrap();

        self.output_device = Some(OutputDevice { id: device_id, path, token, drm, gles, drm_compositor });

        Ok(())
    }

    fn device_changed(&mut self, device_id: dev_t, tsuki: &mut Tsuki) {
        if let Some(output_device) = &self.output_device {
            if output_device.id == device_id {
                println!("output device changed");

                let path = output_device.path.clone();
                self.device_removed(device_id, tsuki);
                if let Err(err) = self.device_added(device_id, path, tsuki) {
                    println!("error adding device: {err:?}");
                }
            }
        }
    }

    fn device_removed(&mut self, device_id: dev_t, tsuki: &mut Tsuki) {
        if let Some(mut output_device) = self.output_device.take() {
            if output_device.id != device_id {
                self.output_device = Some(output_device);
                return;
            }

            tsuki.event_loop.remove(output_device.token);
            tsuki.output = None;
            output_device.gles.unbind_wl_display();
        }
    }

    fn create_drm_compositor(
        &mut self,
        drm: &mut DrmDevice,
        gbm: &GbmDevice<DrmDeviceFd>,
        gles: &GlesRenderer,
        tsuki: &mut Tsuki
    ) -> anyhow::Result<GbmDrmCompositor> {
        let formats = Bind::<Dmabuf>::supported_formats(gles)
            .ok_or_else(|| anyhow!("no supported formats"))?;

        let resources = drm.resource_handles()?;

        let mut connector = None;
        resources
            .connectors()
            .iter()
            .filter_map(|conn| match drm.get_connector(*conn, true) {
                Ok(info) => Some(info),
                Err(err) => {
                    println!("error probing connector: {err}");
                    None
                }
            })
            .inspect(|conn| {
                println!(
                    "connector: {} {}, {:?}, {} modes",
                    conn.interface().as_str(),
                    conn.interface_id(),
                    conn.state(),
                    conn.modes().len()
                );
            })
            .filter(|conn| conn.state() == ConnectorState::Connected)
            .filter(|conn| conn.interface() == ConnectorInterface::EmbeddedDisplayPort)
            .for_each(|conn| connector = Some(conn));

        let connector = connector.ok_or_else(|| anyhow!("no compatible connector"))?;
        println!(
            "picking connector: {} {}",
            connector.interface().as_str(),
            connector.interface_id()
        );

        let mut mode = connector.modes().get(0);
        connector.modes().iter().for_each(|m| {
            println!("mode: {m:?}");

            if m.mode_type().contains(ModeTypeFlags::PREFERRED) && mode
                    .map(|curr| curr.vrefresh() < m.vrefresh())
                    .unwrap_or(true) {
                mode = Some(m);
            }
        });
        let mode = mode.ok_or_else(|| anyhow!("no mode"))?;
        println!("picking mode: {mode:?}");

        let encoders = connector.encoders().iter()
            .filter_map(|enc| drm.get_encoder(*enc).ok());

        let mut all_crtcs = Vec::new();

        for enc in encoders {
            let mut crtcs = resources.filter_crtcs(enc.possible_crtcs());
        
            crtcs.sort_by_cached_key(|crtc| match drm.planes(crtc) {
                Ok(planes) => -(planes.overlay.len() as isize),
                Err(err) => {
                    println!("error probing planes for CRTC: {err}");
                    0
                }
            });
        
            all_crtcs.extend(crtcs);
        }

        let surface = all_crtcs.into_iter().find_map(|crtc| match drm.create_surface(crtc, *mode, &[connector.handle()]) {
            Ok(surface) => Some(surface),
            Err(err) => {
                println!("error creating drm surface: {err}");
                None
            }
        });
        
        let surface = surface.ok_or_else(|| anyhow!("no surface"))?;

        let gbm_flags = GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT;
        let allocator = GbmAllocator::new(gbm.clone(), gbm_flags);

        let (physical_width, physical_height) = connector.size().unwrap_or((0, 0));
        let output_name = format!(
            "{}-{}",
            connector.interface().as_str(),
            connector.interface_id()
        );

        let (make, model) = display_info::for_connector(drm, connector.handle())
            .map(|info| (info.make(), info.model()))
            .unwrap_or_else(|| (Some("Unknown".to_string()), Some("Unknown".to_string())));

        let output = Output::new(
            output_name,
            PhysicalProperties {
                size: (physical_width as i32, physical_height as i32).into(),
                subpixel: Subpixel::Unknown,
                model: model.unwrap(),
                make: make.unwrap()      
            } 
        );

        let wl_mode = Mode::from(*mode);
        output.change_current_state(Some(wl_mode), None, None, Some((0, 0).into()));
        output.set_preferred(wl_mode);

        let _global = output.create_global::<Tsuki>(&tsuki.display_handle);
        tsuki.space.map_output(&output, (0, 0));
        tsuki.output = Some(output.clone());

        let compositor = DrmCompositor::new(
            OutputModeSource::Auto(output),
            surface, 
            None,
            allocator,
            gbm.clone(), 
            SUPPORTED_COLOR_FORMATS.iter().copied(),
            formats, 
            drm.cursor_size(),
            Some(gbm.clone()) 
        )?;
        Ok(compositor)
    }
}