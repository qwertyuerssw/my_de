use de_ipc::ProcessAction;
use de_sdk::{ModuleClient, ModuleEvent};
use std::time::Duration;
use sysinfo::System;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[SysInfo] Starting sysinfo module with de-sdk...");

    // 1. Инициализируем и запускаем клиент модуля через SDK
    let client = ModuleClient::new("sysinfo");
    let (handle, mut rx_events) = client.start().await?;

    println!("[SysInfo] Connected to SDK background engine.");

    // 2. Запускаем фоновую асинхронную задачу для периодического опроса системных ресурсов
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        let mut sys = System::new_all();
        loop {
            sys.refresh_cpu();
            sys.refresh_memory();

            let cpu_usage = sys.global_cpu_info().cpu_usage();

             // Переводим байты в мегабайты (1024 * 1024 = 1_048_576 байт)
            let total_mem = sys.total_memory() / 1_048_576;
            let used_mem = sys.used_memory() / 1_048_576;

            let formatted_data = format!(
                "📊 CPU: {:.1}% | MEM: {}/{} MB",
                cpu_usage, used_mem, total_mem
            );

            if let Err(e) = handle_clone.publish_update(formatted_data) {
                eprintln!("[SysInfo] Failed to send update: {:?}", e);
            }

            // Неблокирующий сон в асинхронном контексте
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    // 3. Реактивный цикл обработки системных событий от de-manager или панели
    while let Some(event) = rx_events.recv().await {
        match event {
            ModuleEvent::Action(ProcessAction::Stop) => {
                println!("[SysInfo] Received Stop command. Exiting gracefully...");
                break; // Выходим из цикла для чистого завершения программы
            }
            ModuleEvent::Action(ProcessAction::Restart) => {
                println!("[SysInfo] Received Restart command. Reinitializing...");
                // Здесь может быть сброс или перезагрузка внутренних конфигураций
            }
            ModuleEvent::RefreshRequest => {
                println!("[SysInfo] Refresh requested by composer/panel.");
                // Поскольку опрос ресурсов происходит часто (раз в 2 сек),
                // принудительное мгновенное обновление обычно избыточно,
                // но при необходимости сюда можно вынести общий метод обновления данных.
            }
            _ => {}
        }
    }

    println!("[SysInfo] Stopped.");
    Ok(())
}
