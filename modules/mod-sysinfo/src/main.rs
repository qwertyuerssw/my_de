use de_ipc::ProcessAction;
use de_sdk::{ModuleClient, ModuleEvent};
use std::time::Duration;
use sysinfo::System;

// Выносим сборку строки в отдельную функцию, чтобы использовать её и в цикле, и при RefreshRequest
fn get_sys_info(sys: &mut System) -> String {
    sys.refresh_cpu();
    sys.refresh_memory();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let total_mem = sys.total_memory() / 1_048_576;
    let used_mem = sys.used_memory() / 1_048_576;

    format!("📊 CPU: {:.1}% | MEM: {}/{} MB", cpu_usage, used_mem, total_mem)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[SysInfo] Starting sysinfo module with de-sdk...");

    // 1. Используем правильный UUID расширения согласно вашей инструкции
    let client = ModuleClient::new("sysinfo@my-de.org");
    let (handle, mut rx_events) = client.start().await?;

    println!("[SysInfo] Connected to SDK background engine.");

    // 2. Запускаем фоновую задачу для периодического опроса ресурсов
        // 2. Запускаем фоновую задачу для периодического опроса ресурсов
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        let mut sys = System::new_all();
        
        // ВАЖНО для sysinfo 0.30+: первый вызов всегда возвращает 0.0%, 
        // поэтому делаем прогревочный refresh перед циклом.
        sys.refresh_cpu(); 
        tokio::time::sleep(Duration::from_millis(200)).await;

        loop {
            let formatted_data = get_sys_info(&mut sys);

            if let Err(e) = handle_clone.publish_update(formatted_data) {
                eprintln!("[SysInfo] Failed to send update: {:?}", e);
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    // Для обработки запросов обновления в основном потоке нам нужен свой экземпляр System
    let mut sys_refresh = System::new_all();

    // 3. Реактивный цикл обработки системных событий от de-manager или панели
    while let Some(event) = rx_events.recv().await {
        match event {
            ModuleEvent::Action(ProcessAction::Stop) => {
                println!("[SysInfo] Received Stop command. Exiting gracefully...");
                break;
            }
            ModuleEvent::Action(ProcessAction::Restart) => {
                println!("[SysInfo] Received Restart command. Reinitializing...");
            }
            ModuleEvent::RefreshRequest => {
                println!("[SysInfo] Refresh requested by composer/panel.");
                // Мгновенно собираем данные и отправляем в ответ на запрос панели
                let formatted_data = get_sys_info(&mut sys_refresh);
                let _ = handle.publish_update(formatted_data);
            }
            _ => {}
        }
    }

    println!("[SysInfo] Stopped.");
    Ok(())
}
