use tauri_plugin_dialog::DialogExt;

use crate::storage;

#[tauri::command]
pub async fn pick_material_images(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let files = app
        .dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_files();

    let paths = files
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file_path| {
            file_path
                .as_path()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect();

    Ok(paths)
}

#[tauri::command]
pub async fn import_material_images(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    storage::import_material_images(&app, paths).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_material_images(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<(), String> {
    storage::remove_material_images(&app, paths).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_material_image_cache(app: tauri::AppHandle) -> Result<(), String> {
    storage::clear_material_image_cache(&app).map_err(|error| error.to_string())
}
