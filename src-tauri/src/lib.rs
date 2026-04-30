mod ai;
mod commands;
mod db;
mod state;
mod storage;

use serde::Serialize;
use state::AppState;
use std::{collections::HashMap, sync::Arc};
use tauri::Manager;
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tauri path error: {0}")]
    Path(#[from] tauri::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("mime error: {0}")]
    Mime(#[from] reqwest::header::InvalidHeaderValue),
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider error: {0}")]
    Provider(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let db = db::init(&app_handle).await?;
                app_handle.manage(AppState {
                    db,
                    cancellations: Arc::new(Mutex::new(HashMap::new())),
                });
                Ok::<(), AppError>(())
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::provider::list_providers,
            commands::provider::upsert_provider,
            commands::provider::delete_provider,
            commands::provider::fetch_provider_models,
            commands::dialog::pick_material_images,
            commands::file::reveal_path,
            commands::file::open_generated_dir,
            commands::gallery::get_gallery_directory,
            commands::gallery::pick_gallery_directory,
            commands::gallery::set_gallery_directory,
            commands::generation::create_generation_task,
            commands::generation::cancel_generation,
            commands::generation::generate_image,
            commands::update::check_for_updates,
            commands::update::open_update_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
