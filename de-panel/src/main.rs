use de_ipc::{IpcMessage, ProcessAction};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Align, Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation};
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::time::Duration;

struct ModuleWidgets {
    label: Label,
    button: Button,
    is_running: bool,
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.myde.panel")
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("MyDE Panel")
        .build();

    // 1. Инициализация Layer Shell
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.auto_exclusive_zone_enable();
    
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);

    // 2. Построение макета UI
    let main_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(15)
        .margin_top(5)
        .margin_bottom(5)
        .margin_start(15)
        .margin_end(15)
        .build();

    let title_label = Label::builder()
        .label("🌌 MyDE")
        .build();
    main_box.append(&title_label);

    let data_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(15)
        .hexpand(true)
        .build();
    main_box.append(&data_box);

    let controls_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(5)
        .halign(Align::End)
        .build();
    main_box.append(&controls_box);

    window.set_child(Some(&main_box));

    // 3. Подключение к IPC-сокету
    let socket_path = "/tmp/my-de-ipc.sock";
    let mut stream_opt = None;

    for attempt in 1..=10 {
        match UnixStream::connect(socket_path) {
            Ok(s) => {
                println!("[Panel] Connected to IPC socket (attempt {}).", attempt);
                stream_opt = Some(s);
                break;
            }
            Err(e) => {
                println!("[Panel] IPC connection failed: {}. Retrying...", e);
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }

    let stream = stream_opt.expect("CRITICAL: Failed to connect to IPC socket.");
    let mut writer_stream = stream.try_clone().expect("Failed to clone stream");

    let reg_msg = IpcMessage::Register {
        client_type: de_ipc::ClientType::Panel,
    };
    let mut serialized = serde_json::to_string(&reg_msg).unwrap();
    serialized.push('\n');
    let _ = writer_stream.write_all(serialized.as_bytes());
    let _ = writer_stream.flush();

    // 4. Современные каналы связи: async_channel -> GTK Main Loop
    let (tx_incoming, rx_incoming) = async_channel::unbounded::<IpcMessage>();
    let (tx_outgoing, rx_outgoing) = std::sync::mpsc::channel::<IpcMessage>();

    // Фоновый поток: Чтение из сокета -> Отправка в канал UI
    let read_stream = stream;
    std::thread::spawn(move || {
        let reader = BufReader::new(read_stream);
        for line in reader.lines() {
            if let Ok(content) = line {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&content) {
                    // Используем send_blocking, так как мы находимся в синхронном потоке
                    let _ = tx_incoming.send_blocking(msg);
                }
            }
        }
    });

    // Фоновый поток: Чтение из UI -> Отправка в сокет
    std::thread::spawn(move || {
        while let Ok(msg) = rx_outgoing.recv() {
            if let Ok(mut serialized) = serde_json::to_string(&msg) {
                serialized.push('\n');
                if writer_stream.write_all(serialized.as_bytes()).is_err() {
                    break;
                }
                let _ = writer_stream.flush();
            }
        }
    });

    let _ = tx_outgoing.send(IpcMessage::QueryStatus);

    let widgets_map: Rc<RefCell<HashMap<String, ModuleWidgets>>> = Rc::new(RefCell::new(HashMap::new()));

    // Клонируем переменные для перемещения в асинхронный блок
    let data_box_clone = data_box.clone();
    let controls_box_clone = controls_box.clone();
    let tx_out = tx_outgoing.clone();

    // 5. Асинхронный локальный цикл внутри главного потока GTK
    glib::MainContext::default().spawn_local(async move {
        // Ожидаем новые сообщения асинхронно, не блокируя UI!
        while let Ok(msg) = rx_incoming.recv().await {
            let mut map = widgets_map.borrow_mut();

            match msg {
                IpcMessage::ModulesList { modules } => {
                    for config in modules {
                        if !map.contains_key(&config.uuid) {
                            let label = Label::builder()
                                .label(format!("{}: Off", config.name))
                                .build();
                            data_box_clone.append(&label);

                            let button = Button::builder()
                                .label(format!("Start {}", config.name)) // Используем name для красивой кнопки
                                .build();
                            
                            let tx_out_inner = tx_out.clone();
                            let mod_uuid = config.uuid.clone(); // <--- Изменено на uuid
                            
                            button.connect_clicked(move |b| {
                                let action = if b.label().unwrap_or_default().starts_with("Stop") {
                                    ProcessAction::Stop
                                } else {
                                    ProcessAction::Start
                                };
                                let _ = tx_out_inner.send(IpcMessage::ControlModule {
                                    module: mod_uuid.clone(), // <--- Изменено
                                    action,
                                });
                            });

                            controls_box_clone.append(&button);

                            map.insert(config.uuid.clone(), ModuleWidgets { // <--- Изменено на uuid
                                label,
                                button,
                                is_running: false,
                            });
                        }
                    }
                }
                IpcMessage::ModuleStatus { module, is_running } => {
                    if let Some(w) = map.get_mut(&module) {
                        w.is_running = is_running;
                        if is_running {
                            w.button.set_label(&format!("Stop {}", module));
                            w.label.set_label("Waiting data...");
                        } else {
                            w.button.set_label(&format!("Start {}", module));
                            w.label.set_label(&format!("{}: Off", module));
                        }
                    }
                }
                IpcMessage::PublishUpdate { module, data } => {
                    if let Some(w) = map.get_mut(&module) {
                        if w.is_running {
                            w.label.set_label(&data);
                        }
                    }
                }
                _ => {}
            }
        }
    });

    window.present();
}