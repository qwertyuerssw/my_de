use chrono::Local;
use de_ipc::ProcessAction;
use de_sdk::{ModuleClient, ModuleEvent};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[Clock] Starting clock module with de-sdk...");

    // Используем правильный UUID расширения!
    let client = ModuleClient::new("clock@my-de.org");
    let (handle, mut rx_events) = client.start().await?;

    println!("[Clock] Connected to SDK background engine.");

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

    while let Some(event) = rx_events.recv().await {
        match event {
            ModuleEvent::Action(ProcessAction::Stop) => {
                println!("[Clock] Received Stop command. Exiting gracefully...");
                break;
            }
            ModuleEvent::Action(ProcessAction::Restart) => {
                println!("[Clock] Received Restart command. Reinitializing...");
            }
            ModuleEvent::RefreshRequest => {
                println!("[Clock] Refresh requested by composer/panel.");
                let current_time = Local::now().format("%H:%M:%S").to_string();
                let _ = handle.publish_update(format!("🕒 {}", current_time));
            }
            _ => {}
        }
    }

    println!("[Clock] Stopped.");
    Ok(())
}