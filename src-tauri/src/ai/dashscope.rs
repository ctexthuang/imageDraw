use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};
use tokio::time::sleep;

use super::provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult};
use crate::AppError;

pub struct DashScopeProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl DashScopeProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }
}

fn is_qwen_image_model(model: &str) -> bool {
    model.starts_with("qwen-image")
}

fn is_sync_multimodal_model(model: &str) -> bool {
    is_qwen_image_model(model) || model.starts_with("z-image")
}

fn dashscope_size(size: Option<&str>) -> Option<String> {
    size.map(|value| value.replace('x', "*"))
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

fn parse_dashscope_error(response_text: &str, fallback: &str) -> String {
    serde_json::from_str::<Value>(response_text)
        .ok()
        .and_then(|value| {
            let code = value
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if code.is_empty() && message.is_empty() {
                None
            } else {
                Some(format!("{code}: {message}"))
            }
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn image_url_from_choices(value: &Value) -> Option<String> {
    value
        .get("output")?
        .get("choices")?
        .as_array()?
        .iter()
        .flat_map(|choice| {
            choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|content| {
            content
                .get("image")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn image_url_from_results(value: &Value) -> Option<String> {
    value
        .get("output")?
        .get("results")?
        .as_array()?
        .iter()
        .find_map(|item| item.get("url").and_then(Value::as_str).map(str::to_string))
}

fn parse_image_url(value: &Value) -> Option<String> {
    image_url_from_choices(value).or_else(|| image_url_from_results(value))
}

fn build_messages(prompt: &str, image_paths: &[String]) -> Result<Vec<Value>, AppError> {
    let mut content = image_paths
        .iter()
        .map(|path| path_to_data_url(path).map(|image| json!({ "image": image })))
        .collect::<Result<Vec<_>, _>>()?;
    content.push(json!({ "text": prompt }));

    Ok(vec![json!({
        "role": "user",
        "content": content,
    })])
}

#[async_trait]
impl AiProvider for DashScopeProvider {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError> {
        if is_sync_multimodal_model(&request.model) {
            return self
                .run_qwen_image(
                    &request.prompt,
                    &request.model,
                    request.size.as_deref(),
                    &[],
                )
                .await;
        }

        self.run_async_image_generation(
            &request.prompt,
            &request.model,
            request.size.as_deref(),
            &[],
        )
        .await
    }

    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError> {
        if is_sync_multimodal_model(&request.model) {
            return self
                .run_qwen_image(
                    &request.prompt,
                    &request.model,
                    request.size.as_deref(),
                    &request.image_paths,
                )
                .await;
        }

        self.run_async_image_generation(
            &request.prompt,
            &request.model,
            request.size.as_deref(),
            &request.image_paths,
        )
        .await
    }
}

impl DashScopeProvider {
    async fn run_qwen_image(
        &self,
        prompt: &str,
        model: &str,
        size: Option<&str>,
        image_paths: &[String],
    ) -> Result<ImageResult, AppError> {
        let mut parameters = json!({
            "n": 1,
            "watermark": false,
            "prompt_extend": true,
        });
        if is_qwen_image_model(model) {
            parameters["negative_prompt"] = json!(" ");
        }
        if let Some(size) = dashscope_size(size) {
            parameters["size"] = json!(size);
        }

        let body = json!({
            "model": model,
            "input": {
                "messages": build_messages(prompt, image_paths)?,
            },
            "parameters": parameters,
        });
        let response = self
            .client
            .post(format!(
                "{}/services/aigc/multimodal-generation/generation",
                self.base_url
            ))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let response_text = response.text().await?;
        if !status.is_success() {
            return Err(AppError::Provider(format!(
                "DashScope image generation failed ({status}): {}",
                parse_dashscope_error(&response_text, &response_text)
            )));
        }

        let response_body = serde_json::from_str::<Value>(&response_text).map_err(|error| {
            AppError::Provider(format!(
                "failed to decode DashScope response: {error}; response body: {response_text}"
            ))
        })?;
        let image_url = parse_image_url(&response_body).ok_or_else(|| {
            AppError::Provider(format!(
                "DashScope response did not include image url: {response_text}"
            ))
        })?;

        Ok(ImageResult {
            mime_type: "image/png".to_string(),
            data: ImageData::Url(image_url),
        })
    }

    async fn run_async_image_generation(
        &self,
        prompt: &str,
        model: &str,
        size: Option<&str>,
        image_paths: &[String],
    ) -> Result<ImageResult, AppError> {
        let mut parameters = json!({
            "n": 1,
            "watermark": false,
        });
        if let Some(size) = dashscope_size(size) {
            parameters["size"] = json!(size);
        }
        if model.starts_with("wan2.7-image") {
            parameters["thinking_mode"] = json!(true);
        } else {
            parameters["prompt_extend"] = json!(true);
        }

        let body = json!({
            "model": model,
            "input": {
                "messages": build_messages(prompt, image_paths)?,
            },
            "parameters": parameters,
        });
        let response = self
            .client
            .post(format!(
                "{}/services/aigc/image-generation/generation",
                self.base_url
            ))
            .bearer_auth(&self.api_key)
            .header("X-DashScope-Async", "enable")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let response_text = response.text().await?;
        if !status.is_success() {
            return Err(AppError::Provider(format!(
                "DashScope async image task failed ({status}): {}",
                parse_dashscope_error(&response_text, &response_text)
            )));
        }

        let response_body = serde_json::from_str::<Value>(&response_text).map_err(|error| {
            AppError::Provider(format!(
                "failed to decode DashScope task response: {error}; response body: {response_text}"
            ))
        })?;
        let task_id = response_body
            .get("output")
            .and_then(|output| output.get("task_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Provider(format!(
                    "DashScope task response did not include task_id: {response_text}"
                ))
            })?;

        self.wait_async_task(task_id).await
    }

    async fn wait_async_task(&self, task_id: &str) -> Result<ImageResult, AppError> {
        for _ in 0..40 {
            sleep(Duration::from_secs(3)).await;
            let response = self
                .client
                .get(format!("{}/tasks/{task_id}", self.base_url))
                .bearer_auth(&self.api_key)
                .send()
                .await?;
            let status = response.status();
            let response_text = response.text().await?;
            if !status.is_success() {
                return Err(AppError::Provider(format!(
                    "DashScope task polling failed ({status}): {}",
                    parse_dashscope_error(&response_text, &response_text)
                )));
            }

            let response_body = serde_json::from_str::<Value>(&response_text).map_err(|error| {
                AppError::Provider(format!(
                    "failed to decode DashScope task result: {error}; response body: {response_text}"
                ))
            })?;
            let task_status = response_body
                .get("output")
                .and_then(|output| output.get("task_status"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if task_status == "SUCCEEDED" {
                let image_url = parse_image_url(&response_body).ok_or_else(|| {
                    AppError::Provider(format!(
                        "DashScope task succeeded but did not include image url: {response_text}"
                    ))
                })?;
                return Ok(ImageResult {
                    mime_type: "image/png".to_string(),
                    data: ImageData::Url(image_url),
                });
            }
            if matches!(task_status, "FAILED" | "CANCELED" | "UNKNOWN") {
                return Err(AppError::Provider(format!(
                    "DashScope task ended with status {task_status}: {}",
                    parse_dashscope_error(&response_text, &response_text)
                )));
            }
        }

        Err(AppError::Provider(
            "DashScope image task polling timed out".to_string(),
        ))
    }
}
