use crate::state::Smallvil;
use smithay::{
    backend::{
        renderer::gles::GlesRenderer, // Убрали Renderer
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    utils::Transform,
};

pub fn init_winit(
    state: &mut Smallvil,
    backend: WinitGraphicsBackend<GlesRenderer>, // <-- добавили <GlesRenderer>
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
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    // 3. Создаем виртуальное устройство ввода (wl_seat)
    let mut seat = state
        .seat_state
        .new_wl_seat(&state.display.handle(), "winit");
    seat.add_keyboard(Default::default(), 200, 25)?;
    seat.add_pointer();

    // Трекер повреждений, который нужен для новой архитектуры рендеринга
    let mut damage_tracker = smithay::backend::renderer::damage::OutputDamageTracker::from_output(&output);

    // 4. Регистрируем источник событий Winit в нашем цикле calloop
    let mut backend = backend;
    state
        .loop_handle
        .insert_source(winit_event_loop, move |event, _, state| {
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
                    // Отрисовываем все окна, зарегистрированные в нашем пространстве (Space)
                    {
                        let (renderer, mut framebuffer) = backend.bind().unwrap();
                        
                        smithay::desktop::space::render_output(
                            &output, // Output
                            renderer,
                            &mut framebuffer,
                            1.0f32, // масштаб (scale)
                            0, // Возраст буфера (age) для winit-бэкенда равен 0
                            [&state.space], // Передаем как итератор/массив пространств
                            &[] as &[smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement<GlesRenderer>], // Пользовательские элементы
                            &mut damage_tracker, // Трекер
                            [0.15f32, 0.15f32, 0.15f32, 1.0f32], // Цвет фона (серо-угольный)
                        )
                        .unwrap();
                    }

                    // Отправляем отрисованный кадр на видеокарту/экран
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