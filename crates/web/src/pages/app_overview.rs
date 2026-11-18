//! App → Übersicht: Werte, Ports, Quelle, letztes Deployment, Webhook.

use dioxus::prelude::*;
use nrz_vps_common::{AppStatus, Source};

use crate::{
    components::{
        CARD, CopyField, Notice, StatCard, Tag, fmt_ago, fmt_bytes, fmt_cpus, fmt_datetime,
        fmt_duration, fmt_mb, pct,
    },
    layout::use_app,
};

#[component]
pub fn AppOverview(id: String) -> Element {
    let ctx = use_app();
    let Some(a) = ctx.app.read().clone() else {
        return rsx! {};
    };
    let stats = a.stats.unwrap_or_default();
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let uptime = a
        .started_at
        .filter(|_| a.status == AppStatus::Running)
        .map(|s| fmt_duration((now - s).max(0) as u64));
    let mem_limit = if stats.memory_limit > 0 {
        stats.memory_limit
    } else {
        a.memory_mb * 1024 * 1024
    };

    rsx! {
        div { class: "flex flex-col gap-6",
            if a.image.is_none() && a.status != AppStatus::Building {
                Notice {
                    "Diese App wurde noch nicht gebaut. "
                    if matches!(a.source, Source::Upload) && a.source_size == 0 {
                        "Lade unter „Quellcode“ dein Projekt hoch und klicke dann auf „Deployen“."
                    } else {
                        "Klicke oben auf „Deployen“."
                    }
                }
            }
            if a.status == AppStatus::Crashed {
                Notice { warning: true, "Die App ist abgestürzt. Die Logs zeigen meistens warum (z. B. falscher Start-Befehl oder zu wenig Arbeitsspeicher)." }
            }
            div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-4",
                StatCard {
                    label: "CPU",
                    value: format!("{:.1} %", stats.cpu_percent),
                    hint: format!("Grenze {}", fmt_cpus(a.cpus)),
                    percent: pct(stats.cpu_percent, a.cpus * 100.0),
                    icon: rsx! { icons::Cpu { class: "size-4" } },
                }
                StatCard {
                    label: "Arbeitsspeicher",
                    value: fmt_bytes(stats.memory_bytes),
                    hint: format!("Grenze {}", fmt_mb(a.memory_mb)),
                    percent: pct(stats.memory_bytes as f64, mem_limit as f64),
                    icon: rsx! { icons::MemoryStick { class: "size-4" } },
                }
                StatCard {
                    label: "Speicher",
                    value: fmt_bytes(a.image_size + a.volume_size + a.source_size),
                    hint: format!("Image {} · Daten {} · Code {}", fmt_bytes(a.image_size), fmt_bytes(a.volume_size), fmt_bytes(a.source_size)),
                    icon: rsx! { icons::HardDrive { class: "size-4" } },
                }
                StatCard {
                    label: "Laufzeit",
                    value: uptime.unwrap_or_else(|| "–".into()),
                    hint: format!("Netz ↓ {} ↑ {}", fmt_bytes(stats.net_rx), fmt_bytes(stats.net_tx)),
                    icon: rsx! { icons::Activity { class: "size-4" } },
                }
            }

            div { class: "grid gap-6 lg:grid-cols-2",
                section { class: "{CARD} p-5",
                    h2 { class: "font-semibold", "Erreichbar unter" }
                    if a.ports.is_empty() {
                        p { class: "mt-2 text-sm text-muted-foreground", "Kein Port – die App läuft im Hintergrund. Ports fügst du unter „Einstellungen“ hinzu." }
                    } else {
                        div { class: "mt-3 flex flex-col gap-2",
                            for (i, p) in a.ports.iter().enumerate() {
                                div { key: "{p.host}", class: "flex flex-wrap items-center gap-2 text-sm",
                                    if let Some(u) = a.urls.get(i) {
                                        a { class: "inline-flex items-center gap-1 font-mono hover:underline", href: "{u}", target: "_blank", rel: "noopener",
                                            icons::ExternalLink { class: "size-3.5" }
                                            "{u}"
                                        }
                                    } else {
                                        span { class: "font-mono", "{p.host}" }
                                    }
                                    span { class: "text-xs text-muted-foreground", "→ Container-Port {p.container}/{p.protocol.as_str()}" }
                                }
                            }
                        }
                    }
                    p { class: "mt-4 text-xs text-muted-foreground",
                        "Andere Apps von dir erreichen diese App direkt unter "
                        code { class: "font-mono", "http://{a.name}:<port>" }
                        ". Dauerhafte Dateien gehören nach "
                        code { class: "font-mono", "/data" }
                        " – alles andere ist nach einem Update weg."
                    }
                }

                section { class: "{CARD} p-5",
                    h2 { class: "font-semibold", "Letztes Deployment" }
                    if let Some(d) = a.deploy.clone() {
                        div { class: "mt-3 flex flex-col gap-2 text-sm",
                            div { class: "flex flex-wrap items-center gap-2",
                                span { class: "font-medium", "#{d.number}" }
                                match d.ok {
                                    None => rsx! { Tag { tone: "warning", icons::LoaderCircle { class: "size-3 animate-spin" } "läuft" } },
                                    Some(true) => rsx! { Tag { tone: "success", "erfolgreich" } },
                                    Some(false) => rsx! { Tag { tone: "destructive", "fehlgeschlagen" } },
                                }
                                span { class: "text-xs text-muted-foreground", title: "{fmt_datetime(d.started_at)}", "{fmt_ago(d.started_at)}" }
                                if let Some(r) = d.runtime {
                                    Tag { "{r.label()}" }
                                }
                                if let Some(c) = d.commit.clone() {
                                    code { class: "font-mono text-xs text-muted-foreground", "{c.chars().take(10).collect::<String>()}" }
                                }
                            }
                            if !d.message.is_empty() {
                                p { class: "text-muted-foreground", "{d.message}" }
                            }
                        }
                    } else {
                        p { class: "mt-2 text-sm text-muted-foreground", "Noch nie deployt." }
                    }
                }

                section { class: "{CARD} p-5",
                    h2 { class: "font-semibold", "Quelle" }
                    div { class: "mt-3 flex flex-col gap-2 text-sm",
                        match a.source.clone() {
                            Source::Upload => rsx! {
                                p { "Hochgeladener Quellcode ({fmt_bytes(a.source_size)})" }
                            },
                            Source::Git { url, branch, subdir, .. } => {
                                let branch = if branch.is_empty() { "Standard".to_string() } else { branch };
                                rsx! {
                                p { class: "break-all font-mono text-xs", "{url}" }
                                p { class: "text-muted-foreground",
                                    "Branch: {branch}"
                                    if !subdir.is_empty() { " · Ordner: {subdir}" }
                                }
                                }
                            }
                            Source::Image { image, .. } => rsx! {
                                p { class: "break-all font-mono text-xs", "{image}" }
                            },
                        }
                        if let Some(img) = a.image.clone() {
                            p { class: "text-xs text-muted-foreground", "Aktuelles Image: " code { class: "font-mono", "{img}" } }
                        }
                    }
                }

                if matches!(a.source, Source::Git { .. }) {
                    section { class: "{CARD} p-5",
                        h2 { class: "font-semibold", "Automatisch deployen (Webhook)" }
                        p { class: "mt-1 text-sm text-muted-foreground",
                            "Trage diese URL in nrzGit unter Einstellungen → Webhooks ein (Ereignis: Push). Jeder Push baut die App dann neu."
                        }
                        CopyField { class: "mt-3", value: a.webhook_url.clone(), secret: true }
                        p { class: "mt-2 text-xs text-muted-foreground",
                            if a.auto_deploy { "Automatisches Deployen ist eingeschaltet." } else { "Automatisches Deployen ist aus – einschalten unter „Einstellungen“." }
                        }
                    }
                }
            }
        }
    }
}
