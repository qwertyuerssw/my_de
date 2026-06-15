use de_ipc::ProcessAction;
use de_sdk::{ModuleClient, ModuleEvent};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Volume] Starting volume module with de-sdk...");

    // 1. Инициализируем и запускаем клиент модуля через SDK
    let client = ModuleClient::new("volume");
    let (handle, mut rx_events) = client.start().await?;

    println!("[Volume] Connected to SDK background engine.");

    // 2. Фоновая задача для периодического обновления громкости
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        let mut mock_volume = 50; // Симуляция уровня громкости
        loop {
            let formatted_data = format!("🔊 Vol: {}%", mock_volume);

            if let Err(e) = handle_clone.publish_update(formatted_data) {
                eprintln!("[Volume] Failed to send update: {:?}", e);
            }

            mock_volume = (mock_volume + 5) % 100;

            // Неблокирующий сон в асинхронном контексте
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    // 3. Реактивный цикл обработки системных событий
    while let Some(event) = rx_events.recv().await {
        match event {
            ModuleEvent::Action(ProcessAction::Stop) => {
                println!("[Volume] Received Stop command. Exiting gracefully...");
                break;
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
