//! Tray icon, tray menu and the window-close policy.
//!
//! Tray labels stay in English on purpose: the shell has no access to the UI locale without
//! reading `~/.synbot/config.json` again and the tray is a shortcut, not a surface.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use crate::daemon;

pub const MENU_SHOW: &str = "tray-show";
pub const MENU_OPEN_PROJECT: &str = "tray-open-project";
pub const MENU_QUIT: &str = "tray-quit";

/// DOM event the bundled UI listens for (`web/src/desktop/bridge.ts`).
pub const OPEN_PROJECT_EVENT: &str = "synbot:open-project";

/// Set once a tray icon exists. Without it, parking the window would leave no way back.
static TRAY_READY: AtomicBool = AtomicBool::new(false);

/// The "still running" notice is worth showing once per launch, not on every close.
static PARKED_NOTICE_SHOWN: AtomicBool = AtomicBool::new(false);

pub fn tray_ready() -> bool {
    TRAY_READY.load(Ordering::Relaxed)
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = daemon::main_window(app) else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Evaluates a dispatch script in the page. The UI turns it into a normal DOM event, so the
/// bridge needs no `@tauri-apps/api` dependency.
pub fn open_project_script(path: &str) -> String {
    let payload = serde_json::to_string(path).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "window.dispatchEvent(new CustomEvent('{OPEN_PROJECT_EVENT}', {{ detail: {payload} }}));"
    )
}

fn dispatch_open_project(app: &AppHandle, path: &str) {
    if let Some(window) = daemon::main_window(app) {
        let _ = window.eval(open_project_script(path));
    }
}

fn pick_project_folder(app: &AppHandle) {
    use tauri_plugin_dialog::DialogExt;

    let app = app.clone();
    app.dialog()
        .file()
        .set_title("Choose a workspace folder")
        .pick_folder(move |picked| {
            let Some(path) = picked.and_then(|file| file.into_path().ok()) else {
                return;
            };
            dispatch_open_project(&app, &path.to_string_lossy());
        });
}

/// Builds the tray. Failure is not fatal: a machine without a status-bar/indicator backend still
/// gets a usable app, it just never hides its window on close.
pub fn install(app: &AppHandle) {
    let show = MenuItem::with_id(app, MENU_SHOW, "Show Synbot", true, None::<&str>);
    let open = MenuItem::with_id(app, MENU_OPEN_PROJECT, "Add Workspace…", true, None::<&str>);
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit Synbot", true, None::<&str>);
    let (Ok(show), Ok(open), Ok(quit)) = (show, open, quit) else {
        eprintln!("[synbot-desktop] failed to build the tray menu");
        return;
    };
    let Ok(separator) = PredefinedMenuItem::separator(app) else {
        return;
    };
    let Ok(menu) = Menu::with_items(app, &[&show, &open, &separator, &quit]) else {
        eprintln!("[synbot-desktop] failed to build the tray menu");
        return;
    };

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("Synbot")
        // Left click restores the window; the menu is right-click only.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_OPEN_PROJECT => pick_project_folder(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    match app.default_window_icon() {
        Some(icon) => builder = builder.icon(icon.clone()),
        // An icon-less status item is invisible on macOS, which would make "close to tray" a trap.
        None => eprintln!("[synbot-desktop] no window icon; the tray item may be invisible"),
    }

    match builder.build(app) {
        Ok(_) => TRAY_READY.store(true, Ordering::Relaxed),
        Err(error) => eprintln!("[synbot-desktop] no system tray available: {error}"),
    }
}

/// What the window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Quit the app (and the daemon with it). The default, and what the close button means
    /// everywhere else in the OS.
    Quit,
    /// Hide the window and keep the daemon running; the tray and the Dock icon bring it back.
    Park,
}

/// Parking is opt-in (`desktop.closeToTray`) and needs a tray to come back from. Without one,
/// hiding would leave a windowless process with no way to restore it.
pub fn close_action(close_to_tray: bool, tray_ready: bool) -> CloseAction {
    if close_to_tray && tray_ready {
        CloseAction::Park
    } else {
        CloseAction::Quit
    }
}

fn notice_parked(app: &AppHandle) {
    if PARKED_NOTICE_SHOWN.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = app
        .notification()
        .builder()
        .title("Synbot is still running")
        .body("Click the menu bar icon or the Dock icon to reopen it. Quit Synbot stops it.")
        .show();
}

/// Window policy for the main window.
///
/// Closing quits by default: the daemon owns channels, cron and the agent runtime, but a window
/// that vanishes while the process lives — and that macOS Dock clicks could not restore — reads
/// as a hang. Users who want the assistant to keep running in the background opt in through
/// `desktop.closeToTray`, and then get three ways back: the tray menu, the menu bar icon and the
/// Dock icon (`RunEvent::Reopen` in `lib.rs`).
pub fn install_close_handler(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        let tauri::WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        // The setting is read per close, so saving it in the UI needs no restart.
        match close_action(daemon::close_to_tray(), tray_ready()) {
            CloseAction::Quit => {
                // macOS keeps a windowless app alive by default; exit explicitly so the close
                // button always means "quit", and `RunEvent::Exit` still kills the sidecar.
                api.prevent_close();
                handle.app_handle().exit(0);
            }
            CloseAction::Park => {
                api.prevent_close();
                let _ = handle.hide();
                notice_parked(handle.app_handle());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_parks_only_when_opted_in_and_a_tray_exists() {
        assert_eq!(close_action(false, true), CloseAction::Quit);
        assert_eq!(close_action(false, false), CloseAction::Quit);
        assert_eq!(close_action(true, true), CloseAction::Park);
        // Opted in but the tray failed to build: quitting beats a window that cannot come back.
        assert_eq!(close_action(true, false), CloseAction::Quit);
    }

    #[test]
    fn open_project_script_quotes_the_path() {
        let script = open_project_script("/tmp/some project");
        assert!(script.contains("CustomEvent('synbot:open-project'"));
        assert!(script.contains("\"/tmp/some project\""));
    }

    #[test]
    fn open_project_script_escapes_quotes_and_backslashes() {
        let script = open_project_script("C:\\work\\it's \"mine\"");
        // `serde_json` does the escaping, so the payload stays a valid string literal.
        assert!(script.contains(r#""C:\\work\\it's \"mine\"""#));
        assert!(script.ends_with(");"));
    }
}
