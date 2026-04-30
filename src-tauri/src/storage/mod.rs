use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::AppError;

pub struct StoredImage {
    pub file_path: PathBuf,
    pub file_size: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GalleryDirectoryInfo {
    pub directory: String,
    pub is_custom: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MovedImagePath {
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct StorageSettings {
    generated_dir: Option<String>,
}

pub fn save_generated_image_bytes(
    app: &AppHandle,
    bytes: &[u8],
    mime_type: &str,
) -> Result<StoredImage, AppError> {
    let images_dir = generated_images_dir(app)?;
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

pub fn gallery_directory_info(app: &AppHandle) -> Result<GalleryDirectoryInfo, AppError> {
    let settings = read_storage_settings(app)?;
    let is_custom = settings
        .generated_dir
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let directory = settings
        .generated_dir
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_generated_images_dir(app)?);

    Ok(GalleryDirectoryInfo {
        directory: directory.to_string_lossy().to_string(),
        is_custom,
    })
}

pub fn generated_images_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let settings = read_storage_settings(app)?;
    Ok(settings
        .generated_dir
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_generated_images_dir(app)?))
}

pub fn default_generated_images_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(app.path().app_data_dir()?.join("images").join("generated"))
}

pub fn set_generated_images_dir(
    app: &AppHandle,
    next_dir: PathBuf,
) -> Result<(GalleryDirectoryInfo, Vec<MovedImagePath>), AppError> {
    if next_dir.as_os_str().is_empty() {
        return Err(AppError::Provider("图库目录不能为空".to_string()));
    }

    let old_dir = generated_images_dir(app)?;
    fs::create_dir_all(&next_dir)?;

    let moved_paths = if is_same_directory(&old_dir, &next_dir) {
        Vec::new()
    } else {
        move_generated_images(&old_dir, &next_dir)?
    };

    write_storage_settings(
        app,
        &StorageSettings {
            generated_dir: Some(next_dir.to_string_lossy().to_string()),
        },
    )?;

    Ok((
        GalleryDirectoryInfo {
            directory: next_dir.to_string_lossy().to_string(),
            is_custom: true,
        },
        moved_paths,
    ))
}

pub fn decode_base64_image(data_base64: &str) -> Result<Vec<u8>, AppError> {
    Ok(general_purpose::STANDARD.decode(data_base64)?)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    Ok(app.path().app_data_dir()?.join("storage-settings.json"))
}

fn read_storage_settings(app: &AppHandle) -> Result<StorageSettings, AppError> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(StorageSettings::default());
    }

    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn write_storage_settings(app: &AppHandle, settings: &StorageSettings) -> Result<(), AppError> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

fn is_same_directory(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn move_generated_images(from_dir: &Path, to_dir: &Path) -> Result<Vec<MovedImagePath>, AppError> {
    if !from_dir.exists() {
        return Ok(Vec::new());
    }

    let mut moved = Vec::new();
    for entry in fs::read_dir(from_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_image_file(&path) {
            continue;
        }

        let Some(file_name) = path.file_name() else {
            continue;
        };
        let target = unique_target_path(&to_dir.join(file_name));
        fs::rename(&path, &target).or_else(|_| {
            fs::copy(&path, &target)?;
            fs::remove_file(&path)
        })?;

        moved.push(MovedImagePath {
            old_path: path.to_string_lossy().to_string(),
            new_path: target.to_string_lossy().to_string(),
        });
    }

    Ok(moved)
}

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp"
            )
        })
        .unwrap_or(false)
}

fn unique_target_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 1.. {
        let file_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unique path search is unbounded")
}
