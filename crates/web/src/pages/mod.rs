//! Seiten der Oberfläche.

mod account;
mod admin_system;
mod admin_users;
mod ai;
mod app_console;
mod app_logs;
mod app_overview;
pub mod app_settings;
mod app_source;
mod dashboard;
mod new_app;

pub use account::Account;
pub use admin_system::AdminSystem;
pub use admin_users::AdminUsers;
pub use ai::AiHosting;
pub use app_console::AppConsole;
pub use app_logs::AppLogs;
pub use app_overview::AppOverview;
pub use app_settings::AppSettings;
pub use app_source::AppSource;
pub use dashboard::Dashboard;
pub use new_app::NewApp;

/// Hinweis unter dem Gesamtbudget: wie viel die Benutzer-Kontingente schon belegen.
pub fn admin_users_quota_hint(cpus: f64, memory_mb: u64) -> String {
    format!(
        "Benutzer-Kontingente zusammen: {} CPU und {} RAM. Gesamt muss mindestens das plus die Build-Reserve sein.",
        crate::components::fmt_cpus(cpus),
        crate::components::fmt_mb(memory_mb)
    )
}
