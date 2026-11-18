//! nrzVPS – Weboberfläche (Dioxus 0.7, nrzUI, nur Rust).

mod api;
mod components;
mod layout;
mod pages;
mod store;

use dioxus::prelude::*;
use nrz_vps_common::{AuthState, Credentials};

use crate::{
    components::{ErrorBox, INPUT, Loading, Toaster, ui::app_shell::AuthFrame, ui::button::Button},
    layout::{AppLayout, Shell},
    pages::*,
    store::AUTH,
};

const TAILWIND: Asset = asset!("/assets/tailwind.css");
const LOGO: Asset = asset!("/assets/logo.svg");

#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        Dashboard {},
        #[route("/new")]
        NewApp {},
        #[route("/ai")]
        AiHosting {},
        #[route("/account")]
        Account {},
        #[route("/admin/users")]
        AdminUsers {},
        #[route("/admin/system")]
        AdminSystem {},
        #[nest("/apps/:id")]
            #[layout(AppLayout)]
                #[route("/")]
                AppOverview { id: String },
                #[route("/logs")]
                AppLogs { id: String },
                #[route("/source")]
                AppSource { id: String },
                #[route("/console")]
                AppConsole { id: String },
                #[route("/settings")]
                AppSettings { id: String },
            #[end_layout]
        #[end_nest]
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_future(|| async {
        match api::get::<AuthState>("/api/auth/me").await {
            Ok(a) => *AUTH.write() = Some(a),
            Err(_) => *AUTH.write() = Some(AuthState::default()),
        }
    });
    let auth = AUTH.read().clone();
    rsx! {
        document::Stylesheet { href: TAILWIND }
        document::Link { rel: "icon", href: LOGO }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1, viewport-fit=cover" }
        document::Title { "nrzVPS" }
        match auth {
            None => rsx! { div { class: "flex h-dvh items-center justify-center bg-background", Loading {} } },
            Some(a) if a.user.is_none() => rsx! {
                LoginPage { setup: a.setup_required }
                Toaster {}
            },
            Some(_) => rsx! { Router::<Route> {} },
        }
    }
}

/// Anmeldung bzw. Ersteinrichtung (erstes Konto = Admin).
#[component]
fn LoginPage(setup: bool) -> Element {
    let mut username = use_signal(|| {
        if setup {
            "admin".to_string()
        } else {
            String::new()
        }
    });
    let mut password = use_signal(String::new);
    let mut password2 = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move |e: FormEvent| {
        e.prevent_default();
        if setup && password() != password2() {
            error.set(Some("Die Passwörter stimmen nicht überein".into()));
            return;
        }
        busy.set(true);
        spawn(async move {
            let creds = Credentials {
                username: username(),
                password: password(),
            };
            let path = if setup {
                "/api/auth/setup"
            } else {
                "/api/auth/login"
            };
            match api::post::<_, AuthState>(path, &creds).await {
                Ok(a) => *AUTH.write() = Some(a),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        AuthFrame {
            logo: rsx! { img { src: LOGO, alt: "" } },
            title: "nrzVPS",
            tagline: if setup { "Willkommen! Lege dein Admin-Konto an." } else { "Deine Apps einfach hosten – die KI erledigt den Rest" },
            form { class: "flex flex-col gap-4", onsubmit: submit,
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-sm font-medium", "Benutzername" }
                    input { class: INPUT, value: "{username}", autocomplete: "username", autofocus: !setup, oninput: move |e| username.set(e.value()) }
                }
                label { class: "flex flex-col gap-1.5",
                    span { class: "text-sm font-medium", "Passwort" }
                    input {
                        class: INPUT,
                        r#type: "password",
                        value: "{password}",
                        autofocus: setup,
                        autocomplete: if setup { "new-password" } else { "current-password" },
                        oninput: move |e| password.set(e.value()),
                    }
                }
                if setup {
                    label { class: "flex flex-col gap-1.5",
                        span { class: "text-sm font-medium", "Passwort wiederholen" }
                        input { class: INPUT, r#type: "password", value: "{password2}", autocomplete: "new-password", oninput: move |e| password2.set(e.value()) }
                        span { class: "text-xs text-muted-foreground", "Mindestens 8 Zeichen." }
                    }
                }
                Button { button_type: "submit", class: "w-full", disabled: busy(),
                    if setup { "Konto anlegen" } else { "Anmelden" }
                }
                if !setup {
                    p { class: "text-center text-xs text-muted-foreground", "Noch kein Konto? Frag den Betreiber dieses nrzVPS." }
                }
            }
        }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        div { class: "flex h-dvh flex-col items-center justify-center gap-3 bg-background text-foreground",
            h1 { class: "text-2xl font-semibold", "Seite nicht gefunden" }
            Link { to: Route::Dashboard {}, class: "text-sm text-muted-foreground underline", "Zur Übersicht" }
        }
    }
}
