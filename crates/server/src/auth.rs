//! Anmeldung: Konten (erstes = Admin), Sitzungen (Cookie) und API-/MCP-Token (Bearer) pro Benutzer.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{
    Json,
    extract::{FromRequestParts, Path, State},
    http::{HeaderMap, HeaderValue, header, request::Parts},
    response::{IntoResponse, Response},
};
use nrz_vps_common::{
    ApiToken, AuthState, ChangePassword, CreateToken, CreatedToken, Credentials, Me, Ok as OkResp,
    slot_ports,
};
use sha2::{Digest, Sha256};

use crate::{
    error::{AppError, AppResult},
    state::AppState,
    store::{Session, StoredToken, User, new_id, now, valid_name},
};

pub const COOKIE: &str = "nrzvps_session";
const SESSION_DAYS: i64 = 30;

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn hash_password(pw: &str) -> AppResult<String> {
    if pw.len() < 8 {
        return Err(AppError::bad("Passwort muss mindestens 8 Zeichen haben"));
    }
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    let salt = SaltString::encode_b64(&bytes).map_err(|e| AppError::internal(e.to_string()))?;
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::internal(e.to_string()))
}

fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

pub fn me(u: &User) -> Me {
    let (port_from, port_to) = slot_ports(u.slot);
    Me {
        id: u.id.clone(),
        username: u.username.clone(),
        admin: u.admin,
        slot: u.slot,
        port_from,
        port_to,
    }
}

/// Angemeldeter Benutzer (Cookie oder Bearer-Token).
#[derive(Debug, Clone)]
pub struct Authed {
    pub user: User,
}

/// Nur für Admins.
#[derive(Debug, Clone)]
pub struct AdminAuthed {
    pub user: User,
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| {
            c.split(';')
                .map(str::trim)
                .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('=').map(str::to_string))
        })
}

pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Option<User> {
    let t = now();
    let mut user_id = None;
    if let Some(sess) = cookie_value(headers, COOKIE) {
        let h = sha256_hex(&sess);
        user_id = state
            .store
            .read(|s| {
                s.sessions
                    .get(&h)
                    .filter(|x| x.expires > t)
                    .map(|x| x.user_id.clone())
            })
            .await;
    }
    if user_id.is_none() {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)?;
        let h = sha256_hex(bearer);
        user_id = state
            .store
            .update(|s| {
                s.tokens.iter_mut().find(|x| x.hash == h).map(|x| {
                    x.last_used = Some(t);
                    x.user_id.clone()
                })
            })
            .await
            .ok()
            .flatten();
    }
    let user = state.store.user(&user_id?).await?;
    (!user.disabled).then_some(user)
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(state, &parts.headers)
            .await
            .map(|user| Authed { user })
            .ok_or(AppError::Unauthorized)
    }
}

impl FromRequestParts<AppState> for AdminAuthed {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = authenticate(state, &parts.headers)
            .await
            .ok_or(AppError::Unauthorized)?;
        if !user.admin {
            return Err(AppError::Forbidden("Nur für Admins".into()));
        }
        Ok(AdminAuthed { user })
    }
}

fn session_cookie(value: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

async fn new_session(state: &AppState, user_id: &str) -> AppResult<HeaderValue> {
    let token = new_id(32);
    let h = sha256_hex(&token);
    let t = now();
    let uid = user_id.to_string();
    state
        .store
        .update(|s| {
            s.sessions.retain(|_, v| v.expires > t);
            s.sessions.insert(
                h,
                Session {
                    user_id: uid.clone(),
                    expires: t + SESSION_DAYS * 86400,
                },
            );
            if let Some(u) = s.users.iter_mut().find(|u| u.id == uid) {
                u.last_login = Some(t);
            }
        })
        .await?;
    Ok(session_cookie(&token, SESSION_DAYS * 86400))
}

pub async fn get_me(State(state): State<AppState>, headers: HeaderMap) -> Json<AuthState> {
    let setup_required = state.store.read(|s| s.users.is_empty()).await;
    let user = authenticate(&state, &headers).await.map(|u| me(&u));
    Json(AuthState {
        setup_required,
        user,
    })
}

pub async fn setup(
    State(state): State<AppState>,
    Json(c): Json<Credentials>,
) -> AppResult<Response> {
    let username = c.username.trim().to_lowercase();
    if !valid_name(&username) {
        return Err(AppError::bad(
            "Benutzername: 2–30 Zeichen, Kleinbuchstaben, Ziffern und -",
        ));
    }
    let hash = hash_password(&c.password)?;
    let user = User {
        id: new_id(8),
        username,
        password_hash: hash,
        admin: true,
        slot: 0,
        created_at: now(),
        ..Default::default()
    };
    let created = state
        .store
        .update(|s| {
            if !s.users.is_empty() {
                return false;
            }
            let mut u = user.clone();
            u.quota = s.settings.default_quota;
            s.users.push(u);
            true
        })
        .await?;
    if !created {
        return Err(AppError::Forbidden(
            "nrzVPS ist bereits eingerichtet".into(),
        ));
    }
    let cookie = new_session(&state, &user.id).await?;
    let u = state.store.user(&user.id).await.unwrap_or(user);
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(AuthState {
            setup_required: false,
            user: Some(me(&u)),
        }),
    )
        .into_response())
}

pub async fn login(
    State(state): State<AppState>,
    Json(c): Json<Credentials>,
) -> AppResult<Response> {
    let name = c.username.trim().to_lowercase();
    let user = state
        .store
        .read(|s| s.users.iter().find(|u| u.username == name).cloned())
        .await;
    let Some(user) = user.filter(|u| verify_password(&c.password, &u.password_hash)) else {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return Err(AppError::Forbidden(
            "Benutzername oder Passwort falsch".into(),
        ));
    };
    if user.disabled {
        return Err(AppError::Forbidden("Dieses Konto ist gesperrt".into()));
    }
    let cookie = new_session(&state, &user.id).await?;
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(AuthState {
            setup_required: false,
            user: Some(me(&user)),
        }),
    )
        .into_response())
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(sess) = cookie_value(&headers, COOKIE) {
        let h = sha256_hex(&sess);
        state.store.update(|s| s.sessions.remove(&h)).await?;
    }
    Ok((
        [(header::SET_COOKIE, session_cookie("", 0))],
        Json(OkResp { ok: true }),
    )
        .into_response())
}

pub async fn change_password(
    State(state): State<AppState>,
    a: Authed,
    Json(c): Json<ChangePassword>,
) -> AppResult<Json<OkResp>> {
    if !verify_password(&c.current, &a.user.password_hash) {
        return Err(AppError::Forbidden("Aktuelles Passwort falsch".into()));
    }
    let hash = hash_password(&c.new)?;
    let uid = a.user.id.clone();
    state
        .store
        .update(|s| {
            if let Some(u) = s.users.iter_mut().find(|u| u.id == uid) {
                u.password_hash = hash;
            }
        })
        .await?;
    Ok(Json(OkResp { ok: true }))
}

fn to_api(t: &StoredToken) -> ApiToken {
    ApiToken {
        id: t.id.clone(),
        name: t.name.clone(),
        prefix: t.prefix.clone(),
        created_at: t.created_at,
        last_used: t.last_used,
    }
}

pub async fn list_tokens(State(state): State<AppState>, a: Authed) -> Json<Vec<ApiToken>> {
    Json(
        state
            .store
            .read(|s| {
                s.tokens
                    .iter()
                    .filter(|t| t.user_id == a.user.id)
                    .map(to_api)
                    .collect()
            })
            .await,
    )
}

pub async fn create_token(
    State(state): State<AppState>,
    a: Authed,
    Json(c): Json<CreateToken>,
) -> AppResult<Json<CreatedToken>> {
    let name = if c.name.trim().is_empty() {
        "KI".to_string()
    } else {
        c.name.trim().chars().take(60).collect()
    };
    let secret = format!("nrzvps_{}", new_id(24));
    let t = StoredToken {
        id: new_id(8),
        user_id: a.user.id.clone(),
        name,
        prefix: format!("{}…", &secret[..14]),
        hash: sha256_hex(&secret),
        created_at: now(),
        last_used: None,
    };
    let api = to_api(&t);
    let uid = a.user.id.clone();
    let ok = state
        .store
        .update(|s| {
            if s.tokens.iter().filter(|t| t.user_id == uid).count() >= 20 {
                return false;
            }
            s.tokens.push(t);
            true
        })
        .await?;
    if !ok {
        return Err(AppError::bad("Höchstens 20 Token pro Konto"));
    }
    Ok(Json(CreatedToken { token: api, secret }))
}

pub async fn delete_token(
    State(state): State<AppState>,
    a: Authed,
    Path(tid): Path<String>,
) -> AppResult<Json<OkResp>> {
    state
        .store
        .update(|s| {
            s.tokens
                .retain(|t| !(t.id == tid && t.user_id == a.user.id))
        })
        .await?;
    Ok(Json(OkResp { ok: true }))
}
