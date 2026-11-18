//! nrzVPS – kleine „VPS“-Plattform für Freunde und Familie: jeder Benutzer bekommt ein striktes
//! Kontingent (CPU, RAM, Speicher, 10 Ports) und hostet Apps als Docker-Container – aus Quellcode,
//! Git (nrzGit) oder fertigen Images. Steuerbar per Weboberfläche und MCP (für KI-Agenten).

mod admin;
mod api;
mod apps;
mod auth;
mod build;
mod config;
mod docker;
mod error;
mod mcp;
mod monitor;
mod ops;
mod state;
mod store;

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::{config::Config, docker::Docker, state::AppInner, store::Store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        let bind = std::env::var("NRZ_VPS_BIND").unwrap_or_else(|_| "0.0.0.0:34100".into());
        let port = bind.rsplit(':').next().unwrap_or("34100");
        let ok = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok();
        std::process::exit(if ok { 0 } else { 1 });
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let config = Config::from_env();
    for d in [
        config.data_dir.clone(),
        config.sources_dir(),
        config.builds_dir(),
    ] {
        std::fs::create_dir_all(&d)?;
    }
    let config = Config {
        data_dir: std::fs::canonicalize(&config.data_dir)?,
        ..config
    };
    // Halbfertige Uploads/Klone vom letzten Lauf wegräumen
    if let Ok(rd) = std::fs::read_dir(config.sources_dir()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with(".upload") || n.ends_with(".git") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let store = Store::load(config.data_dir.join("state.json"))?;
    let docker = Docker::new(config.docker_sock.clone());
    match docker.version().await {
        Ok(v) => tracing::info!("Docker {v} über {}", config.docker_sock.display()),
        Err(e) => tracing::error!("Docker nicht erreichbar: {e:#}"),
    }
    // Abgebrochene Deployments (Neustart mitten im Build) als fehlgeschlagen markieren
    store
        .update(|s| {
            for a in &mut s.apps {
                if let Some(d) = a.deploy.as_mut().filter(|d| d.ok.is_none()) {
                    d.ok = Some(false);
                    d.message = "Abgebrochen (nrzVPS wurde neu gestartet)".into();
                }
            }
        })
        .await?;

    let app = Arc::new(AppInner {
        config,
        store,
        docker,
        live: Default::default(),
        sizes: Default::default(),
        building: Default::default(),
        build_lock: Default::default(),
        build_logs: Default::default(),
        app_locks: Default::default(),
    });
    monitor::spawn(app.clone());

    let listener = tokio::net::TcpListener::bind(&app.config.bind).await?;
    tracing::info!(
        "nrzVPS {} läuft auf http://{}",
        nrz_vps_common::VERSION,
        app.config.bind
    );
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
}
