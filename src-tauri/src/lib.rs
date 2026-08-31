mod commands;
mod firewall;
mod net;
mod qr;
mod reachability;
mod safety;
mod server;
mod server_manager;
mod session;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, WindowEvent};

use commands::{TauriState, watch_sessions};
use session::SessionRegistry;

/// The user's Downloads folder (typical layout). Windows uses %USERPROFILE%,
/// Linux uses $HOME. The app never writes outside Downloads + its own
/// app-data dir.
fn known_downloads_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(|home| PathBuf::from(home).join("Downloads"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join("Downloads"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("aerlynk"));
            // Fall back to temp if we cannot find Downloads (e.g. headless CI);
            // the real path is resolved to a per-user folder on normal installs.
            let downloads_dir = known_downloads_dir().unwrap_or_else(|| data_dir.join("Downloads"));
            let quarantine_dir = data_dir.join("quarantine");
            std::fs::create_dir_all(&downloads_dir).ok();
            std::fs::create_dir_all(&quarantine_dir).ok();

            let registry = Arc::new(SessionRegistry::new(session::DEFAULT_INACTIVITY_TIMEOUT));
            // SEC-7: bind to the discovered LAN address (never 0.0.0.0). If no
            // usable address exists we fall back to loopback so the app still
            // launches; the QR warning surfaces via the network checks.
            let bind = net::bind_addr().unwrap_or_else(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
            let manager = Arc::new(server_manager::ServerManager::new(
                registry.clone(),
                downloads_dir.clone(),
                quarantine_dir.clone(),
                bind,
            ));

            app.manage(TauriState {
                registry: registry.clone(),
                manager: manager.clone(),
                downloads_dir,
                quarantine_dir,
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                watch_sessions(handle, registry, manager).await;
            });

            #[cfg(desktop)]
            {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_focus();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_send_session,
            commands::start_receive_session,
            commands::end_session,
            commands::cancel_transfer,
            commands::set_preferred_port,
            commands::get_downloads_dir,
            commands::open_folder,
            commands::get_network_info,
            commands::verify_reachability,
            commands::check_firewall,
            commands::repair_firewall,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running aerlynk");
}