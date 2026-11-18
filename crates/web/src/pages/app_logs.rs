//! App → Logs: Ausgabe des Containers oder des letzten Builds, aktualisiert sich selbst.

use dioxus::prelude::*;
use nrz_vps_common::{AppStatus, Logs, routes};

use crate::{
    api,
    components::{
        AnsiLine, CARD, Segmented, Toggle,
        ui::button::{Button, ButtonSize, ButtonVariant},
    },
    layout::use_app,
};

#[component]
pub fn AppLogs(id: String) -> Element {
    let ctx = use_app();
    let building = ctx
        .app
        .read()
        .as_ref()
        .is_some_and(|a| a.status == AppStatus::Building);
    // Während des Builds standardmässig das Build-Log zeigen
    let mut mode = use_signal(move || {
        if building {
            "build".to_string()
        } else {
            "app".to_string()
        }
    });
    let mut lines = use_signal(Vec::<String>::new);
    let mut follow = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut tick = use_signal(|| 0u32);

    use_future(move || async move {
        loop {
            let id = ctx.id();
            let path = if *mode.peek() == "build" {
                routes::app(&id, "build-log")
            } else {
                format!("{}?lines=500", routes::app(&id, "logs"))
            };
            match api::get::<Logs>(&path).await {
                Ok(l) => {
                    lines.set(l.lines);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            if *follow.peek() {
                scroll_bottom();
            }
            let start = *tick.peek();
            for _ in 0..30 {
                gloo_timers::future::TimeoutFuture::new(100).await;
                if *tick.peek() != start {
                    break;
                }
            }
        }
    });

    let list = lines.read().clone();
    rsx! {
        div { class: "flex flex-col gap-3",
            div { class: "flex flex-wrap items-center justify-between gap-3",
                Segmented {
                    items: vec![("app".to_string(), "App".to_string()), ("build".to_string(), "Letzter Build".to_string())],
                    active: mode(),
                    onchange: move |v| {
                        mode.set(v);
                        lines.set(Vec::new());
                        tick += 1;
                    },
                }
                div { class: "flex items-center gap-3 text-sm text-muted-foreground",
                    label { class: "flex items-center gap-2",
                        Toggle { checked: follow(), onchange: move |v| follow.set(v), label: "Mitlaufen" }
                        "Mitlaufen"
                    }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| tick += 1,
                        icons::RefreshCw { class: "size-4" }
                        "Aktualisieren"
                    }
                }
            }
            if let Some(e) = error() {
                crate::components::ErrorBox { message: e }
            }
            div { id: "log-box", class: "{CARD} h-[60vh] overflow-y-auto bg-muted/30 p-3 font-mono text-xs leading-relaxed",
                if list.is_empty() {
                    p { class: "text-muted-foreground",
                        if mode() == "build" { "Noch kein Build-Log." } else { "Noch keine Ausgabe – läuft die App?" }
                    }
                }
                for (i, l) in list.iter().enumerate() {
                    div { key: "{i}", class: "console-line", AnsiLine { text: l.clone() } }
                }
            }
        }
    }
}

fn scroll_bottom() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("log-box"))
    {
        el.set_scroll_top(el.scroll_height());
    }
}
