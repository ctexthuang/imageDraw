use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::models::{
    CreateGenerationTaskInput, GenerationTask, ImageAsset, ProviderConfig, ProviderSecret,
    UpsertProviderInput,
};
use crate::AppError;

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
            enabled != 0 AS enabled
        FROM providers
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(providers)
}

pub async fn upsert_provider(pool: &SqlitePool, input: UpsertProviderInput) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    let existing_api_key: Option<String> = sqlx::query_scalar(
        r#"
        SELECT api_key_encrypted
        FROM providers
        WHERE id = ?1
        "#,
    )
    .bind(&input.id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let api_key = input
        .api_key
        .filter(|key| !key.trim().is_empty())
        .or(existing_api_key);

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
    .bind(r#"{"responses_api":true,"images_api":true,"chat_completions":true,"image_edit":true}"#)
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

    sqlx::query(
        r#"
        INSERT INTO generation_tasks (
            id, provider_id, task_type, prompt, model, size, quality, status, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
        "#,
    )
    .bind(&id)
    .bind(&input.provider_id)
    .bind(&input.task_type)
    .bind(&input.prompt)
    .bind(&input.model)
    .bind(&input.size)
    .bind(&input.quality)
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
