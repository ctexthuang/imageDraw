use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, Url};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, time::Duration};
use tokio::time::sleep;

use super::provider::{AiProvider, ImageData, ImageEditRequest, ImageGenerateRequest, ImageResult};
use crate::AppError;

type HmacSha256 = Hmac<Sha256>;

pub struct TencentHunyuanProvider {
    client: Client,
    base_url: String,
    secret_id: String,
    secret_key: String,
}

struct TencentApiConfig {
    service: &'static str,
    version: &'static str,
    lite_action: &'static str,
    rapid_action: &'static str,
    submit_action: &'static str,
    query_action: &'static str,
}

impl TencentHunyuanProvider {
    pub fn new(base_url: String, api_key: String) -> Result<Self, AppError> {
        let (secret_id, secret_key) = parse_secret_pair(&api_key)?;
        Ok(Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            secret_id,
            secret_key,
        })
    }
}

fn parse_secret_pair(api_key: &str) -> Result<(String, String), AppError> {
    let (secret_id, secret_key) = api_key.split_once(':').ok_or_else(|| {
        AppError::Provider("腾讯云 API Key 需要填写为 SecretId:SecretKey".to_string())
    })?;
    let secret_id = secret_id.trim();
    let secret_key = secret_key.trim();
    if secret_id.is_empty() || secret_key.is_empty() {
        return Err(AppError::Provider(
            "腾讯云 API Key 需要填写为 SecretId:SecretKey".to_string(),
        ));
    }

    Ok((secret_id.to_string(), secret_key.to_string()))
}

fn hmac_sha256(key: &[u8], message: &str) -> Result<Vec<u8>, AppError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| AppError::Provider(format!("failed to create HMAC: {error}")))?;
    mac.update(message.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn host_from_base_url(base_url: &str) -> Result<String, AppError> {
    let url = Url::parse(base_url)
        .map_err(|error| AppError::Provider(format!("腾讯云 Base URL 不是有效 URL: {error}")))?;
    url.host_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Provider("腾讯云 Base URL 缺少 host".to_string()))
}

fn api_config(host: &str) -> TencentApiConfig {
    if host == "aiart.tencentcloudapi.com" {
        TencentApiConfig {
            service: "aiart",
            version: "2022-12-29",
            lite_action: "TextToImageLite",
            rapid_action: "TextToImageRapid",
            submit_action: "SubmitTextToImageJob",
            query_action: "QueryTextToImageJob",
        }
    } else {
        TencentApiConfig {
            service: "hunyuan",
            version: "2023-09-01",
            lite_action: "TextToImageLite",
            rapid_action: "TextToImageLite",
            submit_action: "SubmitHunyuanImageJob",
            query_action: "QueryHunyuanImageJob",
        }
    }
}

fn hunyuan_resolution(size: Option<&str>, lite: bool, has_reference: bool) -> Option<String> {
    let size = size?;
    let (width, height) = size.split_once('x')?;
    let width = width.parse::<f32>().ok()?;
    let height = height.parse::<f32>().ok()?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let ratio = width / height;
    let value = if (ratio - 1.0).abs() < 0.12 {
        "1024:1024"
    } else if (ratio - 0.75).abs() < 0.12 {
        "768:1024"
    } else if (ratio - 1.333).abs() < 0.12 {
        "1024:768"
    } else if ratio < 0.65 {
        if lite && !has_reference {
            "1080:1920"
        } else {
            "720:1280"
        }
    } else if ratio > 1.55 {
        if lite && !has_reference {
            "1920:1080"
        } else {
            "1280:720"
        }
    } else if width < height {
        "768:1024"
    } else {
        "1024:768"
    };

    if has_reference && !matches!(value, "1024:1024" | "768:1024" | "1024:768") {
        return None;
    }

    Some(value.to_string())
}

fn first_image_base64(image_paths: &[String]) -> Result<Option<String>, AppError> {
    image_paths
        .first()
        .map(|path| fs::read(path).map(|bytes| general_purpose::STANDARD.encode(bytes)))
        .transpose()
        .map_err(AppError::from)
}

fn parse_tencent_error(response: &Value) -> Option<String> {
    let error = response.get("Error")?;
    let code = error
        .get("Code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = error
        .get("Message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(format!("{code}: {message}"))
}

fn result_image_url(response: &Value) -> Option<String> {
    let result = response.get("ResultImage")?;
    if let Some(url) = result.as_str().filter(|value| value.starts_with("http")) {
        return Some(url.to_string());
    }

    result.as_array()?.first().and_then(|image| {
        image
            .as_str()
            .filter(|value| value.starts_with("http"))
            .map(str::to_string)
            .or_else(|| {
                image
                    .get("Url")
                    .or_else(|| image.get("URL"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    })
}

fn result_image_base64(response: &Value) -> Option<String> {
    let result = response.get("ResultImage")?;
    if let Some(value) = result.as_str().filter(|value| !value.starts_with("http")) {
        return Some(value.to_string());
    }

    result.as_array()?.first().and_then(|image| {
        image
            .as_str()
            .filter(|value| !value.starts_with("http"))
            .map(str::to_string)
            .or_else(|| {
                image
                    .get("Base64")
                    .or_else(|| image.get("ImageBase64"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    })
}

#[async_trait]
impl AiProvider for TencentHunyuanProvider {
    async fn generate_image(&self, request: ImageGenerateRequest) -> Result<ImageResult, AppError> {
        if request.model == "hunyuan-image-lite" {
            return self
                .run_text_to_image_lite(&request.prompt, request.size.as_deref())
                .await;
        }
        if request.model == "hunyuan-image-2.0" {
            return self
                .run_text_to_image_rapid(&request.prompt, request.size.as_deref(), &[])
                .await;
        }

        self.run_async_image_job(&request.prompt, request.size.as_deref(), &[])
            .await
    }

    async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageResult, AppError> {
        if request.model == "hunyuan-image-2.0" {
            return self
                .run_text_to_image_rapid(
                    &request.prompt,
                    request.size.as_deref(),
                    &request.image_paths,
                )
                .await;
        }

        self.run_async_image_job(
            &request.prompt,
            request.size.as_deref(),
            &request.image_paths,
        )
        .await
    }
}

impl TencentHunyuanProvider {
    async fn call(&self, action: &str, payload: Value) -> Result<Value, AppError> {
        let region = "ap-guangzhou";
        let host = host_from_base_url(&self.base_url)?;
        let config = api_config(&host);
        let timestamp = Utc::now().timestamp();
        let date = Utc
            .timestamp_opt(timestamp, 0)
            .single()
            .ok_or_else(|| AppError::Provider("invalid Tencent Cloud timestamp".to_string()))?
            .format("%Y-%m-%d")
            .to_string();
        let payload_text = serde_json::to_string(&payload).map_err(|error| {
            AppError::Provider(format!("failed to encode Tencent Cloud request: {error}"))
        })?;
        let content_type = "application/json; charset=utf-8";
        let signed_headers = "content-type;host";
        let canonical_headers = format!("content-type:{content_type}\nhost:{host}\n");
        let canonical_request = format!(
            "POST\n/\n\n{canonical_headers}\n{signed_headers}\n{}",
            sha256_hex(&payload_text)
        );
        let credential_scope = format!("{}/{}/tc3_request", date, config.service);
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
            sha256_hex(&canonical_request)
        );
        let secret_date = hmac_sha256(format!("TC3{}", self.secret_key).as_bytes(), &date)?;
        let secret_service = hmac_sha256(&secret_date, config.service)?;
        let secret_signing = hmac_sha256(&secret_service, "tc3_request")?;
        let signature = hex::encode(hmac_sha256(&secret_signing, &string_to_sign)?);
        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            self.secret_id
        );

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", authorization)
            .header("Content-Type", content_type)
            .header("Host", host)
            .header("X-TC-Action", action)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", config.version)
            .header("X-TC-Region", region)
            .body(payload_text)
            .send()
            .await?;
        let status = response.status();
        let response_text = response.text().await?;
        if !status.is_success() {
            return Err(AppError::Provider(format!(
                "Tencent Hunyuan request failed ({status}): {response_text}"
            )));
        }

        let response_body = serde_json::from_str::<Value>(&response_text).map_err(|error| {
            AppError::Provider(format!(
                "failed to decode Tencent Hunyuan response: {error}; response body: {response_text}"
            ))
        })?;
        let inner = response_body.get("Response").ok_or_else(|| {
            AppError::Provider(format!(
                "Tencent Hunyuan response missing Response: {response_text}"
            ))
        })?;
        if let Some(message) = parse_tencent_error(inner) {
            return Err(AppError::Provider(format!(
                "Tencent Hunyuan {action} failed: {message}"
            )));
        }

        Ok(inner.clone())
    }

    async fn run_text_to_image_lite(
        &self,
        prompt: &str,
        size: Option<&str>,
    ) -> Result<ImageResult, AppError> {
        let mut payload = json!({
            "Prompt": prompt,
            "RspImgType": "url",
            "LogoAdd": 0,
        });
        if let Some(resolution) = hunyuan_resolution(size, true, false) {
            payload["Resolution"] = json!(resolution);
        }
        let host = host_from_base_url(&self.base_url)?;
        let action = api_config(&host).lite_action;
        let response = self.call(action, payload).await?;
        let image_url = result_image_url(&response).ok_or_else(|| {
            AppError::Provider(format!(
                "Tencent Hunyuan TextToImageLite did not include image url: {response}"
            ))
        })?;

        Ok(ImageResult {
            mime_type: "image/png".to_string(),
            data: ImageData::Url(image_url),
        })
    }

    async fn run_text_to_image_rapid(
        &self,
        prompt: &str,
        size: Option<&str>,
        image_paths: &[String],
    ) -> Result<ImageResult, AppError> {
        let has_reference = !image_paths.is_empty();
        let mut payload = json!({
            "Prompt": prompt,
            "RspImgType": "url",
            "LogoAdd": 0,
        });
        if let Some(resolution) = hunyuan_resolution(size, false, has_reference) {
            payload["Resolution"] = json!(resolution);
        }
        if let Some(image_base64) = first_image_base64(image_paths)? {
            payload["Image"] = json!({
                "Base64": image_base64,
            });
        }
        let host = host_from_base_url(&self.base_url)?;
        let action = api_config(&host).rapid_action;
        let response = self.call(action, payload).await?;
        let image_url = result_image_url(&response).ok_or_else(|| {
            AppError::Provider(format!(
                "Tencent Hunyuan TextToImageRapid did not include image url: {response}"
            ))
        })?;

        Ok(ImageResult {
            mime_type: "image/png".to_string(),
            data: ImageData::Url(image_url),
        })
    }

    async fn run_async_image_job(
        &self,
        prompt: &str,
        size: Option<&str>,
        image_paths: &[String],
    ) -> Result<ImageResult, AppError> {
        let has_reference = !image_paths.is_empty();
        let mut payload = json!({
            "Prompt": prompt,
            "Num": 1,
            "Revise": 1,
            "LogoAdd": 0,
        });
        if let Some(resolution) = hunyuan_resolution(size, false, has_reference) {
            payload["Resolution"] = json!(resolution);
        }
        let host = host_from_base_url(&self.base_url)?;
        let config = api_config(&host);
        if let Some(image_base64) = first_image_base64(image_paths)? {
            if config.service == "aiart" {
                payload["Images"] = json!([image_base64]);
            } else {
                payload["ContentImage"] = json!({
                    "ImageBase64": image_base64,
                });
            }
        }
        let response = self.call(config.submit_action, payload).await?;
        let job_id = response
            .get("JobId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Provider(format!(
                    "Tencent Hunyuan submit response did not include JobId: {response}"
                ))
            })?;

        self.wait_image_job(job_id).await
    }

    async fn wait_image_job(&self, job_id: &str) -> Result<ImageResult, AppError> {
        for _ in 0..40 {
            sleep(Duration::from_secs(3)).await;
            let host = host_from_base_url(&self.base_url)?;
            let action = api_config(&host).query_action;
            let response = self.call(action, json!({ "JobId": job_id })).await?;
            let job_status = response
                .get("JobStatusCode")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if job_status == "5" {
                if let Some(image_base64) = result_image_base64(&response) {
                    return Ok(ImageResult {
                        mime_type: "image/png".to_string(),
                        data: ImageData::Base64(image_base64),
                    });
                }
                let image_url = result_image_url(&response).ok_or_else(|| {
                    AppError::Provider(format!(
                        "Tencent Hunyuan job succeeded but did not include image: {response}"
                    ))
                })?;
                return Ok(ImageResult {
                    mime_type: "image/png".to_string(),
                    data: ImageData::Url(image_url),
                });
            }
            if job_status == "4" {
                let message = response
                    .get("JobErrorMsg")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                return Err(AppError::Provider(format!(
                    "Tencent Hunyuan image job failed: {message}"
                )));
            }
        }

        Err(AppError::Provider(
            "Tencent Hunyuan image job polling timed out".to_string(),
        ))
    }
}
