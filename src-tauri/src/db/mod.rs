use std::fs;

use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use tauri::{AppHandle, Manager};

use crate::AppError;

pub mod models;
pub mod repository;

pub async fn init(app: &AppHandle) -> Result<SqlitePool, AppError> {
    let app_data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&app_data_dir)?;

    let database_path = app_data_dir.join("image_draw_ai.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await?;

    for statement in include_str!("../../migrations/001_init.sql").split(';') {
        let statement = statement.trim();
        if !statement.is_empty() {
            sqlx::query(statement).execute(&pool).await?;
        }
    }
    ensure_column(
        &pool,
        "providers",
        "capabilities",
        "TEXT NOT NULL DEFAULT '{}'",
    )
    .await?;

    Ok(pool)
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AppError> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let has_column = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    });
    if !has_column {
        sqlx::query(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}
