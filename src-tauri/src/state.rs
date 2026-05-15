use std::{collections::HashMap, sync::Arc};

use sqlx::SqlitePool;
use tokio::sync::{oneshot, Mutex};

#[derive(Debug)]
pub enum UpdateDownloadControl {
    Cancel,
    Pause,
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cancellations: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    pub update_download_controls:
        Arc<Mutex<HashMap<String, oneshot::Sender<UpdateDownloadControl>>>>,
}
