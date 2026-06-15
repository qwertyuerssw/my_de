use serde::{Deserialize, Serialize};

/// Структура конфигурации отдельного модуля.
/// Будет считываться de-manager и передаваться панели для отрисовки интерфейса.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ModuleConfig {
    pub id: String,         // Уникальный строковый ID (например, "clock")
    pub name: String,       // Имя для отображения в UI панели (например, "🕒 Clock")
    pub path: String,       // Путь к исполняемому файлу
    pub args: Vec<String>,  // Аргументы запуска процесса
    pub autostart: bool,    // Запускать ли модуль автоматически на старте менеджера
}

/// Команды управления жизненным циклом процессов, которые панель отправляет менеджеру
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    Start,
    Stop,
    Restart,
}

/// Типы клиентов, подключающихся к нашему Unix-сокету.
/// Идентификатор модуля изменен с enum на String.
/// Из-за String внутри Module мы больше не деривируем Copy.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ClientType {
    Panel,            // Графическая панель управления
    Manager,          // Демон-супервизор процессов (de-manager)
    Module(String),   // Конкретный модуль-источник данных (например, Module("clock".to_string()))
}

/// Главный протокол обмена сообщениями
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum IpcMessage {
    /// 1. Регистрация: Новый клиент сообщает сокету, кто он такой
    Register { client_type: ClientType },

    /// 2. Публикация данных: Модуль отправляет свежие данные композитору
    PublishUpdate { module: String, data: String },

    /// 3. Управление: Панель просит запустить/остановить определенный процесс
    ControlModule { module: String, action: ProcessAction },

    /// 4. Изменение статуса: Менеджер уведомляет панель, запущен ли сейчас процесс
    ModuleStatus { module: String, is_running: bool },

    /// 5. Запрос статуса: Панель при старте запрашивает у менеджера текущее состояние всех процессов
    QueryStatus,

    /// 6. Ответ на запрос списка модулей: Менеджер присылает панели список конфигураций
    ModulesList { modules: Vec<ModuleConfig> },
}