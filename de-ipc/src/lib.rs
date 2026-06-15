use serde::{Deserialize, Serialize};

/// Спецификация расширения в стиле GNOME (парсится из metadata.json)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExtensionMetadata {
    pub uuid: String,        // Уникальный ID, например "clock@my-de.org"
    pub name: String,        // Имя для UI
    pub description: String, // Описание
    pub exec: String,        // Имя исполняемого файла внутри папки расширения
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ClientType {
    Panel,
    Manager,
    Module(String), // Здесь теперь будет храниться uuid расширения
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum IpcMessage {
    Register { client_type: ClientType },
    PublishUpdate { module: String, data: String },
    ControlModule { module: String, action: ProcessAction },
    ModuleStatus { module: String, is_running: bool },
    QueryStatus,
    /// Возвращаем панели список найденных расширений
    ModulesList { modules: Vec<ExtensionMetadata> }, 
    Refresh,
}