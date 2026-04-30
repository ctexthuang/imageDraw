use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const GITHUB_REPO: &str = "ctexthuang/imageDraw";
const GITHUB_RELEASES_URL: &str = "https://github.com/ctexthuang/imageDraw/releases";
const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ctexthuang/imageDraw/releases/latest";

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

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<UpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let release = reqwest::Client::new()
        .get(GITHUB_LATEST_RELEASE_API)
        .header(
            reqwest::header::USER_AGENT,
            format!("{}/{}", GITHUB_REPO, current_version),
        )
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("请求 GitHub Release 失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub Release 响应异常：{error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("解析 GitHub Release 失败：{error}"))?;

    let latest_version = normalize_version(&release.tag_name);
    let has_update = is_newer_version(&latest_version, &current_version);
    let asset = select_platform_asset(&release.assets).map(|asset| ReleaseAssetInfo {
        name: asset.name,
        download_url: asset.browser_download_url,
    });

    Ok(UpdateInfo {
        current_version,
        latest_version,
        latest_tag: release.tag_name.clone(),
        release_name: release.name.unwrap_or(release.tag_name),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        has_update,
        asset,
    })
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
