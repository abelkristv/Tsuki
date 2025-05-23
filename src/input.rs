
use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GesturePinchUpdateEvent, InputBackend, InputEvent, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent
    },
    input::{
        keyboard::{keysyms, FilterResult, Keysym},
        pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
};

use crate::{backend::{Backend, Tty}, state::Tsuki};
use std::{cell::RefCell, mem, rc::Rc};
use smithay::backend::input::PointerMotionEvent;

enum TsukiInputAction {
    Quit,
    ChangeVirtTerminal(i32)
}

impl Tsuki {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

                let action = self.seat.get_keyboard().unwrap().input::<TsukiInputAction, _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, modifier_state, keysym| match keysym.modified_sym() {
                        Keysym::Q if modifier_state.ctrl && modifier_state.shift => {
                            FilterResult::Intercept(TsukiInputAction::Quit)
                        },
                        keysym if (u32::from(Keysym::XF86_Switch_VT_1)..=u32::from(Keysym::XF86_Switch_VT_12)).contains(&(keysym.raw()))=> {
                            let vt = (keysym.raw() - u32::from(Keysym::XF86_Switch_VT_1) + 1) as i32;
                            FilterResult::Intercept(TsukiInputAction::ChangeVirtTerminal(vt))
                        }
                        _ => FilterResult::Forward,
                    } 
                );

                if let Some(action) = action {
                    match action {
                        TsukiInputAction::Quit => {
                            self.loop_signal.stop()
                        },
                        TsukiInputAction::ChangeVirtTerminal(vt) => {
                            self.backend_data.clone().borrow_mut().as_any().downcast_mut::<Tty>().unwrap().change_virt_term(vt);
                        }
                    }
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();
                let mut pointer_location = pointer.current_location();

                pointer_location += event.delta();

                let output = self.space.outputs().next().unwrap();
                let output_geo = self.space.output_geometry(output).unwrap();

                pointer_location.x = pointer_location.x.clamp(0., output_geo.size.w as f64);
                pointer_location.y = pointer_location.y.clamp(0., output_geo.size.h as f64);

                let under = self.surface_under(pointer_location);
                pointer.motion(
                    self, 
                    under.clone(),
                    &MotionEvent { location: pointer_location, serial, time: event.time_msec() });

                pointer.relative_motion(
                    self, 
                    under, 
                &RelativeMotionEvent {
                    delta: event.delta(),
                    delta_unaccel: event.delta_unaccel(),
                    utime: event.time()
                });
                self.queue_redraw();

            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = self.space.outputs().next().unwrap();

                let output_geo = self.space.output_geometry(output).unwrap();

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
                self.queue_redraw();
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _loc)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        keyboard.set_focus(
                            self,
                            Some(window.toplevel().unwrap().wl_surface().clone()),
                            serial,
                        );
                        self.space.elements().for_each(|window| {
                            window.toplevel().unwrap().send_pending_configure();
                        });
                    } else {
                        self.space.elements().for_each(|window| {
                            window.set_activated(false);
                            window.toplevel().unwrap().send_pending_configure();
                        });
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                };

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event
                    .amount(Axis::Horizontal)
                    .unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
                let vertical_amount = event
                    .amount(Axis::Vertical)
                    .unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            },
            _ => {}
        }
    }
}