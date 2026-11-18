//! Übersicht: Kontingent, Ports, eigene Apps und „So einfach geht's“.

use dioxus::prelude::*;
use nrz_vps_common::PORTS_PER_USER;

use crate::{
    Route,
    components::{
        CARD, Loading, StatCard, StatusBadge, fmt_bytes, fmt_cpus, fmt_mb, pct, runtime_text,
        ui::{
            app_shell::*,
            button::{Button, ButtonVariant},
        },
    },
    store::{APPS, APPS_LOADED, OVERVIEW},
};

#[component]
pub fn Dashboard() -> Element {
    let ov = OVERVIEW.read().clone();
    let apps = APPS.read().clone();
    let Some(ov) = ov else {
        return rsx! { PageContainer { Loading {} } };
    };
    let q = ov.quota;
    let u = ov.usage;
    let used_ports: Vec<u16> = (ov.me.port_from..=ov.me.port_to)
        .filter(|p| !ov.free_ports.contains(p))
        .collect();

    rsx! {
        document::Title { "Übersicht · nrzVPS" }
        PageContainer { width: PageWidth::Wide,
            PageHeader {
                eyebrow: "Hallo {ov.me.username}",
                title: "Übersicht",
                description: "Deine Apps und was du davon belegst. Die Grenzen legt der Betreiber fest.",
                actions: rsx! {
                    Button { variant: ButtonVariant::Outline, href: "/ai", icons::Sparkles { class: "size-4" } "Mit KI hosten" }
                    Button { href: "/new", icons::Plus { class: "size-4" } "Neue App" }
                },
            }
            div { class: "grid gap-3 sm:grid-cols-2 lg:grid-cols-5",
                StatCard {
                    label: "CPU",
                    value: fmt_cpus(u.cpus),
                    hint: format!("von {} reserviert", fmt_cpus(q.cpus)),
                    percent: pct(u.cpus, q.cpus),
                    icon: rsx! { icons::Cpu { class: "size-4" } },
                }
                StatCard {
                    label: "Arbeitsspeicher",
                    value: fmt_mb(u.memory_mb),
                    hint: format!("von {} reserviert", fmt_mb(q.memory_mb)),
                    percent: pct(u.memory_mb as f64, q.memory_mb as f64),
                    icon: rsx! { icons::MemoryStick { class: "size-4" } },
                }
                StatCard {
                    label: "Speicherplatz",
                    value: fmt_mb(u.disk_mb),
                    hint: format!("von {} belegt", fmt_mb(q.disk_mb)),
                    percent: pct(u.disk_mb as f64, q.disk_mb as f64),
                    icon: rsx! { icons::HardDrive { class: "size-4" } },
                }
                StatCard {
                    label: "Ports",
                    value: format!("{} / {}", u.ports_used, PORTS_PER_USER),
                    hint: format!("{}–{}", ov.me.port_from, ov.me.port_to),
                    percent: pct(f64::from(u.ports_used), f64::from(PORTS_PER_USER)),
                    icon: rsx! { icons::Network { class: "size-4" } },
                }
                StatCard {
                    label: "Apps",
                    value: format!("{} / {}", u.apps, q.max_apps),
                    hint: "erlaubt",
                    percent: pct(f64::from(u.apps), f64::from(q.max_apps)),
                    icon: rsx! { icons::Package { class: "size-4" } },
                }
            }

            div { class: "mt-6 grid gap-6 lg:grid-cols-3",
                div { class: "lg:col-span-2",
                    PageSection {
                        title: "Deine Apps",
                        description: "Jede App läuft abgeschottet in einem eigenen Container.",
                        if !*APPS_LOADED.read() {
                            Loading {}
                        } else if apps.is_empty() {
                            EmptyState {
                                title: "Noch keine App",
                                description: "Lade Quellcode hoch, verbinde ein Git-Repository oder nimm ein fertiges Image – oder lass einfach deine KI das machen.",
                                icon: rsx! { icons::Package {} },
                                Button { href: "/new", icons::Plus { class: "size-4" } "Neue App" }
                                Button { variant: ButtonVariant::Outline, href: "/ai", icons::Sparkles { class: "size-4" } "Mit KI hosten" }
                            }
                        } else {
                            div { class: "-mx-4 divide-y sm:-mx-6",
                                for a in apps {
                                    Link {
                                        key: "{a.id}",
                                        to: Route::AppOverview { id: a.id.clone() },
                                        class: "flex flex-wrap items-center gap-3 px-4 py-3 hover:bg-accent/40 sm:px-6",
                                        div { class: "flex size-9 items-center justify-center rounded-lg bg-muted",
                                            icons::Package { class: "size-4" }
                                        }
                                        div { class: "min-w-0 flex-1",
                                            div { class: "flex flex-wrap items-center gap-2",
                                                span { class: "font-medium", "{a.name}" }
                                                StatusBadge { status: a.status }
                                            }
                                            div { class: "mt-0.5 flex flex-wrap gap-x-3 text-xs text-muted-foreground",
                                                span { "{runtime_text(&a)}" }
                                                for u in a.urls.iter() {
                                                    span { key: "{u}", class: "font-mono", "{u}" }
                                                }
                                            }
                                        }
                                        div { class: "text-right text-xs tabular-nums text-muted-foreground",
                                            if let Some(s) = a.stats {
                                                div { "CPU {s.cpu_percent:.1} %" }
                                                div { "RAM {fmt_bytes(s.memory_bytes)}" }
                                            } else {
                                                div { "{fmt_cpus(a.cpus)} · {fmt_mb(a.memory_mb)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-col gap-6",
                    section { class: "{CARD} p-5",
                        h2 { class: "font-semibold", "So einfach geht's" }
                        ol { class: "mt-3 flex flex-col gap-3 text-sm",
                            li { class: "flex gap-3",
                                span { class: "flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold", "1" }
                                span {
                                    "Verbinde deine KI (Claude, Cursor …) einmal mit nrzVPS. "
                                    Link { to: Route::AiHosting {}, class: "underline underline-offset-2", "Anleitung" }
                                }
                            }
                            li { class: "flex gap-3",
                                span { class: "flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold", "2" }
                                span { "Sag ihr: „Hoste dieses Projekt auf nrzVPS.“" }
                            }
                            li { class: "flex gap-3",
                                span { class: "flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold", "3" }
                                span { "Fertig – die KI lädt den Code hoch, baut ihn und startet die App. Du bekommst die Adresse." }
                            }
                        }
                    }
                    section { class: "{CARD} p-5",
                        h2 { class: "font-semibold", "Deine Ports" }
                        p { class: "mt-1 text-sm text-muted-foreground",
                            "Deine Apps sind unter "
                            code { class: "font-mono text-xs", "http://{ov.public_host}:<port>" }
                            " erreichbar."
                        }
                        div { class: "mt-3 grid grid-cols-5 gap-1.5",
                            for p in ov.me.port_from..=ov.me.port_to {
                                span {
                                    key: "{p}",
                                    class: if used_ports.contains(&p) { "rounded-md bg-primary/15 py-1 text-center font-mono text-xs" } else { "rounded-md bg-muted/50 py-1 text-center font-mono text-xs text-muted-foreground" },
                                    title: if used_ports.contains(&p) { "belegt" } else { "frei" },
                                    "{p}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
