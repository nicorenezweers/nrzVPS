//! Dauerhafter Zustand in `<data>/state.json` (atomar geschrieben).

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use nrz_vps_common::{DeployInfo, PortMap, Quota, Runtime, Settings, Source};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub disabled: bool,
    /// 0–9: bestimmt den Port-Block.
    pub slot: u8,
    pub quota: Quota,
    pub created_at: i64,
    #[serde(default)]
    pub last_login: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub expires: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub prefix: String,
    /// SHA-256 (hex) des Klartexts.
    pub hash: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRecord {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub source: Source,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub build_command: String,
    #[serde(default)]
    pub start_command: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub ports: Vec<PortMap>,
    pub cpus: f64,
    pub memory_mb: u64,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub auto_deploy: bool,
    /// Image des laufenden Containers (Tag oder Referenz) und seine ID.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub image_id: Option<String>,
    /// Zwischen-Images des aktuellen Builds (werden mit dem Image gelöscht).
    #[serde(default)]
    pub build_layers: Vec<String>,
    #[serde(default)]
    pub deploy: Option<DeployInfo>,
    #[serde(default)]
    pub deploy_count: u32,
    pub webhook_secret: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub sessions: HashMap<String, Session>,
    #[serde(default)]
    pub tokens: Vec<StoredToken>,
    #[serde(default)]
    pub apps: Vec<AppRecord>,
}

pub struct Store {
    path: PathBuf,
    state: RwLock<State>,
}

impl Store {
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let state = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)?
        } else {
            State::default()
        };
        Ok(Self {
            path,
            state: RwLock::new(state),
        })
    }

    pub async fn read<R>(&self, f: impl FnOnce(&State) -> R) -> R {
        let s = self.state.read().await;
        f(&s)
    }

    /// Ändern und sofort speichern.
    pub async fn update<R>(&self, f: impl FnOnce(&mut State) -> R) -> anyhow::Result<R> {
        let mut s = self.state.write().await;
        let r = f(&mut s);
        let json = serde_json::to_vec_pretty(&*s)?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(r)
    }

    pub async fn app(&self, id: &str) -> Option<AppRecord> {
        self.read(|s| s.apps.iter().find(|a| a.id == id).cloned())
            .await
    }

    pub async fn user(&self, id: &str) -> Option<User> {
        self.read(|s| s.users.iter().find(|u| u.id == id).cloned())
            .await
    }

    pub async fn settings(&self) -> Settings {
        self.read(|s| s.settings.clone()).await
    }
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn new_id(bytes: usize) -> String {
    use rand::RngCore;
    let mut b = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// Gültiger Kurzname: `a-z0-9-`, beginnt mit Buchstabe, 2–30 Zeichen.
pub fn valid_name(s: &str) -> bool {
    (2..=30).contains(&s.len())
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
