
# My DE (Modular Desktop Environment)

[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Linux-blue.svg)](https://www.kernel.org/)
[![Architecture](https://img.shields.io/badge/architecture-Modular%20%2F%20IPC-green.svg)](#архитектура-проекта-и-назначение-компонентов)

Легковесное модульное графическое окружение (Desktop Environment), разработанное на языке программирования Rust. 

Архитектура системы построена по принципу разделения ответственности (Separation of Concerns): ключевые компоненты системы функционируют как изолированные процессы и взаимодействуют друг с другом через легковесную шину межпроцессного взаимодействия (IPC) на базе Unix Domain Sockets.

---

## Архитектура проекта и назначение компонентов

Проект организован в виде единого Cargo-воркспейса. Ниже приведено описание основных компонентов системы и их зон ответственности:

### 1. `de-compositor` (Графический сервер)
Ядро графического окружения, выступающее в роли дисплейного сервера. Отвечает за:
* Отрисовку и композицию оконных поверхностей.
* Прямое взаимодействие с устройствами ввода (клавиатура, мышь, тачпад).
* Вывод графической информации на дисплеи.
* Менеджмент базовых состояний окон.

### 2. `de-ipc` (Протокол взаимодействия)
Общая библиотека, описывающая строго типизированный протокол обмена сообщениями между процессами окружения. Предоставляет:
* Модели системных событий (изменение фокуса, переключение рабочих столов).
* Команды управления (запуск и закрытие приложений).
* Механизмы синхронизации конфигурации в реальном времени.

### 3. `de-manager` (Диспетчер сессии)
Координирующий компонент окружения. Выполняет функции системного монитора и управляющего центра:
* Считывает конфигурацию из `de-config.json`.
* Управляет жизненным циклом зависимых процессов (запуск, мониторинг состояния, обработка завершения работы).
* Контролирует глобальные правила поведения рабочих пространств.

### 4. `de-panel` (Статус-бар)
Графический пользовательский интерфейс для отображения системной информации. Выводится на краю экрана и визуализирует:
* Активные рабочие пространства (Workspaces).
* Список запущенных / закрепленных приложений.
* Виджеты системных модулей (часы, индикаторы загрузки, сеть, уровень заряда).
* Данные, динамически получаемые от модулей через шину `de-ipc`.

### 5. `de-sdk` (Инструментарий разработчика)
Библиотека-SDK для быстрого создания новых модулей. Абстрагирует низкоуровневую асинхронную работу с сокетами, автоматически обрабатывает переподключение к шине при сбоях и предоставляет готовые каналы для приема системных событий от панели или менеджера.

### 6. `modules/` (Расширения системы)
Каталог вспомогательных независимых микропрограмм (модулей). Каждый модуль решает строго одну задачу (например, опрос системного времени) и транслирует данные через `de-sdk` в шину IPC для последующего отображения на панели.

---

## Схема взаимодействия компонентов

                        +------------------+
                        |    run-de.sh     |  <-- Скрипт инициализации окружения
                        +--------+---------+
                                 |
                                 v
                        +------------------+
                        |    de-manager    |  <-- Читает конфигурацию de-config.json
                        +--------+---------+
                                 |
               +-----------------+-----------------+
               | (IPC)           | (IPC)           | (IPC)
               v                 v                 v
    
    +------------------+ +-----------------+ +-----------------+ 
    | de-compositor   | |  de-panel       | | modules/mod-*   |
    | (Отрисовка/Ввод)| | (Статус-бар/UI) | | (Clock, SysInfo)| 
    +------------------+ +-----------------+ +-----------------+


---

## Сборка и запуск

### Системные требования
* **Rust Toolchain** (cargo, rustc версии 1.70 или выше).
* **Системные графические библиотеки** (в зависимости от бэкенда рендеринга: `wayland-client`, `wayland-server`, `xkbcommon`, `libinput` или `x11`).

### Сборка проекта
Для компиляции всех компонентов в режиме оптимизации выполните команду в корневой директории воркспейса:

```bash
cargo build --release

Инициализация окружения

Запуск всей экосистемы (композитора, диспетчера сессии, панели и системных
виджетов) производится через стартовый сценарий:

./run-de.sh

Инструкция по интеграции нового модуля (на примере модуля звука)

Процесс создания и интеграции нового модуля состоит из 5 последовательных шагов.
Благодаря использованию de-sdk, разработка стала декларативной и безопасной.

Шаг 1: Расширение протокола в de-ipc

Добавьте новый идентификатор модуля в перечисление типов в файле
de-ipc/src/lib.rs:

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleId {
    Clock,
    SysInfo,
    Volume, // <-- Новый идентификатор для модуля громкости звука
}

Шаг 2: Создание крейта модуля и настройка Cargo Workspace

1.  Зарегистрируйте новый модуль в корневом файле Cargo.toml:

    [workspace]
    resolver = "2"
    members = [
        "de-compositor",
        "de-ipc",
        "de-manager",
        "de-panel",
        "de-sdk",
        "modules/mod-clock",
        "modules/mod-sysinfo",
        "modules/mod-volume"  # <-- Добавьте путь к новому крейту
    ]

2.  Создайте структуру каталогов для нового крейта:

    mkdir -p modules/mod-volume/src

3.  Опишите манифест зависимостей в modules/mod-volume/Cargo.toml. Нам
    понадобится только de-sdk для работы и асинхронный рантайм tokio:

    [package]
    name = "mod-volume"
    version = "0.1.0"
    edition = "2021"

    [dependencies]
    de-ipc = { path = "../../de-ipc" }
    de-sdk = { path = "../../de-sdk" }
    tokio = { version = "1", features = ["full"] }

4.  Реализуйте логику модуля в modules/mod-volume/src/main.rs. Работа с сокетами
    и переподключением теперь полностью скрыта в SDK:

    use de_sdk::{ModuleClient, ModuleEvent};
    use de_ipc::ProcessAction;
    use std::time::Duration;

    #[tokio::main]
    async fn main() -> Result<(), Box<dyn std::error::Error>> {
        println!("[Volume] Starting volume module...");

        // Инициализация и запуск отказоустойчивого клиента из SDK
        let client = ModuleClient::new("volume");
        let (handle, mut rx_events) = client.start().await?;

        // Фоновая задача обновления состояния
        let handle_clone = handle.clone();
        tokio::spawn(async move {
            let mut mock_volume = 50; // Симуляция значения громкости
            loop {
                let formatted_data = format!("🔊 Vol: {}%", mock_volume);

                if let Err(e) = handle_clone.publish_update(formatted_data) {
                    eprintln!("[Volume] Failed to send update: {:?}", e);
                }

                mock_volume = (mock_volume + 5) % 100;
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });

        // Реактивный цикл обработки системных событий от de-manager/панели
        while let Some(event) = rx_events.recv().await {
            match event {
                ModuleEvent::Action(ProcessAction::Stop) => {
                    println!("[Volume] Received Stop command. Exiting gracefully...");
                    break; // Мягко завершаем программу
                }
                ModuleEvent::Action(ProcessAction::Restart) => {
                    println!("[Volume] Received Restart command. Reinitializing...");
                }
                ModuleEvent::RefreshRequest => {
                    println!("[Volume] Refresh requested by composer/panel.");
                }
                _ => {}
            }
        }

        println!("[Volume] Stopped.");
        Ok(())
    }

Шаг 3: Регистрация модуля в de-manager и файле конфигурации

1.  В файле de-manager/src/main.rs в функции инициализации системного состояния
    добавьте процесс для нового модуля:

    state.modules.insert(
        ModuleId::Volume,
        ModuleProcess {
            id: ModuleId::Volume,
            path: "./target/debug/mod-volume",
            child: None,
            enabled: false,
        },
    );

2.  Внесите изменения в конфигурационный файл de-config.json в корне проекта:

    {
      "modules": [
        { "id": "Clock", "path": "./target/debug/mod-clock" },
        { "id": "SysInfo", "path": "./target/debug/mod-sysinfo" },
        { "id": "Volume", "path": "./target/debug/mod-volume" }
      ]
    }

Шаг 4: Обновление интерфейса панели (de-panel)

Обучите панель принимать и отображать новые данные. В файле обработки событий
de-panel добавьте реакцию на сообщения от "volume":

// Пример обработки входящих IPC-сообщений в цикле рендеринга панели
match msg {
    IpcMessage::PublishUpdate { module, data } => {
        match module.as_str() {
            "volume" => {
                // Обновление состояния виджета громкости в UI панели
                panel_state.volume_text = data;
            }
            // Другие модули...
            _ => {}
        }
    }
    _ => {}
}

Шаг 5: Пересборка и тестирование

Запустите полную сборку проекта и проверьте интеграцию:

cargo build && ./run-de.sh

