use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    db::repository,
    state::AppState,
    storage::{self, GalleryDirectoryInfo, MovedImagePath},
    AppError,
};

#[derive(Debug, Serialize)]
pub struct SetGalleryDirectoryOutput {
    pub directory: GalleryDirectoryInfo,
    pub moved_paths: Vec<MovedImagePath>,
}

#[tauri::command]
pub async fn get_gallery_directory(app: AppHandle) -> Result<GalleryDirectoryInfo, AppError> {
    storage::gallery_directory_info(&app)
}

#[tauri::command]
pub async fn pick_gallery_directory(app: AppHandle) -> Result<Option<String>, String> {
    let directory = app.dialog().file().blocking_pick_folder().and_then(|path| {
        path.as_path()
            .map(|path| path.to_string_lossy().to_string())
    });

    Ok(directory)
}

#[tauri::command]
pub async fn set_gallery_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    directory: String,
) -> Result<SetGalleryDirectoryOutput, AppError> {
    let (info, moved_paths) = storage::set_generated_images_dir(&app, PathBuf::from(directory))?;
    let updates = moved_paths
        .iter()
        .map(|path| (path.old_path.clone(), path.new_path.clone()))
        .collect::<Vec<_>>();
    repository::update_image_asset_paths(&state.db, &updates).await?;
    Ok(SetGalleryDirectoryOutput {
        directory: info,
        moved_paths,
    })
}
