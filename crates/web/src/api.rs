//! HTTP-Client für die nrzVPS-API (gleicher Ursprung, Sitzungs-Cookie).
//!
//! Alle Funktionen liefern `Result<T, String>` mit einer deutschen Fehlermeldung. Bei 401 wird der
//! Anmeldestatus zurückgesetzt (→ Anmeldeseite).

use gloo_net::http::{Request, RequestBuilder, Response};
use nrz_vps_common::ApiError;
use serde::{Serialize, de::DeserializeOwned};

use crate::store::on_unauthorized;

pub type ApiResult<T> = Result<T, String>;

async fn finish<T: DeserializeOwned>(resp: Result<Response, gloo_net::Error>) -> ApiResult<T> {
    let resp = resp.map_err(|e| format!("Keine Verbindung zu nrzVPS: {e}"))?;
    let status = resp.status();
    if status == 401 {
        on_unauthorized();
        return Err("Nicht angemeldet".into());
    }
    if !(200..300).contains(&status) {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<ApiError>(&text)
            .map(|e| e.error)
            .unwrap_or_else(|_| {
                if text.trim().is_empty() {
                    format!("Fehler {status}")
                } else {
                    text
                }
            });
        return Err(msg);
    }
    if status == 202 || status == 204 {
        return serde_json::from_str("null").map_err(|e| e.to_string());
    }
    resp.json::<T>()
        .await
        .map_err(|e| format!("Antwort nicht lesbar: {e}"))
}

pub async fn get<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    finish(Request::get(path).send().await).await
}

async fn with_body<B: Serialize, T: DeserializeOwned>(
    rb: RequestBuilder,
    body: &B,
) -> ApiResult<T> {
    let req = rb.json(body).map_err(|e| e.to_string())?;
    finish(req.send().await).await
}

pub async fn post<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> ApiResult<T> {
    with_body(Request::post(path), body).await
}

/// POST ohne Body.
pub async fn post_empty<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    finish(Request::post(path).send().await).await
}

pub async fn put<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> ApiResult<T> {
    with_body(Request::put(path), body).await
}

pub async fn patch<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> ApiResult<T> {
    with_body(Request::patch(path), body).await
}

pub async fn delete<T: DeserializeOwned>(path: &str) -> ApiResult<T> {
    finish(Request::delete(path).send().await).await
}

/// Dateien hochladen (multipart, Feld `file`).
pub async fn upload<T: DeserializeOwned>(path: &str, files: &[web_sys::File]) -> ApiResult<T> {
    let form = web_sys::FormData::new().map_err(|_| "FormData nicht verfügbar".to_string())?;
    for f in files {
        form.append_with_blob_and_filename("file", f, &f.name())
            .map_err(|_| "Datei nicht lesbar".to_string())?;
    }
    let req = Request::post(path).body(form).map_err(|e| e.to_string())?;
    finish(req.send().await).await
}

/// Text in die Zwischenablage kopieren.
pub fn copy(text: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.navigator().clipboard().write_text(text);
    }
}
