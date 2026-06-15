use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use de_ipc::{ClientType, IpcMessage, ModuleConfig, ProcessAction};

struct ModuleProcess {
    config: ModuleConfig,
    child: Option<Child>,
    enabled: bool,
}

struct ManagerState {
    socket: UnixStream,
    modules: HashMap<String, ModuleProcess>,
}

/// Функция загрузки конфигурации из JSON-файла
fn load_config(path: &str) -> Result<Vec<ModuleConfig>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let configs = serde_json::from_reader(reader)?;
    Ok(configs)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Manager] Starting de-manager daemon...");

    // 1. Читаем конфигурацию
    let config_path = "de-config.json";
    let configs = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Manager] CRITICAL: Failed to load config from '{}': {:?}", config_path, e);
            return Err(e);
        }
    };
    println!("[Manager] Loaded {} module configuration(s).", configs.len());

    // 2. Подключаемся к сокету
    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;
    println!("[Manager] Connected to central IPC socket.");

    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Manager,
    };
    send_ipc(&mut socket, &reg_msg)?;

    // 3. Инициализируем карту процессов на основе конфига
    let mut modules = HashMap::new();
    for config in configs {
        let id = config.id.clone();
        modules.insert(
            id,
            ModuleProcess {
                config,
                child: None,
                enabled: false,
            },
        );
    }

    let mut state = ManagerState {
        socket,
        modules,
    };

    // 4. Запускаем модули с флагом autostart
    let autostart_ids: Vec<String> = state.modules.iter()
        .filter(|(_, p)| p.config.autostart)
        .map(|(id, _)| id.clone())
        .collect();

    for id in autostart_ids {
        if let Err(e) = state.start_module(id) {
            eprintln!("[Manager] Failed to autostart module: {:?}", e);
        }
    }

    // 5. Поток чтения входящих сообщений
    let (tx, rx): (mpsc::Sender<IpcMessage>, Receiver<IpcMessage>) = mpsc::channel();
    let read_socket = state.socket.try_clone()?;
    thread::spawn(move || {
        let reader = BufReader::new(read_socket);
        for line in reader.lines() {
            match line {
                Ok(content) => {
                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(&content) {
                        if tx.send(msg).is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Manager] Socket read error: {:?}", e);
                    break;
                }
            }
        }
        println!("[Manager] Read thread exited.");
    });

    let mut last_check = Instant::now();
    loop {
        while let Ok(msg) = rx.try_recv() {
            state.handle_message(msg)?;
        }

        if last_check.elapsed() >= Duration::from_secs(1) {
            state.monitor_processes()?;
            last_check = Instant::now();
        }

        thread::sleep(Duration::from_millis(50));
    }
}

impl ManagerState {
    fn handle_message(&mut self, msg: IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            IpcMessage::ControlModule { module, action } => {
                println!("[Manager] Control command received: {:?} -> {}", action, module);
                match action {
                    ProcessAction::Start => {
                        self.start_module(module)?;
                    }
                    ProcessAction::Stop => {
                        self.stop_module(module)?;
                    }
                    ProcessAction::Restart => {
                        // Клонируем модуль, так как stop_module забирает владение строкой,
                        // а нам этот идентификатор нужен следом для start_module
                        self.stop_module(module.clone())?;
                        self.start_module(module)?;
                    }
                }
            }
            IpcMessage::QueryStatus => {
                println!("[Manager] Querying all processes statuses.");
                self.send_all_statuses()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn start_module(&mut self, id: String) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mod_proc) = self.modules.get_mut(&id) {
            mod_proc.enabled = true;
            if mod_proc.child.is_none() {
                println!("[Manager] Spawning process: {} with args: {:?}", mod_proc.config.path, mod_proc.config.args);
                match Command::new(&mod_proc.config.path)
                    .args(&mod_proc.config.args)
                    .spawn() 
                {
                    Ok(child) => {
                        mod_proc.child = Some(child);
                        self.send_status(id, true)?;
                    }
                    Err(e) => {
                        eprintln!("[Manager] CRITICAL: Failed to spawn process {}: {:?}", mod_proc.config.path, e);
                        self.send_status(id, false)?;
                    }
                }
            } else {
                self.send_status(id, true)?;
            }
        }
        Ok(())
    }

    fn stop_module(&mut self, id: String) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mod_proc) = self.modules.get_mut(&id) {
            mod_proc.enabled = false;
            if let Some(mut child) = mod_proc.child.take() {
                println!("[Manager] Stopping process: {}", mod_proc.config.path);
                let _ = child.kill();
                let _ = child.wait();
            }
            self.send_status(id, false)?;
        }
        Ok(())
    }

    fn monitor_processes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut statuses_to_send = Vec::new();

        for (id, mod_proc) in &mut self.modules {
            if let Some(ref mut child) = mod_proc.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        println!("[Manager] Process {} exited unexpectedly with code: {:?}", mod_proc.config.path, status);
                        mod_proc.child = None;
                        
                        if mod_proc.enabled {
                            println!("[Manager] AUTO-RESTART: Spawning dead process: {}", mod_proc.config.path);
                            match Command::new(&mod_proc.config.path)
                                .args(&mod_proc.config.args)
                                .spawn() 
                            {
                                Ok(new_child) => {
                                    mod_proc.child = Some(new_child);
                                    statuses_to_send.push((id.clone(), true));
                                }
                                Err(e) => {
                                    eprintln!("[Manager] Auto-restart failed for {}: {:?}", mod_proc.config.path, e);
                                    statuses_to_send.push((id.clone(), false));
                                }
                            }
                        } else {
                            statuses_to_send.push((id.clone(), false));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("[Manager] Error polling status for {}: {:?}", mod_proc.config.path, e);
                    }
                }
            }
        }

        for (id, is_running) in statuses_to_send {
            self.send_status(id, is_running)?;
        }

        Ok(())
    }

    fn send_status(&mut self, id: String, is_running: bool) -> Result<(), Box<dyn std::error::Error>> {
        let msg = IpcMessage::ModuleStatus {
            module: id,
            is_running,
        };
        send_ipc(&mut self.socket, &msg)
    }

    fn send_all_statuses(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Сначала собираем и отправляем список всех конфигураций, чтобы панель знала их UI-имена
        let mut configs = Vec::new();
        for mod_proc in self.modules.values() {
            configs.push(mod_proc.config.clone());
        }
        let list_msg = IpcMessage::ModulesList { modules: configs };
        send_ipc(&mut self.socket, &list_msg)?;

        // 2. Затем собираем и шлем актуальный статус запущенности
        let mut statuses = Vec::new();
        for (id, mod_proc) in &self.modules {
            statuses.push((id.clone(), mod_proc.child.is_some()));
        }

        for (id, is_running) in statuses {
            self.send_status(id, is_running)?;
        }
        Ok(())
    }
}

fn send_ipc(socket: &mut UnixStream, msg: &IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
    let mut serialized = serde_json::to_string(msg)?;
    serialized.push('\n');
    socket.write_all(serialized.as_bytes())?;
    socket.flush()?;
    Ok(())
}