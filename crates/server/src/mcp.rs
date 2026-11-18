//! MCP-Server (Model Context Protocol, Streamable HTTP, JSON-RPC 2.0) unter `POST /mcp`.
//! Anmeldung: `Authorization: Bearer nrzvps_…` (Token aus „Mit KI hosten“). Jeder Benutzer sieht nur
//! seine eigenen Apps.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use nrz_vps_common::{
    AddPort, AppStatus, CreateApp, PowerAction, Protocol, Runtime, Source, UpdateApp,
};
use serde_json::{Value, json};

use crate::{
    apps::{ctx, power_action},
    auth::authenticate,
    error::{AppError, AppResult},
    ops::{self, Ctx},
    state::AppState,
};

const PROTOCOL: &str = "2025-06-18";

pub async fn get_info() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        "nrzVPS MCP: POST JSON-RPC an /mcp (Authorization: Bearer <Token>). Anleitung: /llm.txt",
    )
        .into_response()
}

pub async fn handle(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> Response {
    let Some(user) = authenticate(&app, &headers).await else {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "Token fehlt oder ungültig – in nrzVPS unter „Mit KI hosten“ erstellen"}))).into_response();
    };
    let c = ctx(&app, user, &headers).await;
    if let Value::Array(items) = req {
        let mut out = Vec::new();
        for item in items {
            if let Some(r) = dispatch(&app, &c, item).await {
                out.push(r);
            }
        }
        return Json(Value::Array(out)).into_response();
    }
    match dispatch(&app, &c, req).await {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

const INSTRUCTIONS: &str = "nrzVPS hostet Apps des Benutzers als Container – aus Quellcode (ohne Dockerfile, Laufzeit wird erkannt), \
aus einem Git-Repository (z. B. nrzGit) oder aus einem fertigen Image. Ablauf: get_info → create_app → upload_files (Quelle upload) → \
deploy → get_app/logs prüfen → dem Benutzer die URL nennen. Die App muss auf der Umgebungsvariable PORT lauschen (0.0.0.0), \
Daten dauerhaft unter /data (DATA_DIR) speichern. Strenge Grenzen: CPU/RAM/Speicher-Kontingent und 10 Ports pro Benutzer. \
Bei Updates wird das alte Image automatisch gelöscht. Vollständige Anleitung: GET /llm.txt.";

async fn dispatch(app: &AppState, c: &Ctx, req: Value) -> Option<Value> {
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
            "capabilities": { "tools": { "listChanged": false }, "resources": {} },
            "serverInfo": { "name": "nrzVPS", "version": nrz_vps_common::VERSION },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(app, c, &name, &args).await {
                Ok(v) => json!({
                    "content": [{ "type": "text", "text": if let Value::String(s) = &v { s.clone() } else { serde_json::to_string_pretty(&v).unwrap_or_default() } }],
                    "isError": false
                }),
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true })
                }
            })
        }
        "resources/list" => Ok(
            json!({ "resources": [{ "uri": "nrzvps://llm.txt", "name": "nrzVPS-Anleitung", "mimeType": "text/plain" }] }),
        ),
        "resources/read" => Ok(
            json!({ "contents": [{ "uri": "nrzvps://llm.txt", "mimeType": "text/plain", "text": crate::api::llm_text(&c.base) }] }),
        ),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        _ => Err((-32601, format!("Unbekannte Methode: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, msg)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
        }
    })
}

fn tool(name: &str, description: &str, props: Value, required: &[&str]) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": props, "required": required } })
}

fn tools() -> Vec<Value> {
    let app = json!({ "type": "string", "description": "App-Name (oder ID)" });
    let runtimes: Vec<&str> = Runtime::ALL.iter().map(|r| r.as_str()).collect();
    let source_props = json!({
        "source": { "type": "string", "enum": ["upload", "git", "image"], "description": "upload = Quellcode per upload_files; git = Repository klonen; image = fertiges Image" },
        "git_url": { "type": "string", "description": "z. B. http://100.78.107.162:8390/besitzer/repo.git" },
        "git_branch": { "type": "string", "description": "leer = Standardzweig" },
        "git_subdir": { "type": "string", "description": "Unterordner im Repo (leer = Wurzel)" },
        "git_token": { "type": "string", "description": "Zugriffstoken für private Repositories" },
        "image": { "type": "string", "description": "z. B. nginx:alpine oder 100.78.107.162:8390/besitzer/app:1.0" },
        "registry_username": { "type": "string" },
        "registry_password": { "type": "string" },
    });
    let mut create_props = json!({
        "name": { "type": "string", "description": "Kurzname: a-z, 0-9, -; 2–30 Zeichen" },
        "runtime": { "type": "string", "enum": runtimes, "description": "auto erkennt: Dockerfile, package.json (node), bun.lock, deno.json, Cargo.toml, go.mod, requirements.txt/pyproject (python), composer.json/index.php, index.html (static)" },
        "build_command": { "type": "string", "description": "Optional, z. B. `npm run build`" },
        "start_command": { "type": "string", "description": "Optional, z. B. `node server.js` (läuft mit sh -c, $PORT verfügbar)" },
        "container_port": { "type": "integer", "description": "Port im Container. Leer = gleich wie aussen; die App bekommt ihn als $PORT. Für fertige Images den Port des Images angeben (z. B. 80)." },
        "no_port": { "type": "boolean", "description": "Kein Port (Bot, Worker)" },
        "env": { "type": "object", "additionalProperties": { "type": "string" } },
        "cpus": { "type": "number", "description": "CPU-Kerne (Standard: freier Rest des Kontingents)" },
        "memory_mb": { "type": "integer", "description": "RAM in MB (Standard: freier Rest)" },
        "auto_deploy": { "type": "boolean", "description": "Git-Webhook löst Deployments aus (Standard bei git: ja)" },
    });
    let mut update_props = json!({
        "app": app,
        "runtime": { "type": "string", "enum": runtimes },
        "build_command": { "type": "string" },
        "start_command": { "type": "string" },
        "cpus": { "type": "number" },
        "memory_mb": { "type": "integer" },
        "auto_start": { "type": "boolean" },
        "auto_deploy": { "type": "boolean" },
    });
    for (k, v) in source_props.as_object().into_iter().flatten() {
        create_props[k] = v.clone();
        update_props[k] = v.clone();
    }
    vec![
        tool(
            "get_info",
            "Dein Konto: Kontingent (CPU, RAM, Speicher, Apps), Nutzung, freie Ports, Host für URLs und die Regeln. Zuerst aufrufen.",
            json!({}),
            &[],
        ),
        tool(
            "list_apps",
            "Alle deine Apps mit Status, URL, Ports und CPU/RAM.",
            json!({}),
            &[],
        ),
        tool(
            "get_app",
            "Details einer App: Status, URLs, Ports, letztes Deployment, Webhook-URL, Messwerte.",
            json!({ "app": app }),
            &["app"],
        ),
        tool(
            "create_app",
            "Neue App anlegen (bekommt automatisch einen freien Port). Danach bei source=upload Dateien mit upload_files hochladen, dann deploy.",
            create_props,
            &["name"],
        ),
        tool(
            "upload_files",
            "Quellcode-Dateien schreiben (nur Quelle upload). Pfade relativ zur Projektwurzel. Binärdateien mit base64=true. replace_all=true löscht vorher alles. Grosse Projekte: mehrere Aufrufe oder upload_archive. node_modules, target, .git nicht hochladen.",
            json!({
                "app": app,
                "files": { "type": "array", "items": { "type": "object", "properties": {
                    "path": { "type": "string" }, "content": { "type": "string" }, "base64": { "type": "boolean" } }, "required": ["path", "content"] } },
                "delete": { "type": "array", "items": { "type": "string" }, "description": "Pfade zum Löschen" },
                "replace_all": { "type": "boolean" },
            }),
            &["app", "files"],
        ),
        tool(
            "upload_archive",
            "Ganzes Projekt als zip oder tar.gz (base64) hochladen. Ein einzelner Wurzelordner wird entfernt. replace_all (Standard true) ersetzt den bisherigen Code.",
            json!({ "app": app, "archive_base64": { "type": "string" }, "replace_all": { "type": "boolean" } }),
            &["app", "archive_base64"],
        ),
        tool(
            "list_files",
            "Hochgeladene Quellcode-Dateien und die erkannte Laufzeit.",
            json!({ "app": app }),
            &["app"],
        ),
        tool(
            "read_file",
            "Eine Quellcode-Datei lesen (max. 512 KB).",
            json!({ "app": app, "path": { "type": "string" } }),
            &["app", "path"],
        ),
        tool(
            "deploy",
            "Bauen bzw. Image laden und (neu) starten. Das alte Image wird danach sofort gelöscht. wait=true (Standard) wartet bis max. wait_seconds (Standard 300) und liefert Ergebnis + Build-Log-Ende.",
            json!({ "app": app, "wait": { "type": "boolean" }, "wait_seconds": { "type": "integer" } }),
            &["app"],
        ),
        tool(
            "power",
            "App starten, stoppen oder neu starten (übernimmt geänderte Einstellungen).",
            json!({ "app": app, "action": { "type": "string", "enum": ["start", "stop", "restart"] } }),
            &["app", "action"],
        ),
        tool(
            "logs",
            "Logs lesen: Container-Ausgabe (Standard) oder build=true für das letzte Build-Protokoll.",
            json!({ "app": app, "lines": { "type": "integer", "description": "Standard 150" }, "build": { "type": "boolean" } }),
            &["app"],
        ),
        tool(
            "update_app",
            "Einstellungen ändern (nur angegebene Felder). Laufende App startet bei Bedarf neu; Build-Einstellungen gelten ab dem nächsten deploy.",
            update_props,
            &["app"],
        ),
        tool(
            "set_env",
            "Umgebungsvariablen setzen/entfernen (startet die App neu, falls sie läuft).",
            json!({ "app": app, "set": { "type": "object", "additionalProperties": { "type": "string" } }, "unset": { "type": "array", "items": { "type": "string" } } }),
            &["app"],
        ),
        tool(
            "add_port",
            "Weiteren Port freigeben (aus deinem Block von 10). container_port leer = gleich wie aussen.",
            json!({ "app": app, "container_port": { "type": "integer" }, "host_port": { "type": "integer" }, "protocol": { "type": "string", "enum": ["tcp", "udp"] } }),
            &["app"],
        ),
        tool(
            "remove_port",
            "Port einer App freigeben (zurück in deinen Block).",
            json!({ "app": app, "host_port": { "type": "integer" } }),
            &["app", "host_port"],
        ),
        tool(
            "exec",
            "Befehl im laufenden Container ausführen (sh -c, max. 60 s), z. B. Migrationen oder `ls /data`.",
            json!({ "app": app, "command": { "type": "string" } }),
            &["app", "command"],
        ),
        tool(
            "delete_app",
            "App endgültig löschen – Container, Image, Daten (/data) und Quellcode. confirm=true nötig.",
            json!({ "app": app, "confirm": { "type": "boolean" } }),
            &["app", "confirm"],
        ),
    ]
}

fn s<'a>(args: &'a Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::bad(format!("Parameter `{key}` fehlt")))
}
fn opt_s(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}
fn runtime(args: &Value) -> AppResult<Option<Runtime>> {
    match args.get("runtime").and_then(Value::as_str) {
        None | Some("") => Ok(None),
        Some(r) => Runtime::parse(r)
            .map(Some)
            .ok_or_else(|| AppError::bad(format!("Unbekannte Laufzeit `{r}`"))),
    }
}
fn env_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str().map_or_else(|| v.to_string(), str::to_string),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
fn source_from(args: &Value) -> AppResult<Option<Source>> {
    let kind = args
        .get("source")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if args.get("git_url").is_some() {
                Some("git".into())
            } else if args.get("image").is_some() {
                Some("image".into())
            } else {
                None
            }
        });
    Ok(match kind.as_deref() {
        None => None,
        Some("upload") => Some(Source::Upload),
        Some("git") => Some(Source::Git {
            url: s(args, "git_url")?.to_string(),
            branch: opt_s(args, "git_branch").unwrap_or_default(),
            subdir: opt_s(args, "git_subdir").unwrap_or_default(),
            token: opt_s(args, "git_token"),
        }),
        Some("image") => Some(Source::Image {
            image: s(args, "image")?.to_string(),
            username: opt_s(args, "registry_username"),
            password: opt_s(args, "registry_password"),
        }),
        Some(k) => {
            return Err(AppError::bad(format!(
                "Unbekannte Quelle `{k}` (upload, git, image)"
            )));
        }
    })
}

fn summary(a: &nrz_vps_common::App) -> Value {
    json!({
        "name": a.name, "status": a.status, "urls": a.urls, "ports": a.ports, "source": a.source.kind(),
        "runtime": a.deploy.as_ref().and_then(|d| d.runtime).unwrap_or(a.runtime).as_str(),
        "cpus": a.cpus, "memory_mb": a.memory_mb,
        "cpu_percent": a.stats.map(|s| s.cpu_percent), "memory_used_mb": a.stats.map(|s| s.memory_bytes / 1024 / 1024),
        "last_deploy": a.deploy.as_ref().map(|d| json!({ "number": d.number, "ok": d.ok, "message": d.message })),
    })
}

async fn call(app: &AppState, c: &Ctx, name: &str, args: &Value) -> AppResult<Value> {
    let key = || s(args, "app");
    match name {
        "get_info" => {
            let usage = ops::usage(app, &c.user.id, None).await;
            let (from, to) = nrz_vps_common::slot_ports(c.user.slot);
            Ok(json!({
                "user": c.user.username,
                "quota": c.user.quota,
                "usage": usage,
                "port_block": format!("{from}-{to}"),
                "free_ports": ops::free_ports(app, &c.user).await,
                "app_url_pattern": format!("http://{}:<port>", c.host),
                "nrzvps_url": c.base,
                "rules": [
                    "Die App muss auf 0.0.0.0:$PORT lauschen (PORT wird gesetzt).",
                    "Dauerhafte Daten nur unter /data (DATA_DIR) – alles andere ist nach einem Deploy weg.",
                    "Apps eines Benutzers erreichen sich gegenseitig über den App-Namen als Hostname (z. B. http://api:34001).",
                    "Summe cpus/memory_mb laufender Apps ≤ Kontingent; sonst startet die App nicht.",
                    "Keine Images nötig: Quellcode hochladen genügt. Dockerfile wird genutzt, wenn vorhanden.",
                    "Nach jedem Update wird das alte Image sofort gelöscht."
                ],
            }))
        }
        "list_apps" => {
            let recs: Vec<_> = app
                .store
                .read(|st| {
                    st.apps
                        .iter()
                        .filter(|a| a.owner_id == c.user.id)
                        .cloned()
                        .collect()
                })
                .await;
            let mut out = Vec::new();
            for r in &recs {
                out.push(summary(&ops::view_ctx(app, c, r).await));
            }
            Ok(Value::Array(out))
        }
        "get_app" => {
            let rec = ops::find(app, &c.user, key()?).await?;
            Ok(serde_json::to_value(ops::view_ctx(app, c, &rec).await)?)
        }
        "create_app" => {
            let req = CreateApp {
                name: s(args, "name")?.to_string(),
                source: source_from(args)?.unwrap_or(Source::Upload),
                runtime: runtime(args)?.unwrap_or_default(),
                build_command: opt_s(args, "build_command").unwrap_or_default(),
                start_command: opt_s(args, "start_command").unwrap_or_default(),
                env: env_map(args.get("env")),
                container_port: args
                    .get("container_port")
                    .and_then(Value::as_u64)
                    .map(|p| p as u16),
                no_port: args
                    .get("no_port")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                cpus: args.get("cpus").and_then(Value::as_f64),
                memory_mb: args.get("memory_mb").and_then(Value::as_u64),
                auto_deploy: args
                    .get("auto_deploy")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            let a = ops::create(app, c, req).await?;
            let next = match a.source {
                Source::Upload => {
                    "Jetzt Dateien mit upload_files (oder upload_archive) hochladen, dann deploy."
                }
                _ => "Jetzt deploy aufrufen.",
            };
            Ok(json!({ "app": summary(&a), "webhook_url": a.webhook_url, "next": next }))
        }
        "upload_files" => {
            let files = args
                .get("files")
                .and_then(Value::as_array)
                .ok_or_else(|| AppError::bad("`files` fehlt"))?;
            let mut out = Vec::new();
            for f in files {
                let path = f
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::bad("Datei ohne `path`"))?;
                let content = f.get("content").and_then(Value::as_str).unwrap_or("");
                let data = if f.get("base64").and_then(Value::as_bool).unwrap_or(false) {
                    base64::engine::general_purpose::STANDARD
                        .decode(content.trim())
                        .map_err(|e| AppError::bad(format!("{path}: base64 ungültig ({e})")))?
                } else {
                    content.as_bytes().to_vec()
                };
                out.push((path.to_string(), data));
            }
            let delete: Vec<String> = args
                .get("delete")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let n = out.len();
            let tree = ops::write_files(
                app,
                c,
                key()?,
                out,
                delete,
                args.get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
            .await?;
            Ok(
                json!({ "written": n, "total_files": tree.files.len(), "total_size": tree.total_size, "detected_runtime": tree.detected.map(Runtime::as_str) }),
            )
        }
        "upload_archive" => {
            let raw = s(args, "archive_base64")?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(raw.trim())
                .map_err(|e| AppError::bad(format!("base64 ungültig: {e}")))?;
            let tree = ops::upload_archive(
                app,
                c,
                key()?,
                data,
                args.get("replace_all")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            )
            .await?;
            Ok(
                json!({ "total_files": tree.files.len(), "total_size": tree.total_size, "detected_runtime": tree.detected.map(Runtime::as_str) }),
            )
        }
        "list_files" => {
            let rec = ops::find(app, &c.user, key()?).await?;
            let tree = ops::source_tree(app, &rec).await;
            let files: Vec<String> = tree
                .files
                .iter()
                .take(2000)
                .map(|f| format!("{} ({} B)", f.path, f.size))
                .collect();
            Ok(
                json!({ "detected_runtime": tree.detected.map(Runtime::as_str), "total_size": tree.total_size, "count": tree.files.len(), "files": files }),
            )
        }
        "read_file" => Ok(Value::String(
            ops::read_source_file(app, c, key()?, s(args, "path")?).await?,
        )),
        "deploy" => {
            let k = key()?;
            let rec = ops::find(app, &c.user, k).await?;
            ops::deploy(app, c, k).await?;
            if !args.get("wait").and_then(Value::as_bool).unwrap_or(true) {
                return Ok(
                    json!({ "started": true, "hint": "Status mit get_app, Protokoll mit logs build=true" }),
                );
            }
            let max = args
                .get("wait_seconds")
                .and_then(Value::as_u64)
                .unwrap_or(300)
                .clamp(5, 1800);
            let start = std::time::Instant::now();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            while app.is_building(&rec.id).await && start.elapsed().as_secs() < max {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            let rec = app.store.app(&rec.id).await.unwrap_or(rec);
            let view = ops::view_ctx(app, c, &rec).await;
            let log = ops::build_log(app, c, &rec.id).await.unwrap_or_default();
            let tail: Vec<String> = log.iter().rev().take(40).rev().cloned().collect();
            if view.status == AppStatus::Building {
                return Ok(
                    json!({ "finished": false, "hint": "Build läuft noch – später get_app oder logs build=true", "log_tail": tail }),
                );
            }
            // Kurz warten und schauen, ob die App gleich wieder abstürzt
            if view.deploy.as_ref().and_then(|d| d.ok) == Some(true) && view.auto_start {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                crate::monitor::refresh_status(app).await;
            }
            let view =
                ops::view_ctx(app, c, &app.store.app(&rec.id).await.unwrap_or(rec.clone())).await;
            let runtime_logs = if matches!(
                view.status,
                AppStatus::Crashed | AppStatus::Restarting | AppStatus::Stopped
            ) {
                ops::logs(app, c, &rec.id, 40).await.unwrap_or_default()
            } else {
                vec![]
            };
            Ok(
                json!({ "finished": true, "app": summary(&view), "build_log_tail": tail, "container_log_tail": runtime_logs }),
            )
        }
        "power" => {
            let action = match s(args, "action")? {
                "start" => PowerAction::Start,
                "stop" => PowerAction::Stop,
                "restart" => PowerAction::Restart,
                a => return Err(AppError::bad(format!("Unbekannte Aktion {a}"))),
            };
            Ok(summary(&power_action(app, c, key()?, action).await?))
        }
        "logs" => {
            let lines = if args.get("build").and_then(Value::as_bool).unwrap_or(false) {
                ops::build_log(app, c, key()?).await?
            } else {
                ops::logs(
                    app,
                    c,
                    key()?,
                    args.get("lines").and_then(Value::as_u64).unwrap_or(150) as u32,
                )
                .await?
            };
            Ok(Value::String(if lines.is_empty() {
                "(keine Ausgabe)".into()
            } else {
                lines.join("\n")
            }))
        }
        "update_app" => {
            let req = UpdateApp {
                source: source_from(args)?,
                runtime: runtime(args)?,
                build_command: opt_s(args, "build_command"),
                start_command: opt_s(args, "start_command"),
                env: None,
                cpus: args.get("cpus").and_then(Value::as_f64),
                memory_mb: args.get("memory_mb").and_then(Value::as_u64),
                auto_start: args.get("auto_start").and_then(Value::as_bool),
                auto_deploy: args.get("auto_deploy").and_then(Value::as_bool),
            };
            Ok(summary(&ops::update(app, c, key()?, req).await?))
        }
        "set_env" => {
            let rec = ops::find(app, &c.user, key()?).await?;
            let mut env = rec.env.clone();
            env.extend(env_map(args.get("set")));
            for k in args
                .get("unset")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                env.remove(k);
            }
            let a = ops::update(
                app,
                c,
                &rec.id,
                UpdateApp {
                    env: Some(env),
                    ..Default::default()
                },
            )
            .await?;
            Ok(json!({ "env": a.env.keys().collect::<Vec<_>>(), "status": a.status }))
        }
        "add_port" => {
            let protocol = match args.get("protocol").and_then(Value::as_str) {
                Some("udp") => Protocol::Udp,
                _ => Protocol::Tcp,
            };
            let req = AddPort {
                container: args
                    .get("container_port")
                    .and_then(Value::as_u64)
                    .map(|p| p as u16),
                host: args
                    .get("host_port")
                    .and_then(Value::as_u64)
                    .map(|p| p as u16),
                protocol,
            };
            Ok(summary(&ops::add_port(app, c, key()?, req).await?))
        }
        "remove_port" => {
            let host = args
                .get("host_port")
                .and_then(Value::as_u64)
                .ok_or_else(|| AppError::bad("`host_port` fehlt"))? as u16;
            Ok(summary(&ops::remove_port(app, c, key()?, host).await?))
        }
        "exec" => {
            let (code, out) = ops::exec(app, c, key()?, s(args, "command")?).await?;
            Ok(json!({ "exit_code": code, "output": out }))
        }
        "delete_app" => {
            if args.get("confirm").and_then(Value::as_bool) != Some(true) {
                return Err(AppError::bad(
                    "Zum Löschen confirm=true setzen (löscht auch alle Daten unter /data)",
                ));
            }
            let rec = ops::find(app, &c.user, key()?).await?;
            ops::delete(app, &rec).await?;
            Ok(json!({ "deleted": rec.name }))
        }
        _ => Err(AppError::bad(format!("Unbekanntes Werkzeug: {name}"))),
    }
}
