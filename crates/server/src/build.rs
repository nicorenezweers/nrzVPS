//! Aus Quellcode ein Image machen: Laufzeit erkennen, passendes Dockerfile erzeugen, Build-Kontext packen.
//! Kein eigenes Dockerfile nötig – liegt eins bei (Laufzeit `dockerfile`), wird es genommen.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use nrz_vps_common::Runtime;
use serde_json::Value;

/// Ordner, die nie in den Build-Kontext kommen.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    ".next/cache",
    ".dioxus",
    ".DS_Store",
];

/// Erkennt die Laufzeit am Inhalt des Ordners.
pub fn detect(dir: &Path) -> Option<Runtime> {
    let has = |f: &str| dir.join(f).exists();
    if has("Dockerfile") {
        Some(Runtime::Dockerfile)
    } else if has("bun.lockb") || has("bun.lock") {
        Some(Runtime::Bun)
    } else if has("package.json") {
        Some(Runtime::Node)
    } else if has("deno.json") || has("deno.jsonc") {
        Some(Runtime::Deno)
    } else if has("Cargo.toml") {
        Some(Runtime::Rust)
    } else if has("go.mod") {
        Some(Runtime::Go)
    } else if has("requirements.txt")
        || has("pyproject.toml")
        || has("Pipfile")
        || has("main.py")
        || has("app.py")
    {
        Some(Runtime::Python)
    } else if has("composer.json") || has("index.php") {
        Some(Runtime::Php)
    } else if static_root(dir).is_some() {
        Some(Runtime::Static)
    } else if has("main.ts") || has("server.ts") {
        Some(Runtime::Deno)
    } else {
        None
    }
}

/// Ordner mit `index.html` für statische Seiten (Wurzel, public, dist, build, site, www).
fn static_root(dir: &Path) -> Option<String> {
    ["", "public", "dist", "build", "site", "www", "out"]
        .into_iter()
        .find(|d| dir.join(d).join("index.html").exists())
        .map(str::to_string)
}

pub struct Plan {
    pub runtime: Runtime,
    /// Inhalt des Dockerfiles (None = eigenes Dockerfile im Code).
    pub dockerfile: Option<String>,
    /// Basis-Images (zum Aufräumen nach dem Build).
    pub base_images: Vec<String>,
}

fn sh_json(cmd: &str) -> String {
    serde_json::to_string(&["sh", "-c", cmd]).unwrap_or_default()
}

fn first_existing<'a>(dir: &Path, names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().find(|n| dir.join(n).exists())
}

/// Plant den Build. `build_cmd`/`start_cmd` leer = automatisch.
pub fn plan(dir: &Path, wanted: Runtime, build_cmd: &str, start_cmd: &str) -> Result<Plan> {
    let runtime = match wanted {
        Runtime::Auto => detect(dir).ok_or_else(|| {
            anyhow::anyhow!(
                "Laufzeit nicht erkannt. Lege ein Dockerfile bei, oder setze `runtime` (node, python, rust, go, php, static, deno, bun) und `start_command`."
            )
        })?,
        r => r,
    };
    let build = build_cmd.trim();
    let start = start_cmd.trim();
    let label = "LABEL nrzvps=1";
    let (df, bases): (Option<String>, Vec<&str>) = match runtime {
        Runtime::Dockerfile => {
            if !dir.join("Dockerfile").exists() {
                bail!("Laufzeit `dockerfile`, aber kein Dockerfile im Code");
            }
            let bases = dockerfile_bases(
                &std::fs::read_to_string(dir.join("Dockerfile")).unwrap_or_default(),
            );
            return Ok(Plan {
                runtime,
                dockerfile: None,
                base_images: bases,
            });
        }
        Runtime::Node => {
            let pkg: Value = std::fs::read_to_string(dir.join("package.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            let has_script = |n: &str| pkg.pointer(&format!("/scripts/{n}")).is_some();
            let install = if dir.join("pnpm-lock.yaml").exists() {
                "corepack enable && pnpm install --frozen-lockfile || pnpm install"
            } else if dir.join("yarn.lock").exists() {
                "corepack enable && yarn install --frozen-lockfile || yarn install"
            } else if dir.join("package-lock.json").exists() {
                "npm ci || npm install"
            } else {
                "npm install"
            };
            let build_step = if !build.is_empty() {
                format!("RUN {build}")
            } else if has_script("build") {
                "RUN npm run build".to_string()
            } else {
                String::new()
            };
            let cmd = if !start.is_empty() {
                start.to_string()
            } else if has_script("start") {
                "npm start".to_string()
            } else if let Some(m) = pkg.get("main").and_then(Value::as_str) {
                format!("node {m}")
            } else if let Some(f) = first_existing(
                dir,
                &[
                    "server.js",
                    "index.js",
                    "app.js",
                    "main.js",
                    "server.mjs",
                    "index.mjs",
                ],
            ) {
                format!("node {f}")
            } else {
                bail!(
                    "Node: kein Startbefehl gefunden (scripts.start, main oder server.js/index.js). Setze `start_command`."
                );
            };
            (
                Some(format!(
                    "FROM node:22-alpine\nWORKDIR /app\nCOPY . .\nRUN {install}\n{build_step}\nENV NODE_ENV=production\nRUN mkdir -p /data && chown -R node:node /app /data\nUSER node\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["node:22-alpine"],
            )
        }
        Runtime::Bun => {
            let build_step = if build.is_empty() {
                "RUN bun run build || true".to_string()
            } else {
                format!("RUN {build}")
            };
            let cmd = if !start.is_empty() {
                start.to_string()
            } else if let Some(f) =
                first_existing(dir, &["index.ts", "server.ts", "src/index.ts", "index.js"])
            {
                format!("bun run {f}")
            } else {
                "bun run start".to_string()
            };
            (
                Some(format!(
                    "FROM oven/bun:1-alpine\nWORKDIR /app\nCOPY . .\nRUN bun install\n{build_step}\nRUN mkdir -p /data && chown -R bun:bun /app /data\nUSER bun\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["oven/bun:1-alpine"],
            )
        }
        Runtime::Deno => {
            let cfg = std::fs::read_to_string(dir.join("deno.json")).unwrap_or_default();
            let cmd = if !start.is_empty() {
                start.to_string()
            } else if cfg.contains("\"start\"") {
                "deno task start".to_string()
            } else if let Some(f) = first_existing(
                dir,
                &["main.ts", "server.ts", "mod.ts", "index.ts", "main.js"],
            ) {
                format!("deno run -A {f}")
            } else {
                bail!("Deno: kein Startbefehl gefunden. Setze `start_command`.");
            };
            let build_step = if build.is_empty() {
                String::new()
            } else {
                format!("RUN {build}")
            };
            (
                Some(format!(
                    "FROM denoland/deno:alpine\nWORKDIR /app\nCOPY . .\nRUN deno install || true\n{build_step}\nRUN mkdir -p /data && chown -R deno:deno /app /data\nUSER deno\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["denoland/deno:alpine"],
            )
        }
        Runtime::Python => {
            let install = if dir.join("requirements.txt").exists() {
                "pip install --no-cache-dir -r requirements.txt"
            } else if dir.join("pyproject.toml").exists() {
                "pip install --no-cache-dir ."
            } else if dir.join("Pipfile").exists() {
                "pip install --no-cache-dir pipenv && pipenv install --system --deploy"
            } else {
                "true"
            };
            let reqs = std::fs::read_to_string(dir.join("requirements.txt"))
                .unwrap_or_default()
                .to_lowercase();
            let cmd = if !start.is_empty() {
                start.to_string()
            } else if dir.join("manage.py").exists() {
                "python manage.py migrate --noinput; exec gunicorn -b 0.0.0.0:$PORT $(basename $(dirname $(ls */wsgi.py | head -1))).wsgi 2>/dev/null || exec python manage.py runserver 0.0.0.0:$PORT".to_string()
            } else if let Some(f) =
                first_existing(dir, &["main.py", "app.py", "server.py", "bot.py", "run.py"])
            {
                let module = f.trim_end_matches(".py");
                let src = std::fs::read_to_string(dir.join(f)).unwrap_or_default();
                if reqs.contains("uvicorn") && src.contains("FastAPI(") {
                    format!("uvicorn {module}:app --host 0.0.0.0 --port $PORT")
                } else if reqs.contains("gunicorn") && src.contains("Flask(") {
                    format!("gunicorn -b 0.0.0.0:$PORT {module}:app")
                } else {
                    format!("python {f}")
                }
            } else {
                bail!(
                    "Python: kein Startbefehl gefunden (main.py, app.py …). Setze `start_command`."
                );
            };
            let build_step = if build.is_empty() {
                String::new()
            } else {
                format!("RUN {build}")
            };
            (
                Some(format!(
                    "FROM python:3.12-slim\nENV PYTHONUNBUFFERED=1 PIP_DISABLE_PIP_VERSION_CHECK=1\nWORKDIR /app\nCOPY . .\nRUN {install}\n{build_step}\nRUN useradd -m -u 1000 app && mkdir -p /data && chown -R app:app /app /data\nUSER app\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["python:3.12-slim"],
            )
        }
        Runtime::Rust => {
            let bin = rust_binary(dir, start)?;
            let build_step = if build.is_empty() {
                "cargo build --release".to_string()
            } else {
                build.to_string()
            };
            let cmd = if start.is_empty() {
                bin.clone()
            } else {
                start.to_string()
            };
            (
                Some(format!(
                    "FROM rust:1-slim AS build\nRUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*\nWORKDIR /src\nCOPY . .\nRUN {build_step} && mkdir -p /out && cp target/release/{bin} /out/\n\nFROM debian:trixie-slim\nRUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3t64 && rm -rf /var/lib/apt/lists/* && useradd -m -u 1000 app && mkdir -p /data && chown app:app /data\nWORKDIR /app\nCOPY . /app/\nCOPY --from=build /out/{bin} /usr/local/bin/{bin}\nUSER app\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["rust:1-slim", "debian:trixie-slim"],
            )
        }
        Runtime::Go => {
            let build_step = if build.is_empty() {
                "CGO_ENABLED=0 go build -o /out/app .".to_string()
            } else {
                build.to_string()
            };
            let cmd = if start.is_empty() {
                "/app/app".to_string()
            } else {
                start.to_string()
            };
            (
                Some(format!(
                    "FROM golang:1-alpine AS build\nWORKDIR /src\nCOPY . .\nRUN mkdir -p /out && {build_step}\n\nFROM alpine:3\nRUN apk add --no-cache ca-certificates && adduser -D -u 1000 app && mkdir -p /data && chown app:app /data\nWORKDIR /app\nCOPY --from=build /out/ /app/\nUSER app\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                vec!["golang:1-alpine", "alpine:3"],
            )
        }
        Runtime::Php => {
            let root = ["public", "web", "htdocs", ""]
                .into_iter()
                .find(|d| dir.join(d).join("index.php").exists())
                .unwrap_or("");
            let composer = if dir.join("composer.json").exists() {
                "COPY --from=composer:2 /usr/bin/composer /usr/bin/composer\nRUN composer install --no-dev --no-interaction --optimize-autoloader\n"
            } else {
                ""
            };
            let build_step = if build.is_empty() {
                String::new()
            } else {
                format!("RUN {build}")
            };
            let cmd = if start.is_empty() {
                format!("php -S 0.0.0.0:$PORT -t /app/{root}")
            } else {
                start.to_string()
            };
            let mut bases = vec!["php:8.3-cli-alpine"];
            if !composer.is_empty() {
                bases.push("composer:2");
            }
            (
                Some(format!(
                    "FROM php:8.3-cli-alpine\nWORKDIR /app\nCOPY . .\n{composer}{build_step}\nRUN adduser -D -u 1000 app && mkdir -p /data && chown -R app:app /app /data\nUSER app\n{label}\nCMD {}\n",
                    sh_json(&cmd)
                )),
                bases,
            )
        }
        Runtime::Static => {
            let root = static_root(dir).unwrap_or_default();
            let build_step = if build.is_empty() {
                String::new()
            } else {
                format!("RUN {build}")
            };
            let conf = "server {\\n  listen ${PORT};\\n  root /usr/share/nginx/html;\\n  index index.html;\\n  location / { try_files $uri $uri/ /index.html; }\\n}\\n";
            (
                Some(format!(
                    "FROM nginx:alpine\nCOPY {src} /usr/share/nginx/html/\n{build_step}\nRUN sed -i 's/^worker_processes.*/worker_processes 1;/' /etc/nginx/nginx.conf && rm -f /etc/nginx/conf.d/default.conf && mkdir -p /etc/nginx/templates && printf '{conf}' > /etc/nginx/templates/default.conf.template\n{label}\n",
                    src = if root.is_empty() {
                        ".".to_string()
                    } else {
                        format!("{root}/")
                    }
                )),
                vec!["nginx:alpine"],
            )
        }
        Runtime::Auto => unreachable!(),
    };
    Ok(Plan {
        runtime,
        dockerfile: df,
        base_images: bases.into_iter().map(str::to_string).collect(),
    })
}

/// Basis-Images eines Dockerfiles (FROM-Zeilen ohne Stufen-Namen und `scratch`).
pub fn dockerfile_bases(df: &str) -> Vec<String> {
    let mut stages = Vec::new();
    let mut out = Vec::new();
    for line in df.lines() {
        let t = line.trim();
        if t.len() < 5 || !t[..5].eq_ignore_ascii_case("FROM ") {
            continue;
        }
        let parts: Vec<&str> = t[5..]
            .split_whitespace()
            .filter(|p| !p.starts_with("--"))
            .collect();
        let Some(img) = parts.first() else { continue };
        if !stages.iter().any(|s: &String| s.eq_ignore_ascii_case(img))
            && *img != "scratch"
            && !img.contains('$')
        {
            out.push((*img).to_string());
        }
        if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("as") {
            stages.push(parts[2].to_string());
        }
    }
    out.dedup();
    out
}

/// Name des Rust-Programms: aus `start_command` (erstes Wort) oder `[[bin]]`/`[package].name`.
fn rust_binary(dir: &Path, start: &str) -> Result<String> {
    if let Some(first) = start.split_whitespace().next() {
        let name = first.rsplit('/').next().unwrap_or(first);
        if !name.is_empty() {
            return Ok(name.to_string());
        }
    }
    let raw = std::fs::read_to_string(dir.join("Cargo.toml"))?;
    let v: toml::Value = toml::from_str(&raw)?;
    if let Some(bin) = v
        .get("bin")
        .and_then(|b| b.as_array())
        .and_then(|a| a.first())
        .and_then(|b| b.get("name"))
        .and_then(|n| n.as_str())
    {
        return Ok(bin.to_string());
    }
    if let Some(name) = v
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        return Ok(name.to_string());
    }
    bail!(
        "Rust-Workspace: setze `start_command` auf den Namen des Programms (z. B. `mein-server`)."
    )
}

/// Packt den Ordner als Tar (ohne node_modules, target, .git …). `dockerfile` ersetzt/ergänzt das Dockerfile.
pub fn tar_context(dir: &Path, dockerfile: Option<&str>, limit: u64) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(Vec::new());
    builder.follow_symlinks(false);
    let mut total = 0u64;
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !SKIP_DIRS.contains(&name.as_ref())
        });
    for entry in walker {
        let entry = entry?;
        let rel = entry.path().strip_prefix(dir)?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        if dockerfile.is_some() && rel == Path::new("Dockerfile") {
            continue;
        }
        let ft = entry.file_type();
        if ft.is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            if total > limit {
                bail!(
                    "Quellcode zu gross für den Build (max. {} MB)",
                    limit / 1024 / 1024
                );
            }
            builder.append_path_with_name(entry.path(), rel)?;
        } else if ft.is_dir() {
            builder.append_dir(rel, entry.path())?;
        }
    }
    if let Some(df) = dockerfile {
        let mut header = tar::Header::new_gnu();
        header.set_size(df.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "Dockerfile", df.as_bytes())?;
    }
    Ok(builder.into_inner()?)
}

/// Alle Dateien eines Ordners (relativ, ohne übersprungene Ordner), sortiert.
pub fn list_files(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            e.depth() == 0 || !SKIP_DIRS.contains(&name.as_ref())
        });
    for e in walker.flatten() {
        if e.file_type().is_file()
            && let Ok(rel) = e.path().strip_prefix(dir)
        {
            out.push((
                rel.to_string_lossy().replace('\\', "/"),
                e.metadata().map(|m| m.len()).unwrap_or(0),
            ));
        }
    }
    out.sort();
    out
}

/// Grösse eines Ordners in Bytes (alles, auch node_modules).
pub fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

/// Relativen Pfad sicher unter `base` auflösen (kein `..`, keine absoluten Pfade).
pub fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.trim().trim_start_matches("./").trim_start_matches('/');
    if rel.is_empty() {
        bail!("Leerer Pfad");
    }
    let mut out = base.to_path_buf();
    for part in Path::new(rel).components() {
        match part {
            std::path::Component::Normal(p) => out.push(p),
            std::path::Component::CurDir => {}
            _ => bail!("Ungültiger Pfad: {rel}"),
        }
    }
    Ok(out)
}

/// Archiv (zip oder tar.gz/tar) nach `dest` entpacken. Ein einzelner Wurzelordner wird entfernt.
pub fn extract_archive(bytes: &[u8], dest: &Path, limit: u64) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut total = 0u64;
    let mut entries: Vec<(String, Vec<u8>, Option<u32>)> = Vec::new();
    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let name = f.name().to_string();
            total += f.size();
            if total > limit {
                bail!("Archiv zu gross (max. {} MB entpackt)", limit / 1024 / 1024);
            }
            let mut buf = Vec::with_capacity(f.size() as usize);
            std::io::Read::read_to_end(&mut f, &mut buf)?;
            entries.push((name, buf, f.unix_mode()));
        }
    } else {
        let reader: Box<dyn std::io::Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
            Box::new(flate2::read::GzDecoder::new(bytes))
        } else {
            Box::new(bytes)
        };
        let mut ar = tar::Archive::new(reader);
        for e in ar.entries()? {
            let mut e = e?;
            if !e.header().entry_type().is_file() {
                continue;
            }
            let name = e.path()?.to_string_lossy().to_string();
            total += e.size();
            if total > limit {
                bail!("Archiv zu gross (max. {} MB entpackt)", limit / 1024 / 1024);
            }
            let mode = e.header().mode().ok();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut buf)?;
            entries.push((name, buf, mode));
        }
    }
    entries.retain(|(n, _, _)| !n.starts_with("__MACOSX/") && !n.ends_with(".DS_Store"));
    if entries.is_empty() {
        bail!("Archiv ist leer oder kein zip/tar.gz");
    }
    // Gemeinsamen Wurzelordner entfernen (z. B. „projekt-main/…“)
    let first = entries[0]
        .0
        .trim_start_matches("./")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    let strip = entries.iter().all(|(n, _, _)| {
        let n = n.trim_start_matches("./");
        n.starts_with(&format!("{first}/"))
    });
    for (name, data, mode) in entries {
        let n = name.trim_start_matches("./");
        let n = if strip { &n[first.len() + 1..] } else { n };
        if n.is_empty() {
            continue;
        }
        let path = safe_join(dest, n)?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&path, data)?;
        #[cfg(unix)]
        if let Some(m) = mode {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(m & 0o755 | 0o600));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases() {
        let df = "FROM rust:1 AS build\nRUN x\nFROM --platform=$X debian:slim\nCOPY --from=build a b\nFROM build\nFROM scratch";
        assert_eq!(
            dockerfile_bases(df),
            vec!["rust:1".to_string(), "debian:slim".to_string()]
        );
    }

    #[test]
    fn detect_and_plan() {
        let d = std::env::temp_dir().join(format!("nrzvps-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("index.html"), "<h1>hi</h1>").unwrap();
        assert_eq!(detect(&d), Some(Runtime::Static));
        std::fs::write(
            d.join("package.json"),
            r#"{"scripts":{"start":"node s.js"}}"#,
        )
        .unwrap();
        assert_eq!(detect(&d), Some(Runtime::Node));
        let p = plan(&d, Runtime::Auto, "", "").unwrap();
        assert!(
            p.dockerfile
                .unwrap()
                .contains(r#"CMD ["sh","-c","npm start"]"#)
        );
        let tar = tar_context(&d, Some("FROM x"), 1 << 20).unwrap();
        assert!(!tar.is_empty());
        assert!(safe_join(&d, "../etc").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
