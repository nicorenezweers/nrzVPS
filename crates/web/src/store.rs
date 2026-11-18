//! Globaler Zustand: Anmeldung, Übersicht (Kontingent + Apps), Meldungen (Toasts).

use dioxus::prelude::*;
use nrz_vps_common::{App, AuthState, Me, Overview};

/// None = noch nicht geladen.
pub static AUTH: GlobalSignal<Option<AuthState>> = Signal::global(|| None);
/// Eigene Apps (Seitenleiste, Übersicht), alle paar Sekunden aktualisiert.
pub static APPS: GlobalSignal<Vec<App>> = Signal::global(Vec::new);
pub static APPS_LOADED: GlobalSignal<bool> = Signal::global(|| false);
pub static OVERVIEW: GlobalSignal<Option<Overview>> = Signal::global(|| None);
pub static TOASTS: GlobalSignal<Vec<Toast>> = Signal::global(Vec::new);

#[derive(Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
}

pub fn me() -> Option<Me> {
    AUTH.read().as_ref().and_then(|a| a.user.clone())
}

pub fn on_unauthorized() {
    let setup_required = AUTH.peek().as_ref().is_some_and(|a| a.setup_required);
    *AUTH.write() = Some(AuthState {
        setup_required,
        user: None,
    });
}

fn push_toast(text: String, error: bool) {
    let id = js_sys::Date::now() as u64 ^ (js_sys::Math::random() * 1e9) as u64;
    TOASTS.write().push(Toast { id, text, error });
    dioxus::dioxus_core::spawn_forever(async move {
        gloo_timers::future::TimeoutFuture::new(if error { 6000 } else { 3500 }).await;
        TOASTS.write().retain(|t| t.id != id);
    });
}

/// Erfolgsmeldung unten rechts.
pub fn toast(text: impl Into<String>) {
    push_toast(text.into(), false);
}

/// Fehlermeldung unten rechts.
pub fn toast_err(text: impl Into<String>) {
    push_toast(text.into(), true);
}

/// Apps und Übersicht neu laden.
pub async fn refresh() {
    if let Ok(list) = crate::api::get::<Vec<App>>("/api/apps").await {
        *APPS.write() = list;
        *APPS_LOADED.write() = true;
    }
    if let Ok(o) = crate::api::get::<Overview>("/api/overview").await {
        *OVERVIEW.write() = Some(o);
    }
}

/// App aus der Liste (falls geladen).
pub fn app(id: &str) -> Option<App> {
    APPS.read().iter().find(|a| a.id == id).cloned()
}
