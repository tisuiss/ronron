// Ronron runs without a console window: Ronron is the whole UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ronron_lib::run()
}
