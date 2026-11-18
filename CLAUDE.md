# nrzVPS

Hosting-Plattform (Apps als Docker-Container, strenge Kontingente, MCP), nur Rust. Workspace:

- `crates/common` – API-Vertrag (Typen; Routen-Tabelle in `routes.rs`). Änderungen betreffen Backend und Web.
- `crates/server` – Backend (axum). `docker.rs` = eigener Docker-Engine-Client (Unix-Socket, hyper),
  `build.rs` = Laufzeit-Erkennung/Dockerfile-Erzeugung, `ops.rs` = gesamte App-Logik (HTTP und MCP
  nutzen sie), `mcp.rs`, `admin.rs`, `monitor.rs` (Status/Stats/Speicher im Hintergrund).
- `crates/web` – Oberfläche (Dioxus 0.7 web, nrzUI-Bausteine in `src/components/ui`, nrz-Design: DESIGN.md im nrzUI-Repo).
  Stile: Thema in `src/style/source.rs`, nach neuen Klassen `cargo xtask css` (erzeugt `src/style/compiled.rs`).
- `llm.txt` – Anleitung für KIs, per `include_str!` eingebettet; bei neuen MCP-Werkzeugen mitpflegen.

Regeln: Oberfläche Deutsch (Schweizer Schreibweise, ss statt ß), nur Theme-Farben, kein JavaScript.
Strikt bleiben: nie Überbuchung (Kontingente + Build-Budget ≤ Gesamt), alte Images nach jedem
Deploy sofort löschen, Benutzer-Container ohne Privilegien. Ports: Benutzer-Slot n → 34000+10n … +9,
Oberfläche 34100. Prüfen: `cargo clippy -p nrz-vps`, `cargo test -p nrz-vps`,
`cargo check -p nrz-vps-web --target wasm32-unknown-unknown`. Release: `cargo xtask release <version>`.
