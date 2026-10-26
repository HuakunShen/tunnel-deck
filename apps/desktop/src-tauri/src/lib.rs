//! `ssh-tunnel-desktop` — the native GUI shell for the tunnel manager.
//!
//! This shell is deliberately thin: it links `tunnel-core` in-process and
//! exposes it to the Web UI through typed Tauri commands. There is no
//! sidecar, no loopback port, and no JavaScript runtime to probe for —
//! installing the app is all it takes to run it. All tunnel rules stay in
//! the core crate; no domain logic lives in this shell.

mod native_material;
mod tunnel_commands;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The embedded WebDriver server is test-only surface: production builds
    // never expose it. E2E sets `SSH_TUNNEL_WEBDRIVER=1` (see e2e/wdio.conf.ts).
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch is always a mistake (a rival manager over the
            // same ports and sockets), so surface the running window instead.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(tunnel_commands::app_state());
    let builder = if std::env::var("SSH_TUNNEL_WEBDRIVER").is_ok() {
        builder.plugin(tauri_plugin_wdio_webdriver::init())
    } else {
        builder
    };
    builder
        .setup(|app| {
            // Build the window by hand: the hidden title bar needs its drag
            // region script installed before the document loads, and the
            // native material has to go in before anything is shown.
            // A material failure must not cost the user their window: fall
            // back to Tauri's own creation, which still works, just opaque.
            if let Err(error) = native_material::create_main_window(app) {
                eprintln!("native material unavailable ({error}); using a plain window");
                let window = tauri::WebviewWindowBuilder::from_config(
                    app,
                    app.config().app.windows.first().expect("main window configuration"),
                )?;
                let _ = window;
            }
            // The manager is in-process, so there is nothing to wait for: show
            // the window as soon as the webview exists.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tunnel_commands::tunnel_health,
            tunnel_commands::tunnel_list_profiles,
            tunnel_commands::tunnel_create_profile,
            tunnel_commands::tunnel_get_profile,
            tunnel_commands::tunnel_update_profile,
            tunnel_commands::tunnel_delete_profile,
            tunnel_commands::tunnel_list_states,
            tunnel_commands::tunnel_get_state,
            tunnel_commands::tunnel_preview,
            tunnel_commands::tunnel_start,
            tunnel_commands::tunnel_stop,
            tunnel_commands::tunnel_stop_all,
            tunnel_commands::tunnel_probe,
            tunnel_commands::tunnel_log_tail,
            tunnel_commands::tunnel_list_hosts,
            tunnel_commands::tunnel_list_terminals,
            tunnel_commands::tunnel_launch_in_terminal,
            tunnel_commands::tunnel_close_session,
            tunnel_commands::tunnel_open_url,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ssh-tunnel desktop shell");
}
