# Desktop shell internals

The desktop app is a [Tauri 2](https://v2.tauri.app/) wrapper around the same dashboard the web build ships. It used to be a thin browser that navigated to `http://127.0.0.1:18888` once the daemon was up; now the window loads **bundled assets** and asks the shell where the backend is. That single change is what makes CSP tightening, the native folder picker, tray integration and notifications possible.

Two halves make the shell:

| Half | Location | Job |
|------|----------|-----|
| Rust crate | `desktop/src-tauri/src/` | Daemon lifecycle, IPC commands, tray, window policy |
| Frontend bridge | `web/src/desktop/` | Startup splash, failure screen, IPC client shared with the web build |

## Why the page is bundled

Loading `http://127.0.0.1:18888` meant the daemon had to serve the UI, so the origin of the page was the daemon's own origin. Bundling the dashboard into the app changes that origin to `tauri://localhost` (macOS/Linux) or `http://tauri.localhost` (Windows), which is cross-origin to the loopback API. Everything below — CORS, the token, `HashRouter` — follows from that one trade.

The sidecar itself is untouched: the shell still spawns `synbot start` and still kills it on exit.

## IPC contract

Three commands are registered in `lib.rs`; all of them are plain `#[tauri::command]` fns.

| Command | Arguments | Returns |
|---------|-----------|---------|
| `desktop_bootstrap` | — | `{ apiBase, wsBase, token, version, platform }` |
| `desktop_pick_folder` | — | absolute path, or `null` when cancelled |
| `desktop_notify` | `title`, `body` | — (errors are surfaced to the caller) |

`desktop_bootstrap` is deliberately **idempotent**: the UI may retry after a failure, so the call takes a mutex gate, returns immediately when the port is already open, onboards first (`synbot onboard`) when `~/.synbot/config.json` is missing, and otherwise spawns the sidecar and waits up to 60 seconds for the port. A second window therefore cannot start a second daemon.

`desktop_notify` clips its inputs (120 characters for the title, 400 for the body) on a character boundary and does nothing when the title is empty — a runaway model answer should never reach the notification centre.

The frontend never imports `@tauri-apps/api`. `web/src/desktop/bridge.ts` reads `window.__TAURI_INTERNALS__.invoke` directly, so the web build keeps working with no extra dependency:

```ts
const session = await bootstrapDesktop(); // null outside the shell
if (session) apiClient.configureDesktop(session);
else apiClient.loadAuth();
```

`main.tsx` renders `StartupSplash` on the first frame only when `isDesktop()` is true, awaits the bootstrap, then renders the app; a rejected bootstrap renders `StartupFailure` with the message instead of a white window. The failure screen carries bilingual static text because `I18nProvider` is not mounted yet.

## Token authentication

A bundled page cannot use the web password: it is never typed, and WS handshakes cannot carry an `Authorization` header. So the shell mints a **per-launch secret** (`daemon::new_desktop_token`, two UUIDv4s → 64 hex characters), passes it to the sidecar as `SYNBOT_DESKTOP_TOKEN`, and returns it over IPC.

| Transport | Credential | Where it is checked |
|-----------|------------|---------------------|
| REST | `Authorization: Bearer <token>` | `src/web/auth.rs`, before Basic auth |
| WebSocket | `?token=<token>` | same middleware, only for paths starting with `/ws/` |

Details worth keeping when touching this code:

- The query token is accepted **only** on `/ws/*`. Ordinary API calls would otherwise leak the secret into request logs and browser history.
- `usable_desktop_token` rejects anything shorter than 32 characters or containing a non-hex character, so a typo like `SYNBOT_DESKTOP_TOKEN=1` cannot turn into a one-character dashboard password.
- Comparison is length-checked and then constant time.
- Token-authenticated requests are inserted as `AuthenticatedUser { username: "desktop" }` and **no session cookie is issued** — the shell's token is not a login.
- With `SYNBOT_DESKTOP_TOKEN` unset, `desktop_enabled()` is false, both credential paths are inert, and the dashboard behaves exactly like the plain web build.

The token lives only in the child process environment and in the page's memory; `configureDesktop` also clears any stale Basic header from `sessionStorage` so it cannot shadow the token after a reload.

## CORS

`src/web/cors.rs` knows the three shell origins: `tauri://localhost`, `http://tauri.localhost`, `https://tauri.localhost`. They are appended only when `auth.desktop_enabled()` is true (`src/web/server.rs`), so a daemon started by a browser-only workflow keeps its original policy.

The subtlety is that an **empty** origin list (or one containing `*`) means *allow everything*. `Cors::new` records that intent in an `allow_all` flag before the desktop origins are appended; otherwise adding three origins would silently downgrade an open daemon to a three-origin whitelist.

## CSP

`tauri.conf.json` ships a strict policy for the bundled page:

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:; font-src 'self' data:;
connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:* ws://127.0.0.1:*;
object-src 'none'; base-uri 'self'; frame-ancestors 'none'
```

`connect-src` is the interesting part: `ipc:` / `http://ipc.localhost` for `invoke`, and loopback `http`/`ws` for the daemon. No external origin is reachable from the shell's own page. `style-src` keeps `'unsafe-inline'` because the dashboard sets inline styles (including the terminal's dynamic geometry).

## Tray, close-to-tray and native events

`src-tauri/src/tray.rs` builds the icon and menu at startup:

| Menu id | Action |
|---------|--------|
| `tray-show` | Show and focus the main window |
| `tray-open-project` | Dispatch the open-project event to the page |
| `tray-quit` | `app.exit(0)` |

Left click restores the window as well (`show_menu_on_left_click(false)`), so the menu does not pop up on the common gesture. A tray build failure is logged and *not* fatal.

Close-to-tray is deliberately conditional: `install_close_to_tray` intercepts `CloseRequested` only when a `TRAY_READY` flag was set, otherwise closing the window quits the app. Hiding a window with no way to bring it back would strand the user.

The open-project action has to cross from Rust into React. The shell evaluates a tiny script in the webview:

```js
window.dispatchEvent(new CustomEvent('synbot:open-project', { detail: "<json-quoted path>" }));
```

The path is serialised with `serde_json` before interpolation, so a directory name containing quotes cannot break out of the string literal. On the React side `onOpenProject(handler)` subscribes to the DOM event and returns an unsubscribe function; `ProjectPanel` reuses the same `openProjectPath` callback it uses for the form submit, so tray and UI share one code path.

## Routing

`App.tsx` picks `HashRouter` when `isDesktop()`, `BrowserRouter` otherwise. Bundled assets are served by the Tauri protocol handler with no SPA rewrite, so a `pushState` URL would 404 on reload. Hash routing keeps deep links reloadable inside the shell without changing the web build's URLs.

## Tests

| Layer | Where |
|-------|-------|
| Endpoint parsing, token shape, readiness probe | `desktop/src-tauri/src/daemon.rs` (6 tests) |
| Bootstrap payload, notification clipping | `desktop/src-tauri/src/commands.rs` (3 tests) |
| Menu script escaping | `desktop/src-tauri/src/tray.rs` (2 tests) |
| Bearer / query token, constant time compare | `src/web/auth.rs` |
| Desktop origins and allow-all preservation | `src/web/cors.rs` |
| Bridge + `wsUrl` behaviour | `web/src/desktop/bridge.test.ts`, `web/src/api/client.test.ts` |

```bash
cargo test -p synbot-desktop --lib
cargo test --lib -- web::auth:: web::cors::
cd web && npx vitest --run
```

These are unit tests. Nothing here replaces a GUI build plus a manual pass on a real machine — window policy, tray and notifications only exist at runtime.

## Build order

`frontendDist` is embedded into the executable and resolved **relative to `desktop/src-tauri/`**, so the repository-level `web/dist` is `"../../web/dist"`. A plain `../web/dist` silently points at `desktop/web/dist` and aborts the bundle with `Unable to find your web assets` — that is the failure mode a fresh clone hits first.

`beforeBuildCommand` runs `build:web && build:sidecar`, so `tauri build` produces the assets in the right order. The existence check happens at **bundle** time, not in `cargo check`: the shell crate compiles without `web/dist`, only `tauri build` refuses to continue.

Bundles land in the workspace target directory, `<repo>/target/release/bundle/` (`macos/Synbot.app`, `dmg/Synbot_{version}_aarch64.dmg`), not under `desktop/src-tauri/target/`. Local packages are ad-hoc signed (no Team ID), which is enough for the building machine but not for distribution.

## Not covered

Auto-update and deep links are **not** implemented. Both need signed release artifacts and a CI publishing path (and cannot be validated without a real build), so they stay out of scope; the app still ships as a normal bundle you replace manually. Distribution signing (Developer ID + notarization) is likewise still a release task, not a build step.
