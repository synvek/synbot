//! Tray icon, tray menu and the "close hides, tray restores" behaviour.
//!
//! Tray labels stay in English on purpose: the shell has no access to the UI locale without
//! reading `~/.synbot/config.json` again and the tray is a shortcut, not a surface.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, WebviewWindow};

use crate::daemon;

pub const MENU_SHOW: &str = "tray-show";
pub const MENU_OPEN_PROJECT: &str = "tray-open-project";
pub const MENU_QUIT: &str = "tray-quit";

/// DOM event the bundled UI listens for (`web/src/desktop/bridge.ts`).
pub const OPEN_PROJECT_EVENT: &str = "synbot:open-project";

/// Set once a tray icon exists. Without it, closing the window would hide the app with no way back.
static TRAY_READY: AtomicBool = AtomicBool::new(false);

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
        .set_title("Open project folder")
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
    let open = MenuItem::with_id(app, MENU_OPEN_PROJECT, "Open Project…", true, None::<&str>);
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

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    match builder.build(app) {
        Ok(_) => TRAY_READY.store(true, Ordering::Relaxed),
        Err(error) => eprintln!("[synbot-desktop] no system tray available: {error}"),
    }
}

/// Closing the window parks the app in the tray instead of quitting, as long as a tray exists.
pub fn install_close_to_tray(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if tray_ready() {
                api.prevent_close();
                let _ = handle.hide();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
