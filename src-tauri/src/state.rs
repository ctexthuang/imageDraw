use std::{collections::HashMap, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::{oneshot, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}
