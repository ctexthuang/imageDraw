use std::{fs, path::PathBuf};

use base64::{engine::general_purpose, Engine as _};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::AppError;

pub struct StoredImage {
    pub file_path: PathBuf,
    pub file_size: i64,
}

pub fn save_generated_image_bytes(
    app: &AppHandle,
    bytes: &[u8],
    mime_type: &str,
) -> Result<StoredImage, AppError> {
    let images_dir = app.path().app_data_dir()?.join("images").join("generated");
    fs::create_dir_all(&images_dir)?;

    let extension = match mime_type {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    let file_path = images_dir.join(format!("{}.{}", Uuid::new_v4(), extension));
    let file_size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    fs::write(&file_path, bytes)?;

    Ok(StoredImage {
        file_path,
        file_size,
    })
}

pub fn decode_base64_image(data_base64: &str) -> Result<Vec<u8>, AppError> {
    Ok(general_purpose::STANDARD.decode(data_base64)?)
}
