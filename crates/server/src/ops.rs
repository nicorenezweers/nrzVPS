//! Kernlogik für Apps – genutzt von HTTP-API und MCP: Ansicht, Kontingente, Ports, Quellcode,
//! Deployment (Build/Pull → Container tauschen → altes Image sofort löschen), Start/Stopp, Löschen.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use nrz_vps_common::{
    AddPort, App, AppStatus, CreateApp, DeployInfo, PortMap, Protocol, Runtime, SECRET_MASK,
    Source, SourceFile, SourceTree, UpdateApp, Usage, slot_ports,
};
use serde_json::{Value, json};

use crate::{
    build,
    error::{AppError, AppResult},
    state::{AppState, Live},
    store::{AppRecord, User, new_id, now, valid_name},
};

pub const LABEL: &str = "nrzvps=1";
const MB: u64 = 1024 * 1024;

/// Wer fragt (für Adressen in Antworten).
#[derive(Clone)]
pub struct Ctx {
    pub user: User,
    /// `http(s)://host:port` von nrzVPS selbst.
    pub base: String,
    /// Host für App-Adressen.
    pub host: String,
}

/* ------------------------------------------------------------------ */
/*                               Namen                                */
/* ------------------------------------------------------------------ */

pub fn container_name(username: &str, app: &str) -> String {
    format!("nrzvps-{username}-{app}")
}
pub fn network_name(username: &str) -> String {
    format!("nrzvps-{username}")
}
pub fn volume_name(app_id: &str) -> String {
    format!("nrzvps-{app_id}-data")
}
fn image_tag(username: &str, app: &str, n: u32) -> String {
    format!("nrzvps/{username}-{app}:{n}")
}

pub fn source_dir(app: &AppState, id: &str) -> PathBuf {
    app.config.sources_dir().join(id)
}

/* ------------------------------------------------------------------ */
/*                         Ansicht & Nutzung                          */
/* ------------------------------------------------------------------ */

fn mask(source: &Source) -> Source {
    match source {
        Source::Git {
            url,
            branch,
            subdir,
            token,
        } => Source::Git {
            url: url.clone(),
            branch: branch.clone(),
            subdir: subdir.clone(),
            token: token.as_ref().map(|_| SECRET_MASK.to_string()),
        },
        Source::Image {
            image,
            username,
            password,
        } => Source::Image {
            image: image.clone(),
            username: username.clone(),
            password: password.as_ref().map(|_| SECRET_MASK.to_string()),
        },
        Source::Upload => Source::Upload,
    }
}

pub async fn view(app: &AppState, rec: &AppRecord, owner: &str, base: &str, host: &str) -> App {
    let live = app
        .live
        .read()
        .await
        .get(&rec.id)
        .cloned()
        .unwrap_or_default();
    let building = app.is_building(&rec.id).await;
    let sizes = app.sizes.read().await;
    let status = if building {
        AppStatus::Building
    } else if rec.image.is_none() && live.status == AppStatus::New {
        AppStatus::New
    } else if live.status == AppStatus::New {
        AppStatus::Stopped
    } else {
        live.status
    };
    let running = matches!(status, AppStatus::Running | AppStatus::Restarting);
    App {
        id: rec.id.clone(),
        owner: owner.to_string(),
        name: rec.name.clone(),
        source: mask(&rec.source),
        runtime: rec.runtime,
        build_command: rec.build_command.clone(),
        start_command: rec.start_command.clone(),
        env: rec.env.clone(),
        ports: rec.ports.clone(),
        cpus: rec.cpus,
        memory_mb: rec.memory_mb,
        auto_start: rec.auto_start,
        auto_deploy: rec.auto_deploy,
        status,
        image: rec.image.clone(),
        image_size: sizes.images.get(&rec.id).copied().unwrap_or(0),
        volume_size: sizes.volumes.get(&rec.id).copied().unwrap_or(0),
        source_size: sizes.sources.get(&rec.id).copied().unwrap_or(0),
        deploy: rec.deploy.clone(),
        stats: if running { live.stats } else { None },
        started_at: if running { live.started_at } else { None },
        webhook_url: format!("{base}/api/hooks/{}/{}", rec.id, rec.webhook_secret),
        urls: rec
            .ports
            .iter()
            .filter(|p| p.protocol == Protocol::Tcp)
            .map(|p| format!("http://{host}:{}", p.host))
            .collect(),
        created_at: rec.created_at,
    }
}

pub async fn view_ctx(app: &AppState, ctx: &Ctx, rec: &AppRecord) -> App {
    let owner = if rec.owner_id == ctx.user.id {
        ctx.user.username.clone()
    } else {
        app.store
            .user(&rec.owner_id)
            .await
            .map(|u| u.username)
            .unwrap_or_default()
    };
    view(app, rec, &owner, &ctx.base, &ctx.host).await
}

fn is_running(live: Option<&Live>) -> bool {
    live.is_some_and(|l| matches!(l.status, AppStatus::Running | AppStatus::Restarting))
}

/// Nutzung eines Benutzers. `exclude` = App, deren Reservierung nicht mitzählt (beim Start/Ändern).
pub async fn usage(app: &AppState, user_id: &str, exclude: Option<&str>) -> Usage {
    let apps: Vec<AppRecord> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .filter(|a| a.owner_id == user_id)
                .cloned()
                .collect()
        })
        .await;
    let live = app.live.read().await;
    let sizes = app.sizes.read().await;
    let mut u = Usage {
        apps: apps.len() as u32,
        ..Default::default()
    };
    let mut disk = 0u64;
    for a in &apps {
        u.ports_used += a.ports.len() as u32;
        disk += sizes.images.get(&a.id).copied().unwrap_or(0)
            + sizes.volumes.get(&a.id).copied().unwrap_or(0)
            + sizes.sources.get(&a.id).copied().unwrap_or(0);
        if Some(a.id.as_str()) != exclude && is_running(live.get(&a.id)) {
            u.cpus += a.cpus;
            u.memory_mb += a.memory_mb;
        }
    }
    u.cpus = (u.cpus * 1000.0).round() / 1000.0;
    u.disk_mb = disk / MB;
    u
}

/// Prüft, ob die App mit ihren Grenzen ins Kontingent passt (streng, keine Überbuchung).
async fn check_resources(app: &AppState, user: &User, rec: &AppRecord) -> AppResult<()> {
    let u = usage(app, &user.id, Some(&rec.id)).await;
    let q = user.quota;
    if u.cpus + rec.cpus > q.cpus + 1e-6 {
        return Err(AppError::Forbidden(format!(
            "CPU-Kontingent reicht nicht: laufende Apps nutzen {:.2} von {:.2} Kernen, diese App braucht {:.2}. Stoppe eine andere App oder senke `cpus`.",
            u.cpus, q.cpus, rec.cpus
        )));
    }
    if u.memory_mb + rec.memory_mb > q.memory_mb {
        return Err(AppError::Forbidden(format!(
            "RAM-Kontingent reicht nicht: laufende Apps nutzen {} von {} MB, diese App braucht {} MB.",
            u.memory_mb, q.memory_mb, rec.memory_mb
        )));
    }
    if u.disk_mb > q.disk_mb {
        return Err(AppError::Forbidden(format!(
            "Speicherplatz voll ({} von {} MB). Lösche Apps oder Daten.",
            u.disk_mb, q.disk_mb
        )));
    }
    Ok(())
}

fn check_limits(user: &User, cpus: f64, memory_mb: u64) -> AppResult<()> {
    if !(0.01..=64.0).contains(&cpus) || cpus > user.quota.cpus + 1e-6 {
        return Err(AppError::bad(format!(
            "`cpus` muss zwischen 0.01 und deinem Kontingent ({:.2}) liegen",
            user.quota.cpus
        )));
    }
    if memory_mb < 32 || memory_mb > user.quota.memory_mb {
        return Err(AppError::bad(format!(
            "`memory_mb` muss zwischen 32 und deinem Kontingent ({} MB) liegen",
            user.quota.memory_mb
        )));
    }
    Ok(())
}

fn check_env(env: &BTreeMap<String, String>) -> AppResult<()> {
    if env.len() > 100 {
        return Err(AppError::bad("Höchstens 100 Umgebungsvariablen"));
    }
    for (k, v) in env {
        let ok = !k.is_empty()
            && k.len() <= 128
            && !k.starts_with(|c: char| c.is_ascii_digit())
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(AppError::bad(format!(
                "Ungültiger Variablenname `{k}` (A-Z, 0-9, _)"
            )));
        }
        if v.len() > 32 * 1024 {
            return Err(AppError::bad(format!("Wert von `{k}` zu lang")));
        }
    }
    Ok(())
}

/// Freie Ports im Block des Benutzers.
pub async fn free_ports(app: &AppState, user: &User) -> Vec<u16> {
    let used: Vec<u16> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .filter(|a| a.owner_id == user.id)
                .flat_map(|a| a.ports.iter().map(|p| p.host))
                .collect()
        })
        .await;
    let (from, to) = slot_ports(user.slot);
    (from..=to).filter(|p| !used.contains(p)).collect()
}

/// App eines Benutzers per ID oder Name (Admins sehen alle per ID).
pub async fn find(app: &AppState, user: &User, key: &str) -> AppResult<AppRecord> {
    let key = key.trim();
    app.store
        .read(|s| {
            s.apps
                .iter()
                .find(|a| {
                    (a.id == key && (a.owner_id == user.id || user.admin))
                        || (a.owner_id == user.id && a.name == key)
                })
                .cloned()
        })
        .await
        .ok_or_else(|| AppError::not_found(format!("App `{key}` nicht gefunden")))
}

async fn owner_of(app: &AppState, rec: &AppRecord) -> AppResult<User> {
    app.store
        .user(&rec.owner_id)
        .await
        .ok_or_else(|| AppError::internal("Besitzer fehlt"))
}

/* ------------------------------------------------------------------ */
/*                           Anlegen & Ändern                         */
/* ------------------------------------------------------------------ */

fn clean_source(s: Source) -> AppResult<Source> {
    Ok(match s {
        Source::Upload => Source::Upload,
        Source::Git {
            url,
            branch,
            subdir,
            token,
        } => {
            let url = url.trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(AppError::bad(
                    "Git-URL muss mit http:// oder https:// beginnen",
                ));
            }
            let subdir = subdir.trim().trim_matches('/').to_string();
            if subdir.contains("..") {
                return Err(AppError::bad("Ungültiger Unterordner"));
            }
            Source::Git {
                url,
                branch: branch.trim().to_string(),
                subdir,
                token: token.filter(|t| !t.trim().is_empty()),
            }
        }
        Source::Image {
            image,
            username,
            password,
        } => {
            let image = image.trim().to_string();
            if image.is_empty() || image.contains(char::is_whitespace) {
                return Err(AppError::bad("Image fehlt (z. B. nginx:alpine)"));
            }
            Source::Image {
                image,
                username: username.filter(|u| !u.trim().is_empty()),
                password: password.filter(|p| !p.is_empty()),
            }
        }
    })
}

/// Neue Quelle übernehmen; maskierte Geheimnisse behalten den alten Wert.
fn merge_source(old: &Source, new: Source) -> Source {
    match (old, new) {
        (
            Source::Git { token: old_t, .. },
            Source::Git {
                url,
                branch,
                subdir,
                token,
            },
        ) => Source::Git {
            url,
            branch,
            subdir,
            token: if token.as_deref() == Some(SECRET_MASK) {
                old_t.clone()
            } else {
                token
            },
        },
        (
            Source::Image {
                password: old_p, ..
            },
            Source::Image {
                image,
                username,
                password,
            },
        ) => Source::Image {
            image,
            username,
            password: if password.as_deref() == Some(SECRET_MASK) {
                old_p.clone()
            } else {
                password
            },
        },
        (_, n) => n,
    }
}

pub async fn create(app: &AppState, ctx: &Ctx, req: CreateApp) -> AppResult<App> {
    let user = &ctx.user;
    let name = req.name.trim().to_lowercase();
    if !valid_name(&name) {
        return Err(AppError::bad(
            "Name: 2–30 Zeichen, Kleinbuchstaben, Ziffern und -, beginnt mit Buchstabe",
        ));
    }
    check_env(&req.env)?;
    let source = clean_source(req.source)?;
    let u = usage(app, &user.id, None).await;
    if u.apps >= user.quota.max_apps {
        return Err(AppError::Forbidden(format!(
            "Höchstens {} Apps erlaubt (Kontingent)",
            user.quota.max_apps
        )));
    }
    // Standard: was vom Kontingent noch frei ist (mind. ein kleiner Teil)
    let cpus = req.cpus.unwrap_or_else(|| {
        let free = user.quota.cpus - u.cpus;
        if free >= 0.01 {
            (free * 100.0).floor() / 100.0
        } else {
            (user.quota.cpus / 2.0 * 100.0).floor().max(1.0) / 100.0
        }
    });
    let memory_mb = req.memory_mb.unwrap_or_else(|| {
        let free = user.quota.memory_mb.saturating_sub(u.memory_mb);
        if free >= 32 {
            free
        } else {
            (user.quota.memory_mb / 2).max(32)
        }
    });
    check_limits(user, cpus, memory_mb)?;
    let mut ports = Vec::new();
    if !req.no_port {
        let host = *free_ports(app, user)
            .await
            .first()
            .ok_or_else(|| AppError::Forbidden("Alle 10 Ports sind belegt".into()))?;
        ports.push(PortMap {
            host,
            container: req.container_port.unwrap_or(host),
            protocol: Protocol::Tcp,
        });
    }
    let is_git = matches!(source, Source::Git { .. });
    let rec = AppRecord {
        id: new_id(6),
        owner_id: user.id.clone(),
        name: name.clone(),
        source,
        runtime: req.runtime,
        build_command: req.build_command.trim().to_string(),
        start_command: req.start_command.trim().to_string(),
        env: req.env,
        ports,
        cpus,
        memory_mb,
        auto_start: true,
        auto_deploy: req.auto_deploy || is_git,
        image: None,
        image_id: None,
        build_layers: vec![],
        deploy: None,
        deploy_count: 0,
        webhook_secret: new_id(16),
        created_at: now(),
    };
    let r2 = rec.clone();
    let uid = user.id.clone();
    let ok = app
        .store
        .update(|s| {
            if s.apps
                .iter()
                .any(|a| a.owner_id == uid && a.name == r2.name)
            {
                return false;
            }
            s.apps.push(r2);
            true
        })
        .await?;
    if !ok {
        return Err(AppError::conflict(format!(
            "Du hast schon eine App `{name}`"
        )));
    }
    std::fs::create_dir_all(source_dir(app, &rec.id))?;
    Ok(view_ctx(app, ctx, &rec).await)
}

pub async fn update(app: &AppState, ctx: &Ctx, key: &str, req: UpdateApp) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    if let Some(env) = &req.env {
        check_env(env)?;
    }
    let source = match req.source {
        Some(s) => Some(merge_source(&rec.source, clean_source(s)?)),
        None => None,
    };
    let cpus = req.cpus.unwrap_or(rec.cpus);
    let memory_mb = req.memory_mb.unwrap_or(rec.memory_mb);
    check_limits(&owner, cpus, memory_mb)?;
    let needs_restart = req.env.as_ref().is_some_and(|e| *e != rec.env)
        || req
            .start_command
            .as_ref()
            .is_some_and(|c| c.trim() != rec.start_command)
        || (cpus - rec.cpus).abs() > 1e-9
        || memory_mb != rec.memory_mb;
    let id = rec.id.clone();
    let updated = app
        .store
        .update(|s| {
            let a = s.apps.iter_mut().find(|a| a.id == id)?;
            if let Some(src) = source {
                a.source = src;
            }
            if let Some(r) = req.runtime {
                a.runtime = r;
            }
            if let Some(c) = req.build_command {
                a.build_command = c.trim().to_string();
            }
            if let Some(c) = req.start_command {
                a.start_command = c.trim().to_string();
            }
            if let Some(e) = req.env {
                a.env = e;
            }
            a.cpus = cpus;
            a.memory_mb = memory_mb;
            if let Some(v) = req.auto_start {
                a.auto_start = v;
            }
            if let Some(v) = req.auto_deploy {
                a.auto_deploy = v;
            }
            Some(a.clone())
        })
        .await?
        .ok_or_else(|| AppError::not_found("App nicht gefunden"))?;
    let running = is_running(app.live.read().await.get(&id));
    if needs_restart && running {
        recreate_and_start(app, &owner, &updated).await?;
    }
    let rec = app.store.app(&id).await.unwrap_or(updated);
    Ok(view_ctx(app, ctx, &rec).await)
}

pub async fn add_port(app: &AppState, ctx: &Ctx, key: &str, req: AddPort) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    let free = free_ports(app, &owner).await;
    let host = match req.host {
        Some(h) if free.contains(&h) => h,
        Some(h) => {
            let (a, b) = slot_ports(owner.slot);
            return Err(AppError::bad(format!(
                "Port {h} ist nicht frei oder nicht in deinem Block {a}–{b}"
            )));
        }
        None => *free
            .first()
            .ok_or_else(|| AppError::Forbidden("Alle 10 Ports sind belegt".into()))?,
    };
    let container = req.container.unwrap_or(host);
    if container == 0 {
        return Err(AppError::bad("Ungültiger Container-Port"));
    }
    if rec
        .ports
        .iter()
        .any(|p| p.container == container && p.protocol == req.protocol)
    {
        return Err(AppError::conflict(format!(
            "Container-Port {container}/{} ist schon freigegeben",
            req.protocol.as_str()
        )));
    }
    set_ports(app, ctx, &owner, &rec, |p| {
        p.push(PortMap {
            host,
            container,
            protocol: req.protocol,
        })
    })
    .await
}

pub async fn remove_port(app: &AppState, ctx: &Ctx, key: &str, host: u16) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    if !rec.ports.iter().any(|p| p.host == host) {
        return Err(AppError::not_found(format!(
            "Port {host} gehört nicht zu dieser App"
        )));
    }
    set_ports(app, ctx, &owner, &rec, |p| p.retain(|x| x.host != host)).await
}

async fn set_ports(
    app: &AppState,
    ctx: &Ctx,
    owner: &User,
    rec: &AppRecord,
    f: impl FnOnce(&mut Vec<PortMap>),
) -> AppResult<App> {
    let id = rec.id.clone();
    let updated = app
        .store
        .update(|s| {
            let a = s.apps.iter_mut().find(|a| a.id == id)?;
            f(&mut a.ports);
            Some(a.clone())
        })
        .await?
        .ok_or_else(|| AppError::not_found("App nicht gefunden"))?;
    if is_running(app.live.read().await.get(&id)) {
        recreate_and_start(app, owner, &updated).await?;
    }
    Ok(view_ctx(app, ctx, &updated).await)
}

pub async fn new_webhook_secret(app: &AppState, ctx: &Ctx, key: &str) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let id = rec.id.clone();
    let secret = new_id(16);
    let updated = app
        .store
        .update(|s| {
            let a = s.apps.iter_mut().find(|a| a.id == id)?;
            a.webhook_secret = secret;
            Some(a.clone())
        })
        .await?
        .ok_or_else(|| AppError::not_found("App nicht gefunden"))?;
    Ok(view_ctx(app, ctx, &updated).await)
}

/* ------------------------------------------------------------------ */
/*                              Quellcode                             */
/* ------------------------------------------------------------------ */

pub async fn source_tree(app: &AppState, rec: &AppRecord) -> SourceTree {
    let dir = source_dir(app, &rec.id);
    tokio::task::spawn_blocking(move || {
        let files = build::list_files(&dir);
        let total_size = files.iter().map(|f| f.1).sum();
        SourceTree {
            files: files
                .into_iter()
                .map(|(path, size)| SourceFile { path, size })
                .collect(),
            total_size,
            detected: build::detect(&dir),
        }
    })
    .await
    .unwrap_or_default()
}

async fn ensure_upload_source(
    app: &AppState,
    owner: &User,
    rec: &AppRecord,
    add_bytes: u64,
) -> AppResult<u64> {
    if !matches!(rec.source, Source::Upload) {
        return Err(AppError::bad(format!(
            "Diese App nutzt als Quelle `{}`. Dateien hochladen geht nur bei Quelle `upload` (ändere die Quelle oder pushe ins Repository).",
            rec.source.kind()
        )));
    }
    let settings = app.store.settings().await;
    let limit = settings.max_upload_mb * MB;
    if add_bytes > limit {
        return Err(AppError::bad(format!(
            "Zu gross (max. {} MB pro Upload)",
            settings.max_upload_mb
        )));
    }
    let u = usage(app, &owner.id, None).await;
    if (u.disk_mb * MB + add_bytes) / MB > owner.quota.disk_mb {
        return Err(AppError::Forbidden(format!(
            "Speicherplatz voll ({} von {} MB)",
            u.disk_mb, owner.quota.disk_mb
        )));
    }
    Ok(limit)
}

async fn refresh_source_size(app: &AppState, id: &str) {
    let dir = source_dir(app, id);
    let size = tokio::task::spawn_blocking(move || build::dir_size(&dir))
        .await
        .unwrap_or(0);
    app.sizes.write().await.sources.insert(id.to_string(), size);
}

/// Archiv (zip/tar.gz) hochladen. `replace` = vorher alles löschen.
pub async fn upload_archive(
    app: &AppState,
    ctx: &Ctx,
    key: &str,
    bytes: Vec<u8>,
    replace: bool,
) -> AppResult<SourceTree> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    let limit = ensure_upload_source(app, &owner, &rec, bytes.len() as u64).await?;
    let dir = source_dir(app, &rec.id);
    let tmp = app.config.sources_dir().join(format!("{}.upload", rec.id));
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let _ = std::fs::remove_dir_all(&tmp);
        build::extract_archive(&bytes, &tmp, limit * 4)
            .map_err(|e| AppError::bad(e.to_string()))?;
        if replace || !dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::rename(&tmp, &dir)?;
        } else {
            for (rel, _) in build::list_files(&tmp) {
                let to = dir.join(&rel);
                if let Some(p) = to.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::rename(tmp.join(&rel), to)?;
            }
            let _ = std::fs::remove_dir_all(&tmp);
        }
        Ok(())
    })
    .await??;
    refresh_source_size(app, &rec.id).await;
    Ok(source_tree(app, &rec).await)
}

/// Einzelne Dateien schreiben/löschen (MCP, Web). Inhalt als Bytes.
pub async fn write_files(
    app: &AppState,
    ctx: &Ctx,
    key: &str,
    files: Vec<(String, Vec<u8>)>,
    delete: Vec<String>,
    replace: bool,
) -> AppResult<SourceTree> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    let total: u64 = files.iter().map(|f| f.1.len() as u64).sum();
    ensure_upload_source(app, &owner, &rec, total).await?;
    if files.len() > 5000 {
        return Err(AppError::bad("Höchstens 5000 Dateien pro Aufruf"));
    }
    let dir = source_dir(app, &rec.id);
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        if replace {
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir_all(&dir)?;
        for p in delete {
            let path = build::safe_join(&dir, &p).map_err(|e| AppError::bad(e.to_string()))?;
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
        for (p, data) in files {
            let path = build::safe_join(&dir, &p).map_err(|e| AppError::bad(e.to_string()))?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, data)?;
        }
        Ok(())
    })
    .await??;
    refresh_source_size(app, &rec.id).await;
    Ok(source_tree(app, &rec).await)
}

pub async fn read_source_file(
    app: &AppState,
    ctx: &Ctx,
    key: &str,
    path: &str,
) -> AppResult<String> {
    let rec = find(app, &ctx.user, key).await?;
    let p = build::safe_join(&source_dir(app, &rec.id), path)
        .map_err(|e| AppError::bad(e.to_string()))?;
    let meta = tokio::fs::metadata(&p).await?;
    if meta.len() > 512 * 1024 {
        return Err(AppError::bad("Datei zu gross (max. 512 KB)"));
    }
    String::from_utf8(tokio::fs::read(&p).await?).map_err(|_| AppError::bad("Binärdatei"))
}

/* ------------------------------------------------------------------ */
/*                              Container                             */
/* ------------------------------------------------------------------ */

fn container_spec(app: &AppState, owner: &User, rec: &AppRecord, image: &str) -> Value {
    let net = network_name(&owner.username);
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    if let Some(p) = rec.ports.first() {
        env.insert("PORT".into(), p.container.to_string());
    }
    env.insert("DATA_DIR".into(), "/data".into());
    env.insert("NRZVPS_APP".into(), rec.name.clone());
    for (k, v) in &rec.env {
        env.insert(k.clone(), v.clone());
    }
    let mut exposed = serde_json::Map::new();
    let mut bindings = serde_json::Map::new();
    for p in &rec.ports {
        let key = format!("{}/{}", p.container, p.protocol.as_str());
        exposed.insert(key.clone(), json!({}));
        let entry = bindings.entry(key).or_insert_with(|| json!([]));
        if let Some(a) = entry.as_array_mut() {
            a.push(json!({ "HostIp": app.config.bind_ip, "HostPort": p.host.to_string() }));
        }
    }
    let mut spec = json!({
        "Image": image,
        "Hostname": rec.name,
        "Env": env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
        "Labels": { "nrzvps": "1", "nrzvps.app": rec.id, "nrzvps.user": owner.id },
        "ExposedPorts": exposed,
        "HostConfig": {
            "NanoCpus": (rec.cpus * 1e9) as u64,
            "Memory": rec.memory_mb * MB,
            "MemorySwap": rec.memory_mb * MB,
            "PidsLimit": 256,
            "Privileged": false,
            "CapDrop": ["ALL"],
            "CapAdd": ["CHOWN", "DAC_OVERRIDE", "FOWNER", "SETGID", "SETUID", "NET_BIND_SERVICE", "KILL"],
            "SecurityOpt": ["no-new-privileges:true"],
            "Init": true,
            "PortBindings": bindings,
            "RestartPolicy": { "Name": "on-failure", "MaximumRetryCount": 5 },
            "LogConfig": { "Type": "json-file", "Config": { "max-size": "5m", "max-file": "2" } },
            "Mounts": [{ "Type": "volume", "Source": volume_name(&rec.id), "Target": "/data" }],
            "NetworkMode": net,
            "Ulimits": [{ "Name": "nofile", "Soft": 4096, "Hard": 8192 }],
        },
        "NetworkingConfig": { "EndpointsConfig": { net.clone(): { "Aliases": [rec.name] } } },
    });
    if !rec.start_command.is_empty() {
        spec["Cmd"] = json!(["sh", "-c", rec.start_command]);
        // Bei eigenen Images den Entrypoint nicht doppelt ausführen
        if matches!(rec.source, Source::Image { .. }) {
            spec["Entrypoint"] = json!([]);
        }
    }
    spec
}

/// Container neu anlegen (aktuelle Einstellungen) und starten. Prüft das Kontingent.
pub async fn recreate_and_start(app: &AppState, owner: &User, rec: &AppRecord) -> AppResult<()> {
    let image = rec
        .image
        .clone()
        .ok_or_else(|| AppError::bad("Noch nicht deployt – zuerst `deploy` ausführen"))?;
    check_resources(app, owner, rec).await?;
    let lock = app.app_lock(&rec.id).await;
    let _g = lock.lock().await;
    let name = container_name(&owner.username, &rec.name);
    app.docker
        .ensure_network(
            &network_name(&owner.username),
            json!({ "nrzvps": "1", "nrzvps.user": owner.id }),
        )
        .await?;
    app.docker.remove_container(&name).await?;
    app.docker
        .create_container(&name, container_spec(app, owner, rec, &image))
        .await
        .map_err(|e| AppError::internal(format!("Container anlegen: {e}")))?;
    if let Err(e) = app.docker.start(&name).await {
        return Err(AppError::internal(format!("Start fehlgeschlagen: {e}")));
    }
    app.live.write().await.insert(
        rec.id.clone(),
        Live {
            status: AppStatus::Running,
            started_at: Some(now()),
            stats: None,
        },
    );
    Ok(())
}

pub async fn start(app: &AppState, ctx: &Ctx, key: &str) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    if app.is_building(&rec.id).await && rec.image.is_none() {
        return Err(AppError::conflict("Erstes Deployment läuft noch"));
    }
    recreate_and_start(app, &owner, &rec).await?;
    set_auto_start(app, &rec.id, true).await?;
    Ok(view_ctx(app, ctx, &app.store.app(&rec.id).await.unwrap_or(rec)).await)
}

pub async fn stop(app: &AppState, ctx: &Ctx, key: &str) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    stop_container(app, &owner, &rec).await?;
    set_auto_start(app, &rec.id, false).await?;
    Ok(view_ctx(app, ctx, &app.store.app(&rec.id).await.unwrap_or(rec)).await)
}

pub async fn stop_container(app: &AppState, owner: &User, rec: &AppRecord) -> AppResult<()> {
    let lock = app.app_lock(&rec.id).await;
    let _g = lock.lock().await;
    app.docker
        .stop(&container_name(&owner.username, &rec.name), 10)
        .await?;
    if let Some(l) = app.live.write().await.get_mut(&rec.id) {
        l.status = AppStatus::Stopped;
        l.stats = None;
    }
    Ok(())
}

async fn set_auto_start(app: &AppState, id: &str, v: bool) -> AppResult<()> {
    let id = id.to_string();
    app.store
        .update(|s| {
            if let Some(a) = s.apps.iter_mut().find(|a| a.id == id) {
                a.auto_start = v;
            }
        })
        .await?;
    Ok(())
}

pub async fn logs(app: &AppState, ctx: &Ctx, key: &str, lines: u32) -> AppResult<Vec<String>> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    match app
        .docker
        .logs(
            &container_name(&owner.username, &rec.name),
            lines.clamp(1, 5000),
        )
        .await
    {
        Ok(l) => Ok(l),
        Err(e) if crate::docker::is_not_found(&e) => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

pub async fn build_log(app: &AppState, ctx: &Ctx, key: &str) -> AppResult<Vec<String>> {
    let rec = find(app, &ctx.user, key).await?;
    if let Some(l) = app
        .build_logs
        .lock()
        .ok()
        .and_then(|m| m.get(&rec.id).cloned())
    {
        return Ok(l);
    }
    let raw = tokio::fs::read_to_string(app.config.builds_dir().join(format!("{}.log", rec.id)))
        .await
        .unwrap_or_default();
    Ok(raw.lines().map(str::to_string).collect())
}

pub async fn exec(app: &AppState, ctx: &Ctx, key: &str, command: &str) -> AppResult<(i64, String)> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    if !is_running(app.live.read().await.get(&rec.id)) {
        return Err(AppError::bad("App läuft nicht – zuerst starten"));
    }
    if command.trim().is_empty() || command.len() > 8000 {
        return Err(AppError::bad("Befehl fehlt oder ist zu lang"));
    }
    let (code, mut out) = app
        .docker
        .exec(
            &container_name(&owner.username, &rec.name),
            command,
            Duration::from_secs(60),
        )
        .await?;
    if out.len() > 64 * 1024 {
        let cut = out.len() - 64 * 1024;
        let cut = (cut..out.len())
            .find(|i| out.is_char_boundary(*i))
            .unwrap_or(cut);
        out = format!("… (gekürzt)\n{}", &out[cut..]);
    }
    Ok((code, out))
}

/// App mit Container, Images, Daten und Quellcode löschen.
pub async fn delete(app: &AppState, rec: &AppRecord) -> AppResult<()> {
    let owner = owner_of(app, rec).await?;
    let lock = app.app_lock(&rec.id).await;
    let _g = lock.lock().await;
    app.docker
        .remove_container(&container_name(&owner.username, &rec.name))
        .await?;
    remove_app_images(app, rec).await;
    let _ = app.docker.remove_volume(&volume_name(&rec.id)).await;
    let _ = tokio::fs::remove_dir_all(source_dir(app, &rec.id)).await;
    let _ = tokio::fs::remove_file(app.config.builds_dir().join(format!("{}.log", rec.id))).await;
    let id = rec.id.clone();
    app.store.update(|s| s.apps.retain(|a| a.id != id)).await?;
    app.live.write().await.remove(&rec.id);
    {
        let mut sz = app.sizes.write().await;
        sz.images.remove(&rec.id);
        sz.volumes.remove(&rec.id);
        sz.sources.remove(&rec.id);
    }
    let _ = app.docker.prune_dangling(LABEL).await;
    Ok(())
}

/// Zwischen-Images eines früheren Builds löschen (neueste zuerst; noch benutzte bleiben).
async fn remove_layers(app: &AppState, layers: &[String]) -> u32 {
    let mut n = 0;
    for l in layers.iter().rev() {
        if app.docker.remove_image(l, false).await.is_ok() {
            n += 1;
        }
    }
    n
}

async fn remove_app_images(app: &AppState, rec: &AppRecord) {
    if let Some(id) = &rec.image_id {
        let _ = app.docker.remove_image(id, true).await;
    }
    if let Some(img) = &rec.image
        && img.starts_with("nrzvps/")
    {
        let _ = app.docker.remove_image(img, true).await;
    }
    remove_layers(app, &rec.build_layers).await;
}

/* ------------------------------------------------------------------ */
/*                              Deployment                            */
/* ------------------------------------------------------------------ */

fn push_log(app: &AppState, id: &str, line: impl Into<String>) {
    if let Ok(mut m) = app.build_logs.lock() {
        let v = m.entry(id.to_string()).or_default();
        v.push(line.into());
        if v.len() > 5000 {
            v.drain(..1000);
        }
    }
}

/// Startet ein Deployment im Hintergrund. Fehler, wenn schon eins läuft.
pub async fn deploy(app: &AppState, ctx: &Ctx, key: &str) -> AppResult<App> {
    let rec = find(app, &ctx.user, key).await?;
    let owner = owner_of(app, &rec).await?;
    start_deploy(app, &owner, &rec).await?;
    Ok(view_ctx(app, ctx, &app.store.app(&rec.id).await.unwrap_or(rec)).await)
}

pub async fn start_deploy(app: &AppState, owner: &User, rec: &AppRecord) -> AppResult<()> {
    if owner.disabled {
        return Err(AppError::Forbidden("Konto gesperrt".into()));
    }
    {
        let mut b = app.building.lock().await;
        if b.contains(&rec.id) {
            return Err(AppError::conflict(
                "Es läuft schon ein Deployment für diese App",
            ));
        }
        b.insert(rec.id.clone());
    }
    if let Ok(mut m) = app.build_logs.lock() {
        m.insert(rec.id.clone(), Vec::new());
    }
    let id = rec.id.clone();
    let number = app
        .store
        .update(|s| {
            let a = s.apps.iter_mut().find(|a| a.id == id)?;
            a.deploy_count += 1;
            a.deploy = Some(DeployInfo {
                number: a.deploy_count,
                started_at: now(),
                message: "Wird vorbereitet …".into(),
                ..Default::default()
            });
            Some(a.deploy_count)
        })
        .await?
        .unwrap_or(1);
    let app2 = Arc::clone(app);
    let owner = owner.clone();
    let rec = rec.clone();
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        push_log(
            &app2,
            &rec.id,
            format!(
                "== Deployment #{number} von {} ({}) ==",
                rec.name,
                rec.source.kind()
            ),
        );
        let result = run_deploy(&app2, &owner, &rec, number).await;
        let (ok, message) = match &result {
            Ok(m) => (true, m.clone()),
            Err(e) => (false, e.to_string()),
        };
        push_log(
            &app2,
            &rec.id,
            format!(
                "== {} nach {} s: {message} ==",
                if ok { "Fertig" } else { "Fehlgeschlagen" },
                started.elapsed().as_secs()
            ),
        );
        let lines = app2
            .build_logs
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&rec.id))
            .unwrap_or_default();
        let _ = tokio::fs::create_dir_all(app2.config.builds_dir()).await;
        let _ = tokio::fs::write(
            app2.config.builds_dir().join(format!("{}.log", rec.id)),
            lines.join("\n"),
        )
        .await;
        let id = rec.id.clone();
        let _ = app2
            .store
            .update(|s| {
                if let Some(d) = s
                    .apps
                    .iter_mut()
                    .find(|a| a.id == id)
                    .and_then(|a| a.deploy.as_mut())
                {
                    d.ok = Some(ok);
                    d.finished_at = Some(now());
                    d.message = message;
                }
            })
            .await;
        app2.building.lock().await.remove(&rec.id);
        crate::monitor::refresh_sizes(&app2).await;
    });
    Ok(())
}

async fn set_deploy_message(
    app: &AppState,
    id: &str,
    msg: &str,
    runtime: Option<Runtime>,
    commit: Option<String>,
) {
    let id = id.to_string();
    let msg = msg.to_string();
    let _ = app
        .store
        .update(|s| {
            if let Some(d) = s
                .apps
                .iter_mut()
                .find(|a| a.id == id)
                .and_then(|a| a.deploy.as_mut())
            {
                d.message = msg;
                if runtime.is_some() {
                    d.runtime = runtime;
                }
                if commit.is_some() {
                    d.commit = commit;
                }
            }
        })
        .await;
}

async fn run_deploy(
    app: &AppState,
    owner: &User,
    rec: &AppRecord,
    number: u32,
) -> AppResult<String> {
    let settings = app.store.settings().await;
    let id = rec.id.clone();
    let log = |l: String| push_log(app, &id, l);
    let dir = source_dir(app, &rec.id);

    // 1. Quelle holen
    let (new_image, new_id, base_images, layers) = match &rec.source {
        Source::Image {
            image,
            username,
            password,
        } => {
            set_deploy_message(app, &rec.id, "Image wird geladen …", None, None).await;
            let img = app.config.rewrite_image(image);
            log(format!("Lade Image {img}"));
            let reg = crate::docker::registry_of(&img);
            let auth = match (username, password) {
                (Some(u), Some(p)) => Some((u.as_str(), p.as_str(), reg.as_str())),
                _ => None,
            };
            app.docker
                .pull(&img, auth, |l| push_log(app, &id, l))
                .await?;
            let (iid, _) = app.docker.image_inspect(&img).await?;
            (img, iid, vec![], vec![])
        }
        src => {
            let mut commit = None;
            if let Source::Git {
                url, branch, token, ..
            } = src
            {
                set_deploy_message(app, &rec.id, "Git-Repository wird geklont …", None, None).await;
                commit = Some(git_fetch(app, &rec.id, url, branch, token.as_deref()).await?);
                refresh_source_size(app, &rec.id).await;
            }
            let build_dir = match src {
                Source::Git { subdir, .. } if !subdir.is_empty() => {
                    build::safe_join(&dir, subdir).map_err(|e| AppError::bad(e.to_string()))?
                }
                _ => dir.clone(),
            };
            if build::list_files(&build_dir).is_empty() {
                return Err(AppError::bad(
                    "Kein Quellcode vorhanden – zuerst Dateien hochladen (MCP `upload_files` oder Web)",
                ));
            }
            // Speicherplatz vor dem Build prüfen
            let u = usage(app, &owner.id, None).await;
            if u.disk_mb > owner.quota.disk_mb {
                return Err(AppError::Forbidden(format!(
                    "Speicherplatz voll ({} von {} MB)",
                    u.disk_mb, owner.quota.disk_mb
                )));
            }
            let plan = {
                let (bd, rt, bc, sc) = (
                    build_dir.clone(),
                    rec.runtime,
                    rec.build_command.clone(),
                    rec.start_command.clone(),
                );
                tokio::task::spawn_blocking(move || build::plan(&bd, rt, &bc, &sc))
                    .await?
                    .map_err(|e| AppError::bad(e.to_string()))?
            };
            log(format!("Laufzeit: {}", plan.runtime.label()));
            set_deploy_message(
                app,
                &rec.id,
                &format!("Wartet auf Build-Platz ({}) …", plan.runtime.label()),
                Some(plan.runtime),
                commit,
            )
            .await;
            if app.build_lock.try_lock().is_err() {
                log("Ein anderer Build läuft – warte …".into());
            }
            let _build_guard = app.build_lock.lock().await;
            set_deploy_message(
                app,
                &rec.id,
                &format!("Wird gebaut ({}) …", plan.runtime.label()),
                None,
                None,
            )
            .await;
            log(format!(
                "Build-Budget: {:.2} CPU, {} MB RAM, max. {} min",
                settings.build_cpus, settings.build_memory_mb, settings.build_timeout_min
            ));
            let ctx_tar = {
                let (bd, df, limit) = (
                    build_dir.clone(),
                    plan.dockerfile.clone(),
                    settings.max_upload_mb.max(50) * MB * 4,
                );
                tokio::task::spawn_blocking(move || build::tar_context(&bd, df.as_deref(), limit))
                    .await?
                    .map_err(|e| AppError::bad(e.to_string()))?
            };
            let tag = image_tag(&owner.username, &rec.name, number);
            let labels = json!({ "nrzvps": "1", "nrzvps.app": rec.id, "nrzvps.user": owner.id });
            let build = app.docker.build(
                ctx_tar,
                &tag,
                &labels,
                (settings.build_cpus * 1e9) as u64,
                settings.build_memory_mb * MB,
                |l| push_log(app, &id, l),
            );
            let res = tokio::time::timeout(
                Duration::from_secs(settings.build_timeout_min.max(1) * 60),
                build,
            )
            .await;
            let iid = match res {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    let _ = app.docker.prune_dangling(LABEL).await;
                    return Err(AppError::bad(format!("{e}")));
                }
                Err(_) => {
                    let _ = app.docker.prune_dangling(LABEL).await;
                    return Err(AppError::bad(format!(
                        "Build-Zeitlimit ({} min) überschritten",
                        settings.build_timeout_min
                    )));
                }
            };
            (
                tag,
                iid.0,
                if settings.remove_base_images {
                    plan.base_images
                } else {
                    vec![]
                },
                iid.1,
            )
        }
    };

    // 2. Grösse prüfen (streng)
    let (_, size) = app.docker.image_inspect(&new_id).await?;
    log(format!("Image fertig: {} MB", size / MB));
    let old_size = app
        .sizes
        .read()
        .await
        .images
        .get(&rec.id)
        .copied()
        .unwrap_or(0);
    let u = usage(app, &owner.id, None).await;
    let after_mb = (u.disk_mb * MB)
        .saturating_sub(old_size)
        .saturating_add(size)
        / MB;
    if after_mb > owner.quota.disk_mb {
        if rec.image_id.as_deref() != Some(new_id.as_str()) {
            let _ = app.docker.remove_image(&new_id, true).await;
        }
        return Err(AppError::Forbidden(format!(
            "Image ({} MB) passt nicht mehr in deinen Speicherplatz ({} MB)",
            size / MB,
            owner.quota.disk_mb
        )));
    }

    // 3. Umschalten: neues Image eintragen, Container neu starten
    let current = app
        .store
        .app(&rec.id)
        .await
        .ok_or_else(|| AppError::not_found("App wurde gelöscht"))?;
    let (old_image, old_id, old_layers) = (
        current.image.clone(),
        current.image_id.clone(),
        current.build_layers.clone(),
    );
    let (ni, nid) = (new_image.clone(), new_id.clone());
    let rid = rec.id.clone();
    let updated = app
        .store
        .update(|s| {
            let a = s.apps.iter_mut().find(|a| a.id == rid)?;
            a.image = Some(ni);
            a.image_id = Some(nid);
            a.build_layers = layers;
            Some(a.clone())
        })
        .await?
        .ok_or_else(|| AppError::not_found("App wurde gelöscht"))?;
    app.sizes.write().await.images.insert(rec.id.clone(), size);
    let message = if updated.auto_start {
        match recreate_and_start(app, owner, &updated).await {
            Ok(()) => {
                log("Container läuft.".into());
                format!(
                    "Läuft ({})",
                    updated
                        .ports
                        .first()
                        .map(|p| format!("Port {}", p.host))
                        .unwrap_or_else(|| "ohne Port".into())
                )
            }
            Err(e) => {
                log(format!("Nicht gestartet: {e}"));
                format!("Gebaut, aber nicht gestartet: {e}")
            }
        }
    } else {
        // Gestoppte App bleibt gestoppt; alter Container weg, damit das alte Image frei wird
        let _ = app
            .docker
            .remove_container(&container_name(&owner.username, &rec.name))
            .await;
        "Gebaut (App ist gestoppt – starte sie, um die neue Version zu nutzen)".into()
    };

    // 4. Altes Image SOFORT löschen (Speicher!), dazu Reste und Basis-Images
    if old_id.is_some() && old_id.as_deref() != Some(new_id.as_str()) {
        let target = old_id.clone().or(old_image.clone()).unwrap_or_default();
        match app.docker.remove_image(&target, true).await {
            Ok(()) => log(format!(
                "Altes Image gelöscht ({}).",
                old_image.unwrap_or_default()
            )),
            Err(e) => log(format!("Altes Image konnte nicht gelöscht werden: {e}")),
        }
    }
    let n = remove_layers(app, &old_layers).await;
    if n > 0 {
        log(format!("{n} alte Zwischen-Images gelöscht."));
    }
    if let Ok((n, bytes)) = app.docker.prune_dangling(LABEL).await
        && n > 0
    {
        log(format!("{n} Zwischen-Images gelöscht ({} MB).", bytes / MB));
    }
    for b in base_images {
        if app.docker.remove_image(&b, false).await.is_ok() {
            log(format!("Basis-Image {b} entfernt."));
        }
    }
    Ok(message)
}

/// Klont das Repository flach nach `sources/<id>` (ersetzt den alten Stand). Liefert den Commit.
async fn git_fetch(
    app: &AppState,
    id: &str,
    url: &str,
    branch: &str,
    token: Option<&str>,
) -> AppResult<String> {
    let mut real = app.config.rewrite_git(url);
    if let Some(t) = token
        && let Some(pos) = real.find("://")
    {
        real.insert_str(pos + 3, &format!("oauth2:{}@", urlencoding::encode(t)));
    }
    let dir = source_dir(app, id);
    let tmp = app.config.sources_dir().join(format!("{id}.git"));
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    push_log(
        app,
        id,
        format!(
            "git clone {url}{}",
            if branch.is_empty() {
                String::new()
            } else {
                format!(" (Zweig {branch})")
            }
        ),
    );
    let mut cmd = tokio::process::Command::new("git");
    // Keine Passwort-Helfer/Abfragen (würden hängen), keine geerbte Git-Konfiguration
    cmd.args([
        "-c",
        "credential.helper=",
        "-c",
        "core.askPass=",
        "clone",
        "--depth",
        "1",
        "--single-branch",
    ]);
    cmd.env_remove("GIT_CONFIG_COUNT")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_ASKPASS", "true");
    if !branch.is_empty() {
        cmd.arg("--branch").arg(branch);
    }
    cmd.arg("--")
        .arg(&real)
        .arg(&tmp)
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(300), cmd.output())
        .await
        .map_err(|_| AppError::bad("git clone: Zeitlimit (5 min)"))?
        .map_err(|e| AppError::internal(format!("git nicht ausführbar: {e}")))?;
    let scrub = |s: &str| match token {
        Some(t) => s
            .replace(t, "****")
            .replace(&*urlencoding::encode(t), "****"),
        None => s.to_string(),
    };
    if !out.status.success() {
        let err = scrub(String::from_utf8_lossy(&out.stderr).trim());
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        return Err(AppError::bad(format!("git clone fehlgeschlagen: {err}")));
    }
    let head = tokio::process::Command::new("git")
        .arg("-C")
        .arg(&tmp)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .await;
    let commit = head
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let msg = tokio::process::Command::new("git")
        .arg("-C")
        .arg(&tmp)
        .args(["log", "-1", "--format=%s"])
        .output()
        .await;
    let subject = msg
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    push_log(app, id, format!("Commit {commit}: {subject}"));
    let _ = tokio::fs::remove_dir_all(tmp.join(".git")).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::rename(&tmp, &dir).await?;
    Ok(commit)
}
