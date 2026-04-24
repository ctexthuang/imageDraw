mod ai;
mod commands;
mod db;
mod storage;
mod state;

use serde::Serialize;
use state::AppState;
use tauri::Manager;

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
                app_handle.manage(AppState { db });
                Ok::<(), AppError>(())
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::provider::list_providers,
            commands::provider::upsert_provider,
            commands::provider::delete_provider,
            commands::dialog::pick_material_images,
            commands::file::reveal_path,
            commands::file::open_generated_dir,
            commands::generation::create_generation_task,
            commands::generation::generate_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
