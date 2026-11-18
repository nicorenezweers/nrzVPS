//! Mit KI hosten: MCP-Verbindung einrichten – einfach erklärt.

use dioxus::prelude::*;
use nrz_vps_common::{ApiToken, CreateToken, CreatedToken};

use crate::{
    api,
    components::ui::{
        app_shell::*,
        button::{Button, ButtonSize, ButtonVariant},
    },
    components::{
        CARD, CodeBlock, ConfirmDialog, CopyField, INPUT, Loading, Notice, fmt_ago, fmt_datetime,
    },
    store::{OVERVIEW, toast, toast_err},
};

#[component]
pub fn AiHosting() -> Element {
    let mut tokens = use_signal(|| None::<Vec<ApiToken>>);
    let mut created = use_signal(|| None::<String>);
    let mut name = use_signal(|| "Meine KI".to_string());
    let mut delete = use_signal(|| None::<ApiToken>);
    let mut reload = use_signal(|| 0u32);

    use_effect(move || {
        let _ = reload();
        spawn(async move {
            match api::get::<Vec<ApiToken>>("/api/tokens").await {
                Ok(t) => tokens.set(Some(t)),
                Err(e) => toast_err(e),
            }
        });
    });

    let ov = OVERVIEW.read().clone();
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();
    let mcp_url = ov
        .as_ref()
        .map(|o| o.mcp_url.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{origin}/mcp"));
    let llm_url = ov
        .as_ref()
        .map(|o| o.llm_url.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{origin}/llm.txt"));
    let token = created().unwrap_or_else(|| "DEIN_TOKEN".to_string());
    let claude = format!(
        "claude mcp add --transport http nrzvps {mcp_url} --header \"Authorization: Bearer {token}\""
    );
    let json = format!(
        "{{\n  \"mcpServers\": {{\n    \"nrzvps\": {{\n      \"type\": \"http\",\n      \"url\": \"{mcp_url}\",\n      \"headers\": {{ \"Authorization\": \"Bearer {token}\" }}\n    }}\n  }}\n}}"
    );
    let prompt = format!(
        "Hoste dieses Projekt auf nrzVPS. Lies zuerst {llm_url} und gib mir am Schluss die Adresse."
    );

    let create = move |e: FormEvent| {
        e.prevent_default();
        let body = CreateToken {
            name: name().trim().to_string(),
        };
        spawn(async move {
            match api::post::<_, CreatedToken>("/api/tokens", &body).await {
                Ok(t) => {
                    created.set(Some(t.secret));
                    reload += 1;
                    toast("Token erstellt – jetzt kopieren, es wird nur einmal angezeigt");
                }
                Err(e) => toast_err(e),
            }
        });
    };

    rsx! {
        document::Title { "Mit KI hosten · nrzVPS" }
        PageContainer { width: PageWidth::Narrow,
            PageHeader {
                title: "Mit KI hosten",
                description: "Du musst nichts von Servern verstehen. Verbinde deine KI einmal – danach sagst du ihr einfach, was online soll.",
            }
            div { class: "flex flex-col gap-6",
                section { class: "{CARD} p-5",
                    h2 { class: "font-semibold", "So funktioniert's" }
                    ol { class: "mt-3 flex flex-col gap-3 text-sm",
                        Step { n: 1, "Erstelle unten einen Schlüssel (Token). Er ist wie ein Passwort für deine KI." }
                        Step { n: 2, "Kopiere den passenden Befehl in deine KI-App (Claude Code, Cursor, Claude Desktop …)." }
                        Step { n: 3, "Sag der KI: „Hoste dieses Projekt auf nrzVPS.“ Sie lädt den Code hoch, baut ihn, startet ihn und sagt dir die Adresse." }
                    }
                    p { class: "mt-4 text-xs text-muted-foreground",
                        "Die KI darf nur in deinem Bereich arbeiten: deine Apps, deine 10 Ports, dein Kontingent. Updates löschen das alte Image automatisch."
                    }
                }

                section { class: "{CARD} flex flex-col gap-4 p-5",
                    h2 { class: "font-semibold", "1. Schlüssel erstellen" }
                    if let Some(secret) = created() {
                        Notice { warning: true, "Nur jetzt sichtbar! Kopiere den Schlüssel – er ist in den Befehlen unten schon eingesetzt." }
                        CopyField { value: secret }
                    } else {
                        form { class: "flex gap-2", onsubmit: create,
                            input { class: INPUT, value: "{name}", placeholder: "Name, z. B. Laptop", oninput: move |e| name.set(e.value()) }
                            Button { button_type: "submit", icons::KeyRound { class: "size-4" } "Erstellen" }
                        }
                    }
                }

                section { class: "{CARD} flex flex-col gap-4 p-5",
                    h2 { class: "font-semibold", "2. KI verbinden" }
                    div { class: "flex flex-col gap-1.5",
                        span { class: "text-sm font-medium", "Adresse des MCP-Servers" }
                        CopyField { value: mcp_url.clone() }
                    }
                    CodeBlock { label: "Claude Code (Terminal)", code: claude }
                    CodeBlock { label: "Cursor, Claude Desktop, Windsurf & andere (JSON)", code: json }
                    p { class: "text-xs text-muted-foreground",
                        "Ohne MCP? Jede KI mit Internet kann auch die Anleitung lesen: "
                        a { class: "underline underline-offset-2", href: "{llm_url}", target: "_blank", rel: "noopener", "{llm_url}" }
                    }
                }

                section { class: "{CARD} flex flex-col gap-3 p-5",
                    h2 { class: "font-semibold", "3. Loslegen" }
                    p { class: "text-sm text-muted-foreground", "Zum Beispiel so:" }
                    CodeBlock { label: "Beispiel-Auftrag", code: prompt }
                    p { class: "text-xs text-muted-foreground", "Weitere Ideen: „Zeig mir die Logs von meiner App“, „Starte meine Seite neu“, „Mach ein Update aus meinem nrzGit-Repo“, „Wie viel Speicher habe ich noch?“" }
                }

                PageSection { title: "Deine Schlüssel", description: "Einen Schlüssel löschen trennt die KI sofort.",
                    match tokens() {
                        None => rsx! { Loading {} },
                        Some(list) if list.is_empty() => rsx! { p { class: "text-sm text-muted-foreground", "Noch keine Schlüssel." } },
                        Some(list) => rsx! {
                            div { class: "-mx-4 divide-y sm:-mx-6",
                                for t in list {
                                    div { key: "{t.id}", class: "flex items-center justify-between gap-3 px-4 py-2.5 sm:px-6",
                                        div { class: "min-w-0",
                                            p { class: "text-sm font-medium", "{t.name}" }
                                            p { class: "text-xs text-muted-foreground",
                                                code { class: "font-mono", "{t.prefix}" }
                                                " · erstellt {fmt_datetime(t.created_at)}"
                                                if let Some(l) = t.last_used { " · zuletzt {fmt_ago(l)}" } else { " · nie benutzt" }
                                            }
                                        }
                                        Button {
                                            size: ButtonSize::Sm,
                                            variant: ButtonVariant::Ghost,
                                            onclick: {
                                                let t = t.clone();
                                                move |_| delete.set(Some(t.clone()))
                                            },
                                            icons::Trash2 { class: "size-4" }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
        if let Some(t) = delete() {
            ConfirmDialog {
                title: "Schlüssel löschen?",
                message: format!("„{}“ funktioniert danach nicht mehr.", t.name),
                confirm_label: "Löschen",
                destructive: true,
                oncancel: move |_| delete.set(None),
                onconfirm: move |_| {
                    let id = t.id.clone();
                    delete.set(None);
                    spawn(async move {
                        match api::delete::<nrz_vps_common::Ok>(&format!("/api/tokens/{id}")).await {
                            Ok(_) => {
                                toast("Schlüssel gelöscht");
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
fn Step(n: u32, children: Element) -> Element {
    rsx! {
        li { class: "flex gap-3",
            span { class: "flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold", "{n}" }
            span { {children} }
        }
    }
}
