use std::{collections::HashMap, io::Cursor, sync::Arc, time::Duration};

use image::{imageops, ImageFormat, RgbaImage};
use tauri::{AppHandle, State};
use tokio::sync::{oneshot, Mutex};

use crate::{
    ai::{
        dashscope::DashScopeProvider,
        google_gemini::GoogleGeminiProvider,
        openai_compatible::OpenAiCompatibleProvider,
        provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult},
        seedream::SeedreamProvider,
        tencent_hunyuan::TencentHunyuanProvider,
    },
    db::{
        models::{
            CreateGenerationTaskInput, GenerateImageInput, GenerateImageOutput, GenerationTask,
            ImageAssetOutput, PosterQrOverlayInput, PosterQrPosition,
        },
        repository,
    },
    state::AppState,
    storage, AppError,
};

#[tauri::command]
pub async fn cancel_generation(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<bool, AppError> {
    let sender = state.cancellations.lock().await.remove(&request_id);
    Ok(sender
        .map(|sender| sender.send(()).is_ok())
        .unwrap_or(false))
}

#[tauri::command]
pub async fn create_generation_task(
    state: State<'_, AppState>,
    input: CreateGenerationTaskInput,
) -> Result<GenerationTask, AppError> {
    repository::create_generation_task(&state.db, input).await
}

#[tauri::command]
pub async fn generate_image(
    app: AppHandle,
    state: State<'_, AppState>,
    input: GenerateImageInput,
) -> Result<GenerateImageOutput, AppError> {
    let provider = repository::get_provider_secret(&state.db, &input.provider_id).await?;
    if !provider.enabled {
        return Err(AppError::Provider(format!(
            "provider {} is disabled",
            provider.name
        )));
    }

    if provider.kind != "openai"
        && provider.kind != "openai-compatible"
        && provider.kind != "volcengine-ark"
        && provider.kind != "dashscope"
        && provider.kind != "tencent-hunyuan"
        && provider.kind != "google-gemini"
    {
        return Err(AppError::Provider(format!(
            "provider kind {} is not supported yet",
            provider.kind
        )));
    }

    if (provider.kind == "openai" || provider.kind == "openai-compatible")
        && !provider.base_url.trim_end_matches('/').ends_with("/v1")
    {
        return Err(AppError::Provider(
            "Base URL 看起来不是 API 地址。OpenAI-compatible 地址通常需要以 /v1 结尾，例如 https://api.openai.com/v1 或 https://你的中转站域名/v1".to_string(),
        ));
    }
    if provider.kind == "volcengine-ark"
        && !provider.base_url.trim_end_matches('/').ends_with("/api/v3")
    {
        return Err(AppError::Provider(
            "火山方舟 Seedream 的 Base URL 通常需要以 /api/v3 结尾，例如 https://ark.cn-beijing.volces.com/api/v3".to_string(),
        ));
    }
    if provider.kind == "dashscope" && !provider.base_url.trim_end_matches('/').ends_with("/api/v1")
    {
        return Err(AppError::Provider(
            "阿里云百炼 DashScope 的 Base URL 通常需要以 /api/v1 结尾，例如 https://dashscope.aliyuncs.com/api/v1".to_string(),
        ));
    }
    if provider.kind == "google-gemini"
        && !provider.base_url.trim_end_matches('/').ends_with("/v1beta")
    {
        return Err(AppError::Provider(
            "Google Gemini / Nano Banana 的 Base URL 通常填写 https://generativelanguage.googleapis.com/v1beta".to_string(),
        ));
    }
    if provider.kind == "tencent-hunyuan"
        && !provider
            .base_url
            .trim_end_matches('/')
            .ends_with("aiart.tencentcloudapi.com")
        && !provider
            .base_url
            .trim_end_matches('/')
            .ends_with("hunyuan.tencentcloudapi.com")
    {
        return Err(AppError::Provider(
            "腾讯混元图像的 Base URL 通常填写 https://aiart.tencentcloudapi.com".to_string(),
        ));
    }

    let api_key = provider
        .api_key_encrypted
        .clone()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| AppError::Provider("provider api_key is empty".to_string()))?;
    let model = input
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
        .or_else(|| provider.image_model.clone())
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| AppError::Provider("image model is empty".to_string()))?;

    let task = repository::create_generation_task(
        &state.db,
        CreateGenerationTaskInput {
            provider_id: provider.id.clone(),
            task_type: if input.image_paths.is_empty() {
                "text_to_image".to_string()
            } else {
                "image_edit".to_string()
            },
            prompt: input.prompt.clone(),
            model: model.clone(),
            size: input.size.clone(),
            quality: input.quality.clone(),
        },
    )
    .await?;

    let request_id = input
        .request_id
        .clone()
        .filter(|value| !value.trim().is_empty());
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    if let Some(request_id) = &request_id {
        state
            .cancellations
            .lock()
            .await
            .insert(request_id.clone(), cancel_sender);
    }
    let cancel_guard = CancellationGuard {
        cancellations: state.cancellations.clone(),
        request_id: request_id.clone(),
    };

    let poster_qr_overlay = input.poster_qr_overlay.clone();
    let image_future = run_image_generation(
        provider.kind.clone(),
        provider.base_url,
        api_key,
        model,
        input,
    );
    let image_result = match if request_id.is_some() {
        tokio::select! {
            result = image_future => result,
            _ = cancel_receiver => {
                repository::mark_generation_task_failed(&state.db, &task.id, "generation cancelled").await?;
                return Err(AppError::Provider("生成已强制停止".to_string()));
            }
        }
    } else {
        image_future.await
    } {
        Ok(result) => result,
        Err(error) => {
            repository::mark_generation_task_failed(&state.db, &task.id, &error.to_string())
                .await?;
            return Err(error);
        }
    };

    let image_bytes = match &image_result.data {
        ImageData::Base64(data_base64) => storage::decode_base64_image(data_base64)?,
        ImageData::Url(url) => {
            let download_future =
                async { Ok::<_, AppError>(reqwest::get(url).await?.bytes().await?.to_vec()) };
            if request_id.is_some() {
                tokio::select! {
                    result = download_future => result?,
                    _ = cancel_guard.cancelled() => {
                        repository::mark_generation_task_failed(&state.db, &task.id, "generation cancelled").await?;
                        return Err(AppError::Provider("生成已强制停止".to_string()));
                    }
                }
            } else {
                download_future.await?
            }
        }
    };
    if cancel_guard.is_cancelled().await {
        repository::mark_generation_task_failed(&state.db, &task.id, "generation cancelled")
            .await?;
        return Err(AppError::Provider("生成已强制停止".to_string()));
    }

    let save_result: Result<ImageAssetOutput, AppError> = async {
        if let Some(overlay) = poster_qr_overlay.as_ref() {
            let composite_bytes = compose_poster_qr_overlay(&image_bytes, overlay)?;
            let stored_composite =
                storage::save_generated_image_bytes(&app, &composite_bytes, "image/png")?;
            let composite_asset = create_stored_image_asset(
                &app,
                &state.db,
                &task.id,
                &stored_composite,
                "image/png",
                "poster_composite",
            )
            .await?;
            Ok(composite_asset)
        } else {
            let stored_image =
                storage::save_generated_image_bytes(&app, &image_bytes, &image_result.mime_type)?;
            create_stored_image_asset(
                &app,
                &state.db,
                &task.id,
                &stored_image,
                &image_result.mime_type,
                "generated",
            )
            .await
        }
    }
    .await;
    let asset = match save_result {
        Ok(output) => output,
        Err(error) => {
            repository::mark_generation_task_failed(&state.db, &task.id, &error.to_string())
                .await?;
            return Err(error);
        }
    };
    repository::mark_generation_task_completed(&state.db, &task.id).await?;

    Ok(GenerateImageOutput {
        task: GenerationTask {
            status: "completed".to_string(),
            ..task
        },
        asset,
    })
}

async fn create_stored_image_asset(
    app: &AppHandle,
    db: &sqlx::SqlitePool,
    task_id: &str,
    stored_image: &storage::StoredImage,
    mime_type: &str,
    source_type: &str,
) -> Result<ImageAssetOutput, AppError> {
    let file_path = stored_image.file_path.to_string_lossy().to_string();
    let display_path = storage::generated_image_display_path(app, &stored_image.file_path)?;
    let asset = repository::create_image_asset(
        db,
        task_id,
        &file_path,
        mime_type,
        stored_image.file_size,
        source_type,
    )
    .await?;

    Ok(ImageAssetOutput {
        asset,
        display_path,
    })
}

fn compose_poster_qr_overlay(
    base_bytes: &[u8],
    overlay: &PosterQrOverlayInput,
) -> Result<Vec<u8>, AppError> {
    let mut base_image = image::load_from_memory(base_bytes)?.to_rgba8();
    let qr_image = image::open(&overlay.image_path)?.to_rgba8();
    let (base_width, base_height) = base_image.dimensions();
    let (qr_width, qr_height) = qr_image.dimensions();
    if base_width == 0 || base_height == 0 || qr_width == 0 || qr_height == 0 {
        return Err(AppError::Provider(
            "二维码合成失败：图片尺寸无效".to_string(),
        ));
    }

    let short_side = base_width.min(base_height);
    let size_ratio = normalized_overlay_ratio(overlay.size_ratio, 0.18, 0.05, 0.5);
    let margin_ratio = normalized_overlay_ratio(overlay.margin_ratio, 0.05, 0.0, 0.2);
    let side = ((short_side as f32) * size_ratio).round().max(1.0) as u32;
    let side = side.min(short_side);
    let margin = ((short_side as f32) * margin_ratio).round().max(0.0) as u32;
    let (x, y) = overlay_position(base_width, base_height, side, margin, overlay.position);

    let trimmed_qr = trim_outer_transparent_pixels(&qr_image);
    let paste_side = side;
    let scale = (paste_side as f32 / trimmed_qr.width() as f32)
        .min(paste_side as f32 / trimmed_qr.height() as f32);
    let target_width = ((trimmed_qr.width() as f32) * scale).round().max(1.0) as u32;
    let target_height = ((trimmed_qr.height() as f32) * scale).round().max(1.0) as u32;
    let resized_qr = imageops::resize(
        &trimmed_qr,
        target_width.min(paste_side),
        target_height.min(paste_side),
        imageops::FilterType::Nearest,
    );
    let qr_x = x + (side.saturating_sub(resized_qr.width())) / 2;
    let qr_y = y + (side.saturating_sub(resized_qr.height())) / 2;
    imageops::overlay(
        &mut base_image,
        &resized_qr,
        i64::from(qr_x),
        i64::from(qr_y),
    );

    let mut bytes = Vec::new();
    base_image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)?;
    Ok(bytes)
}

fn trim_outer_transparent_pixels(image: &RgbaImage) -> RgbaImage {
    let mut min_x = image.width();
    let mut min_y = image.height();
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found_content = false;

    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] >= 16 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            found_content = true;
        }
    }

    if !found_content {
        return image.clone();
    }

    let content_width = max_x - min_x + 1;
    let content_height = max_y - min_y + 1;
    if content_width >= image.width() * 95 / 100 && content_height >= image.height() * 95 / 100 {
        return image.clone();
    }

    imageops::crop_imm(image, min_x, min_y, content_width, content_height).to_image()
}

fn normalized_overlay_ratio(value: Option<f32>, default: f32, min: f32, max: f32) -> f32 {
    let value = value.filter(|ratio| ratio.is_finite()).unwrap_or(default);
    value.clamp(min, max)
}

fn overlay_position(
    image_width: u32,
    image_height: u32,
    side: u32,
    margin: u32,
    position: PosterQrPosition,
) -> (u32, u32) {
    let max_x = image_width.saturating_sub(side);
    let max_y = image_height.saturating_sub(side);
    let x_margin = margin.min(max_x);
    let y_margin = margin.min(max_y);

    let x = match position {
        PosterQrPosition::TopLeft | PosterQrPosition::MiddleLeft | PosterQrPosition::BottomLeft => {
            x_margin
        }
        PosterQrPosition::TopCenter
        | PosterQrPosition::MiddleCenter
        | PosterQrPosition::BottomCenter => max_x / 2,
        PosterQrPosition::TopRight
        | PosterQrPosition::MiddleRight
        | PosterQrPosition::BottomRight => max_x.saturating_sub(x_margin),
    };
    let y = match position {
        PosterQrPosition::TopLeft | PosterQrPosition::TopCenter | PosterQrPosition::TopRight => {
            y_margin
        }
        PosterQrPosition::MiddleLeft
        | PosterQrPosition::MiddleCenter
        | PosterQrPosition::MiddleRight => max_y / 2,
        PosterQrPosition::BottomLeft
        | PosterQrPosition::BottomCenter
        | PosterQrPosition::BottomRight => max_y.saturating_sub(y_margin),
    };

    (x, y)
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn qr_trim_preserves_opaque_white_border() {
        let image = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));

        let trimmed = trim_outer_transparent_pixels(&image);

        assert_eq!(trimmed.dimensions(), (10, 10));
    }

    #[test]
    fn qr_trim_only_removes_transparent_border() {
        let mut image = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        for y in 3..9 {
            for x in 2..8 {
                image.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }

        let trimmed = trim_outer_transparent_pixels(&image);

        assert_eq!(trimmed.dimensions(), (6, 6));
    }
}

struct CancellationGuard {
    cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    request_id: Option<String>,
}

impl CancellationGuard {
    async fn is_cancelled(&self) -> bool {
        if let Some(request_id) = &self.request_id {
            return !self.cancellations.lock().await.contains_key(request_id);
        }
        false
    }

    async fn cancelled(&self) {
        if let Some(request_id) = &self.request_id {
            loop {
                if !self.cancellations.lock().await.contains_key(request_id) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
        }
        std::future::pending::<()>().await;
    }
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        if let Some(request_id) = self.request_id.clone() {
            let cancellations = self.cancellations.clone();
            tauri::async_runtime::spawn(async move {
                cancellations.lock().await.remove(&request_id);
            });
        }
    }
}

async fn run_image_generation(
    provider_kind: String,
    base_url: String,
    api_key: String,
    model: String,
    input: GenerateImageInput,
) -> Result<ImageResult, AppError> {
    if provider_kind == "volcengine-ark" {
        let ai_provider = SeedreamProvider::new(base_url, api_key);
        if input.image_paths.is_empty() {
            ai_provider
                .generate_image(ImageGenerateRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                })
                .await
        } else {
            ai_provider
                .edit_image(ImageEditRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                    image_paths: input.image_paths,
                })
                .await
        }
    } else if provider_kind == "dashscope" {
        let ai_provider = DashScopeProvider::new(base_url, api_key);
        if input.image_paths.is_empty() {
            ai_provider
                .generate_image(ImageGenerateRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                })
                .await
        } else {
            ai_provider
                .edit_image(ImageEditRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                    image_paths: input.image_paths,
                })
                .await
        }
    } else if provider_kind == "tencent-hunyuan" {
        let ai_provider = TencentHunyuanProvider::new(base_url, api_key)?;
        if input.image_paths.is_empty() {
            ai_provider
                .generate_image(ImageGenerateRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                })
                .await
        } else {
            ai_provider
                .edit_image(ImageEditRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                    image_paths: input.image_paths,
                })
                .await
        }
    } else if provider_kind == "google-gemini" {
        let ai_provider = GoogleGeminiProvider::new(base_url, api_key);
        if input.image_paths.is_empty() {
            ai_provider
                .generate_image(ImageGenerateRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                })
                .await
        } else {
            ai_provider
                .edit_image(ImageEditRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                    image_paths: input.image_paths,
                })
                .await
        }
    } else {
        let ai_provider = OpenAiCompatibleProvider::new(base_url, api_key);
        if input.image_paths.is_empty() {
            ai_provider
                .generate_image(ImageGenerateRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                })
                .await
        } else {
            ai_provider
                .edit_image(ImageEditRequest {
                    prompt: input.prompt,
                    model,
                    size: input.size,
                    quality: input.quality,
                    image_paths: input.image_paths,
                })
                .await
        }
    }
}
