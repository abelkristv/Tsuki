use slog::Drain;

mod handlers;
mod grabs;
mod input;
mod state;
mod backend;

pub use state::Tsuki;
use smithay::{
    reexports::{
        calloop::{
            EventLoop
        },
        wayland_server::{
            Display
        }
    }
};


pub struct CalloopData {
    state: Tsuki,
    display: Display<Tsuki>
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let log = slog::Logger::root(slog_stdlog::StdLog.fuse(), slog::o!());
    slog_stdlog::init().unwrap();

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new().unwrap(); 
    let mut display: Display<Tsuki> = Display::new().unwrap();
    let state = Tsuki::new(&mut event_loop, &mut display, log.clone());

    let mut data = CalloopData { state, display };
    
    crate::backend::winit::init_winit(&mut event_loop, &mut data, log)?;

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match(flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("alacritty").spawn().ok();
        }
    }

    event_loop.run(None, &mut data, move |_| {

    })?;

    Ok(())
}
