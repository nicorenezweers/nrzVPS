//! Konto: Passwort, Port-Block, Kontingent.

use dioxus::prelude::*;
use nrz_vps_common::ChangePassword;

use crate::{
    api,
    components::ui::{app_shell::*, button::Button},
    components::{CARD, Field, INPUT, fmt_cpus, fmt_mb},
    store::{OVERVIEW, me, toast, toast_err},
};

#[component]
pub fn Account() -> Element {
    let mut current = use_signal(String::new);
    let mut new = use_signal(String::new);
    let mut new2 = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let me = me().unwrap_or_default();
    let ov = OVERVIEW.read().clone();

    let submit = move |e: FormEvent| {
        e.prevent_default();
        if new() != new2() {
            toast_err("Die neuen Passwörter stimmen nicht überein");
            return;
        }
        busy.set(true);
        let body = ChangePassword {
            current: current(),
            new: new(),
        };
        spawn(async move {
            match api::post::<_, nrz_vps_common::Ok>("/api/auth/password", &body).await {
                Ok(_) => {
                    toast("Passwort geändert");
                    current.set(String::new());
                    new.set(String::new());
                    new2.set(String::new());
                }
                Err(e) => toast_err(e),
            }
            busy.set(false);
        });
    };

    rsx! {
        document::Title { "Konto · nrzVPS" }
        PageContainer { width: PageWidth::Narrow,
            PageHeader { title: "Konto", description: "Angemeldet als {me.username}" }
            div { class: "flex flex-col gap-6",
                SettingsGroup { title: "Dein Bereich",
                    SettingsRow { label: "Benutzer", span { class: "text-sm", "{me.username}" if me.admin { " (Admin)" } } }
                    SettingsRow { label: "Port-Block", hint: "Nur diese 10 Ports kannst du nutzen.",
                        span { class: "font-mono text-sm", "{me.port_from}–{me.port_to}" }
                    }
                    if let Some(o) = &ov {
                        SettingsRow { label: "CPU", span { class: "text-sm tabular-nums", "{fmt_cpus(o.quota.cpus)}" } }
                        SettingsRow { label: "Arbeitsspeicher", span { class: "text-sm tabular-nums", "{fmt_mb(o.quota.memory_mb)}" } }
                        SettingsRow { label: "Speicherplatz", span { class: "text-sm tabular-nums", "{fmt_mb(o.quota.disk_mb)}" } }
                        SettingsRow { label: "Apps", span { class: "text-sm tabular-nums", "max. {o.quota.max_apps}" } }
                        SettingsRow { label: "Adresse deiner Apps", span { class: "font-mono text-sm", "http://{o.public_host}:<port>" } }
                    }
                }
                form { class: "{CARD} flex flex-col gap-4 p-5", onsubmit: submit,
                    h2 { class: "font-semibold", "Passwort ändern" }
                    Field { label: "Aktuelles Passwort",
                        input { class: INPUT, r#type: "password", autocomplete: "current-password", value: "{current}", oninput: move |e| current.set(e.value()) }
                    }
                    div { class: "grid gap-4 sm:grid-cols-2",
                        Field { label: "Neues Passwort", hint: "Mindestens 8 Zeichen",
                            input { class: INPUT, r#type: "password", autocomplete: "new-password", value: "{new}", oninput: move |e| new.set(e.value()) }
                        }
                        Field { label: "Wiederholen",
                            input { class: INPUT, r#type: "password", autocomplete: "new-password", value: "{new2}", oninput: move |e| new2.set(e.value()) }
                        }
                    }
                    div { class: "flex justify-end",
                        Button { button_type: "submit", disabled: busy(), "Passwort ändern" }
                    }
                }
            }
        }
    }
}
