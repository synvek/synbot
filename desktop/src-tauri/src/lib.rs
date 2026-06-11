use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

#[cfg(not(debug_assertions))]
const SYNBOT_HOST: &str = "127.0.0.1";
#[cfg(not(debug_assertions))]
const SYNBOT_PORT: u16 = 18888;
#[cfg(not(debug_assertions))]
const DASHBOARD_URL: &str = "http://127.0.0.1:18888";
#[cfg(not(debug_assertions))]
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(not(debug_assertions))]
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);

struct SidecarState {
    child: Mutex<Option<CommandChild>>,
}

impl SidecarState {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    #[cfg(not(debug_assertions))]
    fn set_child(&self, child: CommandChild) {
        *self.child.lock().expect("sidecar lock poisoned") = Some(child);
    }

    #[allow(dead_code)]
    fn kill(&self) {
        if let Some(child) = self
            .child
            .lock()
            .expect("sidecar lock poisoned")
            .take()
        {
            let _ = child.kill();
        }
    }
}

#[cfg(not(debug_assertions))]
fn synbot_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".synbot")
        .join("config.json")
}

#[cfg(not(debug_assertions))]
fn synbot_config_exists() -> bool {
    synbot_config_path().exists()
}

#[cfg(not(debug_assertions))]
fn is_synbot_port_open() -> bool {
    let addr: SocketAddr = format!("{SYNBOT_HOST}:{SYNBOT_PORT}")
        .parse()
        .expect("valid socket addr");
    TcpStream::connect_timeout(&addr, Duration::from_secs(1)).is_ok()
}

#[cfg(not(debug_assertions))]
async fn wait_for_synbot() -> Result<(), String> {
    let deadline = Instant::now() + HEALTH_TIMEOUT;
    while Instant::now() < deadline {
        if is_synbot_port_open() {
            return Ok(());
        }
        tokio::time::sleep(HEALTH_INTERVAL).await;
    }
    Err(format!(
        "Timed out waiting for synbot on {SYNBOT_HOST}:{SYNBOT_PORT}. \
         The port may be in use or synbot failed to start."
    ))
}

#[cfg(not(debug_assertions))]
fn set_loading_status(window: &tauri::WebviewWindow, message: &str, is_error: bool) {
    let escaped = message
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n");
    let spinner = if is_error { "none" } else { "block" };
    let class = if is_error { " error" } else { "" };
    let script = format!(
        "document.getElementById('spinner').style.display='{spinner}';\
         document.getElementById('status').className='{class}';\
         document.getElementById('status').textContent='{escaped}';"
    );
    let _ = window.eval(&script);
}

#[cfg(not(debug_assertions))]
async fn run_sidecar_command(app: &AppHandle, args: &[&str]) -> Result<(), String> {
    let sidecar = app
        .shell()
        .sidecar("synbot")
        .map_err(|e| format!("Failed to resolve synbot sidecar: {e}"))?
        .args(args)
        .env("SYNBOT_DESKTOP", "1");

    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Failed to spawn synbot sidecar: {e}"))?;

    let mut exit_code: Option<i32> = None;
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code;
                break;
            }
            CommandEvent::Error(err) => {
                return Err(format!("Sidecar error: {err}"));
            }
            CommandEvent::Stdout(line) | CommandEvent::Stderr(line) => {
                let _ = String::from_utf8_lossy(&line);
            }
            _ => {}
        }
    }

    let _ = child.kill();

    match exit_code {
        Some(0) | None => Ok(()),
        Some(code) => Err(format!(
            "synbot {} exited with code {code}",
            args.join(" ")
        )),
    }
}

#[cfg(not(debug_assertions))]
async fn ensure_onboarded(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    if synbot_config_exists() {
        return Ok(());
    }

    set_loading_status(window, "First run: initializing synbot workspace…", false);
    run_sidecar_command(app, &["onboard"]).await
}

#[cfg(not(debug_assertions))]
async fn start_sidecar_daemon(
    app: &AppHandle,
    sidecar_state: &SidecarState,
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    if is_synbot_port_open() {
        return Ok(());
    }

    set_loading_status(window, "Starting synbot daemon…", false);

    let sidecar = app
        .shell()
        .sidecar("synbot")
        .map_err(|e| format!("Failed to resolve synbot sidecar: {e}"))?
        .args(["start"])
        .env("SYNBOT_DESKTOP", "1");

    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Failed to spawn synbot start: {e}"))?;

    sidecar_state.set_child(child);

    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if matches!(event, CommandEvent::Terminated(_)) {
                break;
            }
        }
    });

    wait_for_synbot().await
}

#[cfg(not(debug_assertions))]
fn navigate_to_dashboard(window: &tauri::WebviewWindow) -> Result<(), String> {
    let url: url::Url = DASHBOARD_URL
        .parse()
        .map_err(|e| format!("Invalid dashboard URL: {e}"))?;
    window
        .navigate(url)
        .map_err(|e| format!("Failed to open dashboard: {e}"))
}

#[cfg(not(debug_assertions))]
async fn bootstrap_production(app: AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => return,
    };

    let sidecar_state = app.state::<SidecarState>();

    if let Err(err) = ensure_onboarded(&app, &window).await {
        set_loading_status(&window, &err, true);
        return;
    }

    if let Err(err) = start_sidecar_daemon(&app, sidecar_state.inner(), &window).await {
        set_loading_status(&window, &err, true);
        return;
    }

    if let Err(err) = navigate_to_dashboard(&window) {
        set_loading_status(&window, &err, true);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(SidecarState::new())
        .setup(|_app| {
            #[cfg(not(debug_assertions))]
            {
                let handle = _app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    bootstrap_production(handle).await;
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                if let Some(state) = app.try_state::<SidecarState>() {
                    state.kill();
                }
            }
        });
}
