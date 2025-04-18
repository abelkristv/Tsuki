#![allow(irrefutable_let_patterns)]
mod handlers;

mod grabs;
mod input;
mod state;
mod backend;

use std::{cell::RefCell, env, rc::Rc};

use backend::{Backend, Tty, Winit};
use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};
pub use state::Tsuki;

pub struct CalloopData {
    tsuki: Tsuki,
    display_handle: DisplayHandle,
    backend: Rc<RefCell<dyn Backend>>
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let has_display = env::var_os("WAYLAND_DISPLAY").is_some() || env::var_os("DISPLAY").is_some();

    log::info!("has display: {}", has_display);

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let mut winit: Option<Rc<RefCell<dyn Backend>>> = None;
    let mut tty:  Option<Rc<RefCell<dyn Backend>>> = None;
    let backend: Rc<RefCell<dyn Backend>> = if has_display {
        winit = Some(Rc::new(RefCell::new(Winit::new(event_loop.handle()))));
        winit.unwrap()
    } else {
        log::info!("running on tty");
        tty = Some(Rc::new(RefCell::new(Tty::new(event_loop.handle()))));
        tty.unwrap()
    };



    let display = Display::new().unwrap();
    let display_handle = display.handle();
    let state = Tsuki::new(event_loop.handle(), event_loop.get_signal(), display, backend.clone());


    let mut data = CalloopData {
        tsuki: state,
        display_handle,
        backend: backend.clone()
    };

    backend.clone().borrow_mut().init(&mut data.tsuki);

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();
    std::env::set_var("WAYLAND_DISPLAY", &data.tsuki.socket_name);


    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("weston-terminal").spawn().ok();
        }
    }

    event_loop.run(None, &mut data, move |data| {
        // Tsuki is running
        data.display_handle.flush_clients().unwrap();
    })?;

    Ok(())
}