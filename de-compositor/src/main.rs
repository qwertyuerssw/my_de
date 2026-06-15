mod handlers;
mod ipc;
pub mod state; // Модуль сделан pub для беспрепятственного разрешения зависимостей
mod winit;

use smithay::{
    backend::renderer::gles::GlesRenderer,
    reexports::{calloop::EventLoop, wayland_server::Display},
};
use state::Smallvil;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut event_loop = EventLoop::<Smallvil>::try_new()?;
    let display = Display::<Smallvil>::new()?;
    let mut state = Smallvil::new(display, event_loop.handle());

    // Инициализируем наш IPC UNIX-сокет
    ipc::init_ipc(&mut state)?;

    let (backend, winit_event_loop) = smithay::backend::winit::init::<GlesRenderer>()?;
    winit::init_winit(&mut state, backend, winit_event_loop)?;

    tracing::info!(
        "Wayland compositor and IPC server initialized successfully. Running event loop..."
    );

    while state.running {
        event_loop.dispatch(std::time::Duration::from_millis(16), &mut state)?;
        state.display.flush_clients()?;
    }

    Ok(())
}