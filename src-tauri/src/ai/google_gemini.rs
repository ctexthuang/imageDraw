use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde_json::{json, Value};
use std::{fs, path::Path};

use super::provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult};
use crate::AppError;

pub struct GoogleGeminiProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl GoogleGeminiProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
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

fn size_to_aspect_ratio(size: Option<&str>) -> Option<&'static str> {
    let (width, height) = size?.split_once('x')?;
    let width = width.parse::<f32>().ok()?;
    let height = height.parse::<f32>().ok()?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let ratio = width / height;
    if (ratio - 1.0).abs() < 0.08 {
        Some("1:1")
    } else if (ratio - 16.0 / 9.0).abs() < 0.12 {
        Some("16:9")
    } else if (ratio - 9.0 / 16.0).abs() < 0.12 {
        Some("9:16")
    } else if (ratio - 4.0 / 3.0).abs() < 0.12 {
        Some("4:3")
    } else if (ratio - 3.0 / 4.0).abs() < 0.12 {
        Some("3:4")
    } else {
        None
    }
}

fn image_part(path: &str) -> Result<Value, AppError> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "inline_data": {
            "mime_type": mime_for_path(path),
            "data": general_purpose::STANDARD.encode(bytes),
        }
    }))
}

fn inline_data_from_part(part: &Value) -> Option<(&str, &str)> {
    let inline_data = part.get("inlineData").or_else(|| part.get("inline_data"))?;
    let data = inline_data.get("data").and_then(Value::as_str)?;
    let mime_type = inline_data
        .get("mimeType")
        .or_else(|| inline_data.get("mime_type"))
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    Some((mime_type, data))
}

fn parse_gemini_response(response_text: &str) -> Result<ImageResult, AppError> {
    let response_body = serde_json::from_str::<Value>(response_text).map_err(|error| {
        AppError::Provider(format!(
            "failed to decode Gemini image response: {error}; response body: {response_text}"
        ))
    })?;
    let parts = response_body
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|candidate| {
            candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        });

    for part in parts {
        if let Some((mime_type, data)) = inline_data_from_part(part) {
            return Ok(ImageResult {
                mime_type: mime_type.to_string(),
                data: ImageData::Base64(data.to_string()),
            });
        }
    }

    Err(AppError::Provider(format!(
        "Gemini response did not include image data: {response_text}"
    )))
}

#[async_trait]
impl AiProvider for GoogleGeminiProvider {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError> {
        self.generate_with_parts(
            &request.prompt,
            &request.model,
            request.size.as_deref(),
            &[],
        )
        .await
    }

    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError> {
        self.generate_with_parts(
            &request.prompt,
            &request.model,
            request.size.as_deref(),
            &request.image_paths,
        )
        .await
    }
}

impl GoogleGeminiProvider {
    async fn generate_with_parts(
        &self,
        prompt: &str,
        model: &str,
        size: Option<&str>,
        image_paths: &[String],
    ) -> Result<ImageResult, AppError> {
        let mut parts = vec![json!({ "text": prompt })];
        parts.extend(
            image_paths
                .iter()
                .map(|path| image_part(path))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let mut generation_config = json!({
            "responseModalities": ["TEXT", "IMAGE"],
        });
        if let Some(aspect_ratio) = size_to_aspect_ratio(size) {
            generation_config["imageConfig"] = json!({
                "aspectRatio": aspect_ratio,
            });
        }

        let body = json!({
            "contents": [{
                "role": "user",
                "parts": parts,
            }],
            "generationConfig": generation_config,
        });
        let response = self
            .client
            .post(format!("{}/models/{model}:generateContent", self.base_url))
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let response_text = response.text().await?;
        if !status.is_success() {
            return Err(AppError::Provider(format!(
                "Gemini image generation failed ({status}): {response_text}"
            )));
        }

        parse_gemini_response(&response_text)
    }
}
