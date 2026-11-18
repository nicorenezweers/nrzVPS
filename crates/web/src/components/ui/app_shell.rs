//! nrz-App-Gerüst: das gemeinsame Aussehen aller nrzApps (nrzMedia, nrzPass, nrzPaper, …).
//!
//! - `AppShell`: Seitenleiste links (Desktop), Kopfzeile + Schublade (Handy), Inhalt rechts.
//! - Seitenleiste: `AppBrand`, `AppNav`, `AppNavHeading`, `AppNavItem`, `AppNavSub`,
//!   `AppSidebarFooter`, `AppAccountButton`, `AppVersion`.
//! - Seiten: `PageContainer`, `PageHeader`, `PageSection`, `SettingsGroup`, `SettingsRow`,
//!   `EmptyState`, `LoadingState`.
//! - Anmeldung: `AuthFrame` (zentriert, mit Lichtflecken).
//!
//! Braucht `nrz-theme.css` und `nrz-app.css` (macht `nrzui init`). Regeln: DESIGN.md.

use dioxus::prelude::*;
use tw_merge::tw_merge;

/* ========================================================== */
/*                       ✨ SHELL ✨                          */
/* ========================================================== */

/// Zustand der Schublade (mobile Seitenleiste). Ein `AppNavItem` schliesst sie beim Klick.
#[derive(Clone, Copy)]
pub struct AppShellContext {
    pub drawer: Signal<bool>,
}

impl AppShellContext {
    pub fn open(mut self) {
        self.drawer.set(true);
    }
    pub fn close(mut self) {
        self.drawer.set(false);
    }
}

/// Zugriff auf die Schublade, z. B. um sie nach einer Aktion zu schliessen.
#[must_use]
pub fn use_app_shell() -> Option<AppShellContext> {
    try_use_context::<AppShellContext>()
}

/// Rahmen jeder nrzApp. `sidebar` erscheint auf dem Desktop fest links (ab `lg`), auf
/// kleinen Bildschirmen als Schublade hinter dem Menü-Knopf der Kopfzeile.
#[component]
pub fn AppShell(
    /// Inhalt der Seitenleiste (`AppBrand`, `AppNav`, `AppSidebarFooter`).
    sidebar: Element,
    /// Logo + Name in der mobilen Kopfzeile.
    #[props(optional)]
    header_brand: Option<Element>,
    /// Knöpfe rechts in der mobilen Kopfzeile.
    #[props(optional)]
    header_actions: Option<Element>,
    #[props(into, default = "Menü öffnen".to_string())] menu_label: String,
    #[props(into, optional)] class: Option<String>,
    children: Element,
) -> Element {
    let mut drawer = use_signal(|| false);
    use_context_provider(|| AppShellContext { drawer });

    let root = tw_merge!(
        "flex h-dvh overflow-hidden bg-background text-foreground",
        class.as_deref().unwrap_or("")
    );

    rsx! {
        div {
            class: "{root}",
            "data-name": "AppShell",
            onkeydown: move |e| {
                if e.key() == Key::Escape && drawer() {
                    drawer.set(false);
                }
            },
            // Seitenleiste (Desktop)
            aside { class: "hidden w-64 shrink-0 flex-col border-r border-sidenav-border bg-sidenav text-sidenav-foreground lg:flex",
                {sidebar.clone()}
            }
            // Seitenleiste (Handy, Tablet) als Schublade
            if drawer() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 animate-nrz-fade lg:hidden",
                    onclick: move |_| drawer.set(false),
                }
                aside {
                    class: "fixed inset-y-0 left-0 z-50 flex w-72 max-w-[85vw] flex-col border-r border-sidenav-border bg-sidenav text-sidenav-foreground shadow-xl safe-top animate-nrz-sheet lg:hidden",
                    role: "dialog",
                    "aria-modal": "true",
                    {sidebar}
                }
            }
            div { class: "flex min-w-0 flex-1 flex-col",
                // Kopfzeile nur auf kleinen Bildschirmen
                header { class: "flex h-14 shrink-0 items-center gap-2 border-b px-3 safe-top lg:hidden",
                    button {
                        r#type: "button",
                        class: "inline-flex size-9 items-center justify-center rounded-md text-muted-foreground transition-colors outline-none hover:bg-accent hover:text-accent-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50",
                        title: "{menu_label}",
                        "aria-label": "{menu_label}",
                        "aria-expanded": "{drawer()}",
                        onclick: move |_| drawer.set(true),
                        icons::Menu { class: "size-5" }
                    }
                    if let Some(b) = header_brand {
                        div { class: "flex min-w-0 items-center gap-2 font-semibold", {b} }
                    }
                    div { class: "flex-1" }
                    if let Some(a) = header_actions {
                        div { class: "flex items-center gap-1", {a} }
                    }
                }
                main { id: "app-main", class: "min-h-0 flex-1 overflow-y-auto", {children} }
            }
        }
    }
}

/* ========================================================== */
/*                     ✨ SEITENLEISTE ✨                     */
/* ========================================================== */

/// Logo, App-Name und eine kleine Zeile darunter (Konto, Server, Version …).
#[component]
pub fn AppBrand(
    /// Quadratisches Logo (`img` oder `svg`), wird auf `size-7` gebracht.
    logo: Element,
    #[props(into)] name: String,
    #[props(into, optional)] subtitle: Option<String>,
    /// Ziel beim Klick (Router-Pfad), z. B. `/`.
    #[props(into, optional)]
    href: Option<String>,
    /// Zusätzliche Elemente darunter (Profil-Umschalter, Suche …).
    #[props(optional)]
    children: Option<Element>,
) -> Element {
    let inner = rsx! {
        span { class: "grid size-7 shrink-0 place-items-center overflow-hidden rounded-lg [&>img]:size-full [&>svg]:size-full",
            {logo}
        }
        span { class: "min-w-0",
            span { class: "block truncate text-[15px] font-semibold leading-tight tracking-tight", "{name}" }
            if let Some(s) = subtitle.filter(|s| !s.is_empty()) {
                span { class: "block truncate text-[11px] text-muted-foreground", "{s}" }
            }
        }
    };
    let row = "flex items-center gap-2.5 rounded-md px-1 outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50";
    rsx! {
        div { class: "grid gap-3 px-3 pt-4 pb-2", "data-name": "AppBrand",
            if let Some(to) = href {
                Link { to, class: "{row}", {inner} }
            } else {
                div { class: "{row}", {inner} }
            }
            if let Some(c) = children {
                {c}
            }
        }
    }
}

/// Scrollender Bereich mit den Einträgen der Seitenleiste.
#[component]
pub fn AppNav(
    #[props(into, default = "Navigation".to_string())] label: String,
    #[props(into, optional)] class: Option<String>,
    children: Element,
) -> Element {
    let c = tw_merge!(
        "flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-3 pt-2 pb-3",
        class.as_deref().unwrap_or("")
    );
    rsx! {
        nav { class: "{c}", "aria-label": "{label}", {children} }
    }
}

/// Zwischenüberschrift in der Seitenleiste („Planung“, „Werkzeuge“ …), optional mit kleinem Knopf.
#[component]
pub fn AppNavHeading(
    #[props(into)] label: String,
    #[props(optional)] action: Option<Element>,
) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-2 px-2.5 pt-4 pb-1",
            p { class: "text-xs font-medium tracking-wider text-muted-foreground uppercase", "{label}" }
            if let Some(a) = action {
                div { class: "flex items-center text-muted-foreground [&_button]:rounded [&_button]:p-0.5 [&_button:hover]:text-foreground [&_svg]:size-3.5",
                    {a}
                }
            }
        }
    }
}

/// Eintrag der Seitenleiste. Mit `href` als Router-Link, sonst als Knopf mit `onclick`.
/// `count` = leise Zahl rechts (Anzahl), `badge` = hervorgehobene Zahl (z. B. laufende Aufträge).
#[component]
pub fn AppNavItem(
    #[props(into)] label: String,
    #[props(optional)] icon: Option<Element>,
    #[props(default)] active: bool,
    #[props(into, optional)] href: Option<String>,
    #[props(optional)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(optional)] count: Option<usize>,
    #[props(optional)] badge: Option<usize>,
    /// Kleiner Knopf am rechten Rand (z. B. Aufklappen).
    #[props(optional)]
    trailing: Option<Element>,
) -> Element {
    let shell = use_app_shell();
    let state = if active {
        "bg-sidenav-accent font-medium text-sidenav-accent-foreground"
    } else {
        "text-muted-foreground hover:bg-sidenav-accent/60 hover:text-sidenav-accent-foreground"
    };
    let pad = if trailing.is_some() { "pr-8" } else { "" };
    let class = format!(
        "flex w-full min-w-0 items-center gap-2.5 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 [&_svg]:size-4 [&_svg]:shrink-0 {state} {pad}"
    );
    let current = if active { "page" } else { "false" };
    let content = rsx! {
        if let Some(i) = icon {
            {i}
        }
        span { class: "flex-1 truncate", "{label}" }
        if let Some(n) = badge.filter(|n| *n > 0) {
            span { class: "rounded-full bg-primary px-1.5 text-[11px] font-semibold leading-5 text-primary-foreground tabular-nums", "{n}" }
        }
        if let Some(n) = count {
            span { class: "text-xs text-muted-foreground tabular-nums", "{n}" }
        }
    };
    let close = move || {
        if let Some(s) = shell {
            s.close();
        }
    };
    rsx! {
        div { class: "relative flex items-center", "data-name": "AppNavItem",
            if let Some(to) = href {
                Link {
                    to,
                    class: "{class}",
                    "aria-current": current,
                    onclick: move |e: MouseEvent| {
                        close();
                        if let Some(h) = onclick {
                            h.call(e);
                        }
                    },
                    {content}
                }
            } else {
                button {
                    r#type: "button",
                    class: "{class}",
                    "aria-current": current,
                    onclick: move |e| {
                        close();
                        if let Some(h) = onclick {
                            h.call(e);
                        }
                    },
                    {content}
                }
            }
            if let Some(t) = trailing {
                div { class: "absolute right-1 flex items-center", {t} }
            }
        }
    }
}

/// Eingerückte Untereinträge mit Linie links.
#[component]
pub fn AppNavSub(children: Element) -> Element {
    rsx! {
        div { class: "mt-0.5 ml-[1.1rem] flex flex-col gap-0.5 border-l border-sidenav-border pl-2", {children} }
    }
}

/// Unterer Teil der Seitenleiste: Einstellungen, Konto, Version.
#[component]
pub fn AppSidebarFooter(children: Element) -> Element {
    rsx! {
        div { class: "grid gap-0.5 border-t border-sidenav-border p-3", "data-name": "AppSidebarFooter", {children} }
    }
}

/// Konto-Zeile (Initiale, Name, Zusatz) – als Auslöser für ein Kontomenü (`DropdownMenu`).
#[component]
pub fn AppAccountButton(
    #[props(into)] name: String,
    #[props(into, optional)] detail: Option<String>,
    #[props(optional)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    rsx! {
        button {
            r#type: "button",
            class: "flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left outline-none hover:bg-sidenav-accent focus-visible:ring-[3px] focus-visible:ring-ring/50",
            onclick: move |e| {
                if let Some(h) = onclick {
                    h.call(e);
                }
            },
            span { class: "grid size-8 shrink-0 place-items-center rounded-full bg-muted text-sm font-medium", "{initial}" }
            span { class: "min-w-0 flex-1",
                span { class: "block truncate text-sm font-medium", "{name}" }
                if let Some(d) = detail {
                    span { class: "block truncate text-xs text-muted-foreground", "{d}" }
                }
            }
            icons::ChevronsUpDown { class: "size-4 shrink-0 text-muted-foreground" }
        }
    }
}

/// Versionszeile ganz unten in der Seitenleiste, z. B. „nrzMedia 2.5.3“.
#[component]
pub fn AppVersion(#[props(into)] text: String) -> Element {
    rsx! {
        p { class: "px-2 pt-1 text-[11px] leading-4 text-muted-foreground tabular-nums select-text", "{text}" }
    }
}

/* ========================================================== */
/*                        ✨ SEITEN ✨                        */
/* ========================================================== */

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum PageWidth {
    /// Formulare, Einstellungen (max-w-3xl)
    Narrow,
    /// Normale Seiten (max-w-5xl)
    #[default]
    Default,
    /// Tabellen, Raster (max-w-7xl)
    Wide,
    /// Volle Breite (Kalender, Canvas)
    Full,
}

impl PageWidth {
    const fn class(self) -> &'static str {
        match self {
            Self::Narrow => "max-w-3xl",
            Self::Default => "max-w-5xl",
            Self::Wide => "max-w-7xl",
            Self::Full => "max-w-none",
        }
    }
}

/// Innenabstand und Breite einer Seite im `AppShell`.
#[component]
pub fn PageContainer(
    #[props(default)] width: PageWidth,
    #[props(into, optional)] class: Option<String>,
    children: Element,
) -> Element {
    let c = tw_merge!(
        "mx-auto w-full p-4 md:p-8",
        width.class(),
        class.as_deref().unwrap_or("")
    );
    rsx! {
        div { class: "{c}", "data-name": "PageContainer", {children} }
    }
}

/// Seitenkopf: kleine Oberzeile, Titel, Beschreibung und Knöpfe rechts.
#[component]
pub fn PageHeader(
    #[props(into)] title: String,
    #[props(into, optional)] description: Option<String>,
    #[props(into, optional)] eyebrow: Option<String>,
    #[props(optional)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: "mb-6 flex flex-wrap items-end justify-between gap-4", "data-name": "PageHeader",
            div { class: "min-w-0",
                if let Some(e) = eyebrow {
                    p { class: "text-xs font-medium tracking-wider text-muted-foreground uppercase", "{e}" }
                }
                h1 { class: "truncate text-2xl font-semibold tracking-tight", "{title}" }
                if let Some(d) = description {
                    p { class: "mt-1 text-sm text-muted-foreground", "{d}" }
                }
            }
            if let Some(a) = actions {
                div { class: "flex flex-wrap items-center gap-2", {a} }
            }
        }
    }
}

/// Abschnitt als Karte mit Kopf (Titel, Beschreibung, Knöpfe) und Inhalt.
#[component]
pub fn PageSection(
    #[props(into)] title: String,
    #[props(into, optional)] description: Option<String>,
    #[props(optional)] actions: Option<Element>,
    #[props(into, optional)] class: Option<String>,
    children: Element,
) -> Element {
    let c = tw_merge!(
        "rounded-xl border bg-card text-card-foreground shadow-sm",
        class.as_deref().unwrap_or("")
    );
    rsx! {
        section { class: "{c}", "data-name": "PageSection",
            div { class: "flex flex-wrap items-start justify-between gap-3 border-b px-4 py-4 sm:px-6",
                div { class: "min-w-0",
                    h2 { class: "leading-none font-semibold", "{title}" }
                    if let Some(d) = description {
                        p { class: "mt-1.5 text-sm text-muted-foreground", "{d}" }
                    }
                }
                if let Some(a) = actions {
                    div { class: "flex items-center gap-2", {a} }
                }
            }
            div { class: "px-4 py-5 sm:px-6", {children} }
        }
    }
}

/// Gruppe in Einstellungen: Überschrift + Kasten mit Zeilen (`SettingsRow`), wie in den Mac-Apps.
#[component]
pub fn SettingsGroup(
    #[props(into)] title: String,
    #[props(optional)] actions: Option<Element>,
    #[props(into, optional)] footer: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "grid gap-2", "data-name": "SettingsGroup",
            div { class: "flex items-center justify-between gap-2 px-1",
                h3 { class: "text-sm font-semibold", "{title}" }
                if let Some(a) = actions {
                    div { class: "flex items-center gap-2 text-xs", {a} }
                }
            }
            div { class: "divide-y rounded-lg border bg-card", {children} }
            if let Some(f) = footer {
                p { class: "px-1 text-xs text-muted-foreground", "{f}" }
            }
        }
    }
}

/// Zeile in einer `SettingsGroup`: Beschriftung links, Bedienelement rechts.
#[component]
pub fn SettingsRow(
    #[props(into)] label: String,
    #[props(into, optional)] hint: Option<String>,
    #[props(default)] disabled: bool,
    children: Element,
) -> Element {
    let dim = if disabled { "opacity-50" } else { "" };
    rsx! {
        div { class: "flex min-h-11 flex-wrap items-center justify-between gap-x-4 gap-y-2 px-3 py-2 {dim}",
            div { class: "min-w-0",
                p { class: "text-sm", "{label}" }
                if let Some(h) = hint {
                    p { class: "text-xs text-muted-foreground", "{h}" }
                }
            }
            div { class: "flex min-w-0 items-center gap-2", {children} }
        }
    }
}

/// Leerer Zustand: gestrichelter Rahmen, Symbol, Titel, Text und optional Knöpfe.
#[component]
pub fn EmptyState(
    #[props(into)] title: String,
    #[props(into, optional)] description: Option<String>,
    #[props(optional)] icon: Option<Element>,
    #[props(optional)] children: Option<Element>,
) -> Element {
    rsx! {
        div { class: "flex min-h-56 flex-col items-center justify-center gap-3 rounded-xl border border-dashed p-8 text-center", "data-name": "EmptyState",
            if let Some(i) = icon {
                div { class: "grid size-12 place-items-center rounded-xl bg-muted text-muted-foreground [&_svg]:size-6", {i} }
            }
            div {
                p { class: "font-medium", "{title}" }
                if let Some(d) = description {
                    p { class: "mt-1 max-w-sm text-sm text-muted-foreground", "{d}" }
                }
            }
            if let Some(c) = children {
                div { class: "flex flex-wrap items-center justify-center gap-2", {c} }
            }
        }
    }
}

/// Ladeanzeige: kleiner Kreis und leiser Text.
#[component]
pub fn LoadingState(#[props(into, default = "Lade …".to_string())] text: String) -> Element {
    rsx! {
        div { class: "flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground", role: "status",
            icons::LoaderCircle { class: "size-4 animate-spin" }
            "{text}"
        }
    }
}

/* ========================================================== */
/*                       ✨ ANMELDUNG ✨                      */
/* ========================================================== */

/// Anmelde-, Einrichtungs- und Entsperrseite: zentriert, Logo oben, weiche Lichtflecken.
#[component]
pub fn AuthFrame(
    /// Logo, wird auf `size-14` gebracht.
    logo: Element,
    #[props(into)] title: String,
    /// Kurze Zeile unter dem Namen, z. B. „Zero-Knowledge · Rust“.
    #[props(into, optional)]
    tagline: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "relative flex min-h-dvh items-center justify-center overflow-hidden p-4 safe-top safe-bottom", "data-name": "AuthFrame",
            div { class: "nrz-drift", "aria-hidden": "true", span {} span {} }
            div { class: "relative z-10 w-full max-w-sm",
                div { class: "mb-6 flex flex-col items-center gap-3 text-center",
                    div { class: "grid size-14 place-items-center overflow-hidden rounded-2xl [&>img]:size-full [&>svg]:size-full", {logo} }
                    div {
                        h1 { class: "text-xl font-semibold tracking-tight", "{title}" }
                        if let Some(t) = tagline {
                            p { class: "text-xs text-muted-foreground", "{t}" }
                        }
                    }
                }
                div { class: "rounded-xl border bg-card/80 p-6 text-card-foreground shadow-sm backdrop-blur", {children} }
            }
        }
    }
}
