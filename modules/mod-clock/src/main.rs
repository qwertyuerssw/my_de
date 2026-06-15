use chrono::Local;
use de_sdk::{ModuleClient, ModuleEvent};
use de_ipc::ProcessAction;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Clock] Starting clock module with de-sdk...");

    // 1. Инициализируем и запускаем клиент модуля
    let client = ModuleClient::new("clock");
    let (handle, mut rx_events) = client.start().await?;

    println!("[Clock] Connected to SDK background engine.");

    // 2. Запускаем параллельную асинхронную задачу для ежесекундного обновления времени
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        loop {
            let current_time = Local::now().format("%H:%M:%S").to_string();
            let formatted_data = format!("🕒 {}", current_time);

            if let Err(e) = handle_clone.publish_update(formatted_data) {
                eprintln!("[Clock] Failed to send update: {:?}", e);
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    // 3. Главный поток превращается в реактивную петлю обработки событий
    while let Some(event) = rx_events.recv().await {
        match event {
            ModuleEvent::Action(ProcessAction::Stop) => {
                println!("[Clock] Received Stop command. Exiting gracefully...");
                break; // Завершает цикл и завершает работу модуля чисто
            }
            ModuleEvent::Action(ProcessAction::Restart) => {
                println!("[Clock] Received Restart command. Reinitializing...");
                // Здесь может быть логика перезагрузки конфигурации
            }
            ModuleEvent::RefreshRequest => {
                println!("[Clock] Refresh requested by composer/panel.");
                let current_time = Local::now().format("%H:%M:%S").to_string();
                let _ = handle.publish_update(format!("🕒 {}", current_time));
            }
            _ => {
                // Игнорируем команды вроде Start (так как мы уже запущены) 
                // и любые другие новые типы событий, которые могут появиться в SDK
            }
        }
    }

    println!("[Clock] Stopped.");
    Ok(())
}