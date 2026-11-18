//! HTTP-Routen für Apps, Übersicht und Git-Webhooks (Logik in `ops`).

use axum::{
    Json,
    body::Bytes,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, header},
};
use nrz_vps_common::{
    AddPort, App, CreateApp, ExecRequest, ExecResult, Logs, Ok as OkResp, Overview, PowerAction,
    PowerRequest, Source, SourceTree, UpdateApp,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    auth::{Authed, me},
    error::{AppError, AppResult},
    ops::{self, Ctx},
    state::AppState,
    store::User,
};

/// Anfrage-Kontext: Basis-URL von nrzVPS und Host für App-Adressen.
pub async fn ctx(app: &AppState, user: User, headers: &HeaderMap) -> Ctx {
    let h = |k: &str| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let host_hdr = h("x-forwarded-host")
        .or_else(|| h(header::HOST.as_str()))
        .unwrap_or_else(|| "localhost".into());
    let proto = h("x-forwarded-proto").unwrap_or_else(|| "http".into());
    let base = format!("{proto}://{host_hdr}");
    let settings_host = app.store.read(|s| s.settings.public_host.clone()).await;
    let host = if !settings_host.trim().is_empty() {
        settings_host.trim().to_string()
    } else if let Some(h) = &app.config.public_host {
        h.clone()
    } else if host_hdr.starts_with('[') {
        host_hdr
            .split(']')
            .next()
            .map(|s| format!("{s}]"))
            .unwrap_or(host_hdr.clone())
    } else {
        host_hdr
            .split(':')
            .next()
            .unwrap_or("localhost")
            .to_string()
    };
    Ctx { user, base, host }
}

pub async fn overview(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
) -> Json<Overview> {
    let c = ctx(&app, a.user.clone(), &headers).await;
    Json(Overview {
        me: me(&a.user),
        quota: a.user.quota,
        usage: ops::usage(&app, &a.user.id, None).await,
        public_host: c.host.clone(),
        free_ports: ops::free_ports(&app, &a.user).await,
        mcp_url: format!("{}/mcp", c.base),
        llm_url: format!("{}/llm.txt", c.base),
    })
}

pub async fn list(State(app): State<AppState>, a: Authed, headers: HeaderMap) -> Json<Vec<App>> {
    let c = ctx(&app, a.user.clone(), &headers).await;
    let recs: Vec<_> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .filter(|x| x.owner_id == a.user.id)
                .cloned()
                .collect()
        })
        .await;
    let mut out = Vec::new();
    for r in &recs {
        out.push(ops::view_ctx(&app, &c, r).await);
    }
    out.sort_by(|x, y| x.name.cmp(&y.name));
    Json(out)
}

pub async fn create(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Json(req): Json<CreateApp>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::create(&app, &c, req).await?))
}

pub async fn get(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    let rec = ops::find(&app, &c.user, &id).await?;
    Ok(Json(ops::view_ctx(&app, &c, &rec).await))
}

pub async fn update(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateApp>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::update(&app, &c, &id, req).await?))
}

pub async fn delete(
    State(app): State<AppState>,
    a: Authed,
    Path(id): Path<String>,
) -> AppResult<Json<OkResp>> {
    let rec = ops::find(&app, &a.user, &id).await?;
    ops::delete(&app, &rec).await?;
    Ok(Json(OkResp { ok: true }))
}

pub async fn power(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<PowerRequest>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(power_action(&app, &c, &id, req.action).await?))
}

pub async fn power_action(
    app: &AppState,
    c: &Ctx,
    id: &str,
    action: PowerAction,
) -> AppResult<App> {
    match action {
        PowerAction::Start | PowerAction::Restart => ops::start(app, c, id).await,
        PowerAction::Stop => ops::stop(app, c, id).await,
    }
}

pub async fn deploy(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::deploy(&app, &c, &id).await?))
}

#[derive(Deserialize)]
pub struct LinesQuery {
    #[serde(default)]
    lines: Option<u32>,
}

pub async fn logs(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<LinesQuery>,
) -> AppResult<Json<Logs>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(Logs {
        lines: ops::logs(&app, &c, &id, q.lines.unwrap_or(200)).await?,
    }))
}

pub async fn build_log(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<Logs>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(Logs {
        lines: ops::build_log(&app, &c, &id).await?,
    }))
}

pub async fn source(
    State(app): State<AppState>,
    a: Authed,
    Path(id): Path<String>,
) -> AppResult<Json<SourceTree>> {
    let rec = ops::find(&app, &a.user, &id).await?;
    Ok(Json(ops::source_tree(&app, &rec).await))
}

#[derive(Deserialize)]
pub struct ReplaceQuery {
    #[serde(default)]
    replace: Option<bool>,
}

/// Multipart: ein Archiv (zip/tar.gz) oder mehrere Dateien (Dateiname = relativer Pfad).
pub async fn upload_multipart(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ReplaceQuery>,
    mut mp: Multipart,
) -> AppResult<Json<SourceTree>> {
    let c = ctx(&app, a.user, &headers).await;
    let replace = q.replace.unwrap_or(true);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::bad(e.to_string()))?
    {
        let name = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "datei".into());
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::bad(e.to_string()))?;
        files.push((name, data.to_vec()));
    }
    if files.is_empty() {
        return Err(AppError::bad("Keine Datei erhalten"));
    }
    let is_archive = |n: &str| {
        let n = n.to_lowercase();
        n.ends_with(".zip") || n.ends_with(".tar.gz") || n.ends_with(".tgz") || n.ends_with(".tar")
    };
    if files.len() == 1 && is_archive(&files[0].0) {
        let (_, data) = files.remove(0);
        return Ok(Json(
            ops::upload_archive(&app, &c, &id, data, replace).await?,
        ));
    }
    Ok(Json(
        ops::write_files(&app, &c, &id, files, vec![], replace).await?,
    ))
}

/// Rohes Archiv (zip/tar.gz) als Body – für `curl --data-binary @app.tar.gz`.
pub async fn upload_raw(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<ReplaceQuery>,
    body: Bytes,
) -> AppResult<Json<SourceTree>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(
        ops::upload_archive(&app, &c, &id, body.to_vec(), q.replace.unwrap_or(true)).await?,
    ))
}

pub async fn add_port(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<AddPort>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::add_port(&app, &c, &id, req).await?))
}

pub async fn remove_port(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path((id, host)): Path<(String, u16)>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::remove_port(&app, &c, &id, host).await?))
}

pub async fn exec(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ExecRequest>,
) -> AppResult<Json<ExecResult>> {
    let c = ctx(&app, a.user, &headers).await;
    let (exit_code, output) = ops::exec(&app, &c, &id, &req.command).await?;
    Ok(Json(ExecResult { exit_code, output }))
}

pub async fn webhook_secret(
    State(app): State<AppState>,
    a: Authed,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<App>> {
    let c = ctx(&app, a.user, &headers).await;
    Ok(Json(ops::new_webhook_secret(&app, &c, &id).await?))
}

/// Git-Webhook (nrzGit, Gitea, GitHub, GitLab): Push auf den eingestellten Zweig → neues Deployment.
pub async fn hook(
    State(app): State<AppState>,
    Path((id, secret)): Path<(String, String)>,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let rec = app
        .store
        .app(&id)
        .await
        .filter(|r| !r.webhook_secret.is_empty() && r.webhook_secret == secret);
    let Some(rec) = rec else {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        return Err(AppError::not_found("Unbekannter Webhook"));
    };
    if !rec.auto_deploy {
        return Ok(Json(
            serde_json::json!({ "ok": true, "skipped": "Automatisches Deployment ist aus" }),
        ));
    }
    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if let Some(r) = payload.get("ref").and_then(Value::as_str) {
        if r.starts_with("refs/tags/") {
            return Ok(Json(
                serde_json::json!({ "ok": true, "skipped": "Tag-Push" }),
            ));
        }
        if let Source::Git { branch, .. } = &rec.source
            && !branch.is_empty()
            && r != format!("refs/heads/{branch}")
        {
            return Ok(Json(
                serde_json::json!({ "ok": true, "skipped": format!("anderer Zweig ({r})") }),
            ));
        }
    }
    let owner = app
        .store
        .user(&rec.owner_id)
        .await
        .ok_or_else(|| AppError::not_found("Besitzer fehlt"))?;
    ops::start_deploy(&app, &owner, &rec).await?;
    Ok(Json(
        serde_json::json!({ "ok": true, "deploying": rec.name }),
    ))
}
