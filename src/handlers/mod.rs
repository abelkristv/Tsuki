use crate::Tsuki;

mod compositor;
mod xdg_shell;

//
// Wl seat 
// 
//
use std::os::fd::OwnedFd;

use smithay::input::{SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::data_device::{ClientDndGrabHandler, DataDeviceHandler, ServerDndGrabHandler};
use smithay::{delegate_data_device, delegate_output, delegate_seat, delegate_primary_selection};
use smithay::reexports::wayland_server::protocol::wl_data_source::{WlDataSource, Request};
use smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction;
use smithay::reexports::wayland_server::{ResourceData, Resource};
use smithay::wayland::{
    primary_selection::{set_primary_focus, PrimarySelectionState, PrimarySelectionHandler},
    data_device::set_data_device_focus
};

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

    fn focus_changed(&mut self, seat: &smithay::input::Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;

        let focus = focused
        //    .and_then(WaylandFocus::wl_surface)
            .and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }
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

// 
// Primary Selection
//
impl PrimarySelectionHandler for Tsuki {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}

delegate_primary_selection!(Tsuki);
