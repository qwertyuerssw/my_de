use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use sysinfo::System;
use de_ipc::{IpcMessage, ClientType}; // Убрали импорт ModuleId

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[SysInfo] Starting sysinfo module...");
    
    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;
    println!("[SysInfo] Connected to IPC socket.");

    // 1. Регистрируемся в композиторе как модуль системной информации
    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Module("sysinfo".to_string()),
    };
    send_ipc(&mut socket, &reg_msg)?;

    let mut sys = System::new_all();

    // 2. Отправляем утилизацию ресурсов каждые 2 секунды
    loop {
        sys.refresh_cpu();
        sys.refresh_memory();

        let cpu_usage = sys.global_cpu_info().cpu_usage();
        
        let total_mem = sys.total_memory() / 1_048_576;
        let used_mem = sys.used_memory() / 1_048_576;

        let formatted_data = format!("📊 CPU: {:.1}% | MEM: {}/{} MB", cpu_usage, used_mem, total_mem);

        let update_msg = IpcMessage::PublishUpdate {
            module: "sysinfo".to_string(), // Используем строковый ID "sysinfo"
            data: formatted_data,
        };

        if let Err(e) = send_ipc(&mut socket, &update_msg) {
            eprintln!("[SysInfo] Failed to send update: {:?}", e);
            break;
        }

        thread::sleep(Duration::from_secs(2));
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