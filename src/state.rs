use std::{ffi::OsString, sync::Arc, os::unix::io::AsRawFd};

use slog::Drain;
use slog::Logger;
use crate::CalloopData;

use smithay::{
    desktop::{Space, Window, WindowSurfaceType},
    input::{pointer::PointerHandle, Seat, SeatState},
    reexports::{
        calloop::{
            generic::Generic,
            LoopSignal,
            EventLoop,
            Interest,
            Mode,
            PostAction,
            timer::{TimeoutAction, Timer}
        },
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{
                wl_surface::WlSurface,
                wl_output
            },
            Display
        }
    },
    utils::{Logical, Point},
    wayland::{
        compositor::CompositorState,
        data_device::DataDeviceState,
        output::OutputManagerState,
        shell::xdg::XdgShellState,
        socket::ListeningSocketSource,
        shm::ShmState
    }
};

pub struct Tsuki {
    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    pub log: slog::Logger,

    pub socket_name: OsString,
    pub start_time: std::time::Instant,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Tsuki>,
    pub data_device_state: DataDeviceState,

    pub seat: Seat<Self>
}

impl Tsuki {
    pub fn new(event_loop: &mut EventLoop<CalloopData>, display: &mut Display<Self>, log: Logger) -> Self {
        let dh = display.handle();
        let start_time = std::time::Instant::now();

        let compositor_state = CompositorState::new::<Self, _>(&dh, log.clone());
        let xdg_shell_state = XdgShellState::new::<Self, _>(&dh, log.clone());
        let shm_state = ShmState::new::<Self, _>(&dh, vec![], log.clone());
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self, _>(&dh, log.clone());

        // A seat is a group of keyboards, pointer, and touch device 
        // A seat typically has a pointer and mantains a keyboard focus and a pointer focus
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit", log.clone());

        // Notify clients that we have keyboard, for the sake of the example we assume that
        // keyboard is always present 
        seat.add_keyboard(Default::default(), 200, 200).unwrap(); 

        // Notify clients that we have a pointer (mouse)
        // here we assume that there is always pointer plugged in 
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and outputs can be mapped onto it 
        //
        // Windows get a position and stacking order through mapping 
        // Outputs become views of a part of the Space and can be rendered via Space::
        // render_output
        let space = Space::new(log.clone());

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        let socket_name = Self::init_wayland_listener(display, event_loop, log.clone());

        Self {
            space,
            loop_signal,
            log,

            socket_name,
            start_time,

            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,

            seat
        }
    }

    fn init_wayland_listener(
        display: &mut Display<Self>,
        event_loop: &mut EventLoop<CalloopData>,
        log: slog::Logger,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available 'wayland'
        // socket name. 
        let listening_socket = ListeningSocketSource::new_auto(log).unwrap();

        // Get the name of the listening socket 
        // Clients will connect to this socket 
        let socket_name = listening_socket.socket_name().to_os_string();

        let handle = event_loop.handle();

        event_loop
            .handle()
            .insert_source(listening_socket, move |client_stream, _, state|{
                // Inside the callback, you should insert the client into the display
                //
                // You may also associate some data with the client when inserting the client 
                state.
                    display
                    .handle()
                    .insert_client(client_stream, Arc::new(ClientState))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source");

        // You also need to add the display itself to the event loop, so that client events will be
        // processed by wayland-server
        event_loop
            .handle()
            .insert_source(
                Generic::new(display.backend().poll_fd().as_raw_fd(), Interest::READ, Mode::Level),
                |_, _, state| {
                    state.display.dispatch_clients(&mut state.state).unwrap();
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();
        socket_name
    }

    pub fn surface_under_pointer(
        &self,
        pointer: &PointerHandle<Self>,
    ) -> Option<(WlSurface, Point<i32, Logical>)> {
        let pos = pointer.current_location();
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, p + location))
        })
    }
}

pub struct ClientState;
impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}