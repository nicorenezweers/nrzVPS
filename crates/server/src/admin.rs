//! Admin: globale Ressourcen, Benutzer (max. 10, je 10 Ports), Systemzustand, Aufräumen.

use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use nrz_vps_common::{
    AdminUser, App, CleanupResult, CreateUser, MAX_USERS, Ok as OkResp, Quota, Settings,
    SystemInfo, UpdateUser, Usage, slot_ports,
};
use serde_json::Value;

use crate::{
    apps::ctx,
    auth::{AdminAuthed, hash_password},
    error::{AppError, AppResult},
    ops::{self, LABEL},
    state::AppState,
    store::{User, new_id, now, valid_name},
};

const MB: u64 = 1024 * 1024;
/// Basis-Images, die nrzVPS selbst für Builds nutzt (dürfen beim Aufräumen weg).
const BASE_IMAGES: &[&str] = &[
    "node:22-alpine",
    "oven/bun:1-alpine",
    "denoland/deno:alpine",
    "python:3.12-slim",
    "rust:1-slim",
    "debian:trixie-slim",
    "golang:1-alpine",
    "alpine:3",
    "php:8.3-cli-alpine",
    "composer:2",
    "nginx:alpine",
];

async fn admin_user(app: &AppState, u: &User) -> AdminUser {
    let (port_from, port_to) = slot_ports(u.slot);
    AdminUser {
        id: u.id.clone(),
        username: u.username.clone(),
        admin: u.admin,
        disabled: u.disabled,
        slot: u.slot,
        port_from,
        port_to,
        quota: u.quota,
        usage: ops::usage(app, &u.id, None).await,
        created_at: u.created_at,
        last_login: u.last_login,
    }
}

fn check_quota_shape(q: &Quota) -> AppResult<()> {
    if q.cpus < 0.0 || q.cpus > 256.0 || !q.cpus.is_finite() {
        return Err(AppError::bad("CPU-Kontingent ungültig"));
    }
    if q.max_apps > 50 {
        return Err(AppError::bad("Höchstens 50 Apps pro Benutzer"));
    }
    Ok(())
}

/// Summe aller Kontingente (+ Build-Budget) muss in die Gesamtwerte passen – keine Überbuchung.
fn check_totals(s: &Settings, quotas: &[Quota]) -> AppResult<()> {
    let cpus: f64 = quotas.iter().map(|q| q.cpus).sum::<f64>() + s.build_cpus;
    let mem: u64 = quotas.iter().map(|q| q.memory_mb).sum::<u64>() + s.build_memory_mb;
    let disk: u64 = quotas.iter().map(|q| q.disk_mb).sum();
    if cpus > s.total_cpus + 1e-6 {
        return Err(AppError::bad(format!(
            "CPU überbucht: Kontingente + Build-Budget = {cpus:.2} Kerne, gesamt erlaubt {:.2}. Senke Kontingente oder erhöhe die Gesamt-CPU.",
            s.total_cpus
        )));
    }
    if mem > s.total_memory_mb {
        return Err(AppError::bad(format!(
            "RAM überbucht: Kontingente + Build-Budget = {mem} MB, gesamt erlaubt {} MB.",
            s.total_memory_mb
        )));
    }
    if disk > s.total_disk_mb {
        return Err(AppError::bad(format!(
            "Speicher überbucht: Kontingente = {disk} MB, gesamt erlaubt {} MB.",
            s.total_disk_mb
        )));
    }
    Ok(())
}

pub async fn get_settings(State(app): State<AppState>, _a: AdminAuthed) -> Json<Settings> {
    Json(app.store.settings().await)
}

pub async fn put_settings(
    State(app): State<AppState>,
    _a: AdminAuthed,
    Json(mut s): Json<Settings>,
) -> AppResult<Json<Settings>> {
    if !(0.05..=256.0).contains(&s.total_cpus) || s.build_cpus < 0.05 || s.build_cpus > s.total_cpus
    {
        return Err(AppError::bad(
            "Gesamt-CPU bzw. Build-CPU ungültig (Build mind. 0.05)",
        ));
    }
    if s.build_memory_mb < 256 {
        return Err(AppError::bad("Build-RAM mindestens 256 MB"));
    }
    check_quota_shape(&s.default_quota)?;
    s.public_host = s.public_host.trim().to_string();
    s.max_upload_mb = s.max_upload_mb.clamp(1, 4096);
    s.build_timeout_min = s.build_timeout_min.clamp(1, 240);
    let quotas: Vec<Quota> = app
        .store
        .read(|st| st.users.iter().map(|u| u.quota).collect())
        .await;
    check_totals(&s, &quotas)?;
    let s2 = s.clone();
    app.store.update(|st| st.settings = s2).await?;
    Ok(Json(s))
}

pub async fn system(State(app): State<AppState>, _a: AdminAuthed) -> Json<SystemInfo> {
    let (docker_ok, docker_version) = match app.docker.version().await {
        Ok(v) => (true, v),
        Err(e) => (false, e.to_string()),
    };
    let info = app
        .docker
        .json(hyper::Method::GET, "/info", None)
        .await
        .unwrap_or(Value::Null);
    let (users, apps, settings) = app
        .store
        .read(|s| (s.users.clone(), s.apps.clone(), s.settings.clone()))
        .await;
    let mut allocated = Quota {
        cpus: 0.0,
        memory_mb: 0,
        disk_mb: 0,
        max_apps: 0,
    };
    let mut in_use = Usage::default();
    for u in &users {
        allocated.cpus += u.quota.cpus;
        allocated.memory_mb += u.quota.memory_mb;
        allocated.disk_mb += u.quota.disk_mb;
        allocated.max_apps += u.quota.max_apps;
        let us = ops::usage(&app, &u.id, None).await;
        in_use.cpus += us.cpus;
        in_use.memory_mb += us.memory_mb;
        in_use.disk_mb += us.disk_mb;
        in_use.apps += us.apps;
        in_use.ports_used += us.ports_used;
    }
    let sz = app.sizes.read().await;
    let live = app.live.read().await;
    Json(SystemInfo {
        version: nrz_vps_common::VERSION.to_string(),
        docker_ok,
        docker_version,
        host_cpus: info.get("NCPU").and_then(Value::as_u64).unwrap_or(0) as u32,
        host_memory_mb: info.get("MemTotal").and_then(Value::as_u64).unwrap_or(0) / MB,
        allocated,
        in_use,
        images_bytes: sz.images.values().sum(),
        volumes_bytes: sz.volumes.values().sum(),
        sources_bytes: sz.sources.values().sum(),
        users: users.len() as u32,
        apps: apps.len() as u32,
        running: live
            .values()
            .filter(|l| l.status == nrz_vps_common::AppStatus::Running)
            .count() as u32,
        settings: Some(settings),
    })
}

pub async fn list_users(State(app): State<AppState>, _a: AdminAuthed) -> Json<Vec<AdminUser>> {
    let mut users = app.store.read(|s| s.users.clone()).await;
    users.sort_by_key(|u| u.slot);
    let mut out = Vec::new();
    for u in &users {
        out.push(admin_user(&app, u).await);
    }
    Json(out)
}

pub async fn create_user(
    State(app): State<AppState>,
    _a: AdminAuthed,
    Json(req): Json<CreateUser>,
) -> AppResult<Json<AdminUser>> {
    let username = req.username.trim().to_lowercase();
    if !valid_name(&username) {
        return Err(AppError::bad(
            "Benutzername: 2–30 Zeichen, Kleinbuchstaben, Ziffern und -",
        ));
    }
    let hash = hash_password(&req.password)?;
    let settings = app.store.settings().await;
    let quota = req.quota.unwrap_or(settings.default_quota);
    check_quota_shape(&quota)?;
    let mut quotas: Vec<Quota> = app
        .store
        .read(|s| s.users.iter().map(|u| u.quota).collect())
        .await;
    quotas.push(quota);
    check_totals(&settings, &quotas)?;
    let id = new_id(8);
    let uname = username.clone();
    let created = app
        .store
        .update(|s| {
            if s.users.len() >= MAX_USERS {
                return Err(AppError::Forbidden(format!(
                    "Höchstens {MAX_USERS} Benutzer (Ports 34000–34099, je 10)"
                )));
            }
            if s.users.iter().any(|u| u.username == uname) {
                return Err(AppError::conflict(format!(
                    "Benutzer `{uname}` gibt es schon"
                )));
            }
            let slot = (0..MAX_USERS as u8)
                .find(|i| !s.users.iter().any(|u| u.slot == *i))
                .ok_or_else(|| AppError::Forbidden("Kein freier Slot".into()))?;
            let u = User {
                id: id.clone(),
                username: uname,
                password_hash: hash,
                admin: req.admin,
                slot,
                quota,
                created_at: now(),
                ..Default::default()
            };
            s.users.push(u.clone());
            Ok(u)
        })
        .await??;
    Ok(Json(admin_user(&app, &created).await))
}

pub async fn update_user(
    State(app): State<AppState>,
    a: AdminAuthed,
    Path(uid): Path<String>,
    Json(req): Json<UpdateUser>,
) -> AppResult<Json<AdminUser>> {
    let target = app
        .store
        .user(&uid)
        .await
        .ok_or_else(|| AppError::not_found("Benutzer nicht gefunden"))?;
    if uid == a.user.id && (req.disabled == Some(true) || req.admin == Some(false)) {
        return Err(AppError::bad(
            "Du kannst dich nicht selbst sperren oder dir die Admin-Rechte nehmen",
        ));
    }
    if let Some(q) = &req.quota {
        check_quota_shape(q)?;
        let settings = app.store.settings().await;
        let quotas: Vec<Quota> = app
            .store
            .read(|s| {
                s.users
                    .iter()
                    .map(|u| if u.id == uid { *q } else { u.quota })
                    .collect()
            })
            .await;
        check_totals(&settings, &quotas)?;
    }
    let hash = match &req.password {
        Some(p) if !p.is_empty() => Some(hash_password(p)?),
        _ => None,
    };
    let disable = req.disabled == Some(true) && !target.disabled;
    let u2 = uid.clone();
    let updated = app
        .store
        .update(|s| {
            let u = s.users.iter_mut().find(|u| u.id == u2)?;
            if let Some(q) = req.quota {
                u.quota = q;
            }
            if let Some(d) = req.disabled {
                u.disabled = d;
            }
            if let Some(ad) = req.admin {
                u.admin = ad;
            }
            if let Some(h) = hash {
                u.password_hash = h;
            }
            let u = u.clone();
            if disable {
                s.sessions.retain(|_, v| v.user_id != u2);
            }
            Some(u)
        })
        .await?
        .ok_or_else(|| AppError::not_found("Benutzer nicht gefunden"))?;
    // Gesperrt oder Kontingent kleiner: laufende Apps, die nicht mehr passen, stoppen
    let apps: Vec<_> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .filter(|x| x.owner_id == uid)
                .cloned()
                .collect()
        })
        .await;
    let mut cpus = 0.0;
    let mut mem = 0;
    for rec in apps {
        let running = app
            .live
            .read()
            .await
            .get(&rec.id)
            .is_some_and(|l| l.status == nrz_vps_common::AppStatus::Running);
        if !running {
            continue;
        }
        if updated.disabled
            || cpus + rec.cpus > updated.quota.cpus + 1e-6
            || mem + rec.memory_mb > updated.quota.memory_mb
        {
            let _ = ops::stop_container(&app, &updated, &rec).await;
        } else {
            cpus += rec.cpus;
            mem += rec.memory_mb;
        }
    }
    Ok(Json(admin_user(&app, &updated).await))
}

pub async fn delete_user(
    State(app): State<AppState>,
    a: AdminAuthed,
    Path(uid): Path<String>,
) -> AppResult<Json<OkResp>> {
    if uid == a.user.id {
        return Err(AppError::bad("Du kannst dich nicht selbst löschen"));
    }
    let target = app
        .store
        .user(&uid)
        .await
        .ok_or_else(|| AppError::not_found("Benutzer nicht gefunden"))?;
    let apps: Vec<_> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .filter(|x| x.owner_id == uid)
                .cloned()
                .collect()
        })
        .await;
    for rec in &apps {
        ops::delete(&app, rec).await?;
    }
    let _ = app
        .docker
        .remove_network(&ops::network_name(&target.username))
        .await;
    app.store
        .update(|s| {
            s.users.retain(|u| u.id != uid);
            s.tokens.retain(|t| t.user_id != uid);
            s.sessions.retain(|_, v| v.user_id != uid);
        })
        .await?;
    Ok(Json(OkResp { ok: true }))
}

pub async fn all_apps(
    State(app): State<AppState>,
    a: AdminAuthed,
    headers: HeaderMap,
) -> Json<Vec<App>> {
    let c = ctx(&app, a.user, &headers).await;
    let recs = app.store.read(|s| s.apps.clone()).await;
    let mut out = Vec::new();
    for r in &recs {
        out.push(ops::view_ctx(&app, &c, r).await);
    }
    out.sort_by(|x, y| (&x.owner, &x.name).cmp(&(&y.owner, &y.name)));
    Json(out)
}

/// Löscht hängende nrzVPS-Images, verwaiste App-Images und ungenutzte Basis-Images.
pub async fn cleanup(
    State(app): State<AppState>,
    _a: AdminAuthed,
) -> AppResult<Json<CleanupResult>> {
    let mut res = CleanupResult::default();
    let (n, b) = app.docker.prune_dangling(LABEL).await.unwrap_or((0, 0));
    res.removed_images += n;
    res.freed_bytes += b;
    // Unbenannte, von nichts benutzte Images sind Reste (auch ohne Label, z. B. Build-Zwischenstufen)
    let (n, b) = app.docker.prune_all_dangling().await.unwrap_or((0, 0));
    res.removed_images += n;
    res.freed_bytes += b;
    let keep: Vec<String> = app
        .store
        .read(|s| s.apps.iter().filter_map(|a| a.image_id.clone()).collect())
        .await;
    for img in app.docker.list_images(LABEL).await.unwrap_or_default() {
        let id = img
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !keep.contains(&id)
            && app
                .store
                .read(|s| {
                    !s.apps
                        .iter()
                        .any(|a| a.image_id.as_deref() == Some(id.as_str()))
                })
                .await
        {
            let size = img.get("Size").and_then(Value::as_u64).unwrap_or(0);
            if app.docker.remove_image(&id, true).await.is_ok() {
                res.removed_images += 1;
                res.freed_bytes += size;
            }
        }
    }
    let building = !app.building.lock().await.is_empty();
    if !building {
        for b in BASE_IMAGES {
            if let Ok((_, size)) = app.docker.image_inspect(b).await
                && app.docker.remove_image(b, false).await.is_ok()
            {
                res.removed_images += 1;
                res.freed_bytes += size;
            }
        }
    }
    crate::monitor::refresh_sizes(&app).await;
    Ok(Json(res))
}
