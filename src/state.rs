use std::{borrow::{Borrow, BorrowMut}, cell::RefCell, ffi::OsString, os::fd::AsFd, rc::Rc, sync::Arc, time::Duration};

use smithay::{
    backend::{self, drm::output::DrmOutputRenderElements, renderer::{element::{solid::SolidColorRenderElement, Kind}, utils::CommitCounter, ImportAll}}, desktop::{space::{space_render_elements, SpaceRenderElements}, PopupManager, Space, Window, WindowSurfaceType}, input::{Seat, SeatState}, output::Output, reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        }, x11rb::protocol::shape::Op,
    }, render_elements, utils::{Logical, Point}, wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    }
};

use crate::{backend::Backend, CalloopData};

pub struct Tsuki {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub event_loop: LoopHandle<'static, CalloopData>,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Tsuki>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub backend_data: Rc<RefCell<dyn Backend>>,

    pub seat: Seat<Self>,
    pub output: Option<Output>
}

impl Tsuki {
    pub fn new(event_loop: LoopHandle<'static, CalloopData>, loop_signal: LoopSignal, display: Display<Self>, backend: Rc<RefCell<dyn Backend>>) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25).unwrap();

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        let socket_name = Self::init_wayland_listener(display, event_loop.clone());
        

        Self {
            start_time,
            display_handle: dh,
            event_loop,

            space,
            loop_signal,
            socket_name,
            backend_data: backend,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
            output: None
        }
    }

    fn init_wayland_listener(
        display: Display<Tsuki>,
        loop_handle: LoopHandle<CalloopData>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::with_name("wayland-1").unwrap();
        println!("{:?}", listening_socket.socket_name());

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    unsafe {
                        display.get_mut().dispatch_clients(&mut state.tsuki).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn redraw(&mut self, backend: &mut dyn Backend) {
        if let Some(renderer) = backend.renderer() {
            let elements = space_render_elements(
                renderer, 
                [&self.space], 
                self.output.as_ref().unwrap(), 
            1.0
            ).unwrap();

            let mut elements: Vec<_> = elements
                .into_iter()
                .map(OutputRenderElements::from)
                .collect();

            elements.insert(
                0, 
                OutputRenderElements::Pointer(SolidColorRenderElement::new(
                    smithay::backend::renderer::element::Id::new(),
                    smithay::utils::Rectangle {
                        loc: self
                            .seat
                            .get_pointer()
                            .unwrap()
                            .current_location()
                            .to_physical_precise_round(1.),
                        size: (16, 16).into()
                    }, 
                    CommitCounter::default(), 
                    [1., 0.5, 0., 1.],
                    Kind::Cursor
                ))
            );
    
            backend.render(self, &elements);
    
            let output = self.output.as_ref().unwrap();
            self.space.elements().for_each(|window| {
                window.send_frame(
                    output, 
                    self.start_time.elapsed(),
                    Some(Duration::ZERO),
                    |_, _| Some(output.clone()));
            });
    
            self.space.refresh();
        }
    }

    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        })
    }
}

render_elements! {
    pub OutputRenderElements<R, E> where R: ImportAll;
    Space=SpaceRenderElements<R, E>,
    Pointer = SolidColorRenderElement,
}
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}