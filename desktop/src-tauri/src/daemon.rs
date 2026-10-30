//! Where the `synbot` sidecar lives and how the shell reaches it.
//!
//! The desktop app keeps running the headless daemon — it owns channels, cron, sessions and the
//! agent runtime — and only replaces the *transport to the UI*: the window loads bundled assets
//! and calls the loopback API directly instead of being navigated to the daemon's dashboard.
//! Everything in this module is deterministic (no Tauri types except the spawn helpers) so the
//! address bookkeeping can be unit tested without a display.

use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

/// Set by the shell when it spawns the daemon; the daemon accepts it as a bearer token so the
/// user's web password never has to travel through the desktop UI.
pub const DESKTOP_TOKEN_ENV: &str = "SYNBOT_DESKTOP_TOKEN";
/// Legacy flag the daemon already used to force `web.enabled = true`.
pub const DESKTOP_FLAG_ENV: &str = "SYNBOT_DESKTOP";
/// `web.port` default, also the fallback when the config cannot be read.
pub const DEFAULT_PORT: u16 = 18888;

const LOOPBACK: &str = "127.0.0.1";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(700);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// `synbot start` has to open the database, channels and the web server; first runs on a cold
/// cache regularly take tens of seconds.
pub const START_TIMEOUT: Duration = Duration::from_secs(60);

/// Address the bundled UI should dial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// `127.0.0.1:18888` — `host:port` for URLs, with IPv6 literals bracketed.
    pub fn authority(&self) -> String {
        if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.authority())
    }

    pub fn ws_base(&self) -> String {
        format!("ws://{}", self.authority())
    }

    pub fn socket_addr(&self) -> Option<SocketAddr> {
        // `authority()` already brackets IPv6 literals, which is exactly what `SocketAddr` wants.
        self.authority().parse().ok()
    }
}

#[derive(Debug, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    web: Option<WebSection>,
}

#[derive(Debug, Deserialize, Default)]
struct WebSection {
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    host: Option<String>,
}

/// A bind address such as `0.0.0.0` or `::` is not dialable, so the shell maps wildcards back to
/// loopback. A concrete bind IP (say `192.168.1.5`) is kept, because that is where the daemon listens.
fn dial_host(configured: Option<&str>) -> String {
    match configured.map(str::trim).filter(|host| !host.is_empty()) {
        None | Some("0.0.0.0") | Some("::") | Some("[::]") => LOOPBACK.to_string(),
        Some(host) => host.trim_matches(['[', ']']).to_string(),
    }
}

/// Reads `web.port`/`web.host` out of `~/.synbot/config.json`.
///
/// A missing or broken config is not an error here: the window still has to open so the daemon
/// can report *why* it failed to start.
pub fn endpoint_from_config(json: &str) -> Endpoint {
    let parsed: ConfigFile = serde_json::from_str(json).unwrap_or_default();
    let web = parsed.web.unwrap_or_default();
    let port = web.port.filter(|port| *port != 0).unwrap_or(DEFAULT_PORT);
    Endpoint::new(dial_host(web.host.as_deref()), port)
}

pub fn endpoint_from_file(path: &Path) -> Endpoint {
    std::fs::read_to_string(path)
        .map(|json| endpoint_from_config(&json))
        .unwrap_or_else(|_| Endpoint::new(LOOPBACK, DEFAULT_PORT))
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".synbot")
        .join("config.json")
}

pub fn config_exists() -> bool {
    config_path().exists()
}

/// Per-launch secret: 64 hex characters of UUID randomness. It is only ever handed to the sidecar
/// through its environment and to the bundled page through IPC, so it never lands on disk.
pub fn new_desktop_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn is_port_open(endpoint: &Endpoint) -> bool {
    match endpoint.socket_addr() {
        Some(addr) => TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok(),
        None => false,
    }
}

/// Polls TCP until the daemon listens. A closed port is a good enough liveness signal here: the
/// web server is the last thing `synbot start` brings up.
pub async fn wait_until_ready(endpoint: &Endpoint, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if is_port_open(endpoint) {
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err(format!(
        "Timed out waiting for synbot on {}. The port may be in use or the daemon failed to start.",
        endpoint.authority()
    ))
}

/// Owns the spawned `synbot start` child so it can be killed when the app exits.
#[derive(Default)]
pub struct DaemonProcess {
    child: Mutex<Option<CommandChild>>,
}

impl DaemonProcess {
    pub fn record(&self, child: CommandChild) {
        if let Some(previous) = self
            .child
            .lock()
            .expect("daemon lock poisoned")
            .replace(child)
        {
            // Replacing a child means a stale process from a previous bootstrap; do not leak it.
            let _ = previous.kill();
        }
    }

    pub fn kill(&self) {
        if let Some(child) = self.child.lock().expect("daemon lock poisoned").take() {
            let _ = child.kill();
        }
    }
}

fn sidecar_command(
    app: &AppHandle,
    args: &[&str],
    token: Option<&str>,
) -> Result<tauri_plugin_shell::process::Command, String> {
    let mut command = app
        .shell()
        .sidecar("synbot")
        .map_err(|error| format!("Failed to resolve the synbot sidecar: {error}"))?
        .args(args)
        .env(DESKTOP_FLAG_ENV, "1");
    if let Some(token) = token {
        command = command.env(DESKTOP_TOKEN_ENV, token);
    }
    Ok(command)
}

/// Runs a one-shot sidecar command (`synbot onboard`) and waits for it to exit.
pub async fn run_sidecar_once(app: &AppHandle, args: &[&str]) -> Result<(), String> {
    let (mut events, child) = sidecar_command(app, args, None)?
        .spawn()
        .map_err(|error| format!("Failed to spawn `synbot {}`: {error}", args.join(" ")))?;

    let mut exit_code: Option<i32> = None;
    while let Some(event) = events.recv().await {
        match event {
            CommandEvent::Terminated(payload) => {
                exit_code = payload.code;
                break;
            }
            CommandEvent::Error(error) => {
                let _ = child.kill();
                return Err(format!("synbot {} failed: {error}", args.join(" ")));
            }
            _ => {}
        }
    }
    let _ = child.kill();

    match exit_code {
        Some(0) | None => Ok(()),
        Some(code) => Err(format!("synbot {} exited with code {code}", args.join(" "))),
    }
}

/// Spawns `synbot start` with the desktop token in its environment and drains its output.
pub fn spawn_daemon(app: &AppHandle, token: &str) -> Result<CommandChild, String> {
    let (mut events, child) = sidecar_command(app, &["start"], Some(token))?
        .spawn()
        .map_err(|error| format!("Failed to spawn `synbot start`: {error}"))?;

    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Terminated(_) => break,
                CommandEvent::Error(error) => {
                    eprintln!("[synbot-desktop] sidecar error: {error}");
                }
                CommandEvent::Stderr(line) => {
                    eprint!("{}", String::from_utf8_lossy(&line));
                }
                CommandEvent::Stdout(line) => {
                    print!("{}", String::from_utf8_lossy(&line));
                }
                _ => {}
            }
        }
    });

    Ok(child)
}

/// Convenience for callers that only need the manager (the tray, for instance).
pub fn main_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_defaults_when_the_config_is_missing_or_broken() {
        assert_eq!(
            endpoint_from_config(""),
            Endpoint::new("127.0.0.1", DEFAULT_PORT)
        );
        assert_eq!(
            endpoint_from_config("{ this is not json"),
            Endpoint::new("127.0.0.1", DEFAULT_PORT)
        );
        assert_eq!(
            endpoint_from_config("{}"),
            Endpoint::new("127.0.0.1", DEFAULT_PORT)
        );
    }

    #[test]
    fn endpoint_honours_the_configured_port_and_bind_host() {
        let parsed =
            endpoint_from_config(r#"{"web":{"enabled":true,"host":"192.168.1.5","port":19999}}"#);
        assert_eq!(parsed, Endpoint::new("192.168.1.5", 19999));
        assert_eq!(parsed.base_url(), "http://192.168.1.5:19999");
        assert_eq!(parsed.ws_base(), "ws://192.168.1.5:19999");
    }

    #[test]
    fn wildcard_bind_hosts_are_dialled_over_loopback() {
        for host in ["0.0.0.0", "::", "[::]", "  "] {
            let json = format!(r#"{{"web":{{"host":"{host}","port":19000}}}}"#);
            assert_eq!(
                endpoint_from_config(&json),
                Endpoint::new("127.0.0.1", 19000)
            );
        }
    }

    #[test]
    fn zero_port_falls_back_to_the_default() {
        let parsed = endpoint_from_config(r#"{"web":{"port":0}}"#);
        assert_eq!(parsed, Endpoint::new("127.0.0.1", DEFAULT_PORT));
    }

    #[test]
    fn ipv6_bind_hosts_are_bracketed_in_urls() {
        let endpoint = Endpoint::new("::1", 18888);
        assert_eq!(endpoint.authority(), "[::1]:18888");
        assert_eq!(endpoint.base_url(), "http://[::1]:18888");
        assert_eq!(endpoint.ws_base(), "ws://[::1]:18888");
        assert_eq!(endpoint.socket_addr().map(|addr| addr.port()), Some(18888));
    }

    #[test]
    fn desktop_tokens_are_hex_and_long_enough_to_be_uninteresting_to_guess() {
        let token = new_desktop_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));

        let other = new_desktop_token();
        assert_ne!(token, other);
    }
}
