use smithay::{
    reexports::wayland_server::{
        protocol::{wl_surface::WlSurface, wl_buffer::WlBuffer},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorHandler, CompositorState, CompositorClientState},
        shm::{ShmHandler, ShmState},
        output::OutputHandler, // <--- Добавили импорт
    },
    input::{SeatHandler, Seat, SeatState, pointer::CursorImageStatus},
};
use crate::state::{Smallvil, ClientState};

impl CompositorHandler for Smallvil {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, _surface: &WlSurface) {
        // Срабатывает при обновлении окна клиентом
    }
}

// Реализуем BufferHandler, необходимый для ShmState
impl BufferHandler for Smallvil {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for Smallvil {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

// Реализуем SeatHandler, так как XdgShellState от него зависит
impl SeatHandler for Smallvil {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&Self::KeyboardFocus>) {}
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}

// Реализуем OutputHandler для вывода (экрана)
impl OutputHandler for Smallvil {}

smithay::delegate_compositor!(Smallvil);
smithay::delegate_shm!(Smallvil);
smithay::delegate_seat!(Smallvil);
smithay::delegate_output!(Smallvil); // <--- Добавили макрос делегирования