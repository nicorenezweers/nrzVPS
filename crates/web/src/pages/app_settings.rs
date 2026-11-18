//! App → Einstellungen: Laufzeit, Befehle, Quelle, Ressourcen, Umgebungsvariablen, Ports, Webhook.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use nrz_vps_common::{AddPort, App, Protocol, Runtime, SECRET_MASK, Source, UpdateApp, routes};

use crate::{
    api,
    components::{
        CARD, Field, ICON_BUTTON, INPUT, SELECT, Toggle, fmt_cpus, fmt_mb, parse_f64,
        ui::{
            app_shell::{SettingsGroup, SettingsRow},
            button::{Button, ButtonSize, ButtonVariant},
        },
    },
    layout::use_app,
    store::{OVERVIEW, toast, toast_err},
};

/// Zeilen → Map (leere Schlüssel weg).
pub fn env_from_rows(rows: &[(String, String)]) -> BTreeMap<String, String> {
    rows.iter()
        .filter(|(k, _)| !k.trim().is_empty())
        .map(|(k, v)| (k.trim().to_string(), v.clone()))
        .collect()
}

/// Editor für Umgebungsvariablen (Schlüssel/Wert-Zeilen).
#[component]
pub fn EnvEditor(rows: Signal<Vec<(String, String)>>) -> Element {
    let mut rows = rows;
    let list = rows.read().clone();
    rsx! {
        div { class: "flex flex-col gap-2",
            for (i, (k, v)) in list.into_iter().enumerate() {
                div { key: "{i}", class: "flex gap-2",
                    input {
                        class: "{INPUT} font-mono",
                        value: "{k}",
                        placeholder: "NAME",
                        oninput: move |e| {
                            if let Some(r) = rows.write().get_mut(i) {
                                r.0 = e.value().to_uppercase().replace(' ', "_");
                            }
                        },
                    }
                    input {
                        class: "{INPUT} font-mono",
                        value: "{v}",
                        placeholder: "Wert",
                        oninput: move |e| {
                            if let Some(r) = rows.write().get_mut(i) {
                                r.1 = e.value();
                            }
                        },
                    }
                    button {
                        r#type: "button",
                        class: ICON_BUTTON,
                        title: "Entfernen",
                        onclick: move |_| {
                            rows.write().remove(i);
                        },
                        icons::X {}
                    }
                }
            }
            div {
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| rows.write().push((String::new(), String::new())),
                    icons::Plus { class: "size-4" }
                    "Variable"
                }
            }
            p { class: "text-xs text-muted-foreground", "PORT und DATA_DIR setzt nrzVPS selbst." }
        }
    }
}

#[component]
pub fn AppSettings(id: String) -> Element {
    let ctx = use_app();
    let Some(a) = ctx.app.read().clone() else {
        return rsx! {};
    };
    rsx! {
        SettingsForm { key: "{a.id}", app: a }
    }
}

#[component]
fn SettingsForm(app: App) -> Element {
    let ctx = use_app();
    let a = app;
    // Formularwerte einmalig aus der App übernehmen (Polling überschreibt sie nicht)
    let mut runtime = use_signal(|| a.runtime);
    let mut build_cmd = use_signal(|| a.build_command.clone());
    let mut start_cmd = use_signal(|| a.start_command.clone());
    let mut cpus = use_signal(|| a.cpus.to_string());
    let mut memory = use_signal(|| a.memory_mb.to_string());
    let env_rows = use_signal(|| {
        a.env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>()
    });
    let (g_url, g_branch, g_subdir, g_token) = match &a.source {
        Source::Git {
            url,
            branch,
            subdir,
            token,
        } => (
            url.clone(),
            branch.clone(),
            subdir.clone(),
            token.clone().unwrap_or_default(),
        ),
        _ => Default::default(),
    };
    let (i_image, i_user, i_pass) = match &a.source {
        Source::Image {
            image,
            username,
            password,
        } => (
            image.clone(),
            username.clone().unwrap_or_default(),
            password.clone().unwrap_or_default(),
        ),
        _ => Default::default(),
    };
    let mut git_url = use_signal(|| g_url);
    let mut git_branch = use_signal(|| g_branch);
    let mut git_subdir = use_signal(|| g_subdir);
    let mut git_token = use_signal(|| g_token);
    let mut image = use_signal(|| i_image);
    let mut img_user = use_signal(|| i_user);
    let mut img_pass = use_signal(|| i_pass);
    let mut saving = use_signal(|| false);
    let mut new_port = use_signal(String::new);
    let mut new_host = use_signal(String::new);
    let mut new_proto = use_signal(|| Protocol::Tcp);

    // Aktuelle Werte (für Schalter und Ports) direkt aus dem Kontext
    let live = ctx.app.read().clone().unwrap_or_else(|| a.clone());
    let kind = a.source.kind();
    let quota = OVERVIEW.read().as_ref().map(|o| (o.quota, o.usage));

    let save = move |e: FormEvent| {
        e.prevent_default();
        let source = match kind {
            "git" => Some(Source::Git {
                url: git_url().trim().into(),
                branch: git_branch().trim().into(),
                subdir: git_subdir().trim().into(),
                token: Some(git_token()).filter(|s| !s.is_empty()),
            }),
            "image" => Some(Source::Image {
                image: image().trim().into(),
                username: Some(img_user().trim().to_string()).filter(|s| !s.is_empty()),
                password: Some(img_pass()).filter(|s| !s.is_empty()),
            }),
            _ => None,
        };
        let Some(c) = parse_f64(&cpus()) else {
            toast_err("CPU: bitte eine Zahl, z. B. 0.25");
            return;
        };
        let Ok(m) = memory().trim().parse::<u64>() else {
            toast_err("Arbeitsspeicher: bitte eine ganze Zahl in MB");
            return;
        };
        let body = UpdateApp {
            source,
            runtime: Some(runtime()),
            build_command: Some(build_cmd().trim().into()),
            start_command: Some(start_cmd().trim().into()),
            env: Some(env_from_rows(&env_rows.read())),
            cpus: Some(c),
            memory_mb: Some(m),
            ..Default::default()
        };
        saving.set(true);
        let id = ctx.id();
        spawn(async move {
            match api::patch::<_, App>(&routes::app(&id, ""), &body).await {
                Ok(a) => {
                    ctx.set(a);
                    toast(
                        "Gespeichert – Änderungen an Code oder Befehlen wirken beim nächsten Deployment",
                    );
                }
                Err(e) => toast_err(e),
            }
            saving.set(false);
        });
    };

    let toggle = move |body: UpdateApp| {
        let id = ctx.id();
        spawn(async move {
            match api::patch::<_, App>(&routes::app(&id, ""), &body).await {
                Ok(a) => ctx.set(a),
                Err(e) => toast_err(e),
            }
        });
    };

    let add_port = move |_| {
        let body = AddPort {
            container: new_port().trim().parse().ok(),
            host: new_host().trim().parse().ok(),
            protocol: new_proto(),
        };
        let id = ctx.id();
        spawn(async move {
            match api::post::<_, App>(&routes::app(&id, "ports"), &body).await {
                Ok(a) => {
                    ctx.set(a);
                    new_port.set(String::new());
                    new_host.set(String::new());
                    toast("Port hinzugefügt");
                }
                Err(e) => toast_err(e),
            }
        });
    };

    rsx! {
        div { class: "flex max-w-3xl flex-col gap-6",
            form { class: "flex flex-col gap-6", onsubmit: save,
                if kind != "image" {
                    section { class: "{CARD} flex flex-col gap-4 p-5",
                        h2 { class: "font-semibold", "Bauen & Starten" }
                        Field { label: "Laufzeit", hint: "„Automatisch“ erkennt Node, Python, Rust, Go, PHP, Deno, Bun, statische Seiten oder ein eigenes Dockerfile.",
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
                                input { class: "{INPUT} font-mono", value: "{build_cmd}", oninput: move |e| build_cmd.set(e.value()) }
                            }
                            Field { label: "Start-Befehl", hint: "Leer = automatisch",
                                input { class: "{INPUT} font-mono", value: "{start_cmd}", oninput: move |e| start_cmd.set(e.value()) }
                            }
                        }
                    }
                }
                if kind == "git" {
                    section { class: "{CARD} flex flex-col gap-4 p-5",
                        h2 { class: "font-semibold", "Git-Repository" }
                        Field { label: "URL",
                            input { class: INPUT, value: "{git_url}", oninput: move |e| git_url.set(e.value()) }
                        }
                        div { class: "grid gap-4 sm:grid-cols-2",
                            Field { label: "Branch", hint: "Leer = Standard",
                                input { class: INPUT, value: "{git_branch}", oninput: move |e| git_branch.set(e.value()) }
                            }
                            Field { label: "Unterordner",
                                input { class: INPUT, value: "{git_subdir}", oninput: move |e| git_subdir.set(e.value()) }
                            }
                        }
                        Field { label: "Zugriffstoken", hint: if git_token() == SECRET_MASK { "Gespeichert. Leeren, um es zu entfernen." } else { "Nur für private Repositories." },
                            input { class: INPUT, r#type: "password", autocomplete: "off", value: "{git_token}", oninput: move |e| git_token.set(e.value()) }
                        }
                    }
                }
                if kind == "image" {
                    section { class: "{CARD} flex flex-col gap-4 p-5",
                        h2 { class: "font-semibold", "Image" }
                        Field { label: "Image", hint: "Neuer Tag + „Deployen“ = Update; das alte Image wird sofort gelöscht.",
                            input { class: "{INPUT} font-mono", value: "{image}", oninput: move |e| image.set(e.value()) }
                        }
                        div { class: "grid gap-4 sm:grid-cols-2",
                            Field { label: "Benutzer",
                                input { class: INPUT, autocomplete: "off", value: "{img_user}", oninput: move |e| img_user.set(e.value()) }
                            }
                            Field { label: "Passwort / Token",
                                input { class: INPUT, r#type: "password", autocomplete: "off", value: "{img_pass}", oninput: move |e| img_pass.set(e.value()) }
                            }
                        }
                    }
                }
                section { class: "{CARD} flex flex-col gap-4 p-5",
                    h2 { class: "font-semibold", "Ressourcen" }
                    p { class: "text-sm text-muted-foreground", "Harte Grenzen: mehr bekommt die App nie. Zählen gegen dein Kontingent, solange sie läuft." }
                    div { class: "grid gap-4 sm:grid-cols-2",
                        Field {
                            label: "CPU (Kerne)",
                            hint: quota.map(|(q, _)| format!("Dein Kontingent: {}", fmt_cpus(q.cpus))).unwrap_or_default(),
                            input { class: INPUT, value: "{cpus}", oninput: move |e| cpus.set(e.value()) }
                        }
                        Field {
                            label: "Arbeitsspeicher (MB)",
                            hint: quota.map(|(q, _)| format!("Dein Kontingent: {}", fmt_mb(q.memory_mb))).unwrap_or_default(),
                            input { class: INPUT, r#type: "number", value: "{memory}", oninput: move |e| memory.set(e.value()) }
                        }
                    }
                }
                section { class: "{CARD} flex flex-col gap-3 p-5",
                    h2 { class: "font-semibold", "Umgebungsvariablen" }
                    p { class: "text-sm text-muted-foreground", "Für Passwörter, API-Schlüssel und Einstellungen. Wirken nach dem Speichern beim nächsten Start." }
                    EnvEditor { rows: env_rows }
                }
                div { class: "flex justify-end",
                    Button { button_type: "submit", disabled: saving(),
                        icons::Save { class: "size-4" }
                        "Speichern"
                    }
                }
            }

            SettingsGroup { title: "Verhalten",
                SettingsRow { label: "Automatisch starten", hint: "Startet die App wieder, wenn nrzVPS oder der Server neu startet.",
                    Toggle {
                        checked: live.auto_start,
                        label: "Automatisch starten",
                        onchange: move |v| toggle(UpdateApp { auto_start: Some(v), ..Default::default() }),
                    }
                }
                if kind == "git" {
                    SettingsRow { label: "Bei Push automatisch deployen", hint: "Über den Webhook (siehe Übersicht).",
                        Toggle {
                            checked: live.auto_deploy,
                            label: "Automatisch deployen",
                            onchange: move |v| toggle(UpdateApp { auto_deploy: Some(v), ..Default::default() }),
                        }
                    }
                }
            }

            section { class: "{CARD} flex flex-col gap-4 p-5",
                h2 { class: "font-semibold", "Ports" }
                p { class: "text-sm text-muted-foreground", "Aussen-Ports kommen aus deinem Block. Leerer Container-Port = gleicher Port wie aussen." }
                if live.ports.is_empty() {
                    p { class: "text-sm text-muted-foreground", "Keine Ports." }
                }
                div { class: "flex flex-col divide-y rounded-lg border",
                    for p in live.ports.iter().copied() {
                        div { key: "{p.host}-{p.protocol.as_str()}", class: "flex items-center justify-between gap-3 px-3 py-2 text-sm",
                            span { class: "font-mono", "{p.host} → {p.container}/{p.protocol.as_str()}" }
                            button {
                                class: ICON_BUTTON,
                                title: "Port entfernen",
                                onclick: move |_| {
                                    let id = ctx.id();
                                    spawn(async move {
                                        match api::delete::<App>(&routes::app(&id, &format!("ports/{}", p.host))).await {
                                            Ok(a) => {
                                                ctx.set(a);
                                                toast("Port entfernt");
                                            }
                                            Err(e) => toast_err(e),
                                        }
                                    });
                                },
                                icons::Trash2 {}
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap items-end gap-2",
                    Field { label: "Container-Port", class: "w-36",
                        input { class: INPUT, r#type: "number", value: "{new_port}", placeholder: "gleich", oninput: move |e| new_port.set(e.value()) }
                    }
                    Field { label: "Aussen-Port", class: "w-36",
                        input { class: INPUT, r#type: "number", value: "{new_host}", placeholder: "nächster freier", oninput: move |e| new_host.set(e.value()) }
                    }
                    Field { label: "Protokoll", class: "w-28",
                        select {
                            class: SELECT,
                            onchange: move |e| new_proto.set(if e.value() == "udp" { Protocol::Udp } else { Protocol::Tcp }),
                            option { value: "tcp", selected: new_proto() == Protocol::Tcp, "TCP" }
                            option { value: "udp", selected: new_proto() == Protocol::Udp, "UDP" }
                        }
                    }
                    Button { variant: ButtonVariant::Outline, onclick: add_port,
                        icons::Plus { class: "size-4" }
                        "Port hinzufügen"
                    }
                }
            }

            if kind == "git" {
                SettingsGroup { title: "Webhook",
                    SettingsRow { label: "Neue Webhook-URL erzeugen", hint: "Die alte URL funktioniert danach nicht mehr.",
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            onclick: move |_| {
                                let id = ctx.id();
                                spawn(async move {
                                    match api::post_empty::<App>(&routes::app(&id, "webhook-secret")).await {
                                        Ok(a) => {
                                            ctx.set(a);
                                            toast("Neue Webhook-URL erzeugt – in nrzGit eintragen");
                                        }
                                        Err(e) => toast_err(e),
                                    }
                                });
                            },
                            icons::RefreshCw { class: "size-4" }
                            "Neu erzeugen"
                        }
                    }
                }
            }
        }
    }
}
