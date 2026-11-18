//! Konfiguration über Umgebungsvariablen.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    /// `NRZ_VPS_BIND` (Standard `0.0.0.0:34100`).
    pub bind: String,
    /// `NRZ_VPS_DATA` (Standard `./data`): state.json, sources/, builds/.
    pub data_dir: PathBuf,
    /// `NRZ_VPS_WEB` (Standard `./public`): gebaute Oberfläche.
    pub web_dir: PathBuf,
    /// `NRZ_VPS_DOCKER` (Standard `/var/run/docker.sock`).
    pub docker_sock: PathBuf,
    /// `NRZ_VPS_PUBLIC_HOST`: Hostname/IP für App-Adressen, wenn in den Einstellungen nichts steht.
    pub public_host: Option<String>,
    /// `NRZ_VPS_GIT_REWRITE`: `von=nach,…` für Git-URLs (Klonen passiert im nrzVPS-Container,
    /// z. B. `100.78.107.162:8390=10.21.0.1:8390`).
    pub git_rewrite: Vec<(String, String)>,
    /// `NRZ_VPS_REGISTRY_REWRITE`: `von=nach,…` für Images (Pull macht der Docker-Dienst des Hosts,
    /// z. B. `100.78.107.162:8390=localhost:8390`, damit die HTTP-Registry von nrzGit ohne Einstellung geht).
    pub registry_rewrite: Vec<(String, String)>,
    /// `NRZ_VPS_BIND_IP` (Standard `0.0.0.0`): Adresse, an die App-Ports gebunden werden.
    pub bind_ip: String,
}

fn pairs(var: &str) -> Vec<(String, String)> {
    std::env::var(var)
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| p.split_once('='))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .collect()
}

impl Config {
    pub fn from_env() -> Self {
        let env = |k: &str, d: &str| {
            std::env::var(k)
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| d.to_string())
        };
        Self {
            bind: env("NRZ_VPS_BIND", "0.0.0.0:34100"),
            data_dir: PathBuf::from(env("NRZ_VPS_DATA", "./data")),
            web_dir: PathBuf::from(env("NRZ_VPS_WEB", "./public")),
            docker_sock: PathBuf::from(env("NRZ_VPS_DOCKER", "/var/run/docker.sock")),
            public_host: std::env::var("NRZ_VPS_PUBLIC_HOST")
                .ok()
                .filter(|s| !s.is_empty()),
            git_rewrite: pairs("NRZ_VPS_GIT_REWRITE"),
            registry_rewrite: pairs("NRZ_VPS_REGISTRY_REWRITE"),
            bind_ip: env("NRZ_VPS_BIND_IP", "0.0.0.0"),
        }
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.data_dir.join("sources")
    }
    pub fn builds_dir(&self) -> PathBuf {
        self.data_dir.join("builds")
    }

    pub fn rewrite_git(&self, url: &str) -> String {
        rewrite(url, &self.git_rewrite)
    }
    pub fn rewrite_image(&self, image: &str) -> String {
        rewrite(image, &self.registry_rewrite)
    }
}

fn rewrite(s: &str, rules: &[(String, String)]) -> String {
    for (from, to) in rules {
        if s.contains(from.as_str()) {
            return s.replacen(from.as_str(), to, 1);
        }
    }
    s.to_string()
}
