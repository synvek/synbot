//! Desktop shell for Synbot.
//!
//! Shape of the app after this module was rewritten: the window loads the *bundled* web UI
//! (`frontendDist`) on a Tauri custom scheme, and the UI asks the shell over IPC where the
//! local daemon is and which per-launch token to use. The daemon itself is still the `synbot`
//! sidecar — it owns the agent runtime, channels, cron and the database — but it is no longer
//! the page the user looks at, which is what makes CSP, tray, notifications and the native
//! folder picker possible at all.

mod commands;
mod daemon;
mod tray;

use tauri::Manager;
use tauri::RunEvent;
use tauri_plugin_shell::process::CommandChild;

/// Shared shell state: where the daemon lives and the secret that proves the page may talk to it.
pub struct DesktopState {
    token: String,
    endpoint: daemon::Endpoint,
    daemon: daemon::DaemonProcess,
    /// Serialises bootstrap attempts so a reload while the daemon is starting does not fork it.
    start_gate: tokio::sync::Mutex<()>,
}

impl DesktopState {
    pub fn new(token: String, endpoint: daemon::Endpoint) -> Self {
        Self {
            token,
            endpoint,
            daemon: daemon::DaemonProcess::default(),
            start_gate: tokio::sync::Mutex::new(()),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn endpoint(&self) -> daemon::Endpoint {
        self.endpoint.clone()
    }

    pub fn start_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.start_gate
    }

    pub fn record_child(&self, child: CommandChild) {
        self.daemon.record(child);
    }

    pub fn kill_daemon(&self) {
        self.daemon.kill();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let endpoint = daemon::endpoint_from_file(&daemon::config_path());
    let token = daemon::new_desktop_token();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(DesktopState::new(token, endpoint))
        .invoke_handler(tauri::generate_handler![
            commands::desktop_bootstrap,
            commands::desktop_pick_folder,
            commands::desktop_notify,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tray::install(&handle);
            if let Some(window) = daemon::main_window(&handle) {
                tray::install_close_to_tray(&window);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                if let Some(state) = app.try_state::<DesktopState>() {
                    state.kill_daemon();
                }
            }
        });
}
