use std::io::Write;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use de_ipc::{IpcMessage, ClientType}; // Убрали импорт ModuleId

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = "/tmp/my-de-ipc.sock";
    let mut socket = UnixStream::connect(socket_path)?;

    // Регистрируемся в IPC
    let reg_msg = IpcMessage::Register {
        client_type: ClientType::Module("volume".to_string()),
    };
    send_ipc(&mut socket, &reg_msg)?;

    let mut mock_volume = 50; // Симуляция уровня громкости
    loop {
        let formatted_data = format!("🔊 Vol: {}%", mock_volume);

        let update_msg = IpcMessage::PublishUpdate {
            module: "volume".to_string(), // Используем строковый ID "volume"
            data: formatted_data,
        };

        send_ipc(&mut socket, &update_msg)?;

        mock_volume = (mock_volume + 5) % 100;

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