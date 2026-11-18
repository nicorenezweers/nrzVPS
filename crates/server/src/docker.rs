//! Minimaler Client für die Docker Engine API über den Unix-Socket (HTTP/1.1, eine Verbindung pro
//! Anfrage). Nur was nrzVPS braucht: Netzwerke, Builds, Pulls, Images, Container, Logs, Stats, Exec.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, StatusCode, body::Incoming};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct Docker {
    sock: PathBuf,
}

/// Docker-Fehlermeldung mit HTTP-Status (404 = nicht vorhanden).
#[derive(Debug)]
pub struct DockerError {
    pub status: u16,
    pub message: String,
}

impl std::fmt::Display for DockerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Docker ({}): {}", self.status, self.message)
    }
}
impl std::error::Error for DockerError {}

pub fn is_not_found(e: &anyhow::Error) -> bool {
    e.downcast_ref::<DockerError>()
        .is_some_and(|d| d.status == 404)
}

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

impl Docker {
    pub fn new(sock: PathBuf) -> Self {
        Self { sock }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Bytes,
        content_type: &str,
        extra: &[(&str, String)],
    ) -> Result<hyper::Response<Incoming>> {
        let stream = tokio::net::UnixStream::connect(&self.sock)
            .await
            .with_context(|| format!("Docker-Socket {} nicht erreichbar", self.sock.display()))?;
        let (mut sender, conn) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("Host", "docker")
            .header("Content-Type", content_type);
        for (k, v) in extra {
            req = req.header(*k, v);
        }
        let resp = sender.send_request(req.body(Full::new(body))?).await?;
        Ok(resp)
    }

    async fn check(resp: hyper::Response<Incoming>) -> Result<hyper::Response<Incoming>> {
        let status = resp.status();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            return Ok(resp);
        }
        let bytes = resp
            .into_body()
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| String::from_utf8_lossy(&bytes).trim().to_string());
        Err(DockerError {
            status: status.as_u16(),
            message,
        }
        .into())
    }

    /// Anfrage mit optionalem JSON-Body; Antwort als JSON (`null` bei leerer Antwort).
    pub async fn json(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let b = body.map(|v| Bytes::from(v.to_string())).unwrap_or_default();
        let resp = Self::check(self.send(method, path, b, "application/json", &[]).await?).await?;
        let bytes = resp.into_body().collect().await?.to_bytes();
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// Antwort mit Zeilen-JSON (Build, Pull) lesen; `on_line` bekommt jedes Objekt.
    async fn ndjson(resp: hyper::Response<Incoming>, mut on_line: impl FnMut(Value)) -> Result<()> {
        let mut body = resp.into_body();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(frame) = body.frame().await {
            let frame = frame?;
            if let Some(data) = frame.data_ref() {
                buf.extend_from_slice(data);
                while let Some(pos) = buf.iter().position(|&c| c == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    if let Ok(v) = serde_json::from_slice::<Value>(&line) {
                        on_line(v);
                    }
                }
            }
        }
        if let Ok(v) = serde_json::from_slice::<Value>(&buf) {
            on_line(v);
        }
        Ok(())
    }

    /* ------------------------------ System ------------------------------ */

    pub async fn version(&self) -> Result<String> {
        let v = self.json(Method::GET, "/version", None).await?;
        Ok(v.get("Version")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string())
    }

    pub async fn system_df(&self) -> Result<Value> {
        self.json(Method::GET, "/system/df?type=image&type=volume", None)
            .await
    }

    /* ----------------------------- Netzwerke ---------------------------- */

    pub async fn ensure_network(&self, name: &str, labels: Value) -> Result<()> {
        match self
            .json(Method::GET, &format!("/networks/{}", enc(name)), None)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if is_not_found(&e) => {
                self.json(
                    Method::POST,
                    "/networks/create",
                    Some(json!({ "Name": name, "Driver": "bridge", "CheckDuplicate": true, "Labels": labels })),
                )
                .await?;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn remove_network(&self, name: &str) -> Result<()> {
        match self
            .json(Method::DELETE, &format!("/networks/{}", enc(name)), None)
            .await
        {
            Err(e) if !is_not_found(&e) => Err(e),
            _ => Ok(()),
        }
    }

    /* ------------------------------ Images ------------------------------ */

    /// Image aus einem Tar-Kontext bauen (klassischer Builder, damit CPU/RAM begrenzt werden können).
    /// Liefert die Image-ID und die Zwischen-Images (zum späteren Löschen). `log` bekommt jede Ausgabezeile.
    pub async fn build(
        &self,
        context: Vec<u8>,
        tag: &str,
        labels: &Value,
        nano_cpus: u64,
        memory: u64,
        mut log: impl FnMut(String),
    ) -> Result<(String, Vec<String>)> {
        let quota = (nano_cpus / 10_000).max(1_000); // cpuperiod 100000 µs
        let path = format!(
            "/build?version=1&t={}&rm=1&forcerm=1&cpuperiod=100000&cpuquota={quota}&memory={memory}&memswap={memory}&labels={}",
            enc(tag),
            enc(&labels.to_string())
        );
        let resp = Self::check(
            self.send(
                Method::POST,
                &path,
                Bytes::from(context),
                "application/x-tar",
                &[],
            )
            .await?,
        )
        .await?;
        let mut error: Option<String> = None;
        let mut id: Option<String> = None;
        let mut layers: Vec<String> = Vec::new();
        Self::ndjson(resp, |v| {
            if let Some(s) = v.get("stream").and_then(Value::as_str) {
                for l in s.lines() {
                    if let Some(h) = l.trim().strip_prefix("---> ")
                        && h.len() == 12
                        && h.chars().all(|c| c.is_ascii_hexdigit())
                        && !layers.iter().any(|x| x == h)
                    {
                        layers.push(h.to_string());
                    }
                    if !l.trim().is_empty() {
                        log(l.to_string());
                    }
                }
            }
            if let Some(e) = v.get("error").and_then(Value::as_str) {
                log(format!("FEHLER: {e}"));
                error = Some(e.to_string());
            }
            if let Some(i) = v.pointer("/aux/ID").and_then(Value::as_str) {
                id = Some(i.to_string());
            }
        })
        .await?;
        if let Some(e) = error {
            bail!("Build fehlgeschlagen: {e}");
        }
        let id = match id {
            Some(i) => i,
            None => self.image_inspect(tag).await?.0,
        };
        let short = id
            .trim_start_matches("sha256:")
            .chars()
            .take(12)
            .collect::<String>();
        layers.retain(|l| *l != short);
        Ok((id, layers))
    }

    /// Image ziehen. `auth`: (Benutzer, Passwort, Registry).
    pub async fn pull(
        &self,
        image: &str,
        auth: Option<(&str, &str, &str)>,
        mut log: impl FnMut(String),
    ) -> Result<()> {
        let (name, tag) = split_image(image);
        let path = format!("/images/create?fromImage={}&tag={}", enc(&name), enc(&tag));
        let mut extra = Vec::new();
        if let Some((u, p, reg)) = auth {
            use base64::Engine;
            let a = json!({ "username": u, "password": p, "serveraddress": reg }).to_string();
            extra.push((
                "X-Registry-Auth",
                base64::engine::general_purpose::URL_SAFE.encode(a),
            ));
        }
        let resp = Self::check(
            self.send(
                Method::POST,
                &path,
                Bytes::new(),
                "application/json",
                &extra,
            )
            .await?,
        )
        .await?;
        let mut error = None;
        Self::ndjson(resp, |v| {
            if let Some(e) = v.get("error").and_then(Value::as_str) {
                error = Some(e.to_string());
            } else if let Some(s) = v.get("status").and_then(Value::as_str) {
                // Nur Zusammenfassungen, keine Fortschrittsbalken
                if v.get("progressDetail")
                    .is_none_or(|p| p.as_object().is_none_or(serde_json::Map::is_empty))
                {
                    let id = v.get("id").and_then(Value::as_str).unwrap_or("");
                    log(if id.is_empty() {
                        s.to_string()
                    } else {
                        format!("{id}: {s}")
                    });
                }
            }
        })
        .await?;
        if let Some(e) = error {
            bail!("Image konnte nicht geladen werden: {e}");
        }
        Ok(())
    }

    /// (ID, Grösse in Bytes)
    pub async fn image_inspect(&self, image: &str) -> Result<(String, u64)> {
        let v = self
            .json(Method::GET, &format!("/images/{}/json", enc(image)), None)
            .await?;
        let id = v
            .get("Id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Image ohne ID"))?
            .to_string();
        Ok((id, v.get("Size").and_then(Value::as_u64).unwrap_or(0)))
    }

    /// Image löschen (Fehler wie „wird noch benutzt“ werden zurückgegeben).
    pub async fn remove_image(&self, image: &str, force: bool) -> Result<()> {
        match self
            .json(
                Method::DELETE,
                &format!("/images/{}?force={force}", enc(image)),
                None,
            )
            .await
        {
            Err(e) if !is_not_found(&e) => Err(e),
            _ => Ok(()),
        }
    }

    /// Hängende (unbenannte) Images mit dem Label von nrzVPS löschen. Liefert (Anzahl, Bytes).
    pub async fn prune_dangling(&self, label: &str) -> Result<(u32, u64)> {
        let filters = json!({ "dangling": ["true"], "label": [label] }).to_string();
        let v = self
            .json(
                Method::POST,
                &format!("/images/prune?filters={}", enc(&filters)),
                None,
            )
            .await?;
        let n = v
            .get("ImagesDeleted")
            .and_then(Value::as_array)
            .map_or(0, |a| {
                a.iter().filter(|x| x.get("Deleted").is_some()).count() as u32
            });
        Ok((
            n,
            v.get("SpaceReclaimed").and_then(Value::as_u64).unwrap_or(0),
        ))
    }

    /// Alle hängenden Images löschen (nur auf ausdrücklichen Wunsch des Admins).
    pub async fn prune_all_dangling(&self) -> Result<(u32, u64)> {
        let filters = json!({ "dangling": ["true"] }).to_string();
        let v = self
            .json(
                Method::POST,
                &format!("/images/prune?filters={}", enc(&filters)),
                None,
            )
            .await?;
        let n = v
            .get("ImagesDeleted")
            .and_then(Value::as_array)
            .map_or(0, |a| {
                a.iter().filter(|x| x.get("Deleted").is_some()).count() as u32
            });
        Ok((
            n,
            v.get("SpaceReclaimed").and_then(Value::as_u64).unwrap_or(0),
        ))
    }

    pub async fn list_images(&self, label: &str) -> Result<Vec<Value>> {
        let filters = json!({ "label": [label] }).to_string();
        let v = self
            .json(
                Method::GET,
                &format!("/images/json?filters={}", enc(&filters)),
                None,
            )
            .await?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    /* ----------------------------- Container ---------------------------- */

    pub async fn list_containers(&self, label: &str) -> Result<Vec<Value>> {
        let filters = json!({ "label": [label] }).to_string();
        let v = self
            .json(
                Method::GET,
                &format!("/containers/json?all=1&filters={}", enc(&filters)),
                None,
            )
            .await?;
        Ok(v.as_array().cloned().unwrap_or_default())
    }

    pub async fn create_container(&self, name: &str, spec: Value) -> Result<String> {
        let v = self
            .json(
                Method::POST,
                &format!("/containers/create?name={}", enc(name)),
                Some(spec),
            )
            .await?;
        Ok(v.get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    pub async fn start(&self, name: &str) -> Result<()> {
        self.json(
            Method::POST,
            &format!("/containers/{}/start", enc(name)),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn stop(&self, name: &str, timeout_s: u32) -> Result<()> {
        match self
            .json(
                Method::POST,
                &format!("/containers/{}/stop?t={timeout_s}", enc(name)),
                None,
            )
            .await
        {
            Err(e) if !is_not_found(&e) => Err(e),
            _ => Ok(()),
        }
    }

    /// Container entfernen (gestoppt oder nicht). Volumes bleiben.
    pub async fn remove_container(&self, name: &str) -> Result<()> {
        match self
            .json(
                Method::DELETE,
                &format!("/containers/{}?force=1", enc(name)),
                None,
            )
            .await
        {
            Err(e) if !is_not_found(&e) => Err(e),
            _ => Ok(()),
        }
    }

    pub async fn inspect(&self, name: &str) -> Result<Option<Value>> {
        match self
            .json(
                Method::GET,
                &format!("/containers/{}/json", enc(name)),
                None,
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn remove_volume(&self, name: &str) -> Result<()> {
        match self
            .json(
                Method::DELETE,
                &format!("/volumes/{}?force=1", enc(name)),
                None,
            )
            .await
        {
            Err(e) if !is_not_found(&e) => Err(e),
            _ => Ok(()),
        }
    }

    /// Letzte Logzeilen (stdout + stderr).
    pub async fn logs(&self, name: &str, tail: u32) -> Result<Vec<String>> {
        let path = format!(
            "/containers/{}/logs?stdout=1&stderr=1&timestamps=0&tail={tail}",
            enc(name)
        );
        let resp = Self::check(
            self.send(Method::GET, &path, Bytes::new(), "application/json", &[])
                .await?,
        )
        .await?;
        let bytes = resp.into_body().collect().await?.to_bytes();
        let text = demux(&bytes);
        Ok(text.lines().map(str::to_string).collect())
    }

    /// Einmalige Messung (dauert ca. 1 s, weil Docker zwei Proben nimmt).
    pub async fn stats(&self, name: &str) -> Result<Value> {
        self.json(
            Method::GET,
            &format!("/containers/{}/stats?stream=false", enc(name)),
            None,
        )
        .await
    }

    /// Befehl im laufenden Container (`sh -c`). Liefert (Exit-Code, Ausgabe).
    pub async fn exec(
        &self,
        name: &str,
        command: &str,
        timeout: Duration,
    ) -> Result<(i64, String)> {
        let v = self
            .json(
                Method::POST,
                &format!("/containers/{}/exec", enc(name)),
                Some(json!({ "Cmd": ["sh", "-c", command], "AttachStdout": true, "AttachStderr": true, "Tty": false })),
            )
            .await?;
        let id = v
            .get("Id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Exec ohne ID"))?
            .to_string();
        let run = async {
            let resp = Self::check(
                self.send(
                    Method::POST,
                    &format!("/exec/{id}/start"),
                    Bytes::from(json!({ "Detach": false, "Tty": false }).to_string()),
                    "application/json",
                    &[],
                )
                .await?,
            )
            .await?;
            let mut body = resp.into_body();
            let mut buf = Vec::new();
            while let Some(frame) = body.frame().await {
                if let Ok(f) = frame {
                    if let Some(d) = f.data_ref() {
                        buf.extend_from_slice(d);
                        if buf.len() > 512 * 1024 {
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
            anyhow::Ok(buf)
        };
        let buf = match tokio::time::timeout(timeout, run).await {
            Ok(r) => r?,
            Err(_) => {
                return Ok((
                    -1,
                    format!("Abgebrochen nach {} s (Zeitlimit).", timeout.as_secs()),
                ));
            }
        };
        let out = demux(&buf);
        let info = self
            .json(Method::GET, &format!("/exec/{id}/json"), None)
            .await
            .unwrap_or(Value::Null);
        Ok((
            info.get("ExitCode").and_then(Value::as_i64).unwrap_or(-1),
            out,
        ))
    }
}

/// Docker-Multiplex-Stream (8-Byte-Kopf je Block) zu Text. Ohne Kopf: Rohtext.
pub fn demux(b: &[u8]) -> String {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let framed = b.len() >= 8 && matches!(b[0], 0..=2) && b[1] == 0 && b[2] == 0 && b[3] == 0;
    if !framed {
        return String::from_utf8_lossy(b).into_owned();
    }
    while i + 8 <= b.len() {
        let size = u32::from_be_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        let end = (i + 8 + size).min(b.len());
        out.extend_from_slice(&b[i + 8..end]);
        i = end;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `repo/name:tag` → (`repo/name`, `tag`); Digest bleibt im Namen.
pub fn split_image(image: &str) -> (String, String) {
    if image.contains('@') {
        return (image.to_string(), String::new());
    }
    let last_slash = image.rfind('/').map_or(0, |i| i + 1);
    match image[last_slash..].rfind(':') {
        Some(i) => (
            image[..last_slash + i].to_string(),
            image[last_slash + i + 1..].to_string(),
        ),
        None => (image.to_string(), "latest".to_string()),
    }
}

/// Registry-Host eines Images (`docker.io` wenn keiner angegeben).
pub fn registry_of(image: &str) -> String {
    match image.split_once('/') {
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            first.to_string()
        }
        _ => "docker.io".to_string(),
    }
}

/// CPU in Prozent eines Kerns aus einer Stats-Antwort.
pub fn cpu_percent(v: &Value) -> f64 {
    let g = |p: &str| v.pointer(p).and_then(Value::as_f64).unwrap_or(0.0);
    let cpu_delta =
        g("/cpu_stats/cpu_usage/total_usage") - g("/precpu_stats/cpu_usage/total_usage");
    let sys_delta = g("/cpu_stats/system_cpu_usage") - g("/precpu_stats/system_cpu_usage");
    let online = v
        .pointer("/cpu_stats/online_cpus")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .max(1.0);
    if cpu_delta > 0.0 && sys_delta > 0.0 {
        cpu_delta / sys_delta * online * 100.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_split() {
        assert_eq!(split_image("nginx"), ("nginx".into(), "latest".into()));
        assert_eq!(
            split_image("nginx:alpine"),
            ("nginx".into(), "alpine".into())
        );
        assert_eq!(
            split_image("localhost:8390/a/b:1.0"),
            ("localhost:8390/a/b".into(), "1.0".into())
        );
        assert_eq!(
            split_image("localhost:8390/a/b"),
            ("localhost:8390/a/b".into(), "latest".into())
        );
        assert_eq!(registry_of("localhost:8390/a/b"), "localhost:8390");
        assert_eq!(registry_of("library/nginx"), "docker.io");
        assert_eq!(registry_of("ghcr.io/x/y"), "ghcr.io");
    }

    #[test]
    fn demux_frames() {
        let mut b = vec![1, 0, 0, 0, 0, 0, 0, 3];
        b.extend_from_slice(b"hi\n");
        b.extend_from_slice(&[2, 0, 0, 0, 0, 0, 0, 2]);
        b.extend_from_slice(b"x\n");
        assert_eq!(demux(&b), "hi\nx\n");
        assert_eq!(demux(b"plain"), "plain");
    }
}
