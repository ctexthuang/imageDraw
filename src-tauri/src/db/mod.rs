use std::fs;

use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
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

    Ok(pool)
}
