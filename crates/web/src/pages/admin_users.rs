//! Admin → Benutzer: max. 10 Konten, je 10 Ports und ein hartes Kontingent.

use dioxus::prelude::*;
use nrz_vps_common::{AdminUser, CreateUser, MAX_USERS, Quota, Settings, UpdateUser, routes};

use crate::{
    api,
    components::{
        CheckRow, ConfirmDialog, ErrorBox, Field, INPUT, Loading, Modal, Notice, Tag, fmt_cpus,
        fmt_datetime, fmt_mb, parse_f64,
        ui::{
            app_shell::*,
            button::{Button, ButtonSize, ButtonVariant},
            table::*,
        },
    },
    store::{me, toast, toast_err},
};

/// Kontingent als Textfelder.
#[derive(Clone, Copy, PartialEq)]
struct QuotaForm {
    cpus: Signal<String>,
    memory: Signal<String>,
    disk: Signal<String>,
    apps: Signal<String>,
}

impl QuotaForm {
    fn new(q: Quota) -> Self {
        Self {
            cpus: Signal::new(q.cpus.to_string()),
            memory: Signal::new(q.memory_mb.to_string()),
            disk: Signal::new(q.disk_mb.to_string()),
            apps: Signal::new(q.max_apps.to_string()),
        }
    }
    fn get(&self) -> Result<Quota, String> {
        Ok(Quota {
            cpus: parse_f64(&self.cpus.read()).ok_or("CPU: Zahl eingeben, z. B. 0.25")?,
            memory_mb: self
                .memory
                .read()
                .trim()
                .parse()
                .map_err(|_| "Arbeitsspeicher: ganze Zahl in MB")?,
            disk_mb: self
                .disk
                .read()
                .trim()
                .parse()
                .map_err(|_| "Speicherplatz: ganze Zahl in MB")?,
            max_apps: self
                .apps
                .read()
                .trim()
                .parse()
                .map_err(|_| "Apps: ganze Zahl")?,
        })
    }
}

#[component]
fn QuotaFields(form: QuotaForm) -> Element {
    let QuotaForm {
        mut cpus,
        mut memory,
        mut disk,
        mut apps,
    } = form;
    rsx! {
        div { class: "grid gap-4 sm:grid-cols-2",
            Field { label: "CPU (Kerne)", hint: "z. B. 0.25 = ein Viertel Kern",
                input { class: INPUT, value: "{cpus}", oninput: move |e| cpus.set(e.value()) }
            }
            Field { label: "Arbeitsspeicher (MB)",
                input { class: INPUT, r#type: "number", value: "{memory}", oninput: move |e| memory.set(e.value()) }
            }
            Field { label: "Speicherplatz (MB)", hint: "Code + Images + Daten",
                input { class: INPUT, r#type: "number", value: "{disk}", oninput: move |e| disk.set(e.value()) }
            }
            Field { label: "Max. Apps",
                input { class: INPUT, r#type: "number", value: "{apps}", oninput: move |e| apps.set(e.value()) }
            }
        }
    }
}

#[component]
pub fn AdminUsers() -> Element {
    let mut users = use_signal(|| None::<Vec<AdminUser>>);
    let mut settings = use_signal(|| None::<Settings>);
    let mut reload = use_signal(|| 0u32);
    let mut creating = use_signal(|| false);
    let mut editing = use_signal(|| None::<AdminUser>);
    let mut deleting = use_signal(|| None::<AdminUser>);
    let my_id = me().map(|m| m.id).unwrap_or_default();

    use_effect(move || {
        let _ = reload();
        spawn(async move {
            match api::get::<Vec<AdminUser>>("/api/admin/users").await {
                Ok(u) => users.set(Some(u)),
                Err(e) => toast_err(e),
            }
            if let Ok(s) = api::get::<Settings>("/api/admin/settings").await {
                settings.set(Some(s));
            }
        });
    });

    let list = users();
    let full = list.as_ref().is_some_and(|l| l.len() >= MAX_USERS);
    let s = settings();
    let sum_cpu: f64 = list.iter().flatten().map(|u| u.quota.cpus).sum();
    let sum_mem: u64 = list.iter().flatten().map(|u| u.quota.memory_mb).sum();

    rsx! {
        document::Title { "Benutzer · nrzVPS" }
        PageContainer { width: PageWidth::Wide,
            PageHeader {
                eyebrow: "Admin",
                title: "Benutzer",
                description: "Höchstens {MAX_USERS} Konten. Jedes bekommt einen festen Block aus 10 Ports (34000–34099) und harte Grenzen.",
                actions: rsx! {
                    Button { disabled: full, onclick: move |_| creating.set(true),
                        icons::UserPlus { class: "size-4" }
                        "Benutzer anlegen"
                    }
                },
            }
            if let Some(s) = &s {
                div { class: "mb-4",
                    Notice { warning: sum_cpu + s.build_cpus > s.total_cpus + 1e-9 || sum_mem + s.build_memory_mb > s.total_memory_mb,
                        "Vergeben: {fmt_cpus(sum_cpu)} von {fmt_cpus(s.total_cpus)} CPU und {fmt_mb(sum_mem)} von {fmt_mb(s.total_memory_mb)} RAM "
                        "(plus Build-Reserve {fmt_cpus(s.build_cpus)} / {fmt_mb(s.build_memory_mb)})."
                    }
                }
            }
            if full {
                div { class: "mb-4", Notice { "Alle {MAX_USERS} Plätze sind belegt." } }
            }
            match list {
                None => rsx! { Loading {} },
                Some(list) => rsx! {
                    TableWrapper {
                        Table {
                            TableHeader {
                                TableRow {
                                    TableHead { "Benutzer" }
                                    TableHead { "Ports" }
                                    TableHead { "CPU" }
                                    TableHead { "RAM" }
                                    TableHead { "Speicher" }
                                    TableHead { "Apps" }
                                    TableHead { "Zuletzt" }
                                    TableHead { "" }
                                }
                            }
                            TableBody {
                                for u in list {
                                    TableRow { key: "{u.id}",
                                        TableCell {
                                            div { class: "flex items-center gap-2",
                                                span { class: "font-medium", "{u.username}" }
                                                if u.admin { Tag { "Admin" } }
                                                if u.disabled { Tag { tone: "destructive", "Gesperrt" } }
                                            }
                                        }
                                        TableCell { span { class: "font-mono text-xs", "#{u.slot} · {u.port_from}–{u.port_to}" } }
                                        TableCell { span { class: "tabular-nums text-xs", "{fmt_cpus(u.usage.cpus)} / {fmt_cpus(u.quota.cpus)}" } }
                                        TableCell { span { class: "tabular-nums text-xs", "{fmt_mb(u.usage.memory_mb)} / {fmt_mb(u.quota.memory_mb)}" } }
                                        TableCell { span { class: "tabular-nums text-xs", "{fmt_mb(u.usage.disk_mb)} / {fmt_mb(u.quota.disk_mb)}" } }
                                        TableCell { span { class: "tabular-nums text-xs", "{u.usage.apps} / {u.quota.max_apps}" } }
                                        TableCell { span { class: "text-xs text-muted-foreground", "{u.last_login.map(fmt_datetime).unwrap_or_else(|| \"nie\".into())}" } }
                                        TableCell {
                                            div { class: "flex justify-end gap-1",
                                                Button {
                                                    size: ButtonSize::Sm,
                                                    variant: ButtonVariant::Ghost,
                                                    title: "Bearbeiten",
                                                    onclick: {
                                                        let u = u.clone();
                                                        move |_| editing.set(Some(u.clone()))
                                                    },
                                                    icons::Pencil { class: "size-4" }
                                                }
                                                if u.id != my_id {
                                                    Button {
                                                        size: ButtonSize::Sm,
                                                        variant: ButtonVariant::Ghost,
                                                        title: "Löschen",
                                                        onclick: {
                                                            let u = u.clone();
                                                            move |_| deleting.set(Some(u.clone()))
                                                        },
                                                        icons::Trash2 { class: "size-4" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
        if creating() {
            CreateUserDialog {
                default_quota: s.as_ref().map(|s| s.default_quota).unwrap_or_default(),
                onclose: move |done: bool| {
                    creating.set(false);
                    if done {
                        reload += 1;
                    }
                },
            }
        }
        if let Some(u) = editing() {
            EditUserDialog {
                key: "{u.id}",
                user: u,
                is_me: editing().is_some_and(|u| u.id == my_id),
                onclose: move |done: bool| {
                    editing.set(None);
                    if done {
                        reload += 1;
                    }
                },
            }
        }
        if let Some(u) = deleting() {
            ConfirmDialog {
                title: "Benutzer löschen?",
                message: format!("„{}“ und ALLE Apps, Images und Daten dieses Kontos werden endgültig gelöscht. Die Ports {}–{} werden frei.", u.username, u.port_from, u.port_to),
                confirm_label: "Endgültig löschen",
                destructive: true,
                oncancel: move |_| deleting.set(None),
                onconfirm: move |_| {
                    let id = u.id.clone();
                    deleting.set(None);
                    spawn(async move {
                        match api::delete::<nrz_vps_common::Ok>(&routes::admin_user(&id)).await {
                            Ok(_) => {
                                toast("Benutzer gelöscht");
                                reload += 1;
                            }
                            Err(e) => toast_err(e),
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn CreateUserDialog(default_quota: Quota, onclose: EventHandler<bool>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut admin = use_signal(|| false);
    let quota = use_hook(|| QuotaForm::new(default_quota));
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut created = use_signal(|| None::<AdminUser>);

    let submit = move |_| {
        let q = match quota.get() {
            Ok(q) => q,
            Err(e) => {
                error.set(Some(e));
                return;
            }
        };
        let body = CreateUser {
            username: username().trim().to_string(),
            password: password(),
            admin: admin(),
            quota: Some(q),
        };
        busy.set(true);
        spawn(async move {
            match api::post::<_, AdminUser>("/api/admin/users", &body).await {
                Ok(u) => created.set(Some(u)),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    if let Some(u) = created() {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        let text = format!(
            "Dein nrzVPS-Zugang\nAdresse: {origin}\nBenutzer: {}\nPasswort: {}\nDeine Ports: {}–{}\n\nAnmelden, dann „Mit KI hosten“ öffnen.",
            u.username,
            password(),
            u.port_from,
            u.port_to
        );
        return rsx! {
            Modal {
                title: "Benutzer angelegt",
                description: "Gib diese Zugangsdaten weiter (das Passwort wird nicht nochmal angezeigt).",
                onclose: move |_| onclose.call(true),
                footer: rsx! { Button { onclick: move |_| onclose.call(true), "Fertig" } },
                crate::components::CodeBlock { label: "Zugangsdaten", code: text }
            }
        };
    }

    rsx! {
        Modal {
            title: "Benutzer anlegen",
            description: "Bekommt automatisch den nächsten freien Port-Block.",
            onclose: move |_| onclose.call(false),
            footer: rsx! {
                Button { variant: ButtonVariant::Outline, onclick: move |_| onclose.call(false), "Abbrechen" }
                Button { disabled: busy(), onclick: submit, "Anlegen" }
            },
            div { class: "flex flex-col gap-4",
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                div { class: "grid gap-4 sm:grid-cols-2",
                    Field { label: "Benutzername",
                        input { class: INPUT, value: "{username}", autocomplete: "off", oninput: move |e| username.set(e.value()) }
                    }
                    Field { label: "Passwort", hint: "Mindestens 8 Zeichen",
                        input { class: INPUT, value: "{password}", autocomplete: "off", oninput: move |e| password.set(e.value()) }
                    }
                }
                CheckRow { checked: admin(), onchange: move |v| admin.set(v), label: "Admin", hint: "Darf Benutzer und Ressourcen verwalten." }
                QuotaFields { form: quota }
            }
        }
    }
}

#[component]
fn EditUserDialog(user: AdminUser, is_me: bool, onclose: EventHandler<bool>) -> Element {
    let quota = use_hook(|| QuotaForm::new(user.quota));
    let mut password = use_signal(String::new);
    let mut disabled = use_signal(|| user.disabled);
    let mut admin = use_signal(|| user.admin);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let id = user.id.clone();

    let submit = move |_| {
        let q = match quota.get() {
            Ok(q) => q,
            Err(e) => {
                error.set(Some(e));
                return;
            }
        };
        let body = UpdateUser {
            password: Some(password()).filter(|p| !p.is_empty()),
            quota: Some(q),
            disabled: Some(disabled()),
            admin: Some(admin()),
        };
        busy.set(true);
        let id = id.clone();
        spawn(async move {
            match api::patch::<_, AdminUser>(&routes::admin_user(&id), &body).await {
                Ok(_) => {
                    toast("Gespeichert");
                    onclose.call(true);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        Modal {
            title: "{user.username} bearbeiten",
            description: "Ports {user.port_from}–{user.port_to} (Platz #{user.slot})",
            onclose: move |_| onclose.call(false),
            footer: rsx! {
                Button { variant: ButtonVariant::Outline, onclick: move |_| onclose.call(false), "Abbrechen" }
                Button { disabled: busy(), onclick: submit, "Speichern" }
            },
            div { class: "flex flex-col gap-4",
                if let Some(e) = error() {
                    ErrorBox { message: e }
                }
                QuotaFields { form: quota }
                Field { label: "Neues Passwort", hint: "Leer lassen = unverändert",
                    input { class: INPUT, value: "{password}", autocomplete: "off", oninput: move |e| password.set(e.value()) }
                }
                if !is_me {
                    CheckRow { checked: admin(), onchange: move |v| admin.set(v), label: "Admin" }
                    CheckRow { checked: disabled(), onchange: move |v| disabled.set(v), label: "Gesperrt", hint: "Kann sich nicht mehr anmelden und keine KI mehr verbinden." }
                }
            }
        }
    }
}
