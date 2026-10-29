---
title: Desktop App
description: Run Synbot as a native desktop application with Tauri
---

# Desktop App

Synbot includes a **native desktop application** built with [Tauri 2](https://v2.tauri.app/). It wraps the same Admin Dashboard used in the browser, bundles the `synbot` binary as a **sidecar**, and starts the daemon automatically when you open the app.

## Overview

| Mode | Frontend | Backend | Use case |
|------|----------|---------|----------|
| **Production app** | Embedded UI served by synbot at `http://127.0.0.1:18888` | Sidecar `synbot start` (spawned by the app) | Daily use, packaged `.app` / `.dmg` / `.msi` |
| **Development (`tauri dev`)** | Vite dev server (`http://localhost:3000`, hot reload) | The app also spawns the sidecar; an existing daemon on 18888 is reused | Desktop shell and UI development |

The desktop app lives in the [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop) directory at the repository root.

```text
desktop/
├── loading/           # Startup loading page (before sidecar is ready)
├── scripts/           # Sidecar copy script
├── package.json       # npm scripts & Tauri CLI
└── src-tauri/         # Tauri Rust crate (sidecar lifecycle, WebView)
```

On first launch, if `~/.synbot/config.json` is missing, the **packaged** app runs `synbot onboard` before starting the daemon; development mode never onboards on its own — it only prints a hint.

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

Artifacts are written under `desktop/src-tauri/target/release/bundle/` (for example `bundle/macos/Synbot.app` and `Synbot_{version}_aarch64.dmg` on macOS Apple Silicon).

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

Development still uses the React dashboard in `web/` with Vite hot reload. The Tauri window loads `http://localhost:3000`; `/api` and `/ws` are proxied to **18888** (see `web/vite.config.ts`).

A single command from `desktop/` is enough:

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` does the following:

1. `prepare:sidecar` copies the existing **debug** `target/debug/synbot` into `src-tauri/binaries/` (Tauri needs the file in order to compile).
2. `tauri dev` starts the Vite dev server in `web/` via `beforeDevCommand`, then builds and runs the desktop shell.
3. On startup (`bootstrap_dev`) it checks **18888**:
   - nothing listening and `~/.synbot/config.json` exists → it spawns the sidecar `synbot start` with `SYNBOT_DESKTOP=1` and waits up to 60 seconds for the port;
   - the port is already in use → the running server is reused and no second process is started.

So you **no longer need** a separate terminal running `synbot start`. The terminal prints:

```text
[synbot-desktop] starting local synbot daemon for the dev UI…
[synbot-desktop] synbot daemon ready on 127.0.0.1:18888
```

When an existing server is reused it prints `[synbot-desktop] reusing the daemon already listening on 127.0.0.1:18888`; when the config is missing it prints `no synbot config at …` and suggests running `cargo run -- onboard` first.

Vite is configured with `server.open: true`, so a browser tab is opened as well; both the Tauri window and that tab work.

### Frontend-only changes

Edits under `web/` are picked up by Vite hot reload; just leave the backend running.

### Backend (Rust) changes

`prepare:sidecar` only copies — it does not compile (`--skip-build`), so dev runs `target/debug/synbot`. After changing backend code, rebuild and restart:

```bash
cargo build      # repository root, regenerates target/debug/synbot
cd desktop && npm run dev
```

Otherwise the desktop app keeps running the previously built binary. For the same reason, a daemon left over from an earlier session (parent detached, still holding 18888) will be reused — the check is port-only and does not verify the version — which looks like “my changes had no effect”. Stop the old process first.

### Starting the backend manually

You can also start it yourself; the app reuses it:

```bash
SYNBOT_DESKTOP=1 cargo run -- start
```

`SYNBOT_DESKTOP=1` forces the web dashboard on even when `web.enabled` is `false` in config.

## Runtime behavior

### Packaged app

1. A loading screen appears (`desktop/loading/index.html`).
2. If `~/.synbot/config.json` is missing, the app runs `synbot onboard` (enables the web dashboard and creates Basic auth with `admin` plus a random password).
3. The sidecar starts `synbot start` with `SYNBOT_DESKTOP=1`.
4. The app waits until port **18888** accepts connections (60 seconds max).
5. The WebView navigates to `http://127.0.0.1:18888` (same origin as the API — no frontend changes needed).
6. On quit, the sidecar process is terminated.

### Development build

1. The window loads `devUrl` directly (Vite, `http://localhost:3000`): no loading screen and no navigation.
2. If 18888 is not listening and the config exists, the sidecar is spawned and the app waits for the port; progress is printed to the terminal only.
3. Without a config the window still opens but has no backend (dashboard requests fail) — run `cargo run -- onboard` as prompted.
4. On quit, the sidecar the app spawned is terminated.

Both modes share the configuration, sessions, and channels under `~/.synbot/`. If 18888 is already in use, the app reuses the running server — the check is port-only, with no verification of the peer's version or `--root-dir`.

## npm scripts

| Script | Description |
|--------|-------------|
| `npm run dev` | Copy debug sidecar + `tauri dev` (Vite HMR; the app spawns the daemon) |
| `npm run build` | Production bundle (`.app`, `.dmg`, etc.) |
| `npm run prepare:sidecar` | Copy debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` + copy sidecar |
| `npm run build:sidecar` | Copy release sidecar only (skip cargo build) |

## Troubleshooting

### Port 18888 already in use

Another `synbot` instance or service may be bound to the port. The desktop app does **not** fail — it silently reuses whatever is listening, so make sure that instance is the backend you want (an older build or a different `--root-dir` is reused just the same). Changing `web.port` in config is possible, but the desktop app currently expects **18888**.

### Sidecar binary missing at compile time

Tauri requires `desktop/src-tauri/binaries/synbot-{target-triple}` before building. Run:

```bash
cd desktop && npm run prepare:sidecar        # debug
# or
cd desktop && npm run prepare:sidecar:release # release
```

### Dashboard shows login / 401

`synbot onboard` enables web auth by default (user `admin`, random password printed once during onboard and also stored in `web.auth` inside `~/.synbot/config.json`).

When `web.auth` is `null` (auth disabled), the login page accepts any username and password. If it still reports invalid credentials, the problem is usually that **nothing is running on 18888** — the request fails at the Vite proxy before reaching any auth check.

### Dev: terminal stuck on `connecting` / `connection failed`

That means no backend is available on 18888. Look for `[synbot-desktop] failed to start synbot daemon: …` in the terminal and check `~/.synbot/logs/synbot.<date>.log`, or verify directly:

```bash
curl -i http://127.0.0.1:18888/api/status
```

If the `terminal` tab reports `terminal cwd is not a directory: …`, the shell's start folder does not exist — that is `mainAgent.workspace` (when no project is selected) or the selected project directory. Create the folder or fix the config, then restart the app.

## Related documentation

- [Running Synbot](/getting-started/running) — CLI daemon and services
- [Configuration](/getting-started/configuration) — `web` section and port settings
- [Architecture](/developer-guide/architecture) — web dashboard and desktop layout
