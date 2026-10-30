---
title: Desktop App
description: Run Synbot as a native desktop application with Tauri
---

# Desktop App

Synbot includes a **native desktop application** built with [Tauri 2](https://v2.tauri.app/). The window loads the **bundled** Admin Dashboard from the app itself, asks the shell over IPC where the local daemon is, and then talks to that daemon on loopback. The `synbot` binary still ships as a **sidecar** and is started automatically when you open the app.

## Overview

| Mode | Frontend | Backend | Use case |
|------|----------|---------|----------|
| **Production app** | Bundled assets inside the app (`tauri://localhost`); no navigation to a remote URL | Sidecar `synbot start` (spawned by the app), loopback HTTP/WS | Daily use, packaged `.app` / `.dmg` / `.msi` |
| **Development (`tauri dev`)** | Vite dev server (`http://localhost:3000`, hot reload) | The app also spawns the sidecar; an existing daemon on the configured port is reused | Desktop shell and UI development |

The desktop app lives in the [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop) directory at the repository root.

```text
web/src/desktop/      # IPC bridge, startup splash/failure screens (shared with the web build)
desktop/
├── scripts/          # Sidecar copy script
├── package.json      # npm scripts & Tauri CLI
└── src-tauri/        # Tauri Rust crate (daemon lifecycle, IPC, tray, CSP)
    └── src/
        ├── daemon.rs # endpoint parsing, health check, sidecar spawn
        ├── commands.rs # desktop_bootstrap / desktop_pick_folder / desktop_notify
        └── tray.rs   # tray icon, menu, close-to-tray
```

On first launch, if `~/.synbot/config.json` is missing, the app runs `synbot onboard` before starting the daemon — in development too, so a fresh clone shows a working window instead of a dead UI.

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

`npm run build` runs `beforeBuildCommand` first: `build:web` (compiles `web/` into `web/dist`, which `frontendDist` in `tauri.conf.json` points at) and `build:sidecar` (copies the release `synbot`). Because the UI is embedded at compile time, a fresh clone must build the web app before `cargo build` can even run — `cargo check -p synbot-desktop` fails until `web/dist` exists.

Artifacts are written under `desktop/src-tauri/target/release/bundle/` (for example `bundle/macos/Synbot.app` and `Synbot_{version}_aarch64.dmg` on macOS Apple Silicon).

The sidecar binary is copied to `desktop/src-tauri/binaries/synbot-{target-triple}` (for example `synbot-aarch64-apple-darwin`). Tauri bundles it via `externalBin` in `tauri.conf.json`.

### Subsequent builds

If `synbot` release is already built, you can skip rebuilding it:

```bash
cd desktop
npm run build:sidecar   # copy only, no cargo build
npm run build
```

`npm run build` runs `build:web` and `build:sidecar` automatically via `beforeBuildCommand`.

## Development workflow

Development still uses the React dashboard in `web/` with Vite hot reload. The Tauri window loads `http://localhost:3000`, and the page bootstraps over IPC just like the packaged app; on the dev server `/api` and `/ws` are additionally proxied to **18888** (see `web/vite.config.ts`) for plain browser tabs.

A single command from `desktop/` is enough:

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` does the following:

1. `prepare:sidecar` copies the existing **debug** `target/debug/synbot` into `src-tauri/binaries/` (Tauri needs the file in order to compile).
2. `tauri dev` starts the Vite dev server in `web/` via `beforeDevCommand`, then builds and runs the desktop shell.
3. The page calls the `desktop_bootstrap` IPC command, which checks the configured port (default **18888**):
   - nothing listening → it onboards if `~/.synbot/config.json` is missing, spawns the sidecar `synbot start` with `SYNBOT_DESKTOP=1` and a per-launch token, then waits up to 60 seconds for the port;
   - the port is already in use → the running server is reused and no second process is started.

So you **no longer need** a separate terminal running `synbot start`. Sidecar output is forwarded to the terminal that runs `npm run dev`, so `synbot start` logs show up there. If the sidecar cannot be spawned, the window renders the failure screen with the error message instead of a blank page.

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

You can also start it yourself; the app reuses it. Because the bundled page authenticates with the shell's per-launch token, a manually started daemon must be given the same secret through the environment — otherwise the window cannot authenticate and reloads instead:

```bash
SYNBOT_DESKTOP=1 SYNBOT_DESKTOP_TOKEN=<64 hex chars> cargo run -- start
```

`SYNBOT_DESKTOP=1` forces the web dashboard on even when `web.enabled` is `false` in config. In practice, let the app spawn the daemon (`npm run dev`) and only start one manually when debugging the daemon itself; a browser tab can always reach it through `web.auth` or with auth disabled.

## Runtime behavior

### Packaged app

1. The window opens the bundled dashboard immediately; a splash is shown while the shell prepares the backend.
2. If `~/.synbot/config.json` is missing, the app runs `synbot onboard` (enables the web dashboard and creates Basic auth with `admin` plus a random password).
3. The sidecar starts `synbot start` with `SYNBOT_DESKTOP=1` and a per-launch token in `SYNBOT_DESKTOP_TOKEN`; the shell waits until the configured port (default **18888**) accepts connections (60 seconds max).
4. The shell returns `{ apiBase, wsBase, token }` over IPC; the page sends `Authorization: Bearer <token>` on REST calls and `?token=<token>` on `/ws/*` handshakes. The web password is never typed, stored or sent by the desktop app.
5. On quit (menu bar or tray → Quit) the sidecar process is terminated. Closing the window only hides it — use the tray icon or ⌘/Ctrl+Q to restore or quit.

### Desktop-only features

| Feature | Where it shows up |
|---------|-------------------|
| System tray | Show / Open Project… / Quit; left click restores the window |
| Native folder picker | The folder button next to the workspace path in the chat workbench; tray → Open Project… |
| Notifications | “Turn finished” when a run ends while the window is in the background |
| CSP | Strict policy for the bundled page (`connect-src` limited to itself plus loopback `http`/`ws`) |

### Development build

1. The window loads `devUrl` directly (Vite, `http://localhost:3000`), then bootstraps the daemon over IPC like the packaged app.
2. If nothing listens on the configured port, the sidecar is spawned and the app waits for it; sidecar output goes to the terminal.
3. Without a config the app onboards on its own and then starts the daemon; if that fails, the failure screen shows the error.
4. On quit, the sidecar the app spawned is terminated.

Both modes share the configuration, sessions, and channels under `~/.synbot/`. The daemon address is read from `web.host`/`web.port`, so a non-default port works as long as it is a loopback or concrete LAN bind address (`0.0.0.0`/`::` are dialled as `127.0.0.1`). If the port is already in use, the app reuses the running server — the check is port-only, with no verification of the peer's version or `--root-dir`.

## npm scripts

| Script | Description |
|--------|-------------|
| `npm run dev` | Copy debug sidecar + `tauri dev` (Vite HMR; the app spawns the daemon) |
| `npm run build` | Production bundle (`.app`, `.dmg`, etc.) |
| `npm run build:web` | Build `web/` into `web/dist` (the UI Tauri embeds) |
| `npm run prepare:sidecar` | Copy debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` + copy sidecar |
| `npm run build:sidecar` | Copy release sidecar only (skip cargo build) |

## Troubleshooting

### Port 18888 already in use

Another `synbot` instance or service may be bound to the port. The desktop app does **not** fail — it silently reuses whatever is listening, so make sure that instance is the backend you want (an older build or a different `--root-dir` is reused just the same). To run the app against another port, change `web.port`; the shell reads it from `~/.synbot/config.json`. Note that a daemon started by hand also needs the shell's token (see above), so a stale process from an earlier build is better killed.

### Sidecar binary missing at compile time

Tauri requires `desktop/src-tauri/binaries/synbot-{target-triple}` before building. Run:

```bash
cd desktop && npm run prepare:sidecar        # debug
# or
cd desktop && npm run prepare:sidecar:release # release
```

### Dashboard shows login / 401

The desktop app does **not** show the login form: it authenticates with the per-launch token from the shell. Seeing a login page means the window was opened outside the Tauri shell (for example the browser tab that Vite opens) — that tab still uses `web.auth`.

For the web dashboard, `synbot onboard` enables web auth by default (user `admin`, random password printed once during onboard and also stored in `web.auth` inside `~/.synbot/config.json`). When `web.auth` is `null` (auth disabled), the login page accepts any username and password; if it still reports invalid credentials, nothing is usually running on the port and the request fails at the Vite proxy before reaching any auth check.

If the desktop window keeps reloading instead of opening, the page answered with 401: the daemon was started without the shell's token (for example a leftover process). Quit the app, make sure nothing listens on the port, and start again.

### Dev: terminal stuck on `connecting` / `connection failed`

That means no backend is available on the configured port (default 18888). Look for a spawn error on the splash/failure screen, check `~/.synbot/logs/synbot.<date>.log`, or verify directly:

```bash
curl -i http://127.0.0.1:18888/api/status
```

If the `terminal` tab reports `terminal cwd is not a directory: …`, the shell's start folder does not exist — that is `mainAgent.workspace` (when no project is selected) or the selected project directory. Create the folder or fix the config, then restart the app.

## Related documentation

- [Running Synbot](/getting-started/running) — CLI daemon and services
- [Configuration](/getting-started/configuration) — `web` section and port settings
- [Desktop shell internals](/developer-guide/desktop-shell) — IPC contract, token auth, CSP, tray
- [Architecture](/developer-guide/architecture) — web dashboard and desktop layout
