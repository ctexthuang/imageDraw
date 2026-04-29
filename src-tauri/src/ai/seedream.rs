use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use super::provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult};
use crate::AppError;

pub struct SeedreamProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl SeedreamProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

#[derive(Debug, Serialize)]
struct SeedreamRequestBody<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<&'a str>,
    response_format: &'a str,
    stream: bool,
    watermark: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SeedreamResponseBody {
    data: Vec<SeedreamResponseItem>,
}

#[derive(Debug, Deserialize)]
struct SeedreamResponseItem {
    b64_json: Option<String>,
    url: Option<String>,
}

fn mime_for_path(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

fn path_to_data_url(path: &str) -> Result<String, AppError> {
    let bytes = fs::read(path)?;
    Ok(format!(
        "data:{};base64,{}",
        mime_for_path(path),
        general_purpose::STANDARD.encode(bytes)
    ))
}

fn parse_seedream_response(response_text: &str) -> Result<ImageResult, AppError> {
    if response_text.trim_start().starts_with("<!doctype html")
        || response_text.trim_start().starts_with("<html")
    {
        return Err(AppError::Provider(
            "火山方舟返回了 HTML 页面，不是 API JSON 响应。请检查 Base URL 是否为 API 地址。"
                .to_string(),
        ));
    }

    let response_body: SeedreamResponseBody =
        serde_json::from_str(response_text).map_err(|error| {
            AppError::Provider(format!(
                "failed to decode Seedream response: {error}; response body: {response_text}"
            ))
        })?;
    let image_data = response_body
        .data
        .into_iter()
        .find_map(|item| {
            item.b64_json
                .map(ImageData::Base64)
                .or_else(|| item.url.map(ImageData::Url))
        })
        .ok_or_else(|| {
            AppError::Provider("Seedream response did not include b64_json or url".to_string())
        })?;

    Ok(ImageResult {
        mime_type: "image/png".to_string(),
        data: image_data,
    })
}

#[async_trait]
impl AiProvider for SeedreamProvider {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError> {
        let body = SeedreamRequestBody {
            model: &request.model,
            prompt: &request.prompt,
            size: request.size.as_deref(),
            response_format: "b64_json",
            stream: false,
            watermark: false,
            image: None,
        };

        let response = self
            .client
            .post(format!("{}/images/generations", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".to_string());
            return Err(AppError::Provider(format!(
                "Seedream image generation failed ({status}): {message}"
            )));
        }

        let response_text = response.text().await?;
        parse_seedream_response(&response_text)
    }

    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError> {
        let images = request
            .image_paths
            .iter()
            .map(|path| path_to_data_url(path))
            .collect::<Result<Vec<_>, _>>()?;
        let body = SeedreamRequestBody {
            model: &request.model,
            prompt: &request.prompt,
            size: request.size.as_deref(),
            response_format: "b64_json",
            stream: false,
            watermark: false,
            image: Some(images),
        };

        let response = self
            .client
            .post(format!("{}/images/generations", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".to_string());
            return Err(AppError::Provider(format!(
                "Seedream image edit failed ({status}): {message}"
            )));
        }

        let response_text = response.text().await?;
        parse_seedream_response(&response_text)
    }
}
