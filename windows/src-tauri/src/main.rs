// Ronron runs without a console window: Ronron is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The uninstaller, when asked to delete the application data (nsis/hooks.nsh).
    if std::env::args().skip(1).any(|a| a == "--forget-everything") {
        ronron_lib::forget_everything();
        return;
    }
    ronron_lib::run()
}
