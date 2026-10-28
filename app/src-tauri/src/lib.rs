//! Application de bureau : expose le cœur ReqtiCraft à l'interface par des
//! commandes Tauri.

use serde::Serialize;

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![app_info])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ReqtiCraft");
}
