use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerateRequest {
    pub prompt: String,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEditRequest {
    pub prompt: String,
    pub model: String,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub image_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResult {
    pub mime_type: String,
    pub data: ImageData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageData {
    Base64(String),
    Url(String),
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError>;
    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError>;
}
