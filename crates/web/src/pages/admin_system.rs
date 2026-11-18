//! Admin → Ressourcen: Gesamtbudget, Docker, Speicher, Einstellungen, Aufräumen, alle Apps.

use dioxus::prelude::*;
use nrz_vps_common::{App, CleanupResult, Settings, SystemInfo};

use crate::{
    Route, api,
    components::{
        ErrorBox, Field, INPUT, Loading, Notice, StatCard, StatusBadge, Toggle, fmt_bytes,
        fmt_cpus, fmt_mb, parse_f64, pct, runtime_text,
        ui::{
            app_shell::*,
            button::{Button, ButtonVariant},
            table::*,
        },
    },
    pages::admin_users_quota_hint,
    store::{toast, toast_err},
};

#[component]
pub fn AdminSystem() -> Element {
    let mut info = use_signal(|| None::<SystemInfo>);
    let mut apps = use_signal(|| None::<Vec<App>>);
    let mut reload = use_signal(|| 0u32);
    let mut cleaning = use_signal(|| false);
    let mut last_cleanup = use_signal(|| None::<CleanupResult>);

    use_effect(move || {
        let _ = reload();
        spawn(async move {
            match api::get::<SystemInfo>("/api/admin/system").await {
                Ok(i) => info.set(Some(i)),
                Err(e) => toast_err(e),
            }
            if let Ok(a) = api::get::<Vec<App>>("/api/admin/apps").await {
                apps.set(Some(a));
            }
        });
    });

    let Some(i) = info() else {
        return rsx! { PageContainer { Loading {} } };
    };
    let s = i.settings.clone().unwrap_or_default();

    rsx! {
        document::Title { "Ressourcen · nrzVPS" }
        PageContainer { width: PageWidth::Wide,
            PageHeader {
                eyebrow: "Admin",
                title: "Ressourcen",
                description: "Du gibst nur Netzwerk und Leistung – hier legst du fest, wie viel davon nrzVPS insgesamt bekommt.",
                actions: rsx! {
                    Button {
                        variant: ButtonVariant::Outline,
                        disabled: cleaning(),
                        title: "Ungenutzte nrzVPS-Images, Build-Reste und Basis-Images löschen",
                        onclick: move |_| {
                            cleaning.set(true);
                            spawn(async move {
                                match api::post_empty::<CleanupResult>("/api/admin/cleanup").await {
                                    Ok(r) => {
                                        toast(format!("{} Images entfernt, {} frei", r.removed_images, fmt_bytes(r.freed_bytes)));
                                        last_cleanup.set(Some(r));
                                        reload += 1;
                                    }
                                    Err(e) => toast_err(e),
                                }
                                cleaning.set(false);
                            });
                        },
                        if cleaning() {
                            icons::LoaderCircle { class: "size-4 animate-spin" }
                        } else {
                            icons::Eraser { class: "size-4" }
                        }
                        "Aufräumen"
                    }
                },
            }
            div { class: "flex flex-col gap-6",
                if !i.docker_ok {
                    ErrorBox { message: "Docker ist nicht erreichbar. Ist /var/run/docker.sock eingebunden?" }
                }
                if let Some(r) = last_cleanup() {
                    Notice { "Aufgeräumt: {r.removed_images} Images, {fmt_bytes(r.freed_bytes)} frei." }
                }
                div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-4",
                    StatCard {
                        label: "Docker",
                        value: if i.docker_ok { i.docker_version.clone() } else { "nicht erreichbar".into() },
                        hint: format!("nrzVPS {} · {} Benutzer · {} Apps ({} laufen)", i.version, i.users, i.apps, i.running),
                        icon: rsx! { icons::Container { class: "size-4" } },
                    }
                    StatCard {
                        label: "CPU vergeben",
                        value: format!("{} / {}", fmt_cpus(i.allocated.cpus + s.build_cpus), fmt_cpus(s.total_cpus)),
                        hint: format!("Benutzer {} + Build {} · in Betrieb {} · Gerät {} Kerne", fmt_cpus(i.allocated.cpus), fmt_cpus(s.build_cpus), fmt_cpus(i.in_use.cpus), i.host_cpus),
                        percent: pct(i.allocated.cpus + s.build_cpus, s.total_cpus),
                        icon: rsx! { icons::Cpu { class: "size-4" } },
                    }
                    StatCard {
                        label: "RAM vergeben",
                        value: format!("{} / {}", fmt_mb(i.allocated.memory_mb + s.build_memory_mb), fmt_mb(s.total_memory_mb)),
                        hint: format!("in Betrieb {} · Gerät {}", fmt_mb(i.in_use.memory_mb), fmt_mb(i.host_memory_mb)),
                        percent: pct((i.allocated.memory_mb + s.build_memory_mb) as f64, s.total_memory_mb as f64),
                        icon: rsx! { icons::MemoryStick { class: "size-4" } },
                    }
                    StatCard {
                        label: "Speicher belegt",
                        value: fmt_bytes(i.images_bytes + i.volumes_bytes + i.sources_bytes),
                        hint: format!("Images {} · Daten {} · Code {} · Grenze {}", fmt_bytes(i.images_bytes), fmt_bytes(i.volumes_bytes), fmt_bytes(i.sources_bytes), fmt_mb(s.total_disk_mb)),
                        percent: pct((i.images_bytes + i.volumes_bytes + i.sources_bytes) as f64, s.total_disk_mb as f64 * 1024.0 * 1024.0),
                        icon: rsx! { icons::HardDrive { class: "size-4" } },
                    }
                }

                SettingsForm { key: "{reload}", settings: s, allocated_cpus: i.allocated.cpus, allocated_mem: i.allocated.memory_mb, onsaved: move |_| reload += 1 }

                PageSection { title: "Alle Apps", description: "Apps aller Benutzer.",
                    match apps() {
                        None => rsx! { Loading {} },
                        Some(list) if list.is_empty() => rsx! { p { class: "text-sm text-muted-foreground", "Noch keine Apps." } },
                        Some(list) => rsx! {
                            TableWrapper {
                                Table {
                                    TableHeader {
                                        TableRow {
                                            TableHead { "App" }
                                            TableHead { "Benutzer" }
                                            TableHead { "Status" }
                                            TableHead { "Art" }
                                            TableHead { "Ports" }
                                            TableHead { "Grenzen" }
                                            TableHead { "Speicher" }
                                        }
                                    }
                                    TableBody {
                                        for a in list {
                                            TableRow { key: "{a.id}",
                                                TableCell {
                                                    Link { to: Route::AppOverview { id: a.id.clone() }, class: "font-medium hover:underline", "{a.name}" }
                                                }
                                                TableCell { "{a.owner}" }
                                                TableCell { StatusBadge { status: a.status } }
                                                TableCell { span { class: "text-xs", "{runtime_text(&a)}" } }
                                                TableCell { span { class: "font-mono text-xs", "{a.ports.iter().map(|p| p.host.to_string()).collect::<Vec<_>>().join(\", \")}" } }
                                                TableCell { span { class: "tabular-nums text-xs", "{fmt_cpus(a.cpus)} · {fmt_mb(a.memory_mb)}" } }
                                                TableCell { span { class: "tabular-nums text-xs", "{fmt_bytes(a.image_size + a.volume_size + a.source_size)}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsForm(
    settings: Settings,
    allocated_cpus: f64,
    allocated_mem: u64,
    onsaved: EventHandler<()>,
) -> Element {
    let s = settings;
    let mut total_cpus = use_signal(|| s.total_cpus.to_string());
    let mut total_mem = use_signal(|| s.total_memory_mb.to_string());
    let mut total_disk = use_signal(|| s.total_disk_mb.to_string());
    let mut build_cpus = use_signal(|| s.build_cpus.to_string());
    let mut build_mem = use_signal(|| s.build_memory_mb.to_string());
    let mut public_host = use_signal(|| s.public_host.clone());
    let mut remove_base = use_signal(|| s.remove_base_images);
    let mut max_upload = use_signal(|| s.max_upload_mb.to_string());
    let mut build_timeout = use_signal(|| s.build_timeout_min.to_string());
    let mut dq_cpus = use_signal(|| s.default_quota.cpus.to_string());
    let mut dq_mem = use_signal(|| s.default_quota.memory_mb.to_string());
    let mut dq_disk = use_signal(|| s.default_quota.disk_mb.to_string());
    let mut dq_apps = use_signal(|| s.default_quota.max_apps.to_string());
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let save = move |e: FormEvent| {
        e.prevent_default();
        let num =
            |v: &str, what: &str| parse_f64(v).ok_or_else(|| format!("{what}: bitte eine Zahl"));
        let int = |v: &str, what: &str| {
            v.trim()
                .parse::<u64>()
                .map_err(|_| format!("{what}: bitte eine ganze Zahl"))
        };
        let parsed = (|| -> Result<Settings, String> {
            Ok(Settings {
                total_cpus: num(&total_cpus(), "Gesamt-CPU")?,
                total_memory_mb: int(&total_mem(), "Gesamt-RAM")?,
                total_disk_mb: int(&total_disk(), "Gesamt-Speicher")?,
                build_cpus: num(&build_cpus(), "Build-CPU")?,
                build_memory_mb: int(&build_mem(), "Build-RAM")?,
                public_host: public_host().trim().to_string(),
                remove_base_images: remove_base(),
                max_upload_mb: int(&max_upload(), "Max. Upload")?,
                build_timeout_min: int(&build_timeout(), "Build-Zeitlimit")?,
                default_quota: nrz_vps_common::Quota {
                    cpus: num(&dq_cpus(), "Standard-CPU")?,
                    memory_mb: int(&dq_mem(), "Standard-RAM")?,
                    disk_mb: int(&dq_disk(), "Standard-Speicher")?,
                    max_apps: int(&dq_apps(), "Standard-Apps")? as u32,
                },
            })
        })();
        let body = match parsed {
            Ok(b) => b,
            Err(e) => {
                error.set(Some(e));
                return;
            }
        };
        error.set(None);
        busy.set(true);
        spawn(async move {
            match api::put::<_, Settings>("/api/admin/settings", &body).await {
                Ok(_) => {
                    toast("Einstellungen gespeichert");
                    onsaved.call(());
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let hint = admin_users_quota_hint(allocated_cpus, allocated_mem);

    rsx! {
        form { class: "flex flex-col gap-6", onsubmit: save,
            PageSection {
                title: "Gesamtbudget",
                description: "Harte Obergrenzen für nrzVPS. Regel: Summe aller Benutzer-Kontingente + Build-Reserve ≤ Gesamt (CPU und RAM).",
                div { class: "flex flex-col gap-4",
                    if let Some(e) = error() {
                        ErrorBox { message: e }
                    }
                    div { class: "grid gap-4 sm:grid-cols-3",
                        Field { label: "CPU gesamt (Kerne)", hint: "z. B. 1 = ein ganzer Kern",
                            input { class: INPUT, value: "{total_cpus}", oninput: move |e| total_cpus.set(e.value()) }
                        }
                        Field { label: "RAM gesamt (MB)",
                            input { class: INPUT, r#type: "number", value: "{total_mem}", oninput: move |e| total_mem.set(e.value()) }
                        }
                        Field { label: "Speicher gesamt (MB)",
                            input { class: INPUT, r#type: "number", value: "{total_disk}", oninput: move |e| total_disk.set(e.value()) }
                        }
                    }
                    div { class: "grid gap-4 sm:grid-cols-3",
                        Field { label: "Build-Reserve CPU (Kerne)", hint: "Immer nur ein Build gleichzeitig",
                            input { class: INPUT, value: "{build_cpus}", oninput: move |e| build_cpus.set(e.value()) }
                        }
                        Field { label: "Build-Reserve RAM (MB)",
                            input { class: INPUT, r#type: "number", value: "{build_mem}", oninput: move |e| build_mem.set(e.value()) }
                        }
                        Field { label: "Build-Zeitlimit (Minuten)",
                            input { class: INPUT, r#type: "number", value: "{build_timeout}", oninput: move |e| build_timeout.set(e.value()) }
                        }
                    }
                    p { class: "text-xs text-muted-foreground", "{hint}" }
                }
            }
            PageSection { title: "Standard für neue Benutzer",
                div { class: "grid gap-4 sm:grid-cols-4",
                    Field { label: "CPU (Kerne)",
                        input { class: INPUT, value: "{dq_cpus}", oninput: move |e| dq_cpus.set(e.value()) }
                    }
                    Field { label: "RAM (MB)",
                        input { class: INPUT, r#type: "number", value: "{dq_mem}", oninput: move |e| dq_mem.set(e.value()) }
                    }
                    Field { label: "Speicher (MB)",
                        input { class: INPUT, r#type: "number", value: "{dq_disk}", oninput: move |e| dq_disk.set(e.value()) }
                    }
                    Field { label: "Max. Apps",
                        input { class: INPUT, r#type: "number", value: "{dq_apps}", oninput: move |e| dq_apps.set(e.value()) }
                    }
                }
            }
            SettingsGroup { title: "Allgemein",
                SettingsRow { label: "Öffentliche Adresse", hint: "Hostname oder IP für die App-Links (leer = wie im Browser).",
                    input { class: "{INPUT} w-56", value: "{public_host}", placeholder: "z. B. 100.78.107.162", oninput: move |e| public_host.set(e.value()) }
                }
                SettingsRow { label: "Max. Upload (MB)", hint: "Grösste Datei beim Hochladen von Quellcode.",
                    input { class: "{INPUT} w-28", r#type: "number", value: "{max_upload}", oninput: move |e| max_upload.set(e.value()) }
                }
                SettingsRow { label: "Basis-Images nach Builds löschen", hint: "Spart viel Speicher (z. B. Rust- oder Node-Images); der nächste Build lädt sie neu.",
                    Toggle { checked: remove_base(), label: "Basis-Images löschen", onchange: move |v| remove_base.set(v) }
                }
            }
            div { class: "flex justify-end",
                Button { button_type: "submit", disabled: busy(),
                    icons::Save { class: "size-4" }
                    "Speichern"
                }
            }
        }
    }
}
