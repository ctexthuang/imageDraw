use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;

use crate::{
    db::{models::UpsertProviderInput, repository},
    state::AppState,
    AppError,
};

#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::db::models::ProviderConfig>, AppError> {
    repository::list_providers(&state.db).await
}

#[tauri::command]
pub async fn upsert_provider(
    state: State<'_, AppState>,
    input: UpsertProviderInput,
) -> Result<(), AppError> {
    if input
        .api_key
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(AppError::Provider(
            "请先填写 API Key，再保存配置".to_string(),
        ));
    }

    repository::upsert_provider(&state.db, input).await
}

#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    repository::delete_provider(&state.db, &id).await
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderModel {
    pub id: String,
    pub owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelItem>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
    owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProviderCapabilities {
    image_models: Option<Vec<ProviderModel>>,
    selected_image_models: Option<Vec<String>>,
}

fn is_image_model(model_id: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    [
        "gpt-image",
        "image",
        "dall-e",
        "dalle",
        "imagen",
        "flux",
        "qwen-image",
        "wan",
        "z-image",
        "hunyuan-image",
        "gemini",
        "seedream",
        "seededit",
        "stable-diffusion",
        "sd-",
        "midjourney",
        "recraft",
    ]
    .iter()
    .any(|marker| id.contains(marker))
}

fn default_seedream_models() -> Vec<ProviderModel> {
    vec![
        ProviderModel {
            id: "doubao-seedream-4-5-251128".to_string(),
            owned_by: Some("volcengine".to_string()),
        },
        ProviderModel {
            id: "doubao-seedream-4-0-250828".to_string(),
            owned_by: Some("volcengine".to_string()),
        },
    ]
}

fn default_dashscope_models() -> Vec<ProviderModel> {
    vec![
        ProviderModel {
            id: "qwen-image-2.0-pro".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "qwen-image-2.0".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "qwen-image-plus".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "qwen-image".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "wan2.7-image-pro".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "wan2.7-image".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
        ProviderModel {
            id: "z-image-turbo".to_string(),
            owned_by: Some("alibaba-cloud".to_string()),
        },
    ]
}

fn default_tencent_hunyuan_models() -> Vec<ProviderModel> {
    vec![
        ProviderModel {
            id: "hunyuan-image-3.0".to_string(),
            owned_by: Some("tencent-cloud".to_string()),
        },
        ProviderModel {
            id: "hunyuan-image-2.0".to_string(),
            owned_by: Some("tencent-cloud".to_string()),
        },
        ProviderModel {
            id: "hunyuan-image-lite".to_string(),
            owned_by: Some("tencent-cloud".to_string()),
        },
    ]
}

fn default_google_gemini_models() -> Vec<ProviderModel> {
    vec![
        ProviderModel {
            id: "gemini-2.5-flash-image".to_string(),
            owned_by: Some("google".to_string()),
        },
        ProviderModel {
            id: "gemini-3.1-flash-image-preview".to_string(),
            owned_by: Some("google".to_string()),
        },
        ProviderModel {
            id: "gemini-3-pro-image-preview".to_string(),
            owned_by: Some("google".to_string()),
        },
    ]
}

#[tauri::command]
pub async fn fetch_provider_models(
    state: State<'_, AppState>,
    input: UpsertProviderInput,
) -> Result<Vec<ProviderModel>, AppError> {
    if input.kind != "openai"
        && input.kind != "openai-compatible"
        && input.kind != "volcengine-ark"
        && input.kind != "dashscope"
        && input.kind != "tencent-hunyuan"
        && input.kind != "google-gemini"
    {
        return Err(AppError::Provider(format!(
            "API 分类 {} 暂未接入模型列表获取",
            input.kind
        )));
    }

    if (input.kind == "openai" || input.kind == "openai-compatible")
        && !input.base_url.trim_end_matches('/').ends_with("/v1")
    {
        return Err(AppError::Provider(
            "Base URL 看起来不是 API 地址。OpenAI-compatible 地址通常需要以 /v1 结尾。".to_string(),
        ));
    }
    if input.kind == "volcengine-ark" && !input.base_url.trim_end_matches('/').ends_with("/api/v3")
    {
        return Err(AppError::Provider(
            "火山方舟 Seedream 的 Base URL 通常需要以 /api/v3 结尾。".to_string(),
        ));
    }
    if input.kind == "dashscope" && !input.base_url.trim_end_matches('/').ends_with("/api/v1") {
        return Err(AppError::Provider(
            "阿里云百炼 DashScope 的 Base URL 通常需要以 /api/v1 结尾。".to_string(),
        ));
    }
    if input.kind == "google-gemini" && !input.base_url.trim_end_matches('/').ends_with("/v1beta") {
        return Err(AppError::Provider(
            "Google Gemini / Nano Banana 的 Base URL 通常填写 https://generativelanguage.googleapis.com/v1beta".to_string(),
        ));
    }
    if input.kind == "tencent-hunyuan"
        && !input
            .base_url
            .trim_end_matches('/')
            .ends_with("aiart.tencentcloudapi.com")
        && !input
            .base_url
            .trim_end_matches('/')
            .ends_with("hunyuan.tencentcloudapi.com")
    {
        return Err(AppError::Provider(
            "腾讯混元图像的 Base URL 通常填写 https://aiart.tencentcloudapi.com".to_string(),
        ));
    }

    let saved_provider = repository::get_provider_secret(&state.db, &input.id)
        .await
        .ok();
    let api_key = match input.api_key.clone() {
        Some(key) if !key.trim().is_empty() => Some(key),
        Some(_) => None,
        None => saved_provider.and_then(|provider| provider.api_key_encrypted),
    };
    let Some(api_key) = api_key else {
        return Err(AppError::Provider(
            "API Key 为空，无法获取模型列表".to_string(),
        ));
    };

    if input.kind == "dashscope" {
        return save_provider_models(&state, input, default_dashscope_models()).await;
    }
    if input.kind == "tencent-hunyuan" {
        return save_provider_models(&state, input, default_tencent_hunyuan_models()).await;
    }
    if input.kind == "google-gemini" {
        return save_provider_models(&state, input, default_google_gemini_models()).await;
    }

    let response = Client::new()
        .get(format!("{}/models", input.base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        if input.kind == "volcengine-ark" && matches!(status.as_u16(), 404 | 405 | 501) {
            let fetched_models = default_seedream_models();
            return save_provider_models(&state, input, fetched_models).await;
        }
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "request failed".to_string());
        return Err(AppError::Provider(format!(
            "获取模型列表失败 ({status}): {message}"
        )));
    }

    let mut fetched_models: Vec<ProviderModel> = response
        .json::<ModelsResponse>()
        .await?
        .data
        .into_iter()
        .filter(|model| is_image_model(&model.id))
        .map(|model| ProviderModel {
            id: model.id,
            owned_by: model.owned_by,
        })
        .collect();
    if input.kind == "volcengine-ark" && fetched_models.is_empty() {
        fetched_models = default_seedream_models();
    }

    save_provider_models(&state, input, fetched_models).await
}

async fn save_provider_models(
    state: &State<'_, AppState>,
    input: UpsertProviderInput,
    fetched_models: Vec<ProviderModel>,
) -> Result<Vec<ProviderModel>, AppError> {
    let saved_providers = repository::list_providers(&state.db)
        .await
        .unwrap_or_default();
    let saved_capabilities = saved_providers
        .iter()
        .find(|provider| provider.id == input.id)
        .and_then(|provider| provider.capabilities.as_deref())
        .and_then(|value| serde_json::from_str::<ProviderCapabilities>(value).ok());
    let mut models = saved_capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.image_models.clone())
        .unwrap_or_default();
    models.extend(fetched_models);
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    let model_ids = models
        .iter()
        .map(|model| model.id.clone())
        .collect::<Vec<_>>();
    let mut selected_model_ids = saved_capabilities
        .and_then(|capabilities| capabilities.selected_image_models)
        .unwrap_or_default()
        .into_iter()
        .filter(|model_id| model_ids.contains(model_id))
        .collect::<Vec<_>>();
    for model_id in &model_ids {
        if !selected_model_ids.contains(model_id) {
            selected_model_ids.push(model_id.clone());
        }
    }

    let capabilities = json!({
        "responses_api": true,
        "images_api": true,
        "chat_completions": true,
        "image_edit": true,
        "image_models": models,
        "selected_image_models": selected_model_ids,
    });
    repository::upsert_provider(
        &state.db,
        UpsertProviderInput {
            capabilities: Some(capabilities.to_string()),
            ..input
        },
    )
    .await?;

    Ok(models)
}
