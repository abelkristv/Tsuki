#![allow(irrefutable_let_patterns)]

mod handlers;

mod grabs;
mod input;
mod state;
mod winit;
mod backend;

use std::{cell::RefCell, rc::Rc};

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};
pub use state::Tsuki;
use winit::Winit;

pub struct CalloopData {
    tsuki: Tsuki,
    display_handle: DisplayHandle,
    winit: Option<Winit>
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;


    let display = Display::new().unwrap();
    let display_handle = display.handle();
    let state = Tsuki::new(&mut event_loop, display);

    let winit = Some(Winit::new(event_loop.handle()));

    let mut data = CalloopData {
        tsuki: state,
        display_handle,
        winit
    };

    if let Some(winit) = data.winit.as_mut() {
        winit.init(&mut data.tsuki);
    }

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("weston-terminal").spawn().ok();
        }
    }

    event_loop.run(None, &mut data, move |_| {
        // Smallvil is running
    })?;

    Ok(())
}