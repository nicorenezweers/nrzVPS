//! Werkzeuge für nrzVPS:
//!
//!   cargo xtask css               Stile (nrzUI-Thema + Tailwind) nach crates/web/src/style/compiled.rs
//!   cargo xtask release <version> amd64/arm64 + Web bauen, Image in die nrzGit-Registry pushen
//!
//! Release braucht: cargo-zigbuild + zig, dioxus-cli (dx), docker mit buildx.
//! Registry-Anmeldung: $NRZGIT_REGISTRY_AUTH (base64 von user:token), $NRZGIT_TOKEN oder der
//! Git-extraHeader von nrzCode. Schalter: SKIP_WEB=1 (Web-Bundle wiederverwenden), KEEP_BUILD=1.

#[path = "../../crates/web/src/style/source.rs"]
mod source;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

type Result<T = ()> = std::result::Result<T, String>;

const REGISTRY: &str = "100.78.107.162:8390";
const DEFAULT_IMAGE: &str = "100.78.107.162:8390/nicorenezweers/nrz-vps";

/// Image-Rezept: Programm und Weboberfläche werden vorher auf dem Host gebaut – hier wird nur
/// gepackt (kein Compiler, kleines Image).
const DOCKERFILE: &str = r#"FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4

# git: Apps aus Git-Repositories (nrzGit, GitHub …) klonen
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git tzdata \
 && rm -rf /var/lib/apt/lists/*

ARG TARGETARCH
COPY --chmod=755 dist/nrz-vps-${TARGETARCH} /usr/local/bin/nrz-vps
COPY dist/public /app/public
COPY llm.txt /app/llm.txt

ENV NRZ_VPS_BIND=0.0.0.0:8080 \
    NRZ_VPS_DATA=/data \
    NRZ_VPS_WEB=/app/public \
    NRZ_VPS_DOCKER=/var/run/docker.sock \
    RUST_LOG=info,tower_http=warn

VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["nrz-vps", "healthcheck"]
CMD ["nrz-vps"]
"#;

const BUILDKITD: &str = "docker/buildkitd.toml";

fn main() -> ExitCode {
    // Von `cargo run` gesetzte Paket-Variablen nicht an dx/cargo weiterreichen (dx sucht sonst
    // das Binary von xtask statt das der Weboberfläche).
    for (key, _) in env::vars_os() {
        let k = key.to_string_lossy();
        if ["CARGO_PKG_", "CARGO_MANIFEST_", "CARGO_BIN_", "CARGO_CRATE_", "CARGO_PRIMARY_"].iter().any(|p| k.starts_with(p)) {
            // SAFETY: noch einfädig, vor jedem anderen Zugriff auf die Umgebung
            unsafe { env::remove_var(&key) };
        }
    }
    let args: Vec<String> = env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("css") => css(),
        Some("release") => match args.get(1) {
            Some(v) => release(v),
            None => Err("Version angeben, z. B. cargo xtask release 1.0.1".into()),
        },
        _ => Err("Befehle: css | release <version>".into()),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn flag(name: &str) -> bool {
    env::var(name).is_ok_and(|v| v == "1")
}

fn run(cmd: &mut Command) -> Result {
    let shown = format!("{cmd:?}");
    let status = cmd.status().map_err(|e| format!("{shown}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{shown} fehlgeschlagen ({status})")) }
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{cmd:?}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn home() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_default())
}

// ───────────────────────────── Stile ─────────────────────────────

/// Tailwind-CLI: $TAILWINDCSS, `tailwindcss` im PATH oder die von dx geladene Version.
fn tailwind_bin() -> Result<PathBuf> {
    if let Ok(p) = env::var("TAILWINDCSS") {
        return Ok(p.into());
    }
    if Command::new("tailwindcss").arg("--help").output().is_ok() {
        return Ok("tailwindcss".into());
    }
    let tools = home().join(".dx/tools");
    let mut found: Vec<PathBuf> = fs::read_dir(&tools)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("tailwindcss-v4"))
        .map(|e| e.path().join("tailwindcss"))
        .filter(|p| p.exists())
        .collect();
    found.sort();
    found.pop().ok_or_else(|| "Tailwind-CLI nicht gefunden (TAILWINDCSS setzen oder einmal `dx serve` ausführen)".into())
}

fn css() -> Result {
    let root = root();
    let src = root.join("crates/web/src");
    let tmp = root.join("target/xtask");
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let input = tmp.join("input.css");
    let out = tmp.join("output.css");
    let dir = src.to_string_lossy().replace('\\', "/");
    // Der Stil-Ordner selbst enthält CSS-Text und das Ergebnis: nicht nach Klassen durchsuchen.
    let text = format!("{}\n@source not \"{dir}/style/**\";\n", source::tailwind_input(&dir));
    fs::write(&input, text).map_err(|e| e.to_string())?;
    run(Command::new(tailwind_bin()?).arg("-i").arg(&input).arg("-o").arg(&out).arg("--minify"))?;
    let css = fs::read_to_string(&out).map_err(|e| e.to_string())?;
    let mut hashes = String::from("#");
    while css.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }
    let rs = format!(
        "//! Fertige Stile der Weboberfläche. Erzeugt von `cargo xtask css` – nicht von Hand bearbeiten.\n\n\
         pub const CSS: &str = r{hashes}\"{css}\"{hashes};\n"
    );
    let dest = src.join("style/compiled.rs");
    if fs::read_to_string(&dest).ok().as_deref() != Some(rs.as_str()) {
        fs::write(&dest, rs).map_err(|e| e.to_string())?;
    }
    println!("Stile: {} ({} KB)", dest.display(), css.len() / 1024);
    Ok(())
}

// ───────────────────────────── Release ─────────────────────────────

fn release(version: &str) -> Result {
    let root = root();
    env::set_current_dir(&root).map_err(|e| e.to_string())?;
    let cargo_toml = fs::read_to_string("Cargo.toml").map_err(|e| e.to_string())?;
    if !cargo_toml.lines().any(|l| l == format!("version = \"{version}\"")) {
        return Err(format!("Cargo.toml hat nicht Version {version}"));
    }
    let image = env::var("NRZVPS_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.into());
    let dx = env::var("DX").map(PathBuf::from).unwrap_or_else(|_| home().join(".cargo/bin/dx"));

    // Pfade ohne Leerzeichen (zig-Wrapper und C-Builds vertragen keine)
    let build = PathBuf::from(env::var("NRZVPS_BUILD_DIR").unwrap_or_else(|_| "/tmp/nrzvps-build".into()));
    let bin = build.join("bin");
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    for tool in ["ar", "ranlib"] {
        let p = bin.join(format!("zig-{tool}"));
        fs::write(&p, format!("#!/bin/sh\nexec zig {tool} \"$@\"\n")).map_err(|e| e.to_string())?;
        set_exec(&p)?;
    }
    let sysroot = output(Command::new("rustc").args(["--print", "sysroot"]))?;
    let dyld = match env::var("DYLD_FALLBACK_LIBRARY_PATH") {
        Ok(v) if !v.is_empty() => format!("{sysroot}/lib:{v}"),
        _ => format!("{sysroot}/lib"),
    };
    let envs = [
        ("AR", bin.join("zig-ar").into_os_string()),
        ("RANLIB", bin.join("zig-ranlib").into_os_string()),
        ("CARGO_ZIGBUILD_CACHE_DIR", build.join("zigcache").into_os_string()),
        // rust-objcopy von dx findet libLLVM sonst nicht
        ("DYLD_FALLBACK_LIBRARY_PATH", dyld.into()),
    ];

    // ── Bauen ──
    css()?;
    let _ = fs::remove_dir_all("dist");
    fs::create_dir_all("dist").map_err(|e| e.to_string())?;
    let webdist = build.join("webdist");
    if !flag("SKIP_WEB") || !webdist.join("public/index.html").exists() {
        let _ = fs::remove_dir_all(&webdist);
        run(Command::new(&dx)
            .current_dir("crates/web")
            .envs(envs.clone())
            .env("CARGO_TARGET_DIR", build.join("wasm"))
            .args(["bundle", "--platform", "web", "--release", "--out-dir"])
            .arg(&webdist))?;
        // Speicher sparen: WASM-Zwischenstände wegwerfen
        if !flag("KEEP_BUILD") {
            let _ = fs::remove_dir_all(build.join("wasm"));
        }
    }
    if !webdist.join("public/index.html").exists() {
        return Err("Web-Bundle fehlt (public/index.html)".into());
    }
    copy_dir(&webdist.join("public"), Path::new("dist/public"))?;
    for (arch, target) in [("amd64", "x86_64-unknown-linux-gnu"), ("arm64", "aarch64-unknown-linux-gnu")] {
        run(Command::new("cargo")
            .envs(envs.clone())
            .env("CARGO_TARGET_DIR", build.join("linux"))
            .args(["zigbuild", "--release", "-p", "nrz-vps", "--target", &format!("{target}.2.36")]))?;
        fs::copy(build.join(format!("linux/{target}/release/nrz-vps")), format!("dist/nrz-vps-{arch}"))
            .map_err(|e| e.to_string())?;
        // Speicher sparen: Zwischenstände dieses Targets wegwerfen
        if !flag("KEEP_BUILD") {
            let _ = fs::remove_dir_all(build.join(format!("linux/{target}")));
        }
    }

    // ── Registry-Anmeldung (temporäre Docker-Konfiguration, ~/.docker bleibt unberührt) ──
    let auth = registry_auth().ok_or("Keine Registry-Anmeldung (NRZGIT_REGISTRY_AUTH oder NRZGIT_TOKEN setzen)")?;
    let orig = env::var("DOCKER_CONFIG").map(PathBuf::from).unwrap_or_else(|_| home().join(".docker"));
    let cfg_dir = TempDir::new(&build.join("docker-config"))?;
    for d in ["contexts", "cli-plugins", "buildx"] {
        if orig.join(d).exists() {
            std::os::unix::fs::symlink(orig.join(d), cfg_dir.0.join(d)).map_err(|e| e.to_string())?;
        }
    }
    let mut cfg: serde_json::Value = fs::read_to_string(orig.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let obj = cfg.as_object_mut().unwrap();
    obj.remove("credsStore");
    let auths = obj.entry("auths").or_insert_with(|| serde_json::json!({}));
    if !auths.is_object() {
        *auths = serde_json::json!({});
    }
    auths[REGISTRY] = serde_json::json!({ "auth": auth });
    fs::write(cfg_dir.0.join("config.json"), cfg.to_string()).map_err(|e| e.to_string())?;
    let docker = || {
        let mut c = Command::new("docker");
        c.env("DOCKER_CONFIG", &cfg_dir.0);
        c
    };

    // ── Image ──
    let dockerfile = build.join("Dockerfile");
    fs::write(&dockerfile, DOCKERFILE).map_err(|e| e.to_string())?;
    if !docker().args(["buildx", "inspect", "nrzvps-builder"]).output().is_ok_and(|o| o.status.success()) {
        run(docker().args(["buildx", "create", "--name", "nrzvps-builder", "--driver", "docker-container", "--config", BUILDKITD]))?;
    }
    let tag = format!("{image}:{version}");
    run(docker()
        .args(["buildx", "build", "--builder", "nrzvps-builder", "--platform", "linux/amd64,linux/arm64", "-f"])
        .arg(&dockerfile)
        .args(["-t", &tag, "--push", "."]))?;
    let info = output(docker().args(["buildx", "imagetools", "inspect", "--builder", "nrzvps-builder", &tag]))?;
    println!("{}", info.lines().take(5).collect::<Vec<_>>().join("\n"));
    println!("Fertig: {tag}");
    Ok(())
}

fn registry_auth() -> Option<String> {
    if let Ok(a) = env::var("NRZGIT_REGISTRY_AUTH").map(|a| a.trim().to_string())
        && !a.is_empty()
    {
        return Some(a);
    }
    if let Ok(token) = env::var("NRZGIT_TOKEN")
        && !token.is_empty()
    {
        let user = env::var("NRZGIT_USERNAME").unwrap_or_else(|_| "nicorenezweers".into());
        return Some(base64(format!("{user}:{token}").as_bytes()));
    }
    let count: usize = env::var("GIT_CONFIG_COUNT").ok()?.parse().ok()?;
    (0..count).find_map(|i| {
        let key = env::var(format!("GIT_CONFIG_KEY_{i}")).ok()?;
        let lower = key.to_lowercase();
        if !(key.contains(REGISTRY) && lower.contains("extraheader")) {
            return None;
        }
        let val = env::var(format!("GIT_CONFIG_VALUE_{i}")).ok()?;
        val.rsplit_once("Basic ").map(|(_, a)| a.trim().to_string())
    })
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn set_exec(p: &Path) -> Result {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

fn copy_dir(from: &Path, to: &Path) -> Result {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Verzeichnis, das beim Verlassen (auch bei Fehlern) wieder gelöscht wird.
struct TempDir(PathBuf);

impl TempDir {
    fn new(path: &Path) -> Result<Self> {
        let _ = fs::remove_dir_all(path);
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
        Ok(Self(path.to_path_buf()))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_reference() {
        assert_eq!(super::base64(b"user:token"), "dXNlcjp0b2tlbg==");
        assert_eq!(super::base64(b"ab"), "YWI=");
        assert_eq!(super::base64(b"abc"), "YWJj");
    }
}
