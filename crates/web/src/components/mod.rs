//! Gemeinsame Bausteine der Oberfläche (nrz-Design, nur Theme-Farben).
#![allow(dead_code)]

pub mod ui;

use dioxus::prelude::*;
use nrz_vps_common::AppStatus;

use crate::store::TOASTS;

/// Klassen für Eingabefelder (wie nrzUI `Input`).
pub const INPUT: &str = "h-9 w-full min-w-0 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-input/30";
/// Klassen für `select`.
pub const SELECT: &str = "h-9 w-full rounded-md border border-input bg-background px-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 dark:bg-input/30";
/// Klassen für `textarea`.
pub const TEXTAREA: &str = "w-full min-h-24 rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 dark:bg-input/30";

/* ------------------------------------------------------------------ */
/*                               Dialoge                              */
/* ------------------------------------------------------------------ */

/// Dialog über allem. Vom Aufrufer nur gerendert, wenn offen. Esc / Klick daneben schliesst.
#[component]
pub fn Modal(
    #[props(into)] title: String,
    #[props(into, optional)] description: Option<String>,
    onclose: EventHandler<()>,
    #[props(optional)] footer: Option<Element>,
    #[props(default)] wide: bool,
    children: Element,
) -> Element {
    let width = if wide { "max-w-3xl" } else { "max-w-lg" };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-end justify-center bg-black/60 p-0 animate-nrz-fade sm:items-center sm:p-4",
            onclick: move |_| onclose.call(()),
            onkeydown: move |e| if e.key() == Key::Escape { onclose.call(()) },
            div {
                class: "flex max-h-[90dvh] w-full {width} flex-col rounded-t-xl border bg-popover text-popover-foreground shadow-lg animate-nrz-enter sm:rounded-xl",
                role: "dialog",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-start justify-between gap-4 border-b px-5 py-4",
                    div {
                        h2 { class: "font-semibold", "{title}" }
                        if let Some(d) = description {
                            p { class: "mt-1 text-sm text-muted-foreground", "{d}" }
                        }
                    }
                    button {
                        class: "rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground",
                        aria_label: "Schliessen",
                        onclick: move |_| onclose.call(()),
                        icons::X { class: "size-4" }
                    }
                }
                div { class: "flex-1 overflow-y-auto px-5 py-4", {children} }
                if let Some(f) = footer {
                    div { class: "flex flex-wrap justify-end gap-2 border-t px-5 py-3", {f} }
                }
            }
        }
    }
}

/// Rückfrage vor gefährlichen Aktionen.
#[component]
pub fn ConfirmDialog(
    #[props(into)] title: String,
    #[props(into)] message: String,
    #[props(into, default = "Bestätigen".to_string())] confirm_label: String,
    #[props(default)] destructive: bool,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
) -> Element {
    use ui::button::{Button, ButtonVariant};
    let variant = if destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Default
    };
    rsx! {
        Modal {
            title,
            onclose: move |_| oncancel.call(()),
            footer: rsx! {
                Button { variant: ButtonVariant::Outline, onclick: move |_| oncancel.call(()), "Abbrechen" }
                Button { variant, onclick: move |_| onconfirm.call(()), "{confirm_label}" }
            },
            p { class: "text-sm text-muted-foreground", "{message}" }
        }
    }
}

/// Meldungen unten rechts (Handy: unten Mitte).
#[component]
pub fn Toaster() -> Element {
    let toasts = TOASTS.read().clone();
    rsx! {
        div { class: "pointer-events-none fixed inset-x-0 bottom-4 z-[60] flex flex-col items-center gap-2 px-4 sm:inset-x-auto sm:right-4 sm:items-end",
            for t in toasts {
                div {
                    key: "{t.id}",
                    class: if t.error {
                        "pointer-events-auto flex max-w-sm items-start gap-2 rounded-lg border border-destructive/40 bg-popover px-3 py-2.5 text-sm shadow-lg animate-nrz-toast"
                    } else {
                        "pointer-events-auto flex max-w-sm items-start gap-2 rounded-lg border bg-popover px-3 py-2.5 text-sm shadow-lg animate-nrz-toast"
                    },
                    if t.error {
                        icons::CircleAlert { class: "mt-0.5 size-4 shrink-0 text-destructive" }
                    } else {
                        icons::CircleCheck { class: "mt-0.5 size-4 shrink-0 text-success" }
                    }
                    span { "{t.text}" }
                }
            }
        }
    }
}

/* ------------------------------------------------------------------ */
/*                           Status & Werte                           */
/* ------------------------------------------------------------------ */

pub fn status_color(s: AppStatus) -> &'static str {
    match s {
        AppStatus::Running => "bg-success",
        AppStatus::Building | AppStatus::Restarting => "bg-warning",
        AppStatus::Crashed => "bg-destructive",
        AppStatus::Stopped | AppStatus::New => "bg-muted-foreground/40",
    }
}

#[component]
pub fn StatusDot(status: AppStatus) -> Element {
    let c = status_color(status);
    let pulse = matches!(status, AppStatus::Building | AppStatus::Restarting);
    rsx! {
        span { class: "relative inline-flex size-2 shrink-0",
            if pulse {
                span { class: "absolute inline-flex size-full animate-ping rounded-full {c} opacity-60" }
            }
            span { class: "relative inline-flex size-2 rounded-full {c}" }
        }
    }
}

#[component]
pub fn StatusBadge(status: AppStatus) -> Element {
    let tone = match status {
        AppStatus::Running => "bg-success/15 text-success",
        AppStatus::Building | AppStatus::Restarting => "bg-warning/15 text-warning",
        AppStatus::Crashed => "bg-destructive/15 text-destructive",
        AppStatus::Stopped | AppStatus::New => "bg-muted text-muted-foreground",
    };
    rsx! {
        span { class: "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium {tone}",
            StatusDot { status }
            "{status.label()}"
        }
    }
}

/// Kennzahl-Kachel.
#[component]
pub fn StatCard(
    #[props(into)] label: String,
    #[props(into)] value: String,
    #[props(into, optional)] hint: Option<String>,
    #[props(optional)] icon: Option<Element>,
    /// 0–100 für einen Balken.
    #[props(optional)]
    percent: Option<f64>,
) -> Element {
    rsx! {
        div { class: "rounded-xl border bg-card p-4 shadow-sm",
            div { class: "flex items-center justify-between text-xs font-medium uppercase tracking-wider text-muted-foreground",
                span { "{label}" }
                if let Some(i) = icon {
                    span { class: "text-muted-foreground", {i} }
                }
            }
            div { class: "mt-2 text-2xl font-semibold tabular-nums tracking-tight", "{value}" }
            if let Some(p) = percent {
                div { class: "mt-2 h-1.5 overflow-hidden rounded-full bg-muted",
                    div {
                        class: if p > 85.0 { "h-full rounded-full bg-destructive transition-all" } else if p > 65.0 { "h-full rounded-full bg-warning transition-all" } else { "h-full rounded-full bg-primary transition-all" },
                        style: "width: {p.clamp(0.0, 100.0)}%",
                    }
                }
            }
            if let Some(h) = hint {
                div { class: "mt-1 text-xs text-muted-foreground", "{h}" }
            }
        }
    }
}

/// Schalter (kontrolliert).
#[component]
pub fn Toggle(
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] disabled: bool,
    #[props(into, optional)] label: Option<String>,
) -> Element {
    let state = if checked { "checked" } else { "unchecked" };
    rsx! {
        button {
            r#type: "button",
            role: "switch",
            aria_checked: "{checked}",
            aria_label: label.unwrap_or_default(),
            disabled,
            "data-state": state,
            class: "inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=unchecked]:bg-input",
            onclick: move |_| onchange.call(!checked),
            span {
                "data-state": state,
                class: "pointer-events-none block size-4 rounded-full bg-background shadow-lg ring-0 transition-transform data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0",
            }
        }
    }
}

/// Formularfeld: Beschriftung, Inhalt, Hinweis.
#[component]
pub fn Field(
    #[props(into)] label: String,
    #[props(into, optional)] hint: Option<String>,
    #[props(into, optional)] class: Option<String>,
    children: Element,
) -> Element {
    let class = class.unwrap_or_default();
    rsx! {
        label { class: "flex flex-col gap-1.5 {class}",
            span { class: "text-sm font-medium", "{label}" }
            {children}
            if let Some(h) = hint {
                span { class: "text-xs text-muted-foreground", "{h}" }
            }
        }
    }
}

/// Segmentierte Auswahl / einfache Reiter.
#[component]
pub fn Segmented(
    items: Vec<(String, String)>,
    active: String,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "inline-flex flex-wrap items-center gap-1 rounded-lg bg-muted p-1",
            for (value, label) in items {
                button {
                    key: "{value}",
                    r#type: "button",
                    class: if value == active { "rounded-md bg-background px-3 py-1 text-sm font-medium shadow-sm" } else { "rounded-md px-3 py-1 text-sm text-muted-foreground hover:text-foreground" },
                    onclick: {
                        let v = value.clone();
                        move |_| onchange.call(v.clone())
                    },
                    "{label}"
                }
            }
        }
    }
}

/// Spinner + Text.
#[component]
pub fn Loading(#[props(into, default = "Lade …".to_string())] text: String) -> Element {
    rsx! {
        div { class: "flex items-center gap-2 py-8 text-sm text-muted-foreground justify-center",
            icons::LoaderCircle { class: "size-4 animate-spin" }
            "{text}"
        }
    }
}

/// Fehlerkasten.
#[component]
pub fn ErrorBox(#[props(into)] message: String) -> Element {
    rsx! {
        div { class: "flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2.5 text-sm",
            icons::CircleAlert { class: "mt-0.5 size-4 shrink-0 text-destructive" }
            span { "{message}" }
        }
    }
}

/// Hinweis (neutral oder Warnung).
#[component]
pub fn Notice(
    #[props(default)] warning: bool,
    #[props(optional)] action: Option<Element>,
    children: Element,
) -> Element {
    let tone = if warning {
        "border-warning/30 bg-warning/10"
    } else {
        "bg-muted/50"
    };
    rsx! {
        div { class: "flex flex-wrap items-center gap-3 rounded-lg border px-3 py-2.5 text-sm {tone}",
            if warning {
                icons::TriangleAlert { class: "size-4 shrink-0 text-warning" }
            } else {
                icons::Info { class: "size-4 shrink-0 text-muted-foreground" }
            }
            div { class: "min-w-0 flex-1", {children} }
            if let Some(a) = action {
                {a}
            }
        }
    }
}

/* ------------------------------------------------------------------ */
/*                             ANSI-Farben                            */
/* ------------------------------------------------------------------ */

/// Konsolenzeile mit ANSI-Farbcodes → farbige Abschnitte (nur Theme-Zustandsfarben).
#[component]
pub fn AnsiLine(#[props(into)] text: String) -> Element {
    let parts = parse_ansi(&text);
    rsx! {
        for (i, (class, t)) in parts.into_iter().enumerate() {
            span { key: "{i}", class: "{class}", "{t}" }
        }
    }
}

pub fn parse_ansi(s: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut class: &'static str = "";
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            let mut code = String::new();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    if d != 'm' {
                        code.clear();
                    }
                    break;
                }
                code.push(d);
            }
            if !cur.is_empty() {
                out.push((class, std::mem::take(&mut cur)));
            }
            for part in code.split(';') {
                class = match part.parse::<u32>().unwrap_or(0) {
                    0 | 39 => "",
                    1 => "font-semibold",
                    31 | 91 => "text-destructive",
                    32 | 92 => "text-success",
                    33 | 93 => "text-warning",
                    34 | 94 | 36 | 96 => "text-info",
                    35 | 95 => "text-info",
                    90 | 37 => "text-muted-foreground",
                    _ => class,
                };
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push((class, cur));
    }
    out
}

/* ------------------------------------------------------------------ */
/*                              Formate                               */
/* ------------------------------------------------------------------ */

pub fn fmt_bytes(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn fmt_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    if d > 0 {
        format!("{d} T {h} Std")
    } else if h > 0 {
        format!("{h} Std {m} Min")
    } else if m > 0 {
        format!("{m} Min")
    } else {
        format!("{secs} s")
    }
}

/// Unix-Sekunden → „15.11.2026 14:03“.
pub fn fmt_datetime(secs: i64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(secs as f64 * 1000.0));
    format!(
        "{:02}.{:02}.{} {:02}:{:02}",
        d.get_date(),
        d.get_month() + 1,
        d.get_full_year(),
        d.get_hours(),
        d.get_minutes()
    )
}

/// Unix-Millisekunden → „14:03:22“.
pub fn fmt_time_ms(ms: i64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    format!(
        "{:02}:{:02}:{:02}",
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds()
    )
}

/// „vor 5 Min“.
pub fn fmt_ago(secs: i64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let d = (now - secs).max(0);
    match d {
        0..=59 => "gerade eben".into(),
        60..=3599 => format!("vor {} Min", d / 60),
        3600..=86399 => format!("vor {} Std", d / 3600),
        _ => format!("vor {} Tagen", d / 86400),
    }
}

/* ------------------------------------------------------------------ */
/*                     Weitere allgemeine Bausteine                   */
/* ------------------------------------------------------------------ */

/// Klassen für eine Karte (nrz-Design).
pub const CARD: &str = "rounded-xl border bg-card text-card-foreground shadow-sm";

/// Kontrollkästchen mit Beschriftung und optionalem Hinweis.
#[component]
pub fn CheckRow(
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(into)] label: String,
    #[props(into, optional)] hint: Option<String>,
    #[props(default)] disabled: bool,
    #[props(optional)] children: Option<Element>,
) -> Element {
    let dim = if disabled {
        "opacity-50 cursor-not-allowed"
    } else {
        "cursor-pointer"
    };
    rsx! {
        label { class: "flex items-start gap-2.5 text-sm {dim}",
            input {
                r#type: "checkbox",
                class: "mt-0.5 size-4 shrink-0 accent-primary",
                checked,
                disabled,
                onchange: move |e| onchange.call(e.checked()),
            }
            span { class: "flex min-w-0 flex-col gap-0.5",
                span { "{label}" }
                if let Some(h) = hint {
                    span { class: "text-xs text-muted-foreground", "{h}" }
                }
                if let Some(c) = children {
                    {c}
                }
            }
        }
    }
}

/// Nur-Lese-Wert in Monospace mit Kopier-Knopf; `secret` blendet den Wert aus, bis er aufgedeckt wird.
#[component]
pub fn CopyField(
    #[props(into)] value: String,
    #[props(default)] secret: bool,
    #[props(into, optional)] class: Option<String>,
) -> Element {
    let mut shown = use_signal(|| !secret);
    let class = class.unwrap_or_default();
    let display = if shown() {
        value.clone()
    } else {
        "•".repeat(value.chars().count().clamp(8, 24))
    };
    rsx! {
        div { class: "flex min-w-0 items-center gap-1 rounded-md border bg-muted/40 py-1 pr-1 pl-3 {class}",
            code { class: "min-w-0 flex-1 truncate font-mono text-xs", title: if shown() { value.clone() } else { String::new() }, "{display}" }
            if secret {
                button {
                    r#type: "button",
                    class: "rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground",
                    title: if shown() { "Verbergen" } else { "Anzeigen" },
                    onclick: move |_| shown.toggle(),
                    if shown() {
                        icons::EyeOff { class: "size-3.5" }
                    } else {
                        icons::Eye { class: "size-3.5" }
                    }
                }
            }
            button {
                r#type: "button",
                class: "rounded p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground",
                title: "Kopieren",
                onclick: move |_| {
                    crate::api::copy(&value);
                    crate::store::toast("In die Zwischenablage kopiert");
                },
                icons::Copy { class: "size-3.5" }
            }
        }
    }
}

/// Mehrzeiliger Code-Schnipsel mit Kopier-Knopf.
#[component]
pub fn CodeBlock(
    #[props(into)] code: String,
    #[props(into, optional)] label: Option<String>,
) -> Element {
    let c2 = code.clone();
    let label = label.unwrap_or_default();
    rsx! {
        div { class: "overflow-hidden rounded-lg border bg-muted/40",
            div { class: "flex items-center justify-between gap-2 border-b px-3 py-1.5",
                span { class: "text-xs text-muted-foreground", "{label}" }
                button {
                    r#type: "button",
                    class: "inline-flex items-center gap-1 rounded px-1.5 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                    onclick: move |_| {
                        crate::api::copy(&c2);
                        crate::store::toast("In die Zwischenablage kopiert");
                    },
                    icons::Copy { class: "size-3.5" }
                    "Kopieren"
                }
            }
            pre { class: "overflow-x-auto p-3 font-mono text-xs leading-relaxed whitespace-pre", "{code}" }
        }
    }
}

/// Kleines Etikett in einer Zustandsfarbe: `tone` = "success" | "warning" | "destructive" | "info" | "" (neutral).
#[component]
pub fn Tag(#[props(into, default)] tone: String, children: Element) -> Element {
    let c = match tone.as_str() {
        "success" => "bg-success/15 text-success",
        "warning" => "bg-warning/15 text-warning",
        "destructive" => "bg-destructive/15 text-destructive",
        "info" => "bg-info/15 text-info",
        _ => "bg-muted text-muted-foreground",
    };
    rsx! {
        span { class: "inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium {c}", {children} }
    }
}

/// Unix-Millisekunden → „15.11.2026 14:03“.
pub fn fmt_datetime_ms(ms: i64) -> String {
    fmt_datetime(ms / 1000)
}
/* ------------------------------------------------------------------ */
/*                        Menüs & kleine Knöpfe                       */
/* ------------------------------------------------------------------ */

/// Klassen für einen runden Symbol-Knopf (ohne Rahmen).
pub const ICON_BUTTON: &str = "inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4";
/// Eintrag in einem [`Dropdown`].
pub const MENU_ITEM: &str = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground";
/// Gefährlicher Eintrag in einem [`Dropdown`].
pub const MENU_ITEM_DANGER: &str = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-destructive hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0";

/// Aufklappmenü hinter einem Knopf (Standard: „…“). Klick auf einen Eintrag, daneben oder Esc schliesst.
#[component]
pub fn Dropdown(
    #[props(optional)] trigger: Option<Element>,
    #[props(into, default = "Aktionen".to_string())] label: String,
    /// Menü nach oben öffnen (z. B. am Seitenende).
    #[props(default)]
    up: bool,
    children: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let pos = if up {
        "bottom-full mb-1"
    } else {
        "top-full mt-1"
    };
    rsx! {
        div {
            class: "relative inline-flex",
            onkeydown: move |e| if e.key() == Key::Escape { open.set(false) },
            button {
                r#type: "button",
                class: ICON_BUTTON,
                aria_label: "{label}",
                title: "{label}",
                aria_expanded: "{open}",
                onclick: move |e| {
                    e.stop_propagation();
                    open.toggle();
                },
                if let Some(t) = trigger {
                    {t}
                } else {
                    icons::Ellipsis { class: "size-4" }
                }
            }
            if open() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e| {
                        e.stop_propagation();
                        open.set(false);
                    },
                }
                div {
                    class: "absolute right-0 {pos} z-50 flex min-w-48 flex-col rounded-lg border bg-popover p-1 text-popover-foreground shadow-lg animate-nrz-enter",
                    role: "menu",
                    onclick: move |e| {
                        e.stop_propagation();
                        open.set(false);
                    },
                    {children}
                }
            }
        }
    }
}

/// Trennlinie in einem [`Dropdown`].
#[component]
pub fn MenuSeparator() -> Element {
    rsx! { div { class: "-mx-1 my-1 h-px bg-border" } }
}

/// Kleine Überschrift in einem [`Dropdown`].
#[component]
pub fn MenuLabel(#[props(into)] text: String) -> Element {
    rsx! { div { class: "px-2 pt-1.5 pb-1 text-xs font-medium text-muted-foreground", "{text}" } }
}

/* ------------------------------------------------------------------ */
/*                    Dateien (Upload, Drag & Drop)                   */
/* ------------------------------------------------------------------ */

/// `FileList` → `Vec<File>`.
pub fn file_list_vec(list: Option<web_sys::FileList>) -> Vec<web_sys::File> {
    let Some(list) = list else { return Vec::new() };
    (0..list.length()).filter_map(|i| list.item(i)).collect()
}

/// Gewählte Dateien eines `input type=file` (per `id`); leert das Feld danach (gleiche Datei erneut wählbar).
pub fn take_input_files(id: &str) -> Vec<web_sys::File> {
    use wasm_bindgen::JsCast;
    let Some(input) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return Vec::new();
    };
    let files = file_list_vec(input.files());
    input.set_value("");
    files
}

/// Dateiauswahl eines versteckten `input type=file` öffnen.
pub fn open_file_picker(id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        el.click();
    }
}

/// Dateien aus einem Drop-Ereignis (vom Betriebssystem hineingezogen).
pub fn dropped_files(e: &DragEvent) -> Vec<web_sys::File> {
    e.downcast::<web_sys::DragEvent>()
        .and_then(|d| d.data_transfer())
        .map(|dt| file_list_vec(dt.files()))
        .unwrap_or_default()
}

/// Zieht der Benutzer Dateien vom Betriebssystem (nicht Elemente der Seite)?
pub fn drag_has_files(e: &DragEvent) -> bool {
    e.downcast::<web_sys::DragEvent>()
        .and_then(|d| d.data_transfer())
        .map(|dt| {
            dt.types()
                .iter()
                .any(|t| t.as_string().as_deref() == Some("Files"))
        })
        .unwrap_or(false)
}

/// Zahl mit Tausender-Apostroph (Schweiz): 1234567 → „1’234’567“.
pub fn fmt_number(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('’');
        }
        out.push(c);
    }
    out
}

/* ------------------------------------------------------------------ */
/*                              nrzVPS                                */
/* ------------------------------------------------------------------ */

/// CPU-Kerne lesbar: 0.25 → „0.25 Kerne“, 1 → „1 Kern“.
pub fn fmt_cpus(c: f64) -> String {
    let v = (c * 100.0).round() / 100.0;
    let s = if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    };
    if (v - 1.0).abs() < f64::EPSILON {
        format!("{s} Kern")
    } else {
        format!("{s} Kerne")
    }
}

/// Megabyte lesbar: 512 → „512 MB“, 2048 → „2.0 GB“.
pub fn fmt_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{mb} MB")
    }
}

/// Prozent `used / total` (0 bei total = 0).
pub fn pct(used: f64, total: f64) -> f64 {
    if total <= 0.0 {
        0.0
    } else {
        (used / total * 100.0).clamp(0.0, 100.0)
    }
}

/// Laufzeit bzw. Quelle einer App kurz: „Node.js“, „Image“, „Automatisch (Python)“.
pub fn runtime_text(a: &nrz_vps_common::App) -> String {
    use nrz_vps_common::{Runtime, Source};
    if matches!(a.source, Source::Image { .. }) {
        return "Fertiges Image".into();
    }
    match a.runtime {
        Runtime::Auto => match a.deploy.as_ref().and_then(|d| d.runtime) {
            Some(r) if r != Runtime::Auto => r.label().to_string(),
            _ => "Automatisch".into(),
        },
        r => r.label().to_string(),
    }
}

/// Eingabefeld für Zahlen (als Text, damit Kommas gehen): liefert den Rohtext.
pub fn parse_f64(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok()
}
