//! API-Vertrag von nrzVPS: alle JSON-Typen, die Backend und Weboberfläche austauschen.
//! Routen: siehe [`routes`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub mod routes;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Erster Port für Benutzer-Apps. Benutzer `slot` (0–9) bekommt `PORT_BASE + slot*10 .. +9`.
pub const PORT_BASE: u16 = 34000;
pub const PORTS_PER_USER: u16 = 10;
pub const MAX_USERS: usize = 10;

/// Port-Block eines Benutzer-Slots (inklusive).
pub fn slot_ports(slot: u8) -> (u16, u16) {
    let from = PORT_BASE + u16::from(slot) * PORTS_PER_USER;
    (from, from + PORTS_PER_USER - 1)
}

/* ------------------------------------------------------------------ */
/*                              Allgemein                             */
/* ------------------------------------------------------------------ */

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ApiError {
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ok {
    pub ok: bool,
}

/* ------------------------------------------------------------------ */
/*                               Anmeldung                            */
/* ------------------------------------------------------------------ */

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub admin: bool,
    pub slot: u8,
    pub port_from: u16,
    pub port_to: u16,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AuthState {
    /// Noch kein Konto: die erste Anmeldung legt den Admin an.
    pub setup_required: bool,
    pub user: Option<Me>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChangePassword {
    pub current: String,
    pub new: String,
}

/// MCP-/API-Token (Klartext nur einmal bei der Erstellung).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreateToken {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreatedToken {
    pub token: ApiToken,
    pub secret: String,
}

/* ------------------------------------------------------------------ */
/*                          Kontingent & Nutzung                      */
/* ------------------------------------------------------------------ */

/// Harte Grenzen eines Benutzers (vom Admin festgelegt).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Quota {
    /// CPU-Kerne (z. B. 0.25).
    pub cpus: f64,
    pub memory_mb: u64,
    /// Speicherplatz für Quellcode, Images und App-Daten.
    pub disk_mb: u64,
    pub max_apps: u32,
}

impl Default for Quota {
    fn default() -> Self {
        Self {
            cpus: 0.1,
            memory_mb: 256,
            disk_mb: 2048,
            max_apps: 3,
        }
    }
}

/// Aktuelle Nutzung eines Benutzers.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    /// CPU, die laufende Apps reserviert haben.
    pub cpus: f64,
    /// RAM, den laufende Apps reserviert haben.
    pub memory_mb: u64,
    /// Gemessener Speicherplatz (Quellcode + Images + App-Daten).
    pub disk_mb: u64,
    pub apps: u32,
    pub ports_used: u32,
}

/// Startseite eines Benutzers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Overview {
    pub me: Me,
    pub quota: Quota,
    pub usage: Usage,
    /// Hostname/IP, unter der Apps erreichbar sind (`http://<public_host>:<port>`).
    pub public_host: String,
    pub free_ports: Vec<u16>,
    /// Vollständige MCP-URL (`http://host:port/mcp`).
    pub mcp_url: String,
    /// URL der Anleitung für KIs.
    pub llm_url: String,
}

/* ------------------------------------------------------------------ */
/*                                 Apps                               */
/* ------------------------------------------------------------------ */

/// Woher der Code einer App kommt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Quellcode hochgeladen (Web, MCP `upload_files`, `PUT /api/apps/{id}/source`).
    #[default]
    Upload,
    /// Git-Repository (z. B. nrzGit). `token` wird in Antworten als `"••••"` maskiert.
    Git {
        url: String,
        #[serde(default)]
        branch: String,
        /// Unterordner im Repository (leer = Wurzel).
        #[serde(default)]
        subdir: String,
        #[serde(default)]
        token: Option<String>,
    },
    /// Fertiges Container-Image (z. B. `nginx:alpine` oder aus der nrzGit-Registry).
    Image {
        image: String,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
}

impl Source {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Git { .. } => "git",
            Self::Image { .. } => "image",
        }
    }
}

/// Wert, der ein gespeichertes Geheimnis unverändert lässt.
pub const SECRET_MASK: &str = "••••";

/// Laufzeit für Quellcode. `Auto` erkennt sie am Inhalt.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Runtime {
    #[default]
    Auto,
    /// Eigenes `Dockerfile` im Code.
    Dockerfile,
    Node,
    Bun,
    Deno,
    Python,
    Rust,
    Go,
    Php,
    /// Statische Webseite (HTML/CSS/JS).
    Static,
}

impl Runtime {
    pub const ALL: [Runtime; 10] = [
        Runtime::Auto,
        Runtime::Dockerfile,
        Runtime::Node,
        Runtime::Bun,
        Runtime::Deno,
        Runtime::Python,
        Runtime::Rust,
        Runtime::Go,
        Runtime::Php,
        Runtime::Static,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatisch",
            Self::Dockerfile => "Dockerfile",
            Self::Node => "Node.js",
            Self::Bun => "Bun",
            Self::Deno => "Deno",
            Self::Python => "Python",
            Self::Rust => "Rust",
            Self::Go => "Go",
            Self::Php => "PHP",
            Self::Static => "Statische Webseite",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dockerfile => "dockerfile",
            Self::Node => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Php => "php",
            Self::Static => "static",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str() == s.trim().to_ascii_lowercase())
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }
}

/// Port nach aussen (`host`, aus dem Block des Benutzers) → Port im Container.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortMap {
    pub host: u16,
    pub container: u16,
    #[serde(default)]
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    /// Angelegt, noch nie gestartet.
    #[default]
    New,
    Building,
    Running,
    Restarting,
    Stopped,
    /// Unerwartet beendet (Exit-Code ≠ 0 oder Speicher voll).
    Crashed,
}

impl AppStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "Neu",
            Self::Building => "Wird gebaut",
            Self::Running => "Läuft",
            Self::Restarting => "Startet neu",
            Self::Stopped => "Gestoppt",
            Self::Crashed => "Abgestürzt",
        }
    }
}

/// Ergebnis bzw. Stand des letzten Deployments.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DeployInfo {
    pub number: u32,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// `None` = läuft noch.
    pub ok: Option<bool>,
    pub message: String,
    /// Erkannte bzw. verwendete Laufzeit.
    pub runtime: Option<Runtime>,
    /// Git-Commit (bei Git-Quelle).
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct AppStats {
    /// Prozent eines Kerns (100 = ein ganzer Kern).
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub memory_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
}

/// Eine App, wie API und Oberfläche sie sehen (Geheimnisse maskiert).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct App {
    pub id: String,
    pub owner: String,
    /// Kurzname (klein, `a-z0-9-`), eindeutig pro Benutzer, auch DNS-Name im Benutzer-Netz.
    pub name: String,
    pub source: Source,
    pub runtime: Runtime,
    /// Leer = automatisch.
    pub build_command: String,
    /// Leer = automatisch.
    pub start_command: String,
    pub env: BTreeMap<String, String>,
    pub ports: Vec<PortMap>,
    pub cpus: f64,
    pub memory_mb: u64,
    /// Soll laufen (wird nach Neustart von nrzVPS automatisch gestartet).
    pub auto_start: bool,
    /// Git-Webhook löst automatisch ein neues Deployment aus.
    pub auto_deploy: bool,
    pub status: AppStatus,
    /// Aktuelles Image (Tag).
    pub image: Option<String>,
    pub image_size: u64,
    pub volume_size: u64,
    pub source_size: u64,
    pub deploy: Option<DeployInfo>,
    pub stats: Option<AppStats>,
    pub started_at: Option<i64>,
    /// Webhook für nrzGit/Gitea/GitHub (Push → neues Deployment).
    pub webhook_url: String,
    /// Erreichbare Adressen (`http://host:port`).
    pub urls: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreateApp {
    pub name: String,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub build_command: String,
    #[serde(default)]
    pub start_command: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Port, auf dem die App im Container lauscht. Leer = gleicher Port wie aussen (Umgebungsvariable `PORT`).
    #[serde(default)]
    pub container_port: Option<u16>,
    /// Ohne Port (Hintergrund-Worker, Bot …).
    #[serde(default)]
    pub no_port: bool,
    #[serde(default)]
    pub cpus: Option<f64>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub auto_deploy: bool,
}

/// Teiländerung; `None` = unverändert. `env` ersetzt alle Variablen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdateApp {
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub runtime: Option<Runtime>,
    #[serde(default)]
    pub build_command: Option<String>,
    #[serde(default)]
    pub start_command: Option<String>,
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub cpus: Option<f64>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub auto_deploy: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct AddPort {
    /// Port im Container (leer = gleicher Port wie aussen).
    #[serde(default)]
    pub container: Option<u16>,
    /// Wunsch-Port aussen (leer = nächster freier aus deinem Block).
    #[serde(default)]
    pub host: Option<u16>,
    #[serde(default)]
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PowerRequest {
    pub action: PowerAction,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Logs {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SourceFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SourceTree {
    pub files: Vec<SourceFile>,
    pub total_size: u64,
    /// Was `Auto` erkennen würde.
    pub detected: Option<Runtime>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecRequest {
    pub command: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecResult {
    pub exit_code: i64,
    pub output: String,
}

/* ------------------------------------------------------------------ */
/*                                 Admin                              */
/* ------------------------------------------------------------------ */

/// Globale Einstellungen (nur Admin).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Gesamte CPU für ALLE Benutzer zusammen (Summe der Kontingente darf nicht höher sein).
    pub total_cpus: f64,
    pub total_memory_mb: u64,
    pub total_disk_mb: u64,
    /// Eigenes Budget für Builds (immer nur ein Build gleichzeitig). Zählt zur Gesamt-CPU/-RAM:
    /// Summe der Benutzer-Kontingente + Build-Budget ≤ Gesamt.
    pub build_cpus: f64,
    pub build_memory_mb: u64,
    /// Hostname/IP für App-Adressen (leer = aus der Anfrage).
    pub public_host: String,
    /// Basis-Images (node, rust, python …) nach jedem Build löschen – spart Speicher, nächster Build lädt neu.
    pub remove_base_images: bool,
    pub max_upload_mb: u64,
    pub build_timeout_min: u64,
    /// Standard-Kontingent für neue Benutzer.
    pub default_quota: Quota,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            total_cpus: 1.0,
            total_memory_mb: 2048,
            total_disk_mb: 20 * 1024,
            build_cpus: 0.5,
            build_memory_mb: 1024,
            public_host: String::new(),
            remove_base_images: true,
            max_upload_mb: 200,
            build_timeout_min: 20,
            default_quota: Quota::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub admin: bool,
    pub disabled: bool,
    pub slot: u8,
    pub port_from: u16,
    pub port_to: u16,
    pub quota: Quota,
    pub usage: Usage,
    pub created_at: i64,
    pub last_login: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CreateUser {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub admin: bool,
    /// Leer = Standard-Kontingent aus den Einstellungen.
    #[serde(default)]
    pub quota: Option<Quota>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UpdateUser {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub quota: Option<Quota>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub admin: Option<bool>,
}

/// Gesamtzustand für die Admin-Seite.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SystemInfo {
    pub version: String,
    pub docker_ok: bool,
    pub docker_version: String,
    pub host_cpus: u32,
    pub host_memory_mb: u64,
    /// Summe aller Benutzer-Kontingente.
    pub allocated: Quota,
    /// Summe der Reservierungen laufender Apps.
    pub in_use: Usage,
    /// Speicher aller nrzVPS-Images / -Volumes / Quellen (Bytes).
    pub images_bytes: u64,
    pub volumes_bytes: u64,
    pub sources_bytes: u64,
    pub users: u32,
    pub apps: u32,
    pub running: u32,
    pub settings: Option<Settings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CleanupResult {
    pub removed_images: u32,
    pub freed_bytes: u64,
}
