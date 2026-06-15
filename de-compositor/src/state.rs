use std::os::unix::net::UnixStream;

use smithay::{
    desktop::{Space, Window},
    input::SeatState,
    reexports::wayland_server::{
        backend::{ClientData, ClientId, DisconnectReason},
        Display,
    },
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        shell::xdg::XdgShellState,
        shm::ShmState,
    },
};

/// Сессия подключенного IPC-клиента
pub struct ClientSession {
    pub client_type: Option<de_ipc::ClientType>,
    pub read_buffer: String, // Накопительный строковый буфер для построчного неблокирующего чтения
    pub writer: UnixStream,   // Блокирующий поток для гарантированной отправки ответов
}

pub struct Smallvil {
    pub display: Display<Smallvil>,
    pub space: Space<Window>,
    pub loop_handle: smithay::reexports::calloop::LoopHandle<'static, Smallvil>,

    // Smithay States
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<Self>,

    // IPC State: храним структурированные сессии клиентов
    pub ipc_clients: std::collections::HashMap<u32, ClientSession>,
    pub next_client_id: u32,

    pub running: bool,
}

impl Smallvil {
    pub fn new(
        display: Display<Self>,
        loop_handle: smithay::reexports::calloop::LoopHandle<'static, Self>,
    ) -> Self {
        let compositor_state = CompositorState::new::<Self>(&display.handle());
        let xdg_shell_state = XdgShellState::new::<Self>(&display.handle());
        let shm_state = ShmState::new::<Self>(&display.handle(), Vec::new());
        let seat_state = SeatState::new();
        let space = Space::default();

        Self {
            display,
            space,
            loop_handle,
            compositor_state,
            xdg_shell_state,
            shm_state,
            seat_state,
            ipc_clients: std::collections::HashMap::new(),
            next_client_id: 1,
            running: true,
        }
    }
}

pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientState {
    pub fn new() -> Self {
        Self {
            compositor_state: CompositorClientState::default(),
        }
    }
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}