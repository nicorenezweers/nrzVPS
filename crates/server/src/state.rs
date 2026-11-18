//! Gemeinsamer Zustand aller Routen.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use nrz_vps_common::{AppStats, AppStatus};
use tokio::sync::{Mutex, RwLock};

use crate::{config::Config, docker::Docker, store::Store};

/// Zuletzt gemessener Zustand eines Containers.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub status: AppStatus,
    pub started_at: Option<i64>,
    pub stats: Option<AppStats>,
}

/// Gemessene Grössen (Bytes) je App-ID.
#[derive(Debug, Clone, Default)]
pub struct Sizes {
    pub images: HashMap<String, u64>,
    pub volumes: HashMap<String, u64>,
    pub sources: HashMap<String, u64>,
}

pub struct AppInner {
    pub config: Config,
    pub store: Store,
    pub docker: Docker,
    pub live: RwLock<HashMap<String, Live>>,
    pub sizes: RwLock<Sizes>,
    /// Apps, die gerade gebaut/deployt werden.
    pub building: Mutex<HashSet<String>>,
    /// Immer nur ein Build gleichzeitig (eigenes Build-Budget).
    pub build_lock: Mutex<()>,
    /// Laufende Build-Protokolle je App.
    pub build_logs: std::sync::Mutex<HashMap<String, Vec<String>>>,
    /// Pro App höchstens eine Änderung am Container gleichzeitig.
    pub app_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

pub type AppState = Arc<AppInner>;

impl AppInner {
    pub async fn app_lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.app_locks
            .lock()
            .await
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    pub async fn is_building(&self, id: &str) -> bool {
        self.building.lock().await.contains(id)
    }
}
