use std::path::PathBuf;
use thiserror::Error;
use de_ipc::{IpcMessage, ProcessAction, ClientType};
use tokio::net::UnixStream;
use tokio_util::codec::{Framed, LinesCodec};
use futures_util::{SinkExt, StreamExt};

/// Ошибки, которые могут возникнуть при работе с SDK.
#[derive(Debug, Error)]
pub enum SdkError {
    #[error("I/O error occurred: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Connection lost with IPC socket")]
    ConnectionLost,

    #[error("Failed to perform registration handshake")]
    HandshakeFailed,
}

/// События, которые модуль может получить от системы (de-manager или панели).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleEvent {
    /// Команда управления жизненным циклом (например, Stop / Restart)
    Action(ProcessAction),
    /// Сигнал от менеджера о необходимости принудительно обновить и прислать данные
    RefreshRequest,
}

/// Дескриптор (Handle) активного соединения, с помощью которого модуль
/// может отправлять обновления своего состояния.
#[derive(Clone)]
pub struct ModuleHandle {
    module_id: String,
    tx: tokio::sync::mpsc::UnboundedSender<IpcMessage>,
}

impl ModuleHandle {
    /// Отправляет новое строковое состояние виджета на панель.
    /// Метод неблокирующий и очень быстрый. Если связь временно разорвана,
    /// сообщения будут буферизоваться в канале до момента переподключения сокета.
    pub fn publish_update(&self, data: impl Into<String>) -> Result<(), SdkError> {
        let message = IpcMessage::PublishUpdate {
            module: self.module_id.clone(),
            data: data.into(),
        };
        
        self.tx.send(message)
            .map_err(|_| SdkError::ConnectionLost)
    }
}

/// Клиент для создания и управления жизненным циклом модуля.
pub struct ModuleClient {
    module_id: String,
    socket_path: PathBuf,
}

impl ModuleClient {
    /// Создает конфигурацию клиента для указанного модуля.
    pub fn new(module_id: impl Into<String>) -> Self {
        let socket_path = std::env::var("DE_IPC_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp/my-de-ipc.sock"));

        Self {
            module_id: module_id.into(),
            socket_path,
        }
    }

    /// Позволяет вручную переопределить путь к Unix-сокету.
    pub fn with_socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.socket_path = path.into();
        self
    }

    /// Быстро инициализирует интерфейсы связи и запускает отказоустойчивую
    /// фоновую задачу автоматического переподключения.
    pub async fn start(
        self,
    ) -> Result<(ModuleHandle, tokio::sync::mpsc::UnboundedReceiver<ModuleEvent>), SdkError> {
        // Создаем каналы связи
        let (tx_outgoing, rx_outgoing) = tokio::sync::mpsc::unbounded_channel::<IpcMessage>();
        let (tx_incoming, rx_incoming) = tokio::sync::mpsc::unbounded_channel::<ModuleEvent>();

        let module_id = self.module_id.clone();
        let socket_path = self.socket_path.clone();

        // Запускаем единую отказоустойчивую задачу в фоне
        tokio::spawn(async move {
            run_reconnecting_loop(module_id, socket_path, rx_outgoing, tx_incoming).await;
        });

        let handle = ModuleHandle {
            module_id: self.module_id,
            tx: tx_outgoing,
        };

        Ok((handle, rx_incoming))
    }
}

/// Внутренний бесконечный цикл автоматического переподключения и обработки ввода-вывода
async fn run_reconnecting_loop(
    module_id: String,
    socket_path: PathBuf,
    mut rx_outgoing: tokio::sync::mpsc::UnboundedReceiver<IpcMessage>,
    tx_incoming: tokio::sync::mpsc::UnboundedSender<ModuleEvent>,
) {
    loop {
        log::info!("[SDK] [{}] Connecting to IPC socket at {:?}", module_id, socket_path);

        let stream = match UnixStream::connect(&socket_path).await {
            Ok(s) => s,
            Err(err) => {
                log::warn!(
                    "[SDK] [{}] Connection failed: {:?}. Retrying in 2 seconds...",
                    module_id,
                    err
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let (mut writer, mut reader) = Framed::new(stream, LinesCodec::new()).split();

        // Проводим рукопожатие (регистрацию)
        let reg_msg = IpcMessage::Register {
            client_type: ClientType::Module(module_id.clone()),
        };

        match serde_json::to_string(&reg_msg) {
            Ok(reg_str) => {
                if let Err(err) = writer.send(reg_str).await {
                    log::error!("[SDK] [{}] Registration handshake failed: {:?}", module_id, err);
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    continue;
                }
            }
            Err(err) => {
                log::error!("[SDK] [{}] Handshake serialization error: {:?}", module_id, err);
                return; // Критическая ошибка конфигурации, завершаем поток
            }
        }

        log::info!("[SDK] [{}] Connected and registered successfully.", module_id);

        // Главный цикл обработки ввода-вывода текущего соединения
        loop {
            tokio::select! {
                // Сценарий А: Получили сообщение из сокета от менеджера/панели
                line_opt = reader.next() => {
                    match line_opt {
                        Some(Ok(line)) => {
                            match serde_json::from_str::<IpcMessage>(&line) {
                                Ok(ipc_msg) => {
                                    match ipc_msg {
                                        IpcMessage::ControlModule { module, action } if module == module_id => {
                                            let event = ModuleEvent::Action(action);
                                            if tx_incoming.send(event).is_err() {
                                                log::info!("[SDK] [{}] Module receiver dropped. Terminating SDK task.", module_id);
                                                return; // Модуль завершил работу, останавливаем SDK
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Err(err) => {
                                    log::warn!(
                                        "[SDK] [{}] Failed to parse incoming message: {:?}. Raw line: {}",
                                        module_id,
                                        err,
                                        line
                                    );
                                }
                            }
                        }
                        Some(Err(err)) => {
                            log::error!("[SDK] [{}] IPC read error: {:?}", module_id, err);
                            break; // Выходим во внешний цикл для переподключения
                        }
                        None => {
                            log::warn!("[SDK] [{}] IPC socket closed by remote host.", module_id);
                            break; // Выходим во внешний цикл для переподключения
                        }
                    }
                }

                // Сценарий Б: Модуль прислал обновление для отправки на панель
                msg_opt = rx_outgoing.recv() => {
                    match msg_opt {
                        Some(msg) => {
                            match serde_json::to_string(&msg) {
                                Ok(serialized_msg) => {
                                    if let Err(err) = writer.send(serialized_msg).await {
                                        log::error!("[SDK] [{}] IPC write error: {:?}", module_id, err);
                                        break; // Выходим во внешний цикл для переподключения
                                    }
                                }
                                Err(err) => {
                                    log::error!("[SDK] [{}] Serialization error for outgoing msg: {:?}", module_id, err);
                                }
                            }
                        }
                        None => {
                            log::info!("[SDK] [{}] All module handles dropped. Terminating SDK task.", module_id);
                            return; // Все отправители удалены, закрываем задачу
                        }
                    }
                }
            }
        }

        log::warn!("[SDK] [{}] Connection broken. Reconnecting in 2 seconds...", module_id);
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}