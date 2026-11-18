//! nrz-Thema und App-Stile (nrzUI) als Tailwind-Eingabe. Wird von `cargo xtask css` zu
//! `compiled.rs` übersetzt; die Weboberfläche bindet nur das Ergebnis ein.
#![allow(dead_code)]

/// Gemeinsames nrz-Thema: Farben, Schrift, Radius, Grundstile (Quelle: nrzUI).
pub const THEME: &str = r##"/* =========================================================================
   nrz-Thema – das gemeinsame Aussehen aller nrzApps (nrzCode, nrzGit, nrzMedia,
   nrzPass, nrzPaper, nrzNotes, …). Quelle der Wahrheit für Farben, Schrift,
   Radius und Grundstile. Regeln: siehe DESIGN.md im nrzUI-Repo.

   - Dunkel ist Standard (`:root`). Hell nur, wenn ausdrücklich gewählt: `<html class="light">`.
   - Für die `dark:`-Varianten der nrzUI-Bausteine trägt `<html>` im Normalfall `class="dark"`.
   - Nur Grautöne, Akzente grau statt blau. Farbe nur für Zustände (Erfolg, Warnung, Fehler, Info).

   Einbinden (macht `nrzui init` automatisch):
     @import "tailwindcss";
     @import "./nrz-theme.css";   <- diese Datei
     @import "./nrz-app.css";     <- App-Hilfsklassen (Animationen, Lichtflecken, Safe-Area)
     @source "./src";
   ========================================================================= */

@custom-variant dark (&:where(.dark, .dark *));

:root {
  color-scheme: dark;
  --radius: 0.625rem;
  --font-sans: "Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
  --font-mono: ui-monospace, "SF Mono", "Geist Mono", "JetBrains Mono", Menlo, Consolas, monospace;

  /* Flächen: Hintergrund < Seitenleiste < Karte < Fläche < Popover */
  --background: oklch(0.18 0 0);
  --foreground: oklch(0.985 0 0);
  --sidenav: oklch(0.2 0 0);
  --sidenav-foreground: oklch(0.985 0 0);
  --sidenav-primary: oklch(0.922 0 0);
  --sidenav-primary-foreground: oklch(0.205 0 0);
  --sidenav-accent: oklch(0.269 0 0);
  --sidenav-accent-foreground: oklch(0.985 0 0);
  --sidenav-border: oklch(1 0 0 / 8%);
  --sidenav-ring: oklch(0.439 0 0);
  --card: oklch(0.205 0 0);
  --card-foreground: oklch(0.985 0 0);
  --surface: oklch(0.22 0 0);
  --surface-foreground: oklch(0.708 0 0);
  --popover: oklch(0.235 0 0);
  --popover-foreground: oklch(0.985 0 0);

  /* Bedienelemente: Primär = fast weiss auf dunkel */
  --primary: oklch(0.922 0 0);
  --primary-foreground: oklch(0.205 0 0);
  --secondary: oklch(0.269 0 0);
  --secondary-foreground: oklch(0.985 0 0);
  --muted: oklch(0.269 0 0);
  --muted-foreground: oklch(0.708 0 0);
  --accent: oklch(0.3 0 0);
  --accent-foreground: oklch(0.985 0 0);
  --border: oklch(1 0 0 / 10%);
  --input: oklch(1 0 0 / 15%);
  --ring: oklch(0.556 0 0);

  /* Zustände – die einzigen Farben */
  --destructive: oklch(0.704 0.191 22.216);
  --destructive-foreground: oklch(0.985 0 0);
  --success: oklch(0.65 0.16 145);
  --success-foreground: oklch(0.205 0 0);
  --warning: oklch(0.72 0.15 70);
  --warning-foreground: oklch(0.205 0 0);
  --info: oklch(0.68 0.13 250);
  --info-foreground: oklch(0.205 0 0);

  /* Diagramme: Graustufen + Zustandsfarben, kein Blau-Verlauf */
  --chart-1: oklch(0.922 0 0);
  --chart-2: oklch(0.708 0 0);
  --chart-3: oklch(0.65 0.16 145);
  --chart-4: oklch(0.72 0.15 70);
  --chart-5: oklch(0.704 0.191 22.216);
  --chart-grid: oklch(1 0 0 / 8%);

  --scrollbar: oklch(0.35 0 0);
  --selection: oklch(0.45 0 0);
}

html.light {
  color-scheme: light;
  --background: oklch(1 0 0);
  --foreground: oklch(0.145 0 0);
  --sidenav: oklch(0.975 0 0);
  --sidenav-foreground: oklch(0.145 0 0);
  --sidenav-primary: oklch(0.205 0 0);
  --sidenav-primary-foreground: oklch(0.985 0 0);
  --sidenav-accent: oklch(0.93 0 0);
  --sidenav-accent-foreground: oklch(0.205 0 0);
  --sidenav-border: oklch(0.91 0 0);
  --sidenav-ring: oklch(0.708 0 0);
  --card: oklch(0.99 0 0);
  --card-foreground: oklch(0.145 0 0);
  --surface: oklch(0.97 0 0);
  --surface-foreground: oklch(0.5 0 0);
  --popover: oklch(1 0 0);
  --popover-foreground: oklch(0.145 0 0);
  --primary: oklch(0.205 0 0);
  --primary-foreground: oklch(0.985 0 0);
  --secondary: oklch(0.96 0 0);
  --secondary-foreground: oklch(0.205 0 0);
  --muted: oklch(0.96 0 0);
  --muted-foreground: oklch(0.5 0 0);
  --accent: oklch(0.94 0 0);
  --accent-foreground: oklch(0.205 0 0);
  --border: oklch(0.91 0 0);
  --input: oklch(0.88 0 0);
  --ring: oklch(0.708 0 0);
  --destructive: oklch(0.577 0.245 27.325);
  --success: oklch(0.55 0.15 145);
  --success-foreground: oklch(0.985 0 0);
  --warning: oklch(0.65 0.15 70);
  --info: oklch(0.55 0.15 250);
  --info-foreground: oklch(0.985 0 0);
  --chart-1: oklch(0.205 0 0);
  --chart-2: oklch(0.5 0 0);
  --chart-grid: oklch(0 0 0 / 8%);
  --scrollbar: oklch(0.8 0 0);
  --selection: oklch(0.85 0 0);
}

@theme inline {
  --font-sans: var(--font-sans);
  --font-mono: var(--font-mono);
  --radius-sm: calc(var(--radius) - 4px);
  --radius-md: calc(var(--radius) - 2px);
  --radius-lg: var(--radius);
  --radius-xl: calc(var(--radius) + 4px);
  --color-background: var(--background);
  --color-foreground: var(--foreground);
  --color-sidenav: var(--sidenav);
  --color-sidenav-foreground: var(--sidenav-foreground);
  --color-sidenav-primary: var(--sidenav-primary);
  --color-sidenav-primary-foreground: var(--sidenav-primary-foreground);
  --color-sidenav-accent: var(--sidenav-accent);
  --color-sidenav-accent-foreground: var(--sidenav-accent-foreground);
  --color-sidenav-border: var(--sidenav-border);
  --color-sidenav-ring: var(--sidenav-ring);
  --color-card: var(--card);
  --color-card-foreground: var(--card-foreground);
  --color-surface: var(--surface);
  --color-surface-foreground: var(--surface-foreground);
  --color-popover: var(--popover);
  --color-popover-foreground: var(--popover-foreground);
  --color-primary: var(--primary);
  --color-primary-foreground: var(--primary-foreground);
  --color-secondary: var(--secondary);
  --color-secondary-foreground: var(--secondary-foreground);
  --color-muted: var(--muted);
  --color-muted-foreground: var(--muted-foreground);
  --color-accent: var(--accent);
  --color-accent-foreground: var(--accent-foreground);
  --color-border: var(--border);
  --color-input: var(--input);
  --color-ring: var(--ring);
  --color-destructive: var(--destructive);
  --color-destructive-foreground: var(--destructive-foreground);
  --color-success: var(--success);
  --color-success-foreground: var(--success-foreground);
  --color-warning: var(--warning);
  --color-warning-foreground: var(--warning-foreground);
  --color-info: var(--info);
  --color-info-foreground: var(--info-foreground);
  --color-chart-1: var(--chart-1);
  --color-chart-2: var(--chart-2);
  --color-chart-3: var(--chart-3);
  --color-chart-4: var(--chart-4);
  --color-chart-5: var(--chart-5);
  --color-chart-grid: var(--chart-grid);
}

@layer base {
  * {
    @apply border-border outline-ring/50;
  }
  html,
  body {
    height: 100%;
  }
  body {
    @apply bg-background text-foreground antialiased;
    font-family: var(--font-sans);
    font-synthesis-weight: none;
    text-rendering: optimizeLegibility;
    overscroll-behavior: none;
    -webkit-tap-highlight-color: transparent;
  }
  ::selection {
    background: var(--selection);
    color: var(--foreground);
  }
  button:not(:disabled),
  [role="button"]:not(:disabled) {
    cursor: pointer;
  }
  dialog {
    margin: auto;
  }
  /* Schmale, runde Scrollbalken ohne Spur */
  * {
    scrollbar-color: var(--scrollbar) transparent;
  }
  ::-webkit-scrollbar {
    width: 10px;
    height: 10px;
  }
  ::-webkit-scrollbar-track {
    background: transparent;
  }
  ::-webkit-scrollbar-thumb {
    background: var(--scrollbar);
    border-radius: 9999px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }
  /* Ladekreise auf eigener Ebene: drehen weiter, auch wenn der Hauptthread arbeitet */
  [class*="animate-spin"] {
    will-change: transform;
  }
}
"##;

/// App-Hilfsklassen aus nrzUI: Animationen, Lichtflecken, Safe-Area.
pub const APP: &str = r##"/* =========================================================================
   nrz-App-Hilfsklassen – Bewegung, Lichtflecken und Safe-Area, wie in nrzMedia,
   nrzPass & Co. Wird von den Bausteinen in `ui/app_shell.rs` benutzt.
   Nur Tailwind-`@utility`s: ungenutzte Klassen landen nicht im fertigen CSS.
   ========================================================================= */

@keyframes nrz-enter {
  from { opacity: 0; transform: translateY(4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@keyframes nrz-fade-in {
  from { opacity: 0; }
}
@keyframes nrz-sheet-in {
  from { opacity: 0; transform: translateX(-16px); }
  to { opacity: 1; transform: none; }
}
@keyframes nrz-toast-in {
  from { opacity: 0; transform: translateY(8px) scale(0.97); }
  to { opacity: 1; transform: none; }
}
@keyframes nrz-drift-a {
  0%, 100% { transform: translate3d(0, 0, 0) scale(1); }
  50% { transform: translate3d(40vw, 30vh, 0) scale(1.1); }
}
@keyframes nrz-drift-b {
  0%, 100% { transform: translate3d(0, 0, 0) scale(1); }
  50% { transform: translate3d(-40vw, 10vh, 0) scale(0.95); }
}

/* Popover, Menüs, Karten beim Erscheinen */
@utility animate-nrz-enter {
  animation: nrz-enter 150ms ease-out;
}
/* Abdunkelung hinter Schubladen und Dialogen */
@utility animate-nrz-fade {
  animation: nrz-fade-in 150ms ease-out;
}
/* Schublade (mobile Seitenleiste) von links */
@utility animate-nrz-sheet {
  animation: nrz-sheet-in 200ms cubic-bezier(0.16, 1, 0.3, 1);
}
/* Meldungen unten rechts */
@utility animate-nrz-toast {
  animation: nrz-toast-in 260ms cubic-bezier(0.16, 1, 0.3, 1);
}

/* Hintergrund der Anmeldeseiten: zwei weiche, graue Lichtflecken (wie nrzWebsite/nrzPass).
   Markup: div { class: "nrz-drift", span {} span {} } */
@utility nrz-drift {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;

  & > span {
    position: absolute;
    display: block;
    border-radius: 50%;
    filter: blur(70px);
    will-change: transform;
  }
  & > span:nth-child(1) {
    width: 44vmax;
    height: 44vmax;
    left: -10vmax;
    top: -12vmax;
    background: radial-gradient(circle, oklch(from var(--foreground) l c h / 6%), transparent 65%);
    animation: nrz-drift-a 32s ease-in-out infinite;
  }
  & > span:nth-child(2) {
    width: 38vmax;
    height: 38vmax;
    right: -12vmax;
    top: 18vh;
    background: radial-gradient(circle, oklch(from var(--foreground) l c h / 5%), transparent 65%);
    animation: nrz-drift-b 40s ease-in-out infinite;
  }
}

/* iPhone: Notch und Home-Indikator */
@utility safe-top {
  padding-top: env(safe-area-inset-top);
}
@utility safe-bottom {
  padding-bottom: env(safe-area-inset-bottom);
}

@media (prefers-reduced-motion: reduce) {
  .animate-nrz-enter,
  .animate-nrz-fade,
  .animate-nrz-sheet,
  .animate-nrz-toast {
    animation-duration: 1ms;
  }
  .nrz-drift > span {
    animation-duration: 160s;
  }
}
"##;

/// Eigene Regeln von nrzVPS.
pub const EXTRA: &str = r##"/* Konsole: Zeilen umbrechen, Monospace */
.console-line {
  white-space: pre-wrap;
  word-break: break-word;
}

/* Minecraft-Item-Pixelgrafik nicht weichzeichnen */
.pixelated {
  image-rendering: pixelated;
}
"##;

/// Vollständige Tailwind-Eingabe; `@source` zeigt relativ zur Eingabedatei auf `src/`.
pub fn tailwind_input(source_dir: &str) -> String {
    format!("@import \"tailwindcss\" source(none);\n@source \"{source_dir}/**/*.rs\";\n{THEME}\n{APP}\n{EXTRA}")
}
