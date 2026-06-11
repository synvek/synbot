---
title: Desktop App
description: Run Synbot as a native desktop application with Tauri
---

# Desktop App

Synbot includes a **native desktop application** built with [Tauri 2](https://v2.tauri.app/). It wraps the same Admin Dashboard used in the browser, bundles the `synbot` binary as a **sidecar**, and starts the daemon automatically when you open the app.

## Overview

| Mode | Frontend | Backend | Use case |
|------|----------|---------|----------|
| **Production app** | Embedded UI served by synbot at `http://127.0.0.1:18888` | Sidecar `synbot start` | Daily use, packaged `.app` / `.dmg` / `.msi` |
| **Development** | Vite dev server (`http://localhost:3000`, hot reload) | Manual `synbot start` | UI development |

The desktop app lives in the [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop) directory at the repository root.

```text
desktop/
├── loading/           # Startup loading page (before sidecar is ready)
├── scripts/           # Sidecar copy script
├── package.json       # npm scripts & Tauri CLI
└── src-tauri/         # Tauri Rust crate (sidecar lifecycle, WebView)
```

On first launch, if no workspace exists under `~/.synbot/`, the app runs `synbot onboard` automatically, then starts the daemon.

## Prerequisites

Build the desktop app from source with:

- **Rust** 1.77+ (project uses 1.93.1 in CI)
- **Node.js** 18+ and **npm** (Tauri CLI and release web build)
- Platform build tools for Tauri ([macOS](https://v2.tauri.app/start/prerequisites/#macos), [Linux](https://v2.tauri.app/start/prerequisites/#linux), [Windows](https://v2.tauri.app/start/prerequisites/#windows))

For **development** only, you also need the web frontend dependencies:

```bash
cd web && npm install
```

## Production build

From the repository root:

```bash
# 1. Build release synbot (embeds web/dist) and copy sidecar binary
cd desktop
npm install
npm run prepare:sidecar:release

# 2. Package the desktop app
npm run build
```

Artifacts are written under `target/release/bundle/` (for example `bundle/macos/Synbot.app` and `Synbot_0.11.3_aarch64.dmg` on macOS Apple Silicon).

The sidecar binary is copied to `desktop/src-tauri/binaries/synbot-{target-triple}` (for example `synbot-aarch64-apple-darwin`). Tauri bundles it via `externalBin` in `tauri.conf.json`.

### Subsequent builds

If `synbot` release is already built, you can skip rebuilding it:

```bash
cd desktop
npm run build:sidecar   # copy only, no cargo build
npm run build
```

`npm run build` runs `build:sidecar` automatically via `beforeBuildCommand`.

## Development workflow

Development uses the existing React dashboard in `web/` with Vite hot reload. The Tauri window loads `http://localhost:3000`; API and WebSocket traffic is proxied to synbot on port **18888** (see `web/vite.config.ts`).

**Terminal 1 — synbot backend:**

```bash
SYNBOT_DESKTOP=1 cargo run -- start
```

`SYNBOT_DESKTOP=1` ensures the web dashboard is enabled even if it was not turned on in config.

**Terminal 2 — desktop dev:**

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` copies the **debug** `synbot` binary into `src-tauri/binaries/` (required for Tauri compile) and starts `tauri dev`, which also runs the Vite dev server in `web/`.

## Runtime behavior

When you launch the **packaged** app:

1. A loading screen appears (`desktop/loading/index.html`).
2. If `~/.synbot/config.json` is missing, the app runs `synbot onboard`.
3. The sidecar starts `synbot start` with `SYNBOT_DESKTOP=1`.
4. The app waits until port **18888** accepts connections.
5. The WebView navigates to `http://127.0.0.1:18888` (same origin as the API — no frontend changes needed).
6. On quit, the sidecar process is terminated.

Configuration, sessions, and channels use the same `~/.synbot/` workspace as the CLI. You can still run `synbot start` separately; if port 18888 is already in use, the desktop app reuses the existing server.

## npm scripts

| Script | Description |
|--------|-------------|
| `npm run dev` | Copy debug sidecar + `tauri dev` (Vite HMR) |
| `npm run build` | Production bundle (`.app`, `.dmg`, etc.) |
| `npm run prepare:sidecar` | Copy debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` + copy sidecar |
| `npm run build:sidecar` | Copy release sidecar only (skip cargo build) |

## Troubleshooting

### Port 18888 already in use

Another `synbot` instance or service may be bound to the port. Stop it or change `web.port` in config (the desktop app currently expects the default **18888**).

### Sidecar binary missing at compile time

Tauri requires `desktop/src-tauri/binaries/synbot-{target-triple}` before building. Run:

```bash
cd desktop && npm run prepare:sidecar        # debug
# or
cd desktop && npm run prepare:sidecar:release # release
```

### Dashboard shows login / 401

`synbot onboard` enables web auth by default. Use the username and password printed during onboard (or configured in `web.auth`).

### Dev: API errors in browser

Ensure `synbot start` is running with `SYNBOT_DESKTOP=1` on port 18888 while Vite proxies `/api` and `/ws`.

## Related documentation

- [Running Synbot](/getting-started/running) — CLI daemon and services
- [Configuration](/getting-started/configuration) — `web` section and port settings
- [Architecture](/developer-guide/architecture) — web dashboard and desktop layout
