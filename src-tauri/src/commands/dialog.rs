use tauri_plugin_dialog::DialogExt;

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
        .filter_map(|file_path| file_path.as_path().map(|path| path.to_string_lossy().to_string()))
        .collect();

    Ok(paths)
}
