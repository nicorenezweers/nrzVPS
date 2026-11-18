//! App → Quellcode: Dateiliste, Archiv hochladen (ersetzen oder ergänzen), Drag & Drop.

use dioxus::prelude::*;
use nrz_vps_common::{Source, SourceTree, routes};

use crate::{
    api,
    components::{
        CARD, CheckRow, Loading, Notice, Tag, drag_has_files, dropped_files, fmt_bytes,
        open_file_picker, take_input_files,
        ui::button::{Button, ButtonSize},
    },
    layout::{deploy, use_app},
    store::{toast, toast_err},
};

const INPUT_ID: &str = "source-files";

#[component]
pub fn AppSource(id: String) -> Element {
    let ctx = use_app();
    let mut tree = use_signal(|| None::<SourceTree>);
    let mut replace = use_signal(|| true);
    let mut dragging = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let reload = use_signal(|| 0u32);

    use_effect(move || {
        let _ = reload();
        let id = ctx.id();
        spawn(async move {
            match api::get::<SourceTree>(&routes::app(&id, "source")).await {
                Ok(t) => tree.set(Some(t)),
                Err(e) => {
                    tree.set(Some(SourceTree::default()));
                    toast_err(e);
                }
            }
        });
    });

    let mut upload = move |files: Vec<web_sys::File>| {
        if files.is_empty() {
            return;
        }
        busy.set(true);
        let id = ctx.id();
        let path = format!("{}?replace={}", routes::app(&id, "source"), replace());
        spawn(async move {
            match api::upload::<SourceTree>(&path, &files).await {
                Ok(t) => {
                    tree.set(Some(t));
                    toast("Code hochgeladen – jetzt „Deployen“, damit er live geht");
                    ctx.reload();
                }
                Err(e) => toast_err(e),
            }
            busy.set(false);
        });
    };

    let source = ctx
        .app
        .read()
        .as_ref()
        .map(|a| a.source.clone())
        .unwrap_or_default();
    let t = tree.read().clone();

    rsx! {
        div { class: "flex flex-col gap-4",
            match &source {
                Source::Git { .. } => rsx! {
                    Notice { "Diese App holt ihren Code aus Git. Jedes Deployment lädt den neuesten Stand – hochgeladene Dateien werden dabei ersetzt." }
                },
                Source::Image { .. } => rsx! {
                    Notice { "Diese App läuft aus einem fertigen Image und braucht keinen Quellcode." }
                },
                Source::Upload => rsx! {},
            }
            if matches!(source, Source::Upload) {
                input {
                    id: INPUT_ID,
                    r#type: "file",
                    multiple: true,
                    class: "hidden",
                    onchange: move |_| upload(take_input_files(INPUT_ID)),
                }
                div {
                    class: if dragging() { "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-ring bg-accent/40 p-8 text-center" } else { "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-8 text-center hover:bg-accent/30" },
                    onclick: move |_| if !busy() { open_file_picker(INPUT_ID) },
                    ondragover: move |e| {
                        if drag_has_files(&e) {
                            e.prevent_default();
                            dragging.set(true);
                        }
                    },
                    ondragleave: move |_| dragging.set(false),
                    ondrop: move |e| {
                        e.prevent_default();
                        dragging.set(false);
                        upload(dropped_files(&e));
                    },
                    if busy() {
                        icons::LoaderCircle { class: "size-6 animate-spin text-muted-foreground" }
                        p { class: "text-sm", "Wird hochgeladen …" }
                    } else {
                        icons::Upload { class: "size-6 text-muted-foreground" }
                        p { class: "text-sm font-medium", ".zip / .tar.gz oder einzelne Dateien hierher ziehen" }
                        p { class: "text-xs text-muted-foreground", "Ordner wie node_modules, target oder .git lässt du am besten weg." }
                    }
                }
                div { class: "flex flex-wrap items-center justify-between gap-3",
                    CheckRow {
                        checked: replace(),
                        onchange: move |v| replace.set(v),
                        label: "Alten Code vorher komplett ersetzen",
                        hint: "Aus: Dateien werden ergänzt bzw. überschrieben.",
                    }
                    Button { size: ButtonSize::Sm, onclick: move |_| { spawn(async move { deploy(ctx).await }); },
                        icons::Rocket { class: "size-4" }
                        "Jetzt deployen"
                    }
                }
            }
            section { class: "{CARD}",
                div { class: "flex flex-wrap items-center justify-between gap-2 border-b px-4 py-3",
                    h2 { class: "font-semibold", "Dateien" }
                    if let Some(t) = &t {
                        div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                            if let Some(r) = t.detected {
                                Tag { tone: "info", "Erkannt: {r.label()}" }
                            }
                            span { "{t.files.len()} Dateien · {fmt_bytes(t.total_size)}" }
                        }
                    }
                }
                match t {
                    None => rsx! { Loading {} },
                    Some(t) if t.files.is_empty() => rsx! {
                        p { class: "px-4 py-8 text-center text-sm text-muted-foreground", "Noch kein Code." }
                    },
                    Some(t) => rsx! {
                        div { class: "max-h-[50vh] divide-y overflow-y-auto",
                            for f in t.files.iter().take(2000) {
                                div { key: "{f.path}", class: "flex items-center justify-between gap-3 px-4 py-1.5 text-sm",
                                    span { class: "flex min-w-0 items-center gap-2",
                                        icons::FileCode { class: "size-3.5 shrink-0 text-muted-foreground" }
                                        span { class: "truncate font-mono text-xs", "{f.path}" }
                                    }
                                    span { class: "shrink-0 text-xs tabular-nums text-muted-foreground", "{fmt_bytes(f.size)}" }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
