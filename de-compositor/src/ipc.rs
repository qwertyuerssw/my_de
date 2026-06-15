use std::os::unix::net::UnixListener;
use std::io::{BufRead, BufReader, Write};
use smithay::reexports::calloop::{generic::Generic, Interest, Mode};
use crate::state::{Smallvil, ClientSession};
use de_ipc::{IpcMessage, ClientType};

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
                    let _ = stream.set_nonblocking(true);
                    let client_id = state.next_client_id;
                    state.next_client_id += 1;

                    tracing::info!("New IPC client connected. Assigned ID: {}", client_id);

                    let read_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to clone stream for client {}: {:?}", client_id, e);
                            continue;
                        }
                    };

                    let session = ClientSession {
                        client_type: None,
                        reader: BufReader::new(read_stream),
                        writer: stream,
                    };
                    state.ipc_clients.insert(client_id, session);

                    let raw_read_stream = match state.ipc_clients.get(&client_id).unwrap().reader.get_ref().try_clone() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to clone raw reader stream: {:?}", e);
                            state.ipc_clients.remove(&client_id);
                            continue;
                        }
                    };

                    let reader_source = Generic::new(raw_read_stream, Interest::READ, Mode::Level);
                    
                    if let Err(e) = state.loop_handle.insert_source(reader_source, move |_, _, state| {
                        let mut messages = Vec::new(); // Буфер накопления сообщений
                        let mut should_remove = false;

                        // Блок 1: Быстро читаем и парсим все сообщения, пока session заимствован
                        {
                            if let Some(session) = state.ipc_clients.get_mut(&client_id) {
                                loop {
                                    let mut line = String::new();
                                    match session.reader.read_line(&mut line) {
                                        Ok(0) => {
                                            tracing::info!("IPC client {} disconnected (EOF).", client_id);
                                            should_remove = true;
                                            break;
                                        }
                                        Ok(_) => {
                                            let trimmed = line.trim();
                                            if !trimmed.is_empty() {
                                                if let Ok(msg) = serde_json::from_str::<IpcMessage>(trimmed) {
                                                    messages.push(msg); // Сохраняем сообщение во временный вектор
                                                } else {
                                                    tracing::warn!("Received invalid JSON from client {}: {}", client_id, trimmed);
                                                }
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
                            }
                        } // Ссылка session уничтожена. state.ipc_clients свободен для заимствований!

                        // Блок 2: Теперь безопасно обрабатываем каждое сообщение
                        for msg in messages {
                            state.handle_ipc_message(client_id, msg);
                        }

                        // Блок 3: Удаляем сессию, если клиент отключился
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
                // Сначала выводим лог, пока мы владеем переменной client_type
                tracing::info!("Client {} successfully registered as {:?}", client_id, client_type);
                
                // Теперь перемещаем право владения в session
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
            // Пересылаем полученный от de-manager список конфигураций на панель
            IpcMessage::ModulesList { modules } => {
                self.broadcast_message(IpcMessage::ModulesList { modules });
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