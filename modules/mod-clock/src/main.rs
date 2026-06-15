use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use chrono::Local;
use de_ipc::{IpcMessage, ClientType}; // Убрали импорт ModuleId

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Clock] Starting clock module...");
    
    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;
    println!("[Clock] Connected to IPC socket.");

    // 1. Регистрируемся в композиторе как модуль часов (передаем строковый ID)
    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Module("clock".to_string()),
    };
    send_ipc(&mut socket, &reg_msg)?;

    // 2. Бесконечный цикл отправки времени каждую секунду
    loop {
        let current_time = Local::now().format("%H:%M:%S").to_string();
        let formatted_data = format!("🕒 {}", current_time);

        let update_msg = IpcMessage::PublishUpdate {
            module: "clock".to_string(), // Используем строковый ID "clock"
            data: formatted_data,
        };

        if let Err(e) = send_ipc(&mut socket, &update_msg) {
            eprintln!("[Clock] Failed to send update: {:?}", e);
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

fn send_ipc(socket: &mut UnixStream, msg: &IpcMessage) -> Result<(), Box<dyn std::error::Error>> {
    let mut serialized = serde_json::to_string(msg)?;
    serialized.push('\n');
    socket.write_all(serialized.as_bytes())?;
    socket.flush()?;
    Ok(())
}