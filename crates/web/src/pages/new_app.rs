//! Neue App: Quellcode, Git oder Image – danach automatisch deployen.

use dioxus::prelude::*;
use nrz_vps_common::{App, CreateApp, Runtime, Source, SourceTree, routes};

use crate::{
    Route, api,
    components::{
        CARD, CheckRow, ErrorBox, Field, INPUT, SELECT, Segmented, drag_has_files, dropped_files,
        fmt_bytes, fmt_cpus, fmt_mb, open_file_picker, parse_f64, take_input_files,
        ui::{
            app_shell::*,
            button::{Button, ButtonVariant},
        },
    },
    pages::app_settings::{EnvEditor, env_from_rows},
    store::{OVERVIEW, refresh, toast},
};

const FILE_INPUT: &str = "new-app-files";

#[component]
pub fn NewApp() -> Element {
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| "upload".to_string());
    let mut git_url = use_signal(String::new);
    let mut git_branch = use_signal(String::new);
    let mut git_subdir = use_signal(String::new);
    let mut git_token = use_signal(String::new);
    let mut image = use_signal(String::new);
    let mut img_user = use_signal(String::new);
    let mut img_pass = use_signal(String::new);
    let mut files = use_signal(Vec::<web_sys::File>::new);
    let mut dragging = use_signal(|| false);
    let mut advanced = use_signal(|| false);
    let mut runtime = use_signal(|| Runtime::Auto);
    let mut build_cmd = use_signal(String::new);
    let mut start_cmd = use_signal(String::new);
    let mut container_port = use_signal(String::new);
    let mut no_port = use_signal(|| false);
    let mut cpus = use_signal(String::new);
    let mut memory = use_signal(String::new);
    let env_rows = use_signal(Vec::<(String, String)>::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| None::<String>);

    let quota = OVERVIEW.read().as_ref().map(|o| (o.quota, o.usage));

    let submit = move |e: FormEvent| {
        e.prevent_default();
        error.set(None);
        let n = name().trim().to_lowercase();
        if n.is_empty() {
            error.set(Some("Gib der App einen Namen".into()));
            return;
        }
        let source = match kind().as_str() {
            "git" => {
                if git_url().trim().is_empty() {
                    error.set(Some("Git-URL fehlt".into()));
                    return;
                }
                Source::Git {
                    url: git_url().trim().into(),
                    branch: git_branch().trim().into(),
                    subdir: git_subdir().trim().into(),
                    token: Some(git_token().trim().to_string()).filter(|s| !s.is_empty()),
                }
            }
            "image" => {
                if image().trim().is_empty() {
                    error.set(Some("Image fehlt, z. B. nginx:alpine".into()));
                    return;
                }
                Source::Image {
                    image: image().trim().into(),
                    username: Some(img_user().trim().to_string()).filter(|s| !s.is_empty()),
                    password: Some(img_pass()).filter(|s| !s.is_empty()),
                }
            }
            _ => Source::Upload,
        };
        let is_upload = matches!(source, Source::Upload);
        let body = CreateApp {
            name: n,
            source,
            runtime: runtime(),
            build_command: build_cmd().trim().into(),
            start_command: start_cmd().trim().into(),
            env: env_from_rows(&env_rows.read()),
            container_port: container_port().trim().parse().ok(),
            no_port: no_port(),
            cpus: parse_f64(&cpus()),
            memory_mb: memory().trim().parse().ok(),
            auto_deploy: kind() == "git",
        };
        let upload = files.read().clone();
        spawn(async move {
            busy.set(Some("App wird angelegt …".into()));
            let app = match api::post::<_, App>("/api/apps", &body).await {
                Ok(a) => a,
                Err(e) => {
                    error.set(Some(e));
                    busy.set(None);
                    return;
                }
            };
            let mut can_deploy = !is_upload;
            if is_upload && !upload.is_empty() {
                busy.set(Some("Code wird hochgeladen …".into()));
                match api::upload::<SourceTree>(
                    &format!("{}?replace=true", routes::app(&app.id, "source")),
                    &upload,
                )
                .await
                {
                    Ok(_) => can_deploy = true,
                    Err(e) => crate::store::toast_err(format!("Hochladen fehlgeschlagen: {e}")),
                }
            }
            if can_deploy {
                busy.set(Some("Deployment wird gestartet …".into()));
                match api::post_empty::<App>(&routes::app(&app.id, "deploy")).await {
                    Ok(_) => toast("App angelegt – wird jetzt gebaut und gestartet"),
                    Err(e) => crate::store::toast_err(e),
                }
            } else {
                toast("App angelegt – lade jetzt den Code hoch");
            }
            refresh().await;
            busy.set(None);
            let id = app.id.clone();
            if can_deploy {
                navigator().push(Route::AppLogs { id });
            } else {
                navigator().push(Route::AppSource { id });
            }
        });
    };

    let file_count = files.read().len();
    let file_size: f64 = files.read().iter().map(|f| f.size()).sum();

    rsx! {
        document::Title { "Neue App · nrzVPS" }
        PageContainer { width: PageWidth::Narrow,
            PageHeader {
                title: "Neue App",
                description: "Kein Docker-Wissen nötig: Code hochladen, nrzVPS erkennt die Sprache, baut und startet die App.",
            }
            form { class: "flex flex-col gap-6", onsubmit: submit,
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                section { class: "{CARD} flex flex-col gap-4 p-5",
                    Field { label: "Name", hint: "Klein, ohne Leerzeichen, z. B. „meine-seite“. Andere Apps von dir erreichen sie unter diesem Namen.",
                        input {
                            class: INPUT,
                            value: "{name}",
                            placeholder: "meine-app",
                            autofocus: true,
                            oninput: move |e| name.set(e.value().to_lowercase().replace(' ', "-")),
                        }
                    }
                    div { class: "flex flex-col gap-1.5",
                        span { class: "text-sm font-medium", "Woher kommt der Code?" }
                        Segmented {
                            items: vec![
                                ("upload".to_string(), "Quellcode hochladen".to_string()),
                                ("git".to_string(), "Git-Repository".to_string()),
                                ("image".to_string(), "Fertiges Image".to_string()),
                            ],
                            active: kind(),
                            onchange: move |v| kind.set(v),
                        }
                    }
                    match kind().as_str() {
                        "git" => rsx! {
                            Field { label: "Repository-URL", hint: "Funktioniert mit nrzGit, GitHub, Gitea … – z. B. http://100.78.107.162:8390/du/projekt.git",
                                input { class: INPUT, value: "{git_url}", placeholder: "https://…/projekt.git", oninput: move |e| git_url.set(e.value()) }
                            }
                            div { class: "grid gap-4 sm:grid-cols-2",
                                Field { label: "Branch", hint: "Leer = Standard-Branch",
                                    input { class: INPUT, value: "{git_branch}", placeholder: "main", oninput: move |e| git_branch.set(e.value()) }
                                }
                                Field { label: "Unterordner", hint: "Leer = ganzes Repository",
                                    input { class: INPUT, value: "{git_subdir}", placeholder: "", oninput: move |e| git_subdir.set(e.value()) }
                                }
                            }
                            Field { label: "Zugriffstoken (nur für private Repositories)", hint: "In nrzGit: Einstellungen → Zugriffstokens (nur Lesen genügt).",
                                input { class: INPUT, r#type: "password", value: "{git_token}", autocomplete: "off", oninput: move |e| git_token.set(e.value()) }
                            }
                        },
                        "image" => rsx! {
                            Field { label: "Image", hint: "z. B. nginx:alpine oder ein Image aus der nrzGit-Registry. Bei Updates wird das alte Image sofort gelöscht.",
                                input { class: INPUT, value: "{image}", placeholder: "nginx:alpine", oninput: move |e| image.set(e.value()) }
                            }
                            div { class: "grid gap-4 sm:grid-cols-2",
                                Field { label: "Benutzer (optional)",
                                    input { class: INPUT, value: "{img_user}", autocomplete: "off", oninput: move |e| img_user.set(e.value()) }
                                }
                                Field { label: "Passwort / Token (optional)",
                                    input { class: INPUT, r#type: "password", value: "{img_pass}", autocomplete: "off", oninput: move |e| img_pass.set(e.value()) }
                                }
                            }
                            Field { label: "Port im Container", hint: "Auf welchem Port lauscht das Image? (z. B. 80 bei nginx)",
                                input { class: INPUT, r#type: "number", value: "{container_port}", placeholder: "80", oninput: move |e| container_port.set(e.value()) }
                            }
                        },
                        _ => rsx! {
                            input {
                                id: FILE_INPUT,
                                r#type: "file",
                                multiple: true,
                                class: "hidden",
                                onchange: move |_| {
                                    let f = take_input_files(FILE_INPUT);
                                    if !f.is_empty() {
                                        files.set(f);
                                    }
                                },
                            }
                            div {
                                class: if dragging() { "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-ring bg-accent/40 p-8 text-center" } else { "flex cursor-pointer flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed p-8 text-center hover:bg-accent/30" },
                                onclick: move |_| open_file_picker(FILE_INPUT),
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
                                    let f = dropped_files(&e);
                                    if !f.is_empty() {
                                        files.set(f);
                                    }
                                },
                                icons::Upload { class: "size-6 text-muted-foreground" }
                                if file_count == 0 {
                                    p { class: "text-sm font-medium", "Projekt als .zip oder .tar.gz hierher ziehen" }
                                    p { class: "text-xs text-muted-foreground", "oder klicken und Dateien wählen. Du kannst den Code auch später hochladen." }
                                } else {
                                    p { class: "text-sm font-medium", "{file_count} Datei(en) gewählt" }
                                    p { class: "text-xs text-muted-foreground", "{fmt_bytes(file_size as u64)} – klicken zum Ändern" }
                                }
                            }
                        },
                    }
                }

                section { class: "{CARD} flex flex-col gap-4 p-5",
                    button {
                        r#type: "button",
                        class: "flex items-center justify-between text-left",
                        onclick: move |_| advanced.toggle(),
                        div {
                            p { class: "font-semibold", "Erweitert" }
                            p { class: "text-xs text-muted-foreground", "Optional – nrzVPS wählt sonst sinnvolle Werte." }
                        }
                        if advanced() {
                            icons::ChevronUp { class: "size-4 text-muted-foreground" }
                        } else {
                            icons::ChevronDown { class: "size-4 text-muted-foreground" }
                        }
                    }
                    if advanced() {
                        if kind() != "image" {
                            Field { label: "Laufzeit",
                                select {
                                    class: SELECT,
                                    onchange: move |e| runtime.set(Runtime::parse(&e.value()).unwrap_or_default()),
                                    for r in Runtime::ALL {
                                        option { key: "{r.as_str()}", value: r.as_str(), selected: r == runtime(), "{r.label()}" }
                                    }
                                }
                            }
                            div { class: "grid gap-4 sm:grid-cols-2",
                                Field { label: "Build-Befehl", hint: "Leer = automatisch",
                                    input { class: INPUT, value: "{build_cmd}", placeholder: "npm run build", oninput: move |e| build_cmd.set(e.value()) }
                                }
                                Field { label: "Start-Befehl", hint: "Leer = automatisch",
                                    input { class: INPUT, value: "{start_cmd}", placeholder: "npm start", oninput: move |e| start_cmd.set(e.value()) }
                                }
                            }
                            Field { label: "Port im Container", hint: "Leer = gleicher Port wie aussen; deine App liest ihn aus der Variable PORT.",
                                input { class: INPUT, r#type: "number", value: "{container_port}", oninput: move |e| container_port.set(e.value()) }
                            }
                        }
                        CheckRow {
                            checked: no_port(),
                            onchange: move |v| no_port.set(v),
                            label: "Kein Port (Hintergrund-Dienst, Bot …)",
                        }
                        div { class: "grid gap-4 sm:grid-cols-2",
                            Field {
                                label: "CPU (Kerne)",
                                hint: quota.map(|(q, u)| format!("Frei: {} von {}", fmt_cpus((q.cpus - u.cpus).max(0.0)), fmt_cpus(q.cpus))).unwrap_or_default(),
                                input { class: INPUT, value: "{cpus}", placeholder: "z. B. 0.25", oninput: move |e| cpus.set(e.value()) }
                            }
                            Field {
                                label: "Arbeitsspeicher (MB)",
                                hint: quota.map(|(q, u)| format!("Frei: {} von {}", fmt_mb(q.memory_mb.saturating_sub(u.memory_mb)), fmt_mb(q.memory_mb))).unwrap_or_default(),
                                input { class: INPUT, r#type: "number", value: "{memory}", placeholder: "z. B. 128", oninput: move |e| memory.set(e.value()) }
                            }
                        }
                        div { class: "flex flex-col gap-1.5",
                            span { class: "text-sm font-medium", "Umgebungsvariablen" }
                            EnvEditor { rows: env_rows }
                        }
                    }
                }

                div { class: "flex flex-wrap items-center justify-end gap-2",
                    if let Some(b) = busy() {
                        span { class: "flex items-center gap-2 text-sm text-muted-foreground",
                            icons::LoaderCircle { class: "size-4 animate-spin" }
                            "{b}"
                        }
                    }
                    Button { variant: ButtonVariant::Outline, href: "/", "Abbrechen" }
                    Button { button_type: "submit", disabled: busy().is_some(),
                        icons::Rocket { class: "size-4" }
                        "App anlegen"
                    }
                }
            }
        }
    }
}
