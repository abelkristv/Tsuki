use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer,
        },
        winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{calloop::{timer::Timer, EventLoop, LoopHandle}, winit::platform::pump_events::PumpStatus},
    utils::{Point, Rectangle, Transform},
};

use crate::{backend::Backend, state::Tsuki, CalloopData};

pub struct Winit {
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    winit_event_loop: WinitEventLoop,
    damage_tracker: OutputDamageTracker
}

impl Backend for Winit {
    fn set_name(&self) -> String {
        "winit".to_owned()
    }

    fn renderer(&mut self) -> &mut GlesRenderer {
       self.backend.renderer()
    }

    fn render(
        &mut self,
        tsuki: &mut Tsuki,
        elements: &[smithay::desktop::space::SpaceRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    ) {
        let size = self.backend.window_size();
        let damage = Rectangle::new(Point::from((0, 0)), size);
        self.damage_tracker
            .render_output(self.backend.renderer(), 0, elements, [0.1, 0.2, 0.1, 1.0])
            .unwrap();
        self.backend.submit(Some(&[damage])).unwrap()
    }
}

impl Winit {
    pub fn new(event_loop: LoopHandle<CalloopData>) -> Self {
        let (backend, winit_event_loop) = winit::init().unwrap();

        let mode = Mode {
            size: backend.window_size(),
            refresh: 60_000
        };

        let output = Output::new(
            "winit".to_string(),
            PhysicalProperties { 
                size: (0,0).into(),
                subpixel: Subpixel::Unknown, 
                make: "Smithay".into(), 
                model: "Winit".into() }
        );

        output.change_current_state(
            Some(mode), 
            Some(Transform::Flipped180),
            None, 
            Some((0, 0).into())
        );

        output.set_preferred(mode);

        let damage_tracker = OutputDamageTracker::from_output(&output);

        let timer = Timer::immediate();

        event_loop
            .insert_source(timer, move |_, _, data| {
                let winit = data.winit.as_mut().unwrap();
                winit.dispatch(&mut data.tsuki);
                smithay::reexports::calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(16))
            }).unwrap();
        
        Self {
            output,
            backend,
            winit_event_loop,
            damage_tracker
        }
    }

    pub fn init(&mut self, tsuki: &mut Tsuki) {
        let _global = self.output.create_global::<Tsuki>(&tsuki.display_handle);
            tsuki.space.map_output(&self.output, (0, 0));
            tsuki.output = Some(self.output.clone());
    }

    fn dispatch(&mut self, tsuki: &mut Tsuki) {
        let res = self
            .winit_event_loop
            .dispatch_new_events(|event| match event {
                WinitEvent::Resized { size, scale_factor } => {
                    tsuki.output.as_ref().unwrap().change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000
                        }), 
                        None, 
                        None, 
                    None);
                },
                WinitEvent::Input(event) => tsuki.process_input_event(event),
                _ => ()
            });
        
        if let PumpStatus::Exit(val)= res {
            tsuki.loop_signal.stop();
            return;
        } 

        self.backend.bind().unwrap();
        tsuki.redraw(self);
    }
}