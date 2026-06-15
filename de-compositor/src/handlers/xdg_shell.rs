use smithay::{
    desktop::Window,
    wayland::shell::xdg::{XdgShellHandler, XdgShellState, ToplevelSurface, PopupSurface, PositionerState},
};
use crate::state::Smallvil;

impl XdgShellHandler for Smallvil {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), true);
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {
        // Всплывающие окна
    }

    fn grab(
        &mut self,
        _surface: PopupSurface,
        _seat: smithay::reexports::wayland_server::protocol::wl_seat::WlSeat,
        _serial: smithay::utils::Serial,
    ) {
        // Перехват ввода
    }

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
        // Изменение позиции всплывающего окна
    }
}

smithay::delegate_xdg_shell!(Smallvil);