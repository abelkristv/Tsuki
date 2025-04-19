use std::{any::Any, cell::RefCell, rc::Rc, time::Duration};

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer,
        },
        winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{calloop::{timer::{TimeoutAction, Timer}, EventLoop, LoopHandle}, winit::platform::pump_events::PumpStatus},
    utils::{Point, Rectangle, Transform},
};

use crate::{backend::Backend, state::{OutputRenderElements, Tsuki}, CalloopData};

pub struct Winit {
    output: Output,
    backend: WinitGraphicsBackend<GlesRenderer>,
    winit_event_loop: WinitEventLoop,
    damage_tracker: OutputDamageTracker
}

impl Backend for Winit {
    fn seat_name(&self) -> String {
        "winit".to_owned()
    }

    fn renderer(&mut self) -> Option<&mut GlesRenderer> {
       Some(self.backend.renderer())
    }

    fn render(
        &mut self,
        tsuki: &mut Tsuki,
        elements: &[OutputRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    ) {
        let size = self.backend.window_size();
        let damage = Rectangle::new(Point::from((0, 0)), size);
        self.damage_tracker
            .render_output(self.backend.renderer(), 0, elements, [0.0, 0.0, 0.0, 1.0])
            .unwrap();
        self.backend.submit(Some(&[damage])).unwrap()
    }
    
    fn init(&mut self, tsuki: &mut Tsuki) {
        let _global = self.output.create_global::<Tsuki>(&tsuki.display_handle);
            tsuki.space.map_output(&self.output, (0, 0));
            tsuki.output = Some(self.output.clone());
    }
    
    fn as_any (&mut self) -> &mut dyn Any {
        self
    }
}

impl Winit {
    pub fn new(event_loop: LoopHandle<CalloopData>) -> Self {
        let (backend, winit_event_loop) = winit::init().unwrap();
        log::info!("winit is here somehoww");


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
                let mut binding = data.backend.borrow_mut();
                let binding = binding.as_any().downcast_mut::<Winit>();

                if binding.is_none() {
                    return TimeoutAction::Drop
                }
                // println!("Type id: {:?}", data.backend.borrow_mut().as_any().type_id());
                // println!("Expected: {:?}", std::any::TypeId::of::<Rc<RefCell<Winit>>>());
                let backend = binding.unwrap();
                backend.dispatch(&mut data.tsuki);
                
                smithay::reexports::calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(16))
            }).unwrap();
        
        Self {
            output,
            backend,
            winit_event_loop,
            damage_tracker
        }
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
                WinitEvent::CloseRequested => {tsuki.loop_signal.stop();}, 
                WinitEvent::Input(event) => tsuki.process_input_event(event),
                WinitEvent::Focus(_) => (),
                WinitEvent::Redraw => tsuki.queue_redraw(),
                _ => ()
            });
        
        if let PumpStatus::Exit(val)= res {
            tsuki.loop_signal.stop();
            return;
        } 

        self.backend.bind().unwrap();
        tsuki.queue_redraw();
    }
}