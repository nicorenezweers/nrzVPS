//! App → Konsole: einzelne Befehle im laufenden Container ausführen.

use dioxus::prelude::*;
use nrz_vps_common::{AppStatus, ExecRequest, ExecResult, routes};

use crate::{
    api,
    components::{CARD, INPUT, Notice, ui::button::Button},
    layout::use_app,
};

#[derive(Clone, PartialEq)]
struct Entry {
    command: String,
    result: Result<ExecResult, String>,
}

#[component]
pub fn AppConsole(id: String) -> Element {
    let ctx = use_app();
    let mut command = use_signal(String::new);
    let mut history = use_signal(Vec::<Entry>::new);
    let mut busy = use_signal(|| false);
    let running = ctx
        .app
        .read()
        .as_ref()
        .is_some_and(|a| a.status == AppStatus::Running);

    let run = move |e: FormEvent| {
        e.prevent_default();
        let cmd = command().trim().to_string();
        if cmd.is_empty() || busy() {
            return;
        }
        busy.set(true);
        let id = ctx.id();
        spawn(async move {
            let result = api::post::<_, ExecResult>(
                &routes::app(&id, "exec"),
                &ExecRequest {
                    command: cmd.clone(),
                },
            )
            .await;
            history.write().push(Entry {
                command: cmd,
                result,
            });
            command.set(String::new());
            busy.set(false);
            if let Some(el) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("exec-box"))
            {
                el.set_scroll_top(el.scroll_height());
            }
        });
    };

    rsx! {
        div { class: "flex flex-col gap-3",
            if !running {
                Notice { warning: true, "Befehle gehen nur, wenn die App läuft." }
            }
            div { id: "exec-box", class: "{CARD} h-[55vh] overflow-y-auto bg-muted/30 p-3 font-mono text-xs leading-relaxed",
                if history.read().is_empty() {
                    p { class: "text-muted-foreground", "Befehl unten eingeben, z. B. „ls -la /data“. Jeder Befehl läuft einzeln mit sh -c (max. 60 Sekunden)." }
                }
                for (i, h) in history.read().iter().enumerate() {
                    div { key: "{i}", class: "mb-3",
                        div { class: "text-muted-foreground", "$ {h.command}" }
                        match &h.result {
                            Ok(r) => rsx! {
                                div { class: "console-line", "{r.output}" }
                                if r.exit_code != 0 {
                                    div { class: "text-destructive", "Exit-Code {r.exit_code}" }
                                }
                            },
                            Err(e) => rsx! { div { class: "text-destructive", "{e}" } },
                        }
                    }
                }
            }
            form { class: "flex gap-2", onsubmit: run,
                input {
                    class: "{INPUT} font-mono",
                    value: "{command}",
                    placeholder: "Befehl …",
                    disabled: !running,
                    oninput: move |e| command.set(e.value()),
                }
                Button { button_type: "submit", disabled: busy() || !running,
                    if busy() {
                        icons::LoaderCircle { class: "size-4 animate-spin" }
                    } else {
                        icons::Play { class: "size-4" }
                    }
                    "Ausführen"
                }
            }
        }
    }
}
