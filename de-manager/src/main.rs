use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use de_ipc::{ClientType, ExtensionMetadata, IpcMessage, ProcessAction};

struct ExtensionProcess {
    metadata: ExtensionMetadata,
    dir_path: PathBuf, // Папка, где лежит расширение
    child: Option<Child>,
    enabled: bool,
}

struct ManagerState {
    socket: UnixStream,
    extensions: HashMap<String, ExtensionProcess>,
}

impl Drop for ManagerState {
    fn drop(&mut self) {
        println!("[Manager] Shutting down. Cleaning up child processes...");
        for (uuid, ext) in &mut self.extensions {
            if let Some(mut child) = ext.child.take() {
                println!("[Manager] Terminating process: {}", uuid);
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// Функция сканирования папки расширений
fn scan_extensions() -> HashMap<String, ExtensionProcess> {
    let mut extensions = HashMap::new();
    
    // Определяем путь ~/.local/share/my-de/extensions/
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    let ext_dir = PathBuf::from(home).join(".local/share/my-de/extensions");

    // Если папки нет, создаем её
    if !ext_dir.exists() {
        if let Err(e) = fs::create_dir_all(&ext_dir) {
            eprintln!("[Manager] Warning: failed to create extensions dir: {:?}", e);
            return extensions;
        }
    }

    println!("[Manager] Scanning extensions in: {}", ext_dir.display());

    if let Ok(entries) = fs::read_dir(ext_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                if metadata_path.exists() {
                    if let Ok(file) = fs::File::open(&metadata_path) {
                        let reader = BufReader::new(file);
                        if let Ok(metadata) = serde_json::from_reader::<_, ExtensionMetadata>(reader) {
                            println!("[Manager] Found extension: {} ({})", metadata.name, metadata.uuid);
                            let uuid = metadata.uuid.clone();
                            extensions.insert(
                                uuid,
                                ExtensionProcess {
                                    metadata,
                                    dir_path: path,
                                    child: None,
                                    enabled: false,
                                }
                            );
                        } else {
                            eprintln!("[Manager] Failed to parse metadata in {:?}", metadata_path);
                        }
                    }
                }
            }
        }
    }

    extensions
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Manager] Starting de-manager daemon (GNOME-style extensions)...");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;
    println!("[Manager] Connected to central IPC socket.");

    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Manager,
    };
    send_ipc(&mut socket, &reg_msg)?;

    let extensions = scan_extensions();
    let mut state = ManagerState { socket, extensions };

    let (tx, rx) = mpsc::channel::<IpcMessage>();
    let read_socket = state.socket.try_clone()?;
    
    thread::spawn(move || {
        let reader = BufReader::new(read_socket);
        for line in reader.lines() {
            if let Ok(content) = line {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&content) {
                    let _ = tx.send(msg);
                }
            }
        }
    });

    let mut last_check = Instant::now();
    
    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => {
                if let Err(e) = state.handle_message(msg) {
                    eprintln!("[Manager] Error handling message: {:?}", e);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if last_check.elapsed() >= Duration::from_secs(1) {
            let _ = state.monitor_processes();
            last_check = Instant::now();
        }
    }

    Ok(())
}

impl ManagerState {
    fn handle_message(&mut self, msg: IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            IpcMessage::ControlModule { module, action } => {
                match action {
                    ProcessAction::Start => self.start_module(module)?,
                    ProcessAction::Stop => self.stop_module(module)?,
                    ProcessAction::Restart => {
                        self.stop_module(module.clone())?;
                        self.start_module(module)?;
                    }
                }
            }
            IpcMessage::QueryStatus => {
                self.send_all_statuses()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn start_module(&mut self, uuid: String) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ext) = self.extensions.get_mut(&uuid) {
            ext.enabled = true;
            if ext.child.is_none() {
                // Исполняемый файл лежит прямо в папке расширения
                let exec_path = ext.dir_path.join(&ext.metadata.exec);
                
                match Command::new(&exec_path)
                    .current_dir(&ext.dir_path) // Запускаем процесс прямо в его директории
                    .spawn()
                {
                    Ok(child) => {
                        ext.child = Some(child);
                        self.send_status(uuid, true)?;
                    }
                    Err(e) => {
                        eprintln!("[Manager] Failed to spawn {}: {:?}", exec_path.display(), e);
                        self.send_status(uuid, false)?;
                    }
                }
            } else {
                self.send_status(uuid, true)?;
            }
        }
        Ok(())
    }

    fn stop_module(&mut self, uuid: String) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ext) = self.extensions.get_mut(&uuid) {
            ext.enabled = false;
            if let Some(mut child) = ext.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.send_status(uuid, false)?;
        }
        Ok(())
    }

    fn monitor_processes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut statuses_to_send = Vec::new();

        for (uuid, ext) in &mut self.extensions {
            if let Some(ref mut child) = ext.child {
                match child.try_wait() {
                    Ok(Some(_status)) => {
                        ext.child = None;
                        if ext.enabled {
                            let exec_path = ext.dir_path.join(&ext.metadata.exec);
                            match Command::new(&exec_path).current_dir(&ext.dir_path).spawn() {
                                Ok(new_child) => {
                                    ext.child = Some(new_child);
                                    statuses_to_send.push((uuid.clone(), true));
                                }
                                Err(_) => {
                                    statuses_to_send.push((uuid.clone(), false));
                                }
                            }
                        } else {
                            statuses_to_send.push((uuid.clone(), false));
                        }
                    }
                    _ => {}
                }
            }
        }

        for (uuid, is_running) in statuses_to_send {
            self.send_status(uuid, is_running)?;
        }

        Ok(())
    }

    fn send_status(&mut self, uuid: String, is_running: bool) -> Result<(), Box<dyn std::error::Error>> {
        let msg = IpcMessage::ModuleStatus {
            module: uuid,
            is_running,
        };
        send_ipc(&mut self.socket, &msg)
    }

    fn send_all_statuses(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut metas = Vec::new();
        for ext in self.extensions.values() {
            metas.push(ext.metadata.clone());
        }
        let list_msg = IpcMessage::ModulesList { modules: metas };
        send_ipc(&mut self.socket, &list_msg)?;

        let mut statuses = Vec::new();
        for (uuid, ext) in &self.extensions {
            statuses.push((uuid.clone(), ext.child.is_some()));
        }

        for (uuid, is_running) in statuses {
            self.send_status(uuid, is_running)?;
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