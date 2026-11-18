//! Router: alle Routen + Oberfläche (SPA) + llm.txt.

use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
};

use crate::{admin, apps, auth, mcp, state::AppState};

const LLM_TXT: &str = include_str!("../../../llm.txt");

pub fn base_url(headers: &HeaderMap) -> String {
    let h = |k: &str| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let host = h("x-forwarded-host")
        .or_else(|| h("host"))
        .unwrap_or_else(|| "localhost:34100".into());
    format!(
        "{}://{host}",
        h("x-forwarded-proto").unwrap_or_else(|| "http".into())
    )
}

pub fn llm_text(base: &str) -> String {
    LLM_TXT.replace("{{URL}}", base)
}

async fn llm_txt(State(_app): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        llm_text(&base_url(&headers)),
    )
}

pub fn router(app: AppState) -> Router {
    let api = Router::new()
        .route("/api/auth/me", get(auth::get_me))
        .route("/api/auth/setup", post(auth::setup))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/password", post(auth::change_password))
        .route(
            "/api/tokens",
            get(auth::list_tokens).post(auth::create_token),
        )
        .route("/api/tokens/{tid}", delete(auth::delete_token))
        .route("/api/overview", get(apps::overview))
        .route("/api/apps", get(apps::list).post(apps::create))
        .route(
            "/api/apps/{id}",
            get(apps::get).patch(apps::update).delete(apps::delete),
        )
        .route("/api/apps/{id}/power", post(apps::power))
        .route("/api/apps/{id}/deploy", post(apps::deploy))
        .route("/api/apps/{id}/logs", get(apps::logs))
        .route("/api/apps/{id}/build-log", get(apps::build_log))
        .route(
            "/api/apps/{id}/source",
            get(apps::source)
                .post(apps::upload_multipart)
                .put(apps::upload_raw),
        )
        .route("/api/apps/{id}/ports", post(apps::add_port))
        .route("/api/apps/{id}/ports/{host}", delete(apps::remove_port))
        .route("/api/apps/{id}/exec", post(apps::exec))
        .route("/api/apps/{id}/webhook-secret", post(apps::webhook_secret))
        .route("/api/hooks/{id}/{secret}", post(apps::hook))
        .route(
            "/api/admin/settings",
            get(admin::get_settings).put(admin::put_settings),
        )
        .route("/api/admin/system", get(admin::system))
        .route(
            "/api/admin/users",
            get(admin::list_users).post(admin::create_user),
        )
        .route(
            "/api/admin/users/{uid}",
            axum::routing::patch(admin::update_user).delete(admin::delete_user),
        )
        .route("/api/admin/apps", get(admin::all_apps))
        .route("/api/admin/cleanup", post(admin::cleanup))
        .route("/mcp", post(mcp::handle).get(mcp::get_info))
        .route("/llm.txt", get(llm_txt))
        .route("/llms.txt", get(llm_txt))
        .route("/healthz", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))
        .with_state(app.clone());

    let web = app.config.web_dir.clone();
    let spa = ServeDir::new(&web).fallback(ServeFile::new(web.join("index.html")));
    api.fallback_service(spa).layer(CompressionLayer::new())
}
