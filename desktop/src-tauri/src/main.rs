// Prevents additional lint noise on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    synbot_desktop_lib::run();
}
