use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use de_ipc::{ClientType, IpcMessage, ModuleId, ProcessAction};

struct ModuleProcess {
    id: ModuleId,
    path: &'static str,
    child: Option<Child>,
    enabled: bool,
}

struct ManagerState {
    socket: UnixStream,
    modules: HashMap<ModuleId, ModuleProcess>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Manager] Starting de-manager daemon...");

    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;
    println!("[Manager] Connected to central IPC socket.");

    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Manager,
    };
    send_ipc(&mut socket, &reg_msg)?;

    let mut state = ManagerState {
        socket,
        modules: HashMap::new(),
    };

    state.modules.insert(
        ModuleId::Clock,
        ModuleProcess {
            id: ModuleId::Clock,
            path: "./target/debug/mod-clock",
            child: None,
            enabled: false,
        },
    );

    state.modules.insert(
        ModuleId::SysInfo,
        ModuleProcess {
            id: ModuleId::SysInfo,
            path: "./target/debug/mod-sysinfo",
            child: None,
            enabled: false,
        },
    );

    state.modules.insert(
    ModuleId::Volume,
    ModuleProcess {
        id: ModuleId::Volume,
        path: "./target/debug/mod-volume",
        child: None,
        enabled: false,
    },
);

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
                println!("[Manager] Control command received: {:?} -> {:?}", action, module);
                match action {
                    ProcessAction::Start => {
                        self.start_module(module)?;
                    }
                    ProcessAction::Stop => {
                        self.stop_module(module)?;
                    }
                    ProcessAction::Restart => {
                        self.stop_module(module)?;
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

    fn start_module(&mut self, id: ModuleId) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mod_proc) = self.modules.get_mut(&id) { // Убрали лишний mut в распаковке
            mod_proc.enabled = true;
            if mod_proc.child.is_none() {
                println!("[Manager] Spawning process: {}", mod_proc.path);
                match Command::new(mod_proc.path).spawn() {
                    Ok(child) => {
                        mod_proc.child = Some(child);
                        self.send_status(id, true)?;
                    }
                    Err(e) => {
                        eprintln!("[Manager] CRITICAL: Failed to spawn process {}: {:?}", mod_proc.path, e);
                        self.send_status(id, false)?;
                    }
                }
            } else {
                self.send_status(id, true)?;
            }
        }
        Ok(())
    }

    fn stop_module(&mut self, id: ModuleId) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mod_proc) = self.modules.get_mut(&id) { // Убрали лишний mut в распаковке
            mod_proc.enabled = false;
            if let Some(mut child) = mod_proc.child.take() {
                println!("[Manager] Stopping process: {}", mod_proc.path);
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
                        println!("[Manager] Process {} exited unexpectedly with code: {:?}", mod_proc.path, status);
                        mod_proc.child = None;
                        
                        if mod_proc.enabled {
                            println!("[Manager] AUTO-RESTART: Spawning dead process: {}", mod_proc.path);
                            match Command::new(mod_proc.path).spawn() {
                                Ok(new_child) => {
                                    mod_proc.child = Some(new_child);
                                    statuses_to_send.push((*id, true));
                                }
                                Err(e) => {
                                    eprintln!("[Manager] Auto-restart failed for {}: {:?}", mod_proc.path, e);
                                    statuses_to_send.push((*id, false));
                                }
                            }
                        } else {
                            statuses_to_send.push((*id, false));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("[Manager] Error polling status for {}: {:?}", mod_proc.path, e);
                    }
                }
            }
        }

        for (id, is_running) in statuses_to_send {
            self.send_status(id, is_running)?;
        }

        Ok(())
    }

    fn send_status(&mut self, id: ModuleId, is_running: bool) -> Result<(), Box<dyn std::error::Error>> {
        let msg = IpcMessage::ModuleStatus {
            module: id,
            is_running,
        };
        send_ipc(&mut self.socket, &msg)
    }

    /// Исправленный метод: собираем статусы без удержания долгой ссылки на self.modules
    fn send_all_statuses(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut statuses = Vec::new();
        for (id, mod_proc) in &self.modules {
            statuses.push((*id, mod_proc.child.is_some()));
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