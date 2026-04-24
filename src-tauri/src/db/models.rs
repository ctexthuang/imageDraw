use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub text_model: Option<String>,
    pub image_model: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct ProviderSecret {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key_encrypted: Option<String>,
    pub image_model: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertProviderInput {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub text_model: Option<String>,
    pub image_model: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GenerationTask {
    pub id: String,
    pub provider_id: String,
    pub task_type: String,
    pub prompt: String,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGenerationTaskInput {
    pub provider_id: String,
    pub task_type: String,
    pub prompt: String,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ImageAsset {
    pub id: String,
    pub task_id: Option<String>,
    pub file_path: String,
    pub mime_type: Option<String>,
    pub file_size: Option<i64>,
    pub source_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateImageInput {
    pub provider_id: String,
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub image_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateImageOutput {
    pub task: GenerationTask,
    pub asset: ImageAsset,
}
