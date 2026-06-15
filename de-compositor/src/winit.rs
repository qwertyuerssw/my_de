use smithay::{
    backend::{
        renderer::{gles::GlesRenderer, Frame, Renderer},
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    output::{Output, PhysicalProperties, Subpixel, Mode},
    utils::{Rectangle, Transform},
};
use crate::state::Smallvil;

pub fn init_winit(
    state: &mut Smallvil,
    backend: WinitGraphicsBackend<GlesRenderer>,
    winit_event_loop: winit::WinitEventLoop,
) -> Result<(), Box<dyn std::error::Error>> {
    let size = backend.window_size();

    // 1. Создаем виртуальный вывод для отображения окон клиентов
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".to_string(),
            model: "Winit".to_string(),
        },
    );
    
    // 2. Регистрируем вывод в дисплее, чтобы клиенты знали о его существовании
    let _global = output.create_global::<Smallvil>(&state.display.handle());
    
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    // 3. Создаем виртуальное устройство ввода (wl_seat)
    // Метод принимает ровно два аргумента в Smithay 0.7
    let mut seat = state.seat_state.new_wl_seat(&state.display.handle(), "winit");
    seat.add_keyboard(Default::default(), 200, 25)?;
    seat.add_pointer();

    // 4. Регистрируем источник событий Winit в нашем цикле calloop
    let mut backend = backend;
    state.loop_handle.insert_source(winit_event_loop, move |event, _, state| {
        match event {
            WinitEvent::Resized { size, .. } => {
                let mode = Mode {
                    size,
                    refresh: 60_000,
                };
                output.change_current_state(Some(mode), None, None, None);
            }
            WinitEvent::Input(event) => {
                tracing::debug!("Input event from winit: {:?}", event);
            }
            WinitEvent::Redraw => {
                let size = backend.window_size();
                
                // Создаем изолированную область видимости для отрисовки кадра
                {
                    let (renderer, mut framebuffer) = backend.bind().unwrap();
                    let mut frame = renderer.render(&mut framebuffer, size, Transform::Normal).unwrap();
                    
                    let damage = Rectangle::from_size(size);
                    frame.clear([0.15, 0.15, 0.15, 1.0].into(), &[damage]).unwrap();
                    
                    // В конце блока frame, renderer и framebuffer выходят из области видимости 
                    // и автоматически уничтожаются (drop), возвращая заимствование backend.
                } 
                
                // Теперь backend больше никем не заимствован, и мы можем безопасно вызвать submit!
                backend.submit(None).unwrap();
            }
            WinitEvent::CloseRequested => {
                state.running = false;
            }
            _ => {}
        }
    })?;

    Ok(())
}