use std::{collections::HashMap, sync::Arc, time::Duration};

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

    let stored_image =
        storage::save_generated_image_bytes(&app, &image_bytes, &image_result.mime_type)?;
    let file_path = stored_image.file_path.to_string_lossy().to_string();
    let asset = repository::create_image_asset(
        &state.db,
        &task.id,
        &file_path,
        &image_result.mime_type,
        stored_image.file_size,
        "generated",
    )
    .await?;
    repository::mark_generation_task_completed(&state.db, &task.id).await?;

    Ok(GenerateImageOutput {
        task: GenerationTask {
            status: "completed".to_string(),
            ..task
        },
        asset,
    })
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
