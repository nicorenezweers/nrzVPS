//! Hintergrund: Container-Zustand (5 s), CPU/RAM (15 s), Speicherplatz (5 min), Autostart,
//! strenge Durchsetzung des Speicher-Kontingents.

use std::{collections::HashMap, time::Duration};

use nrz_vps_common::{AppStats, AppStatus};
use serde_json::Value;

use crate::{
    docker::cpu_percent,
    ops::{self, LABEL},
    state::{AppState, Live},
};

pub fn spawn(app: AppState) {
    let a = app.clone();
    tokio::spawn(async move {
        refresh_status(&a).await;
        refresh_sizes(&a).await;
        autostart(&a).await;
        let mut tick = 0u64;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            tick += 1;
            refresh_status(&a).await;
            if tick.is_multiple_of(3) {
                refresh_stats(&a).await;
            }
            if tick.is_multiple_of(60) {
                refresh_sizes(&a).await;
                enforce_disk(&a).await;
            }
        }
    });
}

fn exit_code(status_text: &str) -> Option<i64> {
    let s = status_text.split_once('(')?.1;
    s.split_once(')')?.0.trim().parse().ok()
}

pub async fn refresh_status(app: &AppState) {
    let Ok(list) = app.docker.list_containers(LABEL).await else {
        return;
    };
    let records: HashMap<String, bool> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .map(|a| (a.id.clone(), a.auto_start))
                .collect()
        })
        .await;
    let mut seen = HashMap::new();
    for c in &list {
        let Some(id) = c.pointer("/Labels/nrzvps.app").and_then(Value::as_str) else {
            continue;
        };
        let state = c.get("State").and_then(Value::as_str).unwrap_or("");
        let text = c.get("Status").and_then(Value::as_str).unwrap_or("");
        let want = records.get(id).copied().unwrap_or(false);
        let status = match state {
            "running" => AppStatus::Running,
            "restarting" => AppStatus::Restarting,
            "exited" | "dead" => {
                let code = exit_code(text).unwrap_or(0);
                if want && code != 0 && code != 143 {
                    AppStatus::Crashed
                } else {
                    AppStatus::Stopped
                }
            }
            _ => AppStatus::Stopped,
        };
        seen.insert(id.to_string(), status);
    }
    let mut live = app.live.write().await;
    for id in records.keys() {
        let entry = live.entry(id.clone()).or_default();
        match seen.get(id) {
            Some(st) => entry.status = *st,
            None => {
                entry.status = AppStatus::Stopped;
                entry.stats = None;
            }
        }
        if !matches!(entry.status, AppStatus::Running | AppStatus::Restarting) {
            entry.stats = None;
        }
    }
    live.retain(|id, _| records.contains_key(id));
}

async fn refresh_stats(app: &AppState) {
    let running: Vec<String> = app
        .live
        .read()
        .await
        .iter()
        .filter(|(_, l)| l.status == AppStatus::Running)
        .map(|(id, _)| id.clone())
        .collect();
    let names: Vec<(String, String)> = app
        .store
        .read(|s| {
            running
                .iter()
                .filter_map(|id| {
                    let a = s.apps.iter().find(|a| &a.id == id)?;
                    let u = s.users.iter().find(|u| u.id == a.owner_id)?;
                    Some((id.clone(), ops::container_name(&u.username, &a.name)))
                })
                .collect()
        })
        .await;
    let futs = names.into_iter().map(|(id, name)| {
        let d = app.docker.clone();
        async move {
            let stats = d.stats(&name).await.ok();
            let started = d
                .inspect(&name)
                .await
                .ok()
                .flatten()
                .and_then(|v| {
                    v.pointer("/State/StartedAt")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|t| t.timestamp());
            (id, stats, started)
        }
    });
    let results = futures::future::join_all(futs).await;
    let mut live = app.live.write().await;
    for (id, stats, started) in results {
        let l = live.entry(id).or_insert_with(Live::default);
        if let Some(v) = stats {
            let g = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
            let cache = g("/memory_stats/stats/inactive_file");
            let nets = v.get("networks").and_then(Value::as_object);
            let sum = |k: &str| {
                nets.map_or(0, |n| {
                    n.values()
                        .map(|x| x.get(k).and_then(Value::as_u64).unwrap_or(0))
                        .sum()
                })
            };
            l.stats = Some(AppStats {
                cpu_percent: (cpu_percent(&v) * 10.0).round() / 10.0,
                memory_bytes: g("/memory_stats/usage").saturating_sub(cache),
                memory_limit: g("/memory_stats/limit"),
                net_rx: sum("rx_bytes"),
                net_tx: sum("tx_bytes"),
            });
        }
        if started.is_some() {
            l.started_at = started;
        }
    }
}

/// Grössen von Images, Volumes und Quellcode neu messen.
pub async fn refresh_sizes(app: &AppState) {
    let apps: Vec<(String, Option<String>)> = app
        .store
        .read(|s| {
            s.apps
                .iter()
                .map(|a| (a.id.clone(), a.image_id.clone()))
                .collect()
        })
        .await;
    let df = app.docker.system_df().await.unwrap_or(Value::Null);
    let images: HashMap<String, u64> = df
        .get("Images")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    Some((i.get("Id")?.as_str()?.to_string(), i.get("Size")?.as_u64()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let volumes: HashMap<String, u64> = df
        .get("Volumes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    Some((
                        v.get("Name")?.as_str()?.to_string(),
                        v.pointer("/UsageData/Size")?.as_i64().unwrap_or(0).max(0) as u64,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let src_root = app.config.sources_dir();
    let ids: Vec<String> = apps.iter().map(|a| a.0.clone()).collect();
    let sources: HashMap<String, u64> = tokio::task::spawn_blocking(move || {
        ids.into_iter()
            .map(|id| (id.clone(), crate::build::dir_size(&src_root.join(&id))))
            .collect()
    })
    .await
    .unwrap_or_default();
    let mut sz = app.sizes.write().await;
    for (id, image_id) in &apps {
        if let Some(s) = image_id.as_ref().and_then(|i| images.get(i)) {
            sz.images.insert(id.clone(), *s);
        } else if image_id.is_none() {
            sz.images.remove(id);
        }
        if let Some(v) = volumes.get(&ops::volume_name(id)) {
            sz.volumes.insert(id.clone(), *v);
        }
    }
    sz.sources = sources;
}

/// Apps mit `auto_start` nach dem Start von nrzVPS hochfahren (soweit das Kontingent reicht).
async fn autostart(app: &AppState) {
    let apps = app.store.read(|s| s.apps.clone()).await;
    for rec in apps
        .into_iter()
        .filter(|a| a.auto_start && a.image.is_some())
    {
        let running = app
            .live
            .read()
            .await
            .get(&rec.id)
            .is_some_and(|l| matches!(l.status, AppStatus::Running | AppStatus::Restarting));
        if running {
            continue;
        }
        let Some(owner) = app.store.user(&rec.owner_id).await else {
            continue;
        };
        if owner.disabled {
            continue;
        }
        if let Err(e) = ops::recreate_and_start(app, &owner, &rec).await {
            tracing::warn!("Autostart von {}/{}: {e}", owner.username, rec.name);
        }
    }
}

/// Wer sein Speicher-Kontingent um mehr als 10 % überschreitet, dessen Apps werden gestoppt.
async fn enforce_disk(app: &AppState) {
    let users = app.store.read(|s| s.users.clone()).await;
    for u in users {
        let usage = ops::usage(app, &u.id, None).await;
        if usage.disk_mb * 10 <= u.quota.disk_mb * 11 || usage.cpus == 0.0 {
            continue;
        }
        tracing::warn!(
            "{}: Speicher {} MB > Kontingent {} MB – stoppe Apps",
            u.username,
            usage.disk_mb,
            u.quota.disk_mb
        );
        let apps: Vec<_> = app
            .store
            .read(|s| {
                s.apps
                    .iter()
                    .filter(|a| a.owner_id == u.id)
                    .cloned()
                    .collect()
            })
            .await;
        for rec in apps {
            let _ = ops::stop_container(app, &u, &rec).await;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn exit_codes() {
        assert_eq!(super::exit_code("Exited (137) 2 minutes ago"), Some(137));
        assert_eq!(super::exit_code("Up 3 hours"), None);
    }
}
