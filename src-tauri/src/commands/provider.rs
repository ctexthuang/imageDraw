use tauri::State;

use crate::{
    db::{models::UpsertProviderInput, repository},
    state::AppState,
    AppError,
};

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<crate::db::models::ProviderConfig>, AppError> {
    repository::list_providers(&state.db).await
}

#[tauri::command]
pub async fn upsert_provider(state: State<'_, AppState>, input: UpsertProviderInput) -> Result<(), AppError> {
    repository::upsert_provider(&state.db, input).await
}

#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    repository::delete_provider(&state.db, &id).await
}
