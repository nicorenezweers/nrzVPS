//! App-Gerüst: Seitenleiste (eigene Apps, KI, Konto, Admin) und App-Bereich mit Kopf und Reitern.

use dioxus::prelude::*;
use nrz_vps_common::{App, AppStatus, PowerAction, PowerRequest, routes};

use crate::{
    Route, api,
    components::{
        ConfirmDialog, StatusBadge, StatusDot, Toaster, runtime_text,
        ui::{
            app_shell::*,
            button::{Button, ButtonSize, ButtonVariant},
        },
    },
    store::{APPS, me, refresh, toast, toast_err},
};

const LOGO: Asset = asset!("/assets/logo.svg");

#[component]
pub fn Shell() -> Element {
    let route = use_route::<Route>();
    // Apps + Übersicht alle 4 s aktualisieren
    use_future(|| async move {
        loop {
            refresh().await;
            gloo_timers::future::TimeoutFuture::new(4000).await;
        }
    });
    let apps = APPS.read().clone();
    let me = me().unwrap_or_default();
    let current_id = route_app_id(&route);
    let version = format!("nrzVPS {}", nrz_vps_common::VERSION);
    let subtitle = format!("Ports {}–{}", me.port_from, me.port_to);

    rsx! {
        AppShell {
            header_brand: rsx! {
                img { src: LOGO, class: "size-7 rounded-lg", alt: "" }
                span { class: "font-semibold", "nrzVPS" }
            },
            sidebar: rsx! {
                AppBrand {
                    logo: rsx! { img { src: LOGO, alt: "" } },
                    name: "nrzVPS",
                    subtitle,
                    href: "/",
                }
                AppNav {
                    AppNavItem {
                        label: "Übersicht",
                        href: "/",
                        active: matches!(route, Route::Dashboard {}),
                        icon: rsx! { icons::LayoutDashboard { class: "size-4" } },
                    }
                    AppNavItem {
                        label: "Mit KI hosten",
                        href: "/ai",
                        active: matches!(route, Route::AiHosting {}),
                        icon: rsx! { icons::Sparkles { class: "size-4" } },
                    }
                    AppNavHeading {
                        label: "Apps",
                        action: rsx! {
                            Link {
                                to: Route::NewApp {},
                                class: "rounded p-0.5 text-muted-foreground hover:bg-sidenav-accent hover:text-foreground",
                                title: "Neue App",
                                icons::Plus { class: "size-3.5" }
                            }
                        },
                    }
                    for a in apps.iter() {
                        AppNavItem {
                            key: "{a.id}",
                            label: a.name.clone(),
                            href: format!("/apps/{}", a.id),
                            active: current_id.as_deref() == Some(a.id.as_str()),
                            icon: rsx! {
                                span { class: "relative flex size-4 items-center justify-center",
                                    icons::Package { class: "size-4" }
                                    span { class: "absolute -right-0.5 -bottom-0.5", StatusDot { status: a.status } }
                                }
                            },
                        }
                    }
                    AppNavItem {
                        label: "Neue App",
                        href: "/new",
                        active: matches!(route, Route::NewApp {}),
                        icon: rsx! { icons::CirclePlus { class: "size-4" } },
                    }
                    if me.admin {
                        AppNavHeading { label: "Admin" }
                        AppNavItem {
                            label: "Benutzer",
                            href: "/admin/users",
                            active: matches!(route, Route::AdminUsers {}),
                            icon: rsx! { icons::Users { class: "size-4" } },
                        }
                        AppNavItem {
                            label: "Ressourcen",
                            href: "/admin/system",
                            active: matches!(route, Route::AdminSystem {}),
                            icon: rsx! { icons::Gauge { class: "size-4" } },
                        }
                    }
                }
                AppSidebarFooter {
                    AppNavItem {
                        label: "Konto",
                        href: "/account",
                        active: matches!(route, Route::Account {}),
                        icon: rsx! { icons::User { class: "size-4" } },
                    }
                    AppAccountButton {
                        name: me.username.clone(),
                        detail: "Abmelden",
                        onclick: move |_| {
                            spawn(async move {
                                let _: Result<nrz_vps_common::Ok, _> = api::post_empty("/api/auth/logout").await;
                                crate::store::on_unauthorized();
                            });
                        },
                    }
                    AppVersion { text: version }
                }
            },
            main { id: "app-main", class: "min-h-0 flex-1 overflow-y-auto",
                Outlet::<Route> {}
            }
        }
        Toaster {}
    }
}

fn route_app_id(r: &Route) -> Option<String> {
    let s = r.to_string();
    let rest = s.strip_prefix("/apps/")?;
    Some(rest.split('/').next()?.to_string())
}

/* ------------------------------------------------------------------ */
/*                              App-Bereich                           */
/* ------------------------------------------------------------------ */

/// Daten der geöffneten App für alle Reiter (per `use_app()`).
#[derive(Clone, Copy, PartialEq)]
pub struct AppCtx {
    pub id: Signal<String>,
    pub app: Signal<Option<App>>,
}

impl AppCtx {
    pub fn id(&self) -> String {
        self.id.read().clone()
    }
    /// App neu laden (nach Änderungen).
    pub fn reload(&self) {
        let mut app = self.app;
        let id = self.id();
        spawn(async move {
            if let Ok(a) = api::get::<App>(&routes::app(&id, "")).await {
                app.set(Some(a));
            }
            refresh().await;
        });
    }
    /// Antwort einer Änderung übernehmen.
    pub fn set(&self, a: App) {
        let mut app = self.app;
        app.set(Some(a));
    }
}

pub fn use_app() -> AppCtx {
    use_context::<AppCtx>()
}

/// Start/Stopp/Neustart.
pub async fn power(ctx: AppCtx, action: PowerAction) {
    let id = ctx.id();
    match api::post::<_, App>(&routes::app(&id, "power"), &PowerRequest { action }).await {
        Ok(a) => {
            ctx.set(a);
            toast(match action {
                PowerAction::Start => "App wird gestartet",
                PowerAction::Stop => "App wurde gestoppt",
                PowerAction::Restart => "App wird neu gestartet",
            });
        }
        Err(e) => toast_err(e),
    }
    refresh().await;
}

/// Neues Deployment (Build im Hintergrund).
pub async fn deploy(ctx: AppCtx) {
    let id = ctx.id();
    match api::post_empty::<App>(&routes::app(&id, "deploy")).await {
        Ok(a) => {
            ctx.set(a);
            toast("Deployment gestartet – die Logs zeigen den Fortschritt");
        }
        Err(e) => toast_err(e),
    }
    refresh().await;
}

#[component]
pub fn AppLayout(id: String) -> Element {
    let mut id_sig = use_signal(|| id.clone());
    if *id_sig.peek() != id {
        id_sig.set(id.clone());
    }
    let mut app = use_signal(|| crate::store::app(&id));
    let ctx = AppCtx { id: id_sig, app };
    use_context_provider(|| ctx);
    let mut confirm_delete = use_signal(|| false);
    let mut load_error = use_signal(|| None::<String>);

    // Laden beim Wechsel + alle 3 s
    use_future(move || async move {
        let mut last = String::new();
        loop {
            let id = id_sig.peek().clone();
            if id != last {
                app.set(crate::store::app(&id));
                last = id.clone();
            }
            match api::get::<App>(&routes::app(&id, "")).await {
                Ok(a) => {
                    if *id_sig.peek() == id {
                        app.set(Some(a));
                        load_error.set(None);
                    }
                }
                Err(e) => load_error.set(Some(e)),
            }
            // Bei ID-Wechsel sofort neu laden
            for _ in 0..30 {
                gloo_timers::future::TimeoutFuture::new(100).await;
                if *id_sig.peek() != id {
                    break;
                }
            }
        }
    });

    let route = use_route::<Route>();
    let Some(a) = app.read().clone() else {
        return rsx! {
            if let Some(e) = load_error() {
                div { class: "p-8", crate::components::ErrorBox { message: e } }
            } else {
                crate::components::Loading {}
            }
        };
    };
    let st = a.status;
    let building = st == AppStatus::Building;
    let running = matches!(st, AppStatus::Running | AppStatus::Restarting);

    let tabs: Vec<(Route, &str, Element)> = vec![
        (
            Route::AppOverview { id: id.clone() },
            "Übersicht",
            rsx! { icons::LayoutDashboard { class: "size-4" } },
        ),
        (
            Route::AppLogs { id: id.clone() },
            "Logs",
            rsx! { icons::ScrollText { class: "size-4" } },
        ),
        (
            Route::AppSource { id: id.clone() },
            "Quellcode",
            rsx! { icons::FileCode { class: "size-4" } },
        ),
        (
            Route::AppConsole { id: id.clone() },
            "Konsole",
            rsx! { icons::SquareTerminal { class: "size-4" } },
        ),
        (
            Route::AppSettings { id: id.clone() },
            "Einstellungen",
            rsx! { icons::Settings2 { class: "size-4" } },
        ),
    ];
    let route_str = route.to_string();
    let source_text = match &a.source {
        nrz_vps_common::Source::Upload => "Hochgeladener Code".to_string(),
        nrz_vps_common::Source::Git { url, branch, .. } => {
            if branch.is_empty() {
                format!("Git: {url}")
            } else {
                format!("Git: {url} ({branch})")
            }
        }
        nrz_vps_common::Source::Image { image, .. } => format!("Image: {image}"),
    };

    rsx! {
        document::Title { "{a.name} · nrzVPS" }
        div { class: "border-b bg-background/95",
            div { class: "mx-auto flex max-w-7xl flex-wrap items-center gap-3 px-4 pt-5 md:px-8",
                div { class: "flex size-10 items-center justify-center rounded-lg bg-muted",
                    icons::Package { class: "size-5" }
                }
                div { class: "min-w-0 flex-1",
                    div { class: "flex flex-wrap items-center gap-2",
                        h1 { class: "truncate text-xl font-semibold tracking-tight", "{a.name}" }
                        StatusBadge { status: st }
                    }
                    div { class: "mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground",
                        span { "{runtime_text(&a)}" }
                        span { class: "truncate max-w-xs", "{source_text}" }
                        for u in a.urls.iter() {
                            a {
                                key: "{u}",
                                class: "inline-flex items-center gap-1 font-mono hover:text-foreground",
                                href: "{u}",
                                target: "_blank",
                                rel: "noopener",
                                icons::ExternalLink { class: "size-3" }
                                "{u}"
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap items-center gap-2",
                    if !running && !building {
                        Button {
                            size: ButtonSize::Sm,
                            disabled: a.image.is_none(),
                            title: if a.image.is_none() { "Erst deployen" } else { "Starten" },
                            onclick: move |_| { spawn(async move { power(ctx, PowerAction::Start).await }); },
                            icons::Play { class: "size-4" }
                            "Starten"
                        }
                    }
                    if running {
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            onclick: move |_| { spawn(async move { power(ctx, PowerAction::Restart).await }); },
                            icons::RotateCw { class: "size-4" }
                            "Neustart"
                        }
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            onclick: move |_| { spawn(async move { power(ctx, PowerAction::Stop).await }); },
                            icons::Square { class: "size-4" }
                            "Stoppen"
                        }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        variant: if a.image.is_none() { ButtonVariant::Default } else { ButtonVariant::Outline },
                        disabled: building,
                        title: "Code neu bauen bzw. Image neu laden und starten",
                        onclick: move |_| { spawn(async move { deploy(ctx).await }); },
                        if building {
                            icons::LoaderCircle { class: "size-4 animate-spin" }
                            "Wird gebaut …"
                        } else {
                            icons::Rocket { class: "size-4" }
                            "Deployen"
                        }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        title: "App löschen",
                        onclick: move |_| confirm_delete.set(true),
                        icons::Trash2 { class: "size-4" }
                    }
                }
            }
            nav { class: "mx-auto mt-3 flex max-w-7xl gap-1 overflow-x-auto px-4 md:px-8",
                for (r, label, icon) in tabs {
                    {
                        let active = r.to_string() == route_str;
                        rsx! {
                            Link {
                                key: "{label}",
                                to: r,
                                class: if active { "inline-flex shrink-0 items-center gap-1.5 border-b-2 border-foreground px-3 py-2 text-sm font-medium" } else { "inline-flex shrink-0 items-center gap-1.5 border-b-2 border-transparent px-3 py-2 text-sm text-muted-foreground hover:text-foreground" },
                                {icon}
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
        div { class: "mx-auto w-full max-w-7xl p-4 md:p-8",
            Outlet::<Route> {}
        }
        if confirm_delete() {
            ConfirmDialog {
                title: "App löschen?",
                message: format!("„{}“ wird gestoppt und mit Image, Quellcode und allen Daten (/data) endgültig gelöscht.", a.name),
                confirm_label: "Endgültig löschen",
                destructive: true,
                oncancel: move |_| confirm_delete.set(false),
                onconfirm: move |_| {
                    confirm_delete.set(false);
                    let id = ctx.id();
                    spawn(async move {
                        match api::delete::<nrz_vps_common::Ok>(&routes::app(&id, "")).await {
                            Ok(_) => {
                                toast("App gelöscht");
                                refresh().await;
                                navigator().push(Route::Dashboard {});
                            }
                            Err(e) => toast_err(e),
                        }
                    });
                },
            }
        }
    }
}
