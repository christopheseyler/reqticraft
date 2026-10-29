//! Application de bureau : expose la session du cœur ReqtiCraft à l'interface.
//!
//! Une seule commande, `api`, relaie les requêtes de l'interface vers
//! [`rc_core::session::call`] ; le serveur local (`reqticraft serve`) relaie les
//! mêmes requêtes, ce qui permet de développer l'interface dans un navigateur.

use rc_core::session::{call, Session};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct AppState(Arc<Mutex<Option<Session>>>);

#[tauri::command]
async fn api(
    state: tauri::State<'_, AppState>,
    method: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let session = Arc::clone(&state.0);
    // L'analyse peut durer : hors du fil de l'interface.
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = session
            .lock()
            .map_err(|_| "session indisponible".to_owned())?;
        call(
            &mut guard,
            &method,
            &args.unwrap_or_else(|| Value::Object(Default::default())),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("échec du démarrage de ReqtiCraft");
}
