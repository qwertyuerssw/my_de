use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use eframe::egui;
use de_ipc::{IpcMessage, ModuleId, ProcessAction};

struct PanelApp {
    // Каналы для безопасного общения потока GUI с сетевыми потоками
    ipc_tx: std::sync::mpsc::Sender<IpcMessage>,
    ipc_rx: std::sync::mpsc::Receiver<IpcMessage>,
    
    // Текущие статусы запущенности процессов
    clock_running: bool,
    sysinfo_running: bool,
    
    // Последние данные, присланные модулями
    clock_data: String,
    sysinfo_data: String,
}

impl PanelApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 1. Подключаемся к нашему центральному сокету композитора
        let socket_path = "/tmp/my-de-ipc.sock";
        let stream = UnixStream::connect(socket_path).expect("Failed to connect to central IPC socket");
        
        // Клонируем поток для отправки сообщений наружу
        let mut writer_stream = stream.try_clone().expect("Failed to clone socket stream");
        
        // 2. Регистрируем это окно как Панель
        let reg_msg = IpcMessage::Register {
            client_type: de_ipc::ClientType::Panel,
        };
        let mut serialized = serde_json::to_string(&reg_msg).unwrap();
        serialized.push('\n');
        let _ = writer_stream.write_all(serialized.as_bytes());
        let _ = writer_stream.flush();
        
        // 3. Настраиваем канал для входящих сетевых сообщений
        let (tx, rx) = std::sync::mpsc::channel();
        
        // Запускаем асинхронный поток чтения
        let read_stream = stream;
        let ctx_clone = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(read_stream);
            for line in reader.lines() {
                if let Ok(content) = line {
                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(&content) {
                        let _ = tx.send(msg);
                        // Вызываем принудительную перерисовку GUI, как только получили данные
                        ctx_clone.request_repaint();
                    }
                }
            }
        });
        
        // 4. Настраиваем канал для исходящих команд управления процессом
        let (gui_tx, gui_rx) = std::sync::mpsc::channel::<IpcMessage>();
        std::thread::spawn(move || {
            while let Ok(msg) = gui_rx.recv() {
                if let Ok(mut serialized) = serde_json::to_string(&msg) {
                    serialized.push('\n');
                    if writer_stream.write_all(serialized.as_bytes()).is_err() {
                        break;
                    }
                    let _ = writer_stream.flush();
                }
            }
        });
        
        // Сразу после подключения запрашиваем статус всех процессов у de-manager
        let _ = gui_tx.send(IpcMessage::QueryStatus);

        Self {
            ipc_tx: gui_tx,
            ipc_rx: rx,
            clock_running: false,
            sysinfo_running: false,
            clock_data: "Waiting clock data...".to_string(),
            sysinfo_data: "Waiting sysinfo data...".to_string(),
        }
    }
}

impl eframe::App for PanelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Вычитываем все накопившиеся сетевые сообщения
        while let Ok(msg) = self.ipc_rx.try_recv() {
            match msg {
                IpcMessage::ModuleStatus { module, is_running } => {
                    match module {
                        ModuleId::Clock => self.clock_running = is_running,
                        ModuleId::SysInfo => self.sysinfo_running = is_running,
                    }
                }
                IpcMessage::PublishUpdate { module, data } => {
                    match module {
                        ModuleId::Clock => self.clock_data = data,
                        ModuleId::SysInfo => self.sysinfo_data = data,
                    }
                }
                _ => {}
            }
        }

        // Рисуем панель, прижатую к верхнему краю экрана
        egui::TopBottomPanel::top("de_panel")
            .resizable(false)
            .min_height(45.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;

                    // Наш логотип и название окружения
                    ui.heading("🌌 MyDE");

                    ui.separator();

                    // Отображение данных модуля Часов
                    if self.clock_running {
                        ui.label(
                            egui::RichText::new(&self.clock_data)
                                .strong()
                                .color(egui::Color32::LIGHT_BLUE),
                        );
                    } else {
                        ui.label("🕒 Clock: Off");
                    }

                    ui.separator();

                    // Отображение данных модуля системных ресурсов
                    if self.sysinfo_running {
                        ui.label(
                            egui::RichText::new(&self.sysinfo_data)
                                .strong()
                                .color(egui::Color32::LIGHT_GREEN),
                        );
                    } else {
                        ui.label("📊 SysInfo: Off");
                    }

                    // Отрисовка кнопок управления процессами у правого края
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        
                        // Переключатель модуля системной информации
                        let sysinfo_text = if self.sysinfo_running { "Stop SysInfo" } else { "Start SysInfo" };
                        if ui.button(sysinfo_text).clicked() {
                            let action = if self.sysinfo_running { ProcessAction::Stop } else { ProcessAction::Start };
                            let _ = self.ipc_tx.send(IpcMessage::ControlModule {
                                module: ModuleId::SysInfo,
                                action,
                            });
                        }

                        // Переключатель модуля часов
                        let clock_text = if self.clock_running { "Stop Clock" } else { "Start Clock" };
                        if ui.button(clock_text).clicked() {
                            let action = if self.clock_running { ProcessAction::Stop } else { ProcessAction::Start };
                            let _ = self.ipc_tx.send(IpcMessage::ControlModule {
                                module: ModuleId::Clock,
                                action,
                            });
                        }
                    });
                });
            });
    }
}

fn main() -> Result<(), eframe::Error> {
    // Настраиваем eframe так, чтобы окно вело себя как панель: без рамок, фиксированного размера
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 45.0])
            .with_decorations(false) // <--- Убираем рамки (декорации окна)
            .with_resizable(false),  // <--- Запрещаем менять размер
        ..Default::default()
    };

    eframe::run_native(
        "MyDE Panel",
        options,
        Box::new(|cc| Box::new(PanelApp::new(cc))),
    )
}