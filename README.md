
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

### 5. `modules/` (Расширения системы)
Каталог вспомогательных независимых микропрограмм (модулей). Каждый модуль решает строго одну задачу (например, опрос системного времени или сбор статистики CPU) и транслирует данные в шину IPC для последующего отображения на панели.

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

Процесс расширения возможностей панели состоит из 5 последовательных шагов.

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
        "modules/mod-clock",
        "modules/mod-sysinfo",
        "modules/mod-volume"  # <-- Добавьте путь к новому крейту
    ]

2.  Создайте структуру каталогов для нового крейта:

    mkdir -p modules/mod-volume/src

3.  Опишите манифест зависимости в modules/mod-volume/Cargo.toml:

    [package]
    name = "mod-volume"
    version = "0.1.0"
    edition = "2021"

    [dependencies]
    de-ipc = { path = "../../de-ipc" }
    serde = { version = "1.0", features = ["derive"] }
    serde_json = "1.0"

4.  Реализуйте логику модуля в modules/mod-volume/src/main.rs:

    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::Duration;
    use de_ipc::{IpcMessage, ClientType, ModuleId};

    fn main() -> Result<(), Box<dyn std::error::Error>> {
        let socket_path = "/tmp/my-de-ipc.sock";
        let mut socket = UnixStream::connect(socket_path)?;

        // Регистрация в IPC-сервере в качестве модуля громкости
        let reg_msg = IpcMessage::Register {
            client_type: ClientType::Module(ModuleId::Volume),
        };
        send_ipc(&mut socket, &reg_msg)?;

        let mut mock_volume = 50; // Симуляция значения громкости

        loop {
            let formatted_data = format!("🔊 Vol: {}%", mock_volume);

            let update_msg = IpcMessage::PublishUpdate {
                module: ModuleId::Volume,
                data: formatted_data,
            };
            send_ipc(&mut socket, &update_msg)?;

            mock_volume = (mock_volume + 5) % 105;
            thread::sleep(Duration::from_secs(3));
        }
    }

    fn send_ipc(socket: &mut UnixStream, msg: &IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
        let mut serialized = serde_json::to_string(msg)?;
        serialized.push('\n');
        socket.write_all(serialized.as_bytes())?;
        socket.flush()?;
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
de-panel добавьте реакцию на сообщения от ModuleId::Volume:

// Пример обработки входящих IPC-сообщений в цикле рендеринга панели
match msg {
    IpcMessage::PublishUpdate { module, data } => {
        match module {
            ModuleId::Volume => {
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

