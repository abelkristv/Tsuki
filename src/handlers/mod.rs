use crate::Tsuki;

mod compositor;
mod xdg_shell;

//
// Wl seat 
// 

use smithay::input::{SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::data_device::{ClientDndGrabHandler, DataDeviceHandler, ServerDndGrabHandler};
use smithay::{delegate_data_device, delegate_output, delegate_seat};

impl SeatHandler for Tsuki {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Tsuki> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &smithay::input::Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus
    ) {}

    fn focus_changed(&mut self, _seat: &smithay::input::Seat<Self>, _focused: Option<&WlSurface>) {}
}

delegate_seat!(Tsuki);

// 
// Wl Data Device 
// 

impl DataDeviceHandler for Tsuki {
    fn data_device_state(&self) -> &smithay::wayland::data_device::DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for Tsuki {}
impl ServerDndGrabHandler for Tsuki {}

delegate_data_device!(Tsuki);

// 
// Wl Output & Xdg output
//

delegate_output!(Tsuki);
