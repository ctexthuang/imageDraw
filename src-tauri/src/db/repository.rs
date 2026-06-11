use std::collections::HashSet;

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::models::{
    CreateGenerationTaskInput, GeneratedImageRecord, GenerationTask, ImageAsset,
    LegacyGeneratedImageInput, ProviderConfig, ProviderSecret, UpsertProviderInput,
};
use crate::AppError;

const LEGACY_HISTORY_PROVIDER_ID: &str = "legacy-local-history";
const LEGACY_HISTORY_PROVIDER_NAME: &str = "旧版本地历史";
const LEGACY_HISTORY_PROMPT: &str = "旧版本地图片导入：原提示词不可恢复";
const LEGACY_HISTORY_MODEL: &str = "legacy-local";

fn normalize_workspace(value: Option<String>) -> String {
    match value.as_deref() {
        Some("poster") => "poster".to_string(),
        _ => "generate".to_string(),
    }
}

pub async fn backfill_legacy_generated_images(
    pool: &SqlitePool,
    images: Vec<LegacyGeneratedImageInput>,
) -> Result<usize, AppError> {
    if images.is_empty() {
        return Ok(0);
    }

    let mut existing_paths: HashSet<String> = sqlx::query_scalar(
        r#"
        SELECT file_path
        FROM image_assets
        "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    let mut imported_count = 0;
    let mut legacy_provider_ready = false;
    for image in images {
        if existing_paths.contains(&image.file_path) {
            continue;
        }

        if !legacy_provider_ready {
            ensure_legacy_history_provider(pool).await?;
            legacy_provider_ready = true;
        }

        let task_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO generation_tasks (
                id, provider_id, task_type, prompt, model, size, quality,
                workspace, status, created_at, updated_at, finished_at
            ) VALUES (?1, ?2, 'legacy_import', ?3, ?4, NULL, NULL, 'generate', 'completed', ?5, ?5, ?5)
            "#,
        )
        .bind(&task_id)
        .bind(LEGACY_HISTORY_PROVIDER_ID)
        .bind(LEGACY_HISTORY_PROMPT)
        .bind(LEGACY_HISTORY_MODEL)
        .bind(&image.created_at)
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO image_assets (
                id, task_id, file_path, mime_type, file_size, source_type, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'generated', ?6)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(task_id)
        .bind(&image.file_path)
        .bind(image.mime_type)
        .bind(image.file_size)
        .bind(image.created_at)
        .execute(pool)
        .await?;

        existing_paths.insert(image.file_path);
        imported_count += 1;
    }

    Ok(imported_count)
}

async fn ensure_legacy_history_provider(pool: &SqlitePool) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO providers (
            id, name, kind, base_url, capabilities, enabled, created_at, updated_at
        ) VALUES (?1, ?2, 'legacy', 'local://history', '{}', 0, ?3, ?3)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(LEGACY_HISTORY_PROVIDER_ID)
    .bind(LEGACY_HISTORY_PROVIDER_NAME)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_providers(pool: &SqlitePool) -> Result<Vec<ProviderConfig>, AppError> {
    let providers = sqlx::query_as::<_, ProviderConfig>(
        r#"
        SELECT
            id,
            name,
            kind,
            base_url,
            api_key_encrypted AS api_key,
            text_model,
            image_model,
            capabilities,
            enabled != 0 AS enabled
        FROM providers
        WHERE enabled != 0
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(providers)
}

pub async fn upsert_provider(
    pool: &SqlitePool,
    input: UpsertProviderInput,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    let existing: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT api_key_encrypted, capabilities
        FROM providers
        WHERE id = ?1
        "#,
    )
    .bind(&input.id)
    .fetch_optional(pool)
    .await?;
    let api_key = match input.api_key {
        Some(key) if key.trim().is_empty() => None,
        Some(key) => Some(key),
        None => existing.as_ref().and_then(|item| item.0.clone()),
    };
    let capabilities = input
        .capabilities
        .filter(|value| !value.trim().is_empty())
        .or_else(|| existing.and_then(|item| item.1))
        .unwrap_or_else(|| {
            r#"{"responses_api":true,"images_api":true,"chat_completions":true,"image_edit":true,"image_models":[],"selected_image_models":[]}"#.to_string()
        });

    sqlx::query(
        r#"
        INSERT INTO providers (
            id, name, kind, base_url, api_key_encrypted, text_model, image_model,
            capabilities, enabled, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            kind = excluded.kind,
            base_url = excluded.base_url,
            api_key_encrypted = excluded.api_key_encrypted,
            text_model = excluded.text_model,
            image_model = excluded.image_model,
            capabilities = excluded.capabilities,
            enabled = excluded.enabled,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(input.id)
    .bind(input.name)
    .bind(input.kind)
    .bind(input.base_url)
    .bind(api_key)
    .bind(input.text_model)
    .bind(input.image_model)
    .bind(capabilities)
    .bind(input.enabled)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_provider_secret(pool: &SqlitePool, id: &str) -> Result<ProviderSecret, AppError> {
    let provider = sqlx::query_as::<_, ProviderSecret>(
        r#"
        SELECT id, name, kind, base_url, api_key_encrypted, image_model, enabled != 0 AS enabled
        FROM providers
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(provider)
}

pub async fn delete_provider(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let reference_count: i64 = sqlx::query_scalar(
        r#"
        SELECT
            (SELECT COUNT(*) FROM generation_tasks WHERE provider_id = ?1) +
            (SELECT COUNT(*) FROM conversations WHERE provider_id = ?1) +
            (SELECT COUNT(*) FROM ai_request_logs WHERE provider_id = ?1)
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    if reference_count > 0 {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE providers
            SET enabled = 0, updated_at = ?2
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .bind(now)
        .execute(pool)
        .await?;
        return Ok(());
    }

    sqlx::query("DELETE FROM providers WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn create_generation_task(
    pool: &SqlitePool,
    input: CreateGenerationTaskInput,
) -> Result<GenerationTask, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let status = "pending".to_string();
    let workspace = normalize_workspace(input.workspace);

    sqlx::query(
        r#"
        INSERT INTO generation_tasks (
            id, provider_id, task_type, prompt, model, size, quality, workspace, status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
        "#,
    )
    .bind(&id)
    .bind(&input.provider_id)
    .bind(&input.task_type)
    .bind(&input.prompt)
    .bind(&input.model)
    .bind(&input.size)
    .bind(&input.quality)
    .bind(&workspace)
    .bind(&status)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(GenerationTask {
        id,
        provider_id: input.provider_id,
        task_type: input.task_type,
        prompt: input.prompt,
        model: input.model,
        size: input.size,
        quality: input.quality,
        workspace,
        status,
        created_at: now,
    })
}

pub async fn mark_generation_task_completed(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE generation_tasks
        SET status = 'completed', updated_at = ?2, finished_at = ?2
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_generation_task_failed(
    pool: &SqlitePool,
    id: &str,
    error_message: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        UPDATE generation_tasks
        SET status = 'failed', error_message = ?2, updated_at = ?3, finished_at = ?3
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(error_message)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_image_asset(
    pool: &SqlitePool,
    task_id: &str,
    file_path: &str,
    mime_type: &str,
    file_size: i64,
    source_type: &str,
) -> Result<ImageAsset, AppError> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO image_assets (
            id, task_id, file_path, mime_type, file_size, source_type, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&id)
    .bind(task_id)
    .bind(file_path)
    .bind(mime_type)
    .bind(file_size)
    .bind(source_type)
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(ImageAsset {
        id,
        task_id: Some(task_id.to_string()),
        file_path: file_path.to_string(),
        mime_type: Some(mime_type.to_string()),
        file_size: Some(file_size),
        source_type: source_type.to_string(),
        created_at: now,
    })
}

pub async fn update_image_asset_paths(
    pool: &SqlitePool,
    moved_paths: &[(String, String)],
) -> Result<(), AppError> {
    for (old_path, new_path) in moved_paths {
        sqlx::query(
            r#"
            UPDATE image_assets
            SET file_path = ?2
            WHERE file_path = ?1
            "#,
        )
        .bind(old_path)
        .bind(new_path)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn list_generated_images(
    pool: &SqlitePool,
    limit: Option<i64>,
) -> Result<Vec<GeneratedImageRecord>, AppError> {
    let limit = limit.unwrap_or(500).clamp(1, 1000);
    let records = sqlx::query_as::<_, GeneratedImageRecord>(
        r#"
        SELECT
            ia.id,
            gt.id AS task_id,
            ia.file_path,
            NULL AS display_path,
            gt.prompt,
            gt.model,
            gt.size,
            gt.quality,
            ia.source_type,
            ia.created_at,
            COALESCE(gt.workspace, 'generate') AS workspace
        FROM image_assets ia
        INNER JOIN generation_tasks gt ON gt.id = ia.task_id
        WHERE gt.status = 'completed'
          AND ia.source_type IN ('generated', 'poster_composite')
        ORDER BY ia.created_at DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn list_all_generated_images(
    pool: &SqlitePool,
) -> Result<Vec<GeneratedImageRecord>, AppError> {
    let records = sqlx::query_as::<_, GeneratedImageRecord>(
        r#"
        SELECT
            ia.id,
            gt.id AS task_id,
            ia.file_path,
            NULL AS display_path,
            gt.prompt,
            gt.model,
            gt.size,
            gt.quality,
            ia.source_type,
            ia.created_at,
            COALESCE(gt.workspace, 'generate') AS workspace
        FROM image_assets ia
        INNER JOIN generation_tasks gt ON gt.id = ia.task_id
        WHERE gt.status = 'completed'
          AND ia.source_type IN ('generated', 'poster_composite')
        ORDER BY ia.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(records)
}

pub async fn generated_image_file_path(
    pool: &SqlitePool,
    asset_id: &str,
) -> Result<Option<String>, AppError> {
    let file_path = sqlx::query_scalar::<_, String>(
        r#"
        SELECT file_path
        FROM image_assets
        WHERE id = ?1
          AND source_type IN ('generated', 'poster_composite')
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?;

    Ok(file_path)
}

pub async fn list_generated_image_file_paths(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let file_paths = sqlx::query_scalar::<_, String>(
        r#"
        SELECT file_path
        FROM image_assets
        WHERE source_type IN ('generated', 'poster_composite')
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(file_paths)
}

pub async fn delete_generated_image_history(
    pool: &SqlitePool,
    asset_id: &str,
) -> Result<bool, AppError> {
    let task_id = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT task_id
        FROM image_assets
        WHERE id = ?1
        "#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let result = sqlx::query("DELETE FROM image_assets WHERE id = ?1")
        .bind(asset_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Ok(false);
    }

    if let Some(task_id) = task_id {
        let remaining_asset_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM image_assets
            WHERE task_id = ?1
            "#,
        )
        .bind(&task_id)
        .fetch_one(pool)
        .await?;

        if remaining_asset_count == 0 {
            sqlx::query("DELETE FROM generation_tasks WHERE id = ?1")
                .bind(task_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(true)
}

pub async fn clear_generated_image_history(pool: &SqlitePool) -> Result<(), AppError> {
    let task_ids = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT DISTINCT task_id
        FROM image_assets
        WHERE source_type IN ('generated', 'poster_composite')
        "#,
    )
    .fetch_all(pool)
    .await?;

    sqlx::query(
        r#"
        DELETE FROM image_assets
        WHERE source_type IN ('generated', 'poster_composite')
        "#,
    )
    .execute(pool)
    .await?;

    for task_id in task_ids.into_iter().flatten() {
        let remaining_asset_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM image_assets
            WHERE task_id = ?1
            "#,
        )
        .bind(&task_id)
        .fetch_one(pool)
        .await?;

        if remaining_asset_count == 0 {
            sqlx::query("DELETE FROM generation_tasks WHERE id = ?1")
                .bind(task_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}
