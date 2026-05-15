use std::{collections::HashMap, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    sync::{oneshot, Mutex},
};

use crate::state::{AppState, UpdateDownloadControl};

const GITHUB_REPO: &str = "ctexthuang/imageDraw";
const GITHUB_RELEASES_URL: &str = "https://github.com/ctexthuang/imageDraw/releases";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ctexthuang/imageDraw/releases/latest";
const GITHUB_LATEST_RELEASE_URL: &str = "https://github.com/ctexthuang/imageDraw/releases/latest";
const UPDATE_DOWNLOAD_PROGRESS_EVENT: &str = "update-download-progress";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize)]
pub struct ReleaseAssetInfo {
    name: String,
    download_url: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    latest_tag: String,
    release_name: String,
    release_url: String,
    release_notes: String,
    published_at: Option<String>,
    has_update: bool,
    asset: Option<ReleaseAssetInfo>,
}

#[derive(Debug, Serialize)]
pub struct UpdateDownloadInfo {
    file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateDownloadProgress {
    file_name: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let client = github_client(&current_version);
    let release = match fetch_latest_release_from_api(&client).await {
        Ok(release) => release,
        Err(api_error) => {
            fetch_latest_release_from_web(&client)
                .await
                .map_err(|fallback_error| {
                    format!("{api_error}；网页备用通道也失败：{fallback_error}")
                })?
        }
    };

    Ok(update_info_from_release(current_version, release))
}

#[tauri::command]
pub async fn open_update_url(app: AppHandle, url: String) -> Result<(), String> {
    if !is_allowed_release_url(&url) {
        return Err("只允许打开本项目的 GitHub Release 地址".to_string());
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn download_update_asset(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    file_name: String,
) -> Result<UpdateDownloadInfo, String> {
    if !is_allowed_release_url(&url) {
        return Err("只允许下载本项目的 GitHub Release 安装包".to_string());
    }

    let safe_file_name = sanitize_file_name(&file_name);
    if safe_file_name.is_empty() {
        return Err("安装包文件名为空".to_string());
    }

    let (file_path, partial_path) = update_download_paths(&app, &safe_file_name).await?;
    let partial_bytes = tokio::fs::metadata(&partial_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let (control_sender, mut control_receiver) = oneshot::channel();
    if let Some(previous_sender) = state
        .update_download_controls
        .lock()
        .await
        .insert(safe_file_name.clone(), control_sender)
    {
        let _ = previous_sender.send(UpdateDownloadControl::Cancel);
    }
    let control_guard = UpdateDownloadControlGuard {
        controls: state.update_download_controls.clone(),
        file_name: safe_file_name.clone(),
    };

    let current_version = app.package_info().version.to_string();
    let mut request = github_client(&current_version).get(&url);
    if partial_bytes > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={partial_bytes}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("下载安装包失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("下载安装包响应异常：{error}"))?;
    let is_resuming =
        partial_bytes > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let downloaded_base = if is_resuming { partial_bytes } else { 0 };
    let total_bytes = response
        .content_length()
        .map(|length| length + downloaded_base);
    emit_download_progress(&app, &safe_file_name, downloaded_base, total_bytes);

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(is_resuming)
        .truncate(!is_resuming)
        .open(&partial_path)
        .await
        .map_err(|error| format!("创建安装包文件失败：{error}"))?;
    let mut downloaded_bytes = downloaded_base;
    loop {
        tokio::select! {
            action = &mut control_receiver => {
                file.flush()
                    .await
                    .map_err(|error| format!("保存安装包失败：{error}"))?;
                match action.unwrap_or(UpdateDownloadControl::Cancel) {
                    UpdateDownloadControl::Pause => {
                        drop(control_guard);
                        return Err("下载已暂停".to_string());
                    }
                    UpdateDownloadControl::Cancel => {
                        let _ = tokio::fs::remove_file(&partial_path).await;
                        drop(control_guard);
                        return Err("下载已取消".to_string());
                    }
                }
            }
            chunk = response.chunk() => {
                let Some(chunk) = chunk
                    .map_err(|error| format!("读取安装包数据失败：{error}"))?
                else {
                    break;
                };
                file.write_all(&chunk)
                    .await
                    .map_err(|error| format!("写入安装包失败：{error}"))?;
                downloaded_bytes += u64::try_from(chunk.len()).unwrap_or(0);
                emit_download_progress(&app, &safe_file_name, downloaded_bytes, total_bytes);
            }
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("保存安装包失败：{error}"))?;
    emit_download_progress(&app, &safe_file_name, downloaded_bytes, total_bytes);

    if tokio::fs::metadata(&file_path).await.is_ok() {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|error| format!("替换旧安装包失败：{error}"))?;
    }
    tokio::fs::rename(&partial_path, &file_path)
        .await
        .map_err(|error| format!("完成安装包保存失败：{error}"))?;

    drop(control_guard);
    app.opener()
        .open_path(file_path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| format!("打开安装包失败：{error}"))?;

    Ok(UpdateDownloadInfo {
        file_path: file_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn pause_update_download(
    state: State<'_, AppState>,
    file_name: String,
) -> Result<bool, String> {
    control_update_download(state, file_name, UpdateDownloadControl::Pause).await
}

#[tauri::command]
pub async fn cancel_update_download(
    app: AppHandle,
    state: State<'_, AppState>,
    file_name: String,
) -> Result<bool, String> {
    let safe_file_name = sanitize_file_name(&file_name);
    let sent = control_update_download_by_safe_name(
        &state,
        &safe_file_name,
        UpdateDownloadControl::Cancel,
    )
    .await;
    if !sent && !safe_file_name.is_empty() {
        let (_file_path, partial_path) = update_download_paths(&app, &safe_file_name).await?;
        let _ = tokio::fs::remove_file(partial_path).await;
    }
    Ok(sent)
}

async fn control_update_download(
    state: State<'_, AppState>,
    file_name: String,
    action: UpdateDownloadControl,
) -> Result<bool, String> {
    let safe_file_name = sanitize_file_name(&file_name);
    Ok(control_update_download_by_safe_name(&state, &safe_file_name, action).await)
}

async fn control_update_download_by_safe_name(
    state: &State<'_, AppState>,
    safe_file_name: &str,
    action: UpdateDownloadControl,
) -> bool {
    if safe_file_name.is_empty() {
        return false;
    }
    state
        .update_download_controls
        .lock()
        .await
        .remove(safe_file_name)
        .map(|sender| sender.send(action).is_ok())
        .unwrap_or(false)
}

struct UpdateDownloadControlGuard {
    controls: Arc<Mutex<HashMap<String, oneshot::Sender<UpdateDownloadControl>>>>,
    file_name: String,
}

impl Drop for UpdateDownloadControlGuard {
    fn drop(&mut self) {
        let controls = self.controls.clone();
        let file_name = self.file_name.clone();
        tauri::async_runtime::spawn(async move {
            controls.lock().await.remove(&file_name);
        });
    }
}

async fn update_download_paths(
    app: &AppHandle,
    safe_file_name: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let downloads_dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir().map(|path| path.join("updates")))
        .map_err(|error| format!("获取下载目录失败：{error}"))?;
    tokio::fs::create_dir_all(&downloads_dir)
        .await
        .map_err(|error| format!("创建下载目录失败：{error}"))?;
    let file_path = downloads_dir.join(safe_file_name);
    let partial_path = downloads_dir.join(format!("{safe_file_name}.download"));
    Ok((file_path, partial_path))
}

fn github_client(current_version: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(format!("{}/{}", GITHUB_REPO, current_version))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn emit_download_progress(
    app: &AppHandle,
    file_name: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let _ = app.emit(
        UPDATE_DOWNLOAD_PROGRESS_EVENT,
        UpdateDownloadProgress {
            file_name: file_name.to_string(),
            downloaded_bytes,
            total_bytes,
        },
    );
}

async fn fetch_latest_release_from_api(client: &reqwest::Client) -> Result<GithubRelease, String> {
    client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("请求 GitHub Release 失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub Release 响应异常：{error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("解析 GitHub Release 失败：{error}"))
}

async fn fetch_latest_release_from_web(client: &reqwest::Client) -> Result<GithubRelease, String> {
    let latest_response = client
        .get(GITHUB_LATEST_RELEASE_URL)
        .send()
        .await
        .map_err(|error| format!("请求 GitHub Releases 页面失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub Releases 页面响应异常：{error}"))?;
    let latest_url = latest_response.url().to_string();
    let tag_name = latest_tag_from_url(&latest_url)
        .ok_or_else(|| "无法从 GitHub Releases 页面识别最新版本".to_string())?;

    let expanded_assets_url = format!("{GITHUB_RELEASES_URL}/expanded_assets/{tag_name}");
    let assets_html = client
        .get(&expanded_assets_url)
        .send()
        .await
        .map_err(|error| format!("请求 GitHub Release 资源列表失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub Release 资源列表响应异常：{error}"))?
        .text()
        .await
        .map_err(|error| format!("读取 GitHub Release 资源列表失败：{error}"))?;

    let assets = parse_release_assets_from_html(&assets_html);

    Ok(GithubRelease {
        tag_name: tag_name.clone(),
        name: Some(tag_name.clone()),
        html_url: format!("{GITHUB_RELEASES_URL}/tag/{tag_name}"),
        body: Some(String::new()),
        published_at: None,
        assets,
    })
}

fn update_info_from_release(current_version: String, release: GithubRelease) -> UpdateInfo {
    let latest_version = normalize_version(&release.tag_name);
    let has_update = is_newer_version(&latest_version, &current_version);
    let asset = select_platform_asset(&release.assets).map(|asset| ReleaseAssetInfo {
        name: asset.name,
        download_url: asset.browser_download_url,
    });

    UpdateInfo {
        current_version,
        latest_version,
        latest_tag: release.tag_name.clone(),
        release_name: release.name.unwrap_or(release.tag_name),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        has_update,
        asset,
    }
}

fn normalize_version(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    compare_versions(latest, current).is_gt()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = parse_version_parts(left);
    let right_parts = parse_version_parts(right);
    left_parts.cmp(&right_parts)
}

fn parse_version_parts(version: &str) -> [u64; 3] {
    let mut parts = [0, 0, 0];
    for (index, part) in version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-'])
        .take(3)
        .enumerate()
    {
        parts[index] = part.parse::<u64>().unwrap_or(0);
    }
    parts
}

fn select_platform_asset(assets: &[GithubReleaseAsset]) -> Option<GithubReleaseAsset> {
    assets
        .iter()
        .filter(|asset| matches_current_platform(&asset.name))
        .max_by_key(|asset| asset_priority(&asset.name))
        .cloned()
        .or_else(|| {
            assets
                .iter()
                .filter(|asset| matches_current_os(&asset.name))
                .max_by_key(|asset| asset_priority(&asset.name))
                .cloned()
        })
}

fn matches_current_platform(name: &str) -> bool {
    matches_current_os(name) && matches_current_arch(name)
}

fn matches_current_os(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    if cfg!(target_os = "macos") {
        normalized.contains("darwin") || normalized.contains("macos")
    } else if cfg!(target_os = "windows") {
        normalized.contains("windows") || normalized.contains("win32")
    } else {
        normalized.contains("linux")
    }
}

fn matches_current_arch(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    if cfg!(target_arch = "aarch64") {
        normalized.contains("aarch64") || normalized.contains("arm64")
    } else if cfg!(target_arch = "x86_64") {
        normalized.contains("x86_64") || normalized.contains("x64") || normalized.contains("amd64")
    } else {
        true
    }
}

fn asset_priority(name: &str) -> i32 {
    let normalized = name.to_ascii_lowercase();
    if cfg!(target_os = "macos") && normalized.ends_with(".dmg") {
        return 40;
    }
    if cfg!(target_os = "windows") && normalized.ends_with(".exe") {
        return 40;
    }
    if cfg!(target_os = "windows") && normalized.ends_with(".msi") {
        return 30;
    }
    if normalized.ends_with(".app.tar.gz") {
        return 20;
    }
    10
}

fn is_allowed_release_url(url: &str) -> bool {
    url == GITHUB_RELEASES_URL
        || url.starts_with(&format!("{GITHUB_RELEASES_URL}/"))
        || url.starts_with(&format!("{GITHUB_RELEASES_URL}/download/"))
}

fn latest_tag_from_url(url: &str) -> Option<String> {
    let marker = "/releases/tag/";
    let tag = url.split(marker).nth(1)?;
    let tag = tag.split(['?', '#']).next()?.trim_matches('/');
    if tag.is_empty() {
        None
    } else {
        Some(percent_decode(tag))
    }
}

fn parse_release_assets_from_html(html: &str) -> Vec<GithubReleaseAsset> {
    let mut assets: Vec<GithubReleaseAsset> = Vec::new();
    let href_prefix = format!("/{GITHUB_REPO}/releases/download/");
    for href in html
        .split("href=\"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next())
    {
        if !href.starts_with(&href_prefix) {
            continue;
        }
        let url = format!("https://github.com{href}");
        let Some(name) = href.split('/').last().map(percent_decode) else {
            continue;
        };
        if name.is_empty() || assets.iter().any(|asset| asset.name == name) {
            continue;
        }
        assets.push(GithubReleaseAsset {
            name,
            browser_download_url: url,
        });
    }
    assets
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(decoded) = u8::from_str_radix(hex, 16) {
                    output.push(decoded);
                    index += 3;
                    continue;
                }
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn sanitize_file_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        .collect::<String>()
        .trim()
        .to_string()
}
