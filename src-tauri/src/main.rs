// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Headless subcommands run before Tauri starts so they never open a window.
    match args.get(1).map(String::as_str) {
        Some("git-credential") => std::process::exit(session_relay_lib::git_credential::run(args.get(2).map(String::as_str))),
        Some("hook-save") => std::process::exit(session_relay_lib::hook::save()),
        Some("hook-worker") => std::process::exit(session_relay_lib::hook_worker::worker(&args)),
        Some("hook-uninstall") => std::process::exit(session_relay_lib::hook::uninstall()),
        Some("post-install") => std::process::exit(session_relay_lib::hook::post_install()),
        _ => session_relay_lib::run(),
    }
}
