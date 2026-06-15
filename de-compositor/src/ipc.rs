use crate::state::{ClientSession, Smallvil};
use de_ipc::{ClientType, IpcMessage};
use smithay::reexports::calloop::{generic::Generic, Interest, Mode};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;

pub fn init_ipc(state: &mut Smallvil) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = "/tmp/my-de-ipc.sock";

    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    listener.set_nonblocking(true)?;

    let listener_source = Generic::new(listener, Interest::READ, Mode::Level);

    state.loop_handle.insert_source(listener_source, |_, listener, state| {
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    // Пишущую сторону оставляем в блокирующем режиме для безопасного write_all
                    let _ = stream.set_nonblocking(false);
                    let client_id = state.next_client_id;
                    state.next_client_id += 1;

                    tracing::info!("New IPC client connected. Assigned ID: {}", client_id);

                    // Клонируем поток для асинхронного чтения (убрали mut)
                    let read_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to clone stream for client {}: {:?}", client_id, e);
                            continue;
                        }
                    };

                    // Читающий поток переводим в неблокирующий режим для calloop
                    let _ = read_stream.set_nonblocking(true);

                    let session = ClientSession {
                        client_type: None,
                        read_buffer: String::new(),
                        writer: stream,
                    };
                    state.ipc_clients.insert(client_id, session);

                    let reader_source = Generic::new(read_stream, Interest::READ, Mode::Level);
                    
                    if let Err(e) = state.loop_handle.insert_source(reader_source, move |_, read_stream, state| {
                        let mut messages = Vec::new();
                        let mut should_remove = false;

                        // Блок 1: Читаем сырые байты до тех пор, пока сокет не вернет WouldBlock
                        if let Some(session) = state.ipc_clients.get_mut(&client_id) {
                            let mut buf = [0u8; 1024];
                            
                            // 1. Получаем разделяемую ссылку &UnixStream
                            // 2. Кладем её в мутабельную локальную переменную, чтобы удовлетворить сигнатуру `&mut &UnixStream`
                            let mut reader = &**read_stream;
                            
                            loop {
                                match reader.read(&mut buf) {
                                    Ok(0) => {
                                        tracing::info!("IPC client {} disconnected (EOF).", client_id);
                                        should_remove = true;
                                        break;
                                    }
                                    Ok(n) => {
                                        if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                                            session.read_buffer.push_str(s);
                                        } else {
                                            tracing::warn!("Non-UTF8 bytes received from client {}", client_id);
                                        }
                                    }
                                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                        break;
                                    }
                                    Err(e) => {
                                        tracing::error!("Error reading from client {}: {:?}", client_id, e);
                                        should_remove = true;
                                        break;
                                    }
                                }
                            }

                            // Вытаскиваем все законченные строки (разделенные \n) из буфера
                            while let Some(pos) = session.read_buffer.find('\n') {
                                let line = session.read_buffer.drain(..=pos).collect::<String>();
                                let trimmed = line.trim();
                                if !trimmed.is_empty() {
                                    if let Ok(msg) = serde_json::from_str::<IpcMessage>(trimmed) {
                                        messages.push(msg);
                                    } else {
                                        tracing::warn!("Received invalid JSON from client {}: {}", client_id, trimmed);
                                    }
                                }
                            }
                        }
                        // Блок 2: Безопасно обрабатываем каждое собранное сообщение
                        for msg in messages {
                            state.handle_ipc_message(client_id, msg);
                        }

                        // Блок 3: Удаление сессии в случае отключения клиента
                        if should_remove {
                            state.ipc_clients.remove(&client_id);
                            Ok(smithay::reexports::calloop::PostAction::Remove)
                        } else {
                            Ok(smithay::reexports::calloop::PostAction::Continue)
                        }
                    }) {
                        tracing::error!("Failed to register client stream in event loop: {:?}", e);
                        state.ipc_clients.remove(&client_id);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => {
                    tracing::error!("Error accepting IPC client: {:?}", e);
                    break;
                }
            }
        }
        Ok(smithay::reexports::calloop::PostAction::Continue)
    })?;

    tracing::info!("IPC server bound to {}", socket_path);
    Ok(())
}

impl Smallvil {
    pub fn handle_ipc_message(&mut self, client_id: u32, msg: IpcMessage) {
        tracing::info!("Received IPC message from client {}: {:?}", client_id, msg);
        match msg {
            IpcMessage::Register { client_type } => {
                tracing::info!(
                    "Client {} successfully registered as {:?}",
                    client_id,
                    client_type
                );

                if let Some(session) = self.ipc_clients.get_mut(&client_id) {
                    session.client_type = Some(client_type);
                }
            }
            IpcMessage::PublishUpdate { module, data } => {
                self.broadcast_message(IpcMessage::PublishUpdate { module, data });
            }
            IpcMessage::ControlModule { module, action } => {
                self.send_to_manager(IpcMessage::ControlModule { module, action });
            }
            IpcMessage::ModuleStatus { module, is_running } => {
                self.broadcast_message(IpcMessage::ModuleStatus { module, is_running });
            }
            IpcMessage::QueryStatus => {
                self.send_to_manager(IpcMessage::QueryStatus);
            }
            IpcMessage::ModulesList { modules } => {
                self.broadcast_message(IpcMessage::ModulesList { modules });
            }
            IpcMessage::Refresh => {
                let serialized = match serde_json::to_string(&IpcMessage::Refresh) {
                    Ok(s) => s + "\n",
                    Err(e) => {
                        tracing::error!("Failed to serialize Refresh message: {:?}", e);
                        return;
                    }
                };

                for (id, session) in &mut self.ipc_clients {
                    if let Some(ClientType::Module(_)) = &session.client_type {
                        if let Err(e) = session.writer.write_all(serialized.as_bytes()) {
                            tracing::error!("Failed to send Refresh to module client {}: {:?}", id, e);
                        } else {
                            let _ = session.writer.flush();
                        }
                    }
                }
            }
        }
    }

    pub fn broadcast_message(&mut self, msg: IpcMessage) {
        let serialized = match serde_json::to_string(&msg) {
            Ok(s) => s + "\n",
            Err(e) => {
                tracing::error!("Failed to serialize message: {:?}", e);
                return;
            }
        };

        for (client_id, session) in &mut self.ipc_clients {
            if let Some(ClientType::Panel) = session.client_type {
                if let Err(e) = session.writer.write_all(serialized.as_bytes()) {
                    tracing::error!("Failed to send to panel client {}: {:?}", client_id, e);
                } else {
                    let _ = session.writer.flush();
                }
            }
        }
    }

    pub fn send_to_manager(&mut self, msg: IpcMessage) {
        let serialized = match serde_json::to_string(&msg) {
            Ok(s) => s + "\n",
            Err(e) => {
                tracing::error!("Failed to serialize message: {:?}", e);
                return;
            }
        };

        for (client_id, session) in &mut self.ipc_clients {
            if let Some(ClientType::Manager) = session.client_type {
                if let Err(e) = session.writer.write_all(serialized.as_bytes()) {
                    tracing::error!("Failed to send to de-manager client {}: {:?}", client_id, e);
                } else {
                    let _ = session.writer.flush();
                }
                return;
            }
        }
        tracing::warn!("Message dropped: de-manager is not connected to IPC yet!");
    }
}