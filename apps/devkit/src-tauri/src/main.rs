#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // a copy of this exe started as the vault session agent (ws_core::vault) runs without a window
    if devkit::vault_agent() {
        return;
    }
    devkit::run();
}
