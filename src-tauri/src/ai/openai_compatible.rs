use async_trait::async_trait;
use std::{fs, path::Path, time::Duration};

use reqwest::{multipart, Client, Error as ReqwestError, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use super::provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult};
use crate::AppError;

pub struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

const MAX_IMAGE_REQUEST_ATTEMPTS: usize = 3;

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

fn parse_image_response(response_text: &str) -> Result<ImageResult, AppError> {
    if response_text.trim_start().starts_with("<!doctype html")
        || response_text.trim_start().starts_with("<html")
    {
        return Err(AppError::Provider(
            "provider returned an HTML page, not an API JSON response. Please use the API base URL, usually ending with /v1, not the gateway website URL.".to_string(),
        ));
    }

    let response_body: ImageResponseBody =
        serde_json::from_str(response_text).map_err(|error| {
            AppError::Provider(format!(
                "failed to decode image response: {error}; response body: {response_text}"
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
            AppError::Provider("image response did not include b64_json or url".to_string())
        })?;

    Ok(ImageResult {
        mime_type: "image/png".to_string(),
        data: image_data,
    })
}

fn is_retryable_image_error(status: StatusCode, message: &str) -> bool {
    let normalized_message = message.to_ascii_lowercase();
    matches!(
        status,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    ) || normalized_message.contains("upstream_error")
}

fn is_retryable_send_error(error: &ReqwestError) -> bool {
    error.is_connect() || error.is_timeout()
}

async fn wait_before_retry(attempt: usize) {
    sleep(Duration::from_millis(500 * attempt as u64)).await;
}

fn build_image_edit_form(request: &ImageEditRequest) -> Result<multipart::Form, AppError> {
    let mut form = multipart::Form::new()
        .text("model", request.model.clone())
        .text("prompt", request.prompt.clone())
        .text("response_format", "b64_json");

    if let Some(size) = &request.size {
        form = form.text("size", size.clone());
    }
    if let Some(quality) = &request.quality {
        form = form.text("quality", quality.clone());
    }

    for image_path in &request.image_paths {
        let bytes = fs::read(image_path)?;
        let file_name = Path::new(image_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image.png")
            .to_string();
        let mime = match Path::new(image_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            _ => "image/png",
        };
        let part = multipart::Part::bytes(bytes)
            .file_name(file_name)
            .mime_str(mime)?;
        form = form.part("image[]", part);
    }

    Ok(form)
}

#[derive(Debug, Serialize)]
struct ImageRequestBody<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<&'a str>,
    response_format: &'a str,
}

#[derive(Debug, Deserialize)]
struct ImageResponseBody {
    data: Vec<ImageResponseItem>,
}

#[derive(Debug, Deserialize)]
struct ImageResponseItem {
    b64_json: Option<String>,
    url: Option<String>,
}

#[async_trait]
impl AiProvider for OpenAiCompatibleProvider {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError> {
        for attempt in 1..=MAX_IMAGE_REQUEST_ATTEMPTS {
            let body = ImageRequestBody {
                model: &request.model,
                prompt: &request.prompt,
                size: request.size.as_deref(),
                quality: request.quality.as_deref(),
                response_format: "b64_json",
            };

            let response = match self
                .client
                .post(format!("{}/images/generations", self.base_url))
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error)
                    if attempt < MAX_IMAGE_REQUEST_ATTEMPTS && is_retryable_send_error(&error) =>
                {
                    wait_before_retry(attempt).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            let status = response.status();
            if status.is_success() {
                let response_text = response.text().await?;
                return parse_image_response(&response_text);
            }

            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".to_string());
            if attempt < MAX_IMAGE_REQUEST_ATTEMPTS && is_retryable_image_error(status, &message) {
                wait_before_retry(attempt).await;
                continue;
            }

            return Err(AppError::Provider(format!(
                "image generation failed ({status}) after {attempt} attempt(s): {message}"
            )));
        }

        unreachable!("image request retry loop always returns")
    }

    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError> {
        for attempt in 1..=MAX_IMAGE_REQUEST_ATTEMPTS {
            let form = build_image_edit_form(&request)?;
            let response = match self
                .client
                .post(format!("{}/images/edits", self.base_url))
                .bearer_auth(&self.api_key)
                .multipart(form)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error)
                    if attempt < MAX_IMAGE_REQUEST_ATTEMPTS && is_retryable_send_error(&error) =>
                {
                    wait_before_retry(attempt).await;
                    continue;
                }
                Err(error) => return Err(error.into()),
            };

            let status = response.status();
            if status.is_success() {
                let response_text = response.text().await?;
                return parse_image_response(&response_text);
            }

            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "request failed".to_string());
            if attempt < MAX_IMAGE_REQUEST_ATTEMPTS && is_retryable_image_error(status, &message) {
                wait_before_retry(attempt).await;
                continue;
            }

            return Err(AppError::Provider(format!(
                "image edit failed ({status}) after {attempt} attempt(s): {message}"
            )));
        }

        unreachable!("image request retry loop always returns")
    }
}
