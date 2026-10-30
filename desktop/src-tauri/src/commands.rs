//! IPC surface of the desktop shell.
//!
//! The bundled UI no longer guesses where the daemon lives: it asks for a bootstrap payload once
//! (address + per-launch token) and then talks to the loopback API directly. Everything else here
//! is a native capability that only the shell can provide.

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

use crate::daemon;
use crate::DesktopState;

/// Longest notification strings we forward; the OS truncates anyway and a runaway title (a whole
/// model answer) should never reach the notification centre.
const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 400;

/// What the bundled page needs before its first HTTP call.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInfo {
    /// e.g. `http://127.0.0.1:18888` — REST base URL.
    pub api_base: String,
    /// e.g. `ws://127.0.0.1:18888` — WebSocket base URL.
    pub ws_base: String,
    /// Per-launch bearer token; also accepted as `?token=` on `/ws/*` handshakes.
    pub token: String,
    pub version: String,
    pub platform: String,
}

impl BootstrapInfo {
    pub fn new(endpoint: &daemon::Endpoint, token: &str) -> Self {
        Self {
            api_base: endpoint.base_url(),
            ws_base: endpoint.ws_base(),
            token: token.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    }
}

/// Truncates on a character boundary and marks the cut, so notifications stay readable.
fn clip(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let clipped: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

/// Makes sure the daemon answers, onboarding the user and spawning the sidecar if needed.
///
/// Safe to call more than once (the UI may retry): the gate serialises concurrent bootstraps and
/// an already open port short-circuits the whole thing.
pub async fn ensure_daemon(app: &AppHandle, state: &DesktopState) -> Result<(), String> {
    let _gate = state.start_gate().lock().await;

    let endpoint = state.endpoint();
    if daemon::is_port_open(&endpoint) {
        return Ok(());
    }

    if !daemon::config_exists() {
        daemon::run_sidecar_once(app, &["onboard"]).await?;
        if daemon::is_port_open(&endpoint) {
            return Ok(());
        }
    }

    let child = daemon::spawn_daemon(app, state.token())?;
    state.record_child(child);

    daemon::wait_until_ready(&endpoint, daemon::START_TIMEOUT).await
}

/// Called once per window load by `web/src/desktop/bridge.ts`.
#[tauri::command]
pub async fn desktop_bootstrap(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<BootstrapInfo, String> {
    ensure_daemon(&app, state.inner()).await?;
    Ok(BootstrapInfo::new(&state.endpoint(), state.token()))
}

/// Native folder picker for "open project"; returns `None` when the user cancels.
#[tauri::command]
pub async fn desktop_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();

    app.dialog()
        .file()
        .set_title("Open project folder")
        .pick_folder(move |picked| {
            let _ = sender.send(picked);
        });

    let picked = receiver
        .await
        .map_err(|_| "The folder picker closed before returning a folder".to_string())?;

    Ok(picked
        .and_then(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned()))
}

/// System notification, used when a turn finishes while the window is in the background.
#[tauri::command]
pub fn desktop_notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    let title = clip(&title, MAX_TITLE_CHARS);
    if title.is_empty() {
        return Ok(());
    }
    let body = clip(&body, MAX_BODY_CHARS);

    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| format!("Failed to show notification: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_keeps_short_values_and_trims_padding() {
        assert_eq!(clip("  done  ", 32), "done");
        assert_eq!(clip("", 32), "");
    }

    #[test]
    fn clip_truncates_multibyte_text_on_a_char_boundary() {
        let clipped = clip("一二三四五六", 3);
        assert_eq!(clipped, "一二三…");
        // Still valid UTF-8 and no panic for the ellipsis budget.
        assert_eq!(clip("一二三四五六", 6), "一二三四五六");
    }

    #[test]
    fn bootstrap_payload_matches_the_configured_endpoint() {
        let info = BootstrapInfo::new(&daemon::Endpoint::new("127.0.0.1", 19000), "abc");
        assert_eq!(info.api_base, "http://127.0.0.1:19000");
        assert_eq!(info.ws_base, "ws://127.0.0.1:19000");
        assert_eq!(info.token, "abc");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
