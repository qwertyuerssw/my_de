use serde::{Deserialize, Serialize};

/// Уникальные идентификаторы наших изолированных модулей
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleId {
    Clock,
    SysInfo,
    Volume,
}

/// Команды управления жизненным циклом процессов, которые панель шлет менеджеру
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ProcessAction {
    Start,
    Stop,
    Restart,
}

/// Типы клиентов, подключающихся к нашему Unix-сокету
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ClientType {
    Panel,      // Графическая панель управления
    Manager,    // Демон-супервизор процессов (de-manager)
    Module(ModuleId), // Конкретный модуль-источник данных
}

/// Главный протокол обмена сообщениями
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum IpcMessage {
    /// 1. Регистрация: Новый клиент сообщает сокету, кто он такой
    Register { client_type: ClientType },

    /// 2. Публикация данных: Модуль (например, Clock) отправляет свежие данные композитору
    PublishUpdate { module: ModuleId, data: String },

    /// 3. Управление: Панель просит запустить/остановить определенный процесс
    ControlModule { module: ModuleId, action: ProcessAction },

    /// 4. Изменение статуса: Менеджер уведомляет панель, запущен ли сейчас процесс
    ModuleStatus { module: ModuleId, is_running: bool },

    /// 5. Запрос статуса: Панель при старте запрашивает у менеджера текущее состояние всех процессов
    QueryStatus,
}