//! Tauri command layer — the glue between the React UI and the Rust core.
//!
//! Every IPC call the frontend makes resolves here or fails loudly instead of
//! silently falling back to mock data (the v0.1.0 bug B1). Session lifecycle
//! (start/end/cancel), QR rendering, network info, reachability, and firewall
//! checks all live behind these commands. Progress and lifecycle events are
//! emitted by `watch_sessions`, spawned from `run()` in lib.rs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::firewall::{self, FirewallStatus};
use crate::net;
use crate::qr;
use crate::reachability::{self, ReachReport};
use crate::server_manager::ServerManager;
use crate::session::{FileStatus, Session, SessionKind, SessionRegistry};

/// Shared managed state for all commands.
pub struct TauriState {
    pub registry: Arc<SessionRegistry>,
    pub manager: Arc<ServerManager>,
    pub downloads_dir: PathBuf,
    pub quarantine_dir: PathBuf,
}

/// Result of starting a send or receive session.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartResult {
    pub token: String,
    pub display_code: String,
    /// Full session URL encoded into the QR (the AES key lives in the fragment).
    pub url: String,
    /// Ready-to-render SVG QR for the desktop label face.
    pub qr_svg: String,
    pub files: Vec<PhoneFileInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneFileInfo {
    pub id: u32,
    pub original_name: String,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfoResult {
    pub lan_ip: String,
    pub interface_name: String,
    pub port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallStatusResult {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<String>>,
}

/// The URL a phone opens: `http://<lan-ip>:<port>/<s|r>/<token>#<key>`.
/// The fragment carries the AES key and is never sent to the server (SEC-13).
fn session_url(addr: std::net::SocketAddr, kind: &str, token: &str, key: &[u8; 32]) -> String {
    let key_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
    format!("http://{}:{}/{}/{}#{}", addr.ip(), addr.port(), kind, token, key_b64)
}

fn assemble_result(session: &Session, addr: std::net::SocketAddr) -> Result<SessionStartResult, String> {
    let kind = if session.kind == SessionKind::Send { "s" } else { "r" };
    let url = session_url(addr, kind, &session.token, &session.key);
    let svg = qr::session_url_svg(&url).map_err(|e| format!("failed to render QR: {e}"))?;
    let files = session
        .files()
        .into_iter()
        .map(|f| PhoneFileInfo {
            id: f.id,
            original_name: f.original_name,
            size: f.size,
        })
        .collect();
    Ok(SessionStartResult {
        token: session.token.clone(),
        display_code: session.display_code.clone(),
        url,
        qr_svg: svg,
        files,
    })
}

/// Ensure the phone-facing server is bound; returns the actually-bound address.
async fn ensure_server(state: &TauriState) -> Result<std::net::SocketAddr, String> {
    state
        .manager
        .ensure_running()
        .await
        .map_err(|e| format!("server failed to start: {e}"))
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_send_session(
    state: State<'_, TauriState>,
    app: AppHandle,
    paths: Vec<String>,
) -> Result<SessionStartResult, String> {
    // Paths come from the OS (drag-drop or the file picker), so the backend
    // can read real file names and sizes — JS File objects cannot cross IPC.
    let mut files = Vec::with_capacity(paths.len());
    for path in &paths {
        let p = PathBuf::from(path);
        let meta = std::fs::metadata(&p).map_err(|e| format!("cannot read {path}: {e}"))?;
        if !meta.is_file() {
            continue;
        }
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .ok_or_else(|| format!("invalid file name in {path}"))?;
        files.push((name, meta.len(), p));
    }
    if files.is_empty() {
        return Err("no readable files selected".into());
    }

    let session = state.registry.create_send_session(files);
    let addr = ensure_server(&state).await?;
    let result = assemble_result(&session, addr)?;
    let _ = app.emit(
        "session-opened",
        serde_json::json!({ "token": session.token.clone(), "displayCode": session.display_code.clone() }),
    );
    Ok(result)
}

#[tauri::command]
pub async fn start_receive_session(
    state: State<'_, TauriState>,
    app: AppHandle,
) -> Result<SessionStartResult, String> {
    let session = state.registry.create_receive_session();
    let addr = ensure_server(&state).await?;
    let result = assemble_result(&session, addr)?;
    let _ = app.emit(
        "session-opened",
        serde_json::json!({ "token": session.token.clone(), "displayCode": session.display_code.clone() }),
    );
    Ok(result)
}

#[tauri::command]
pub async fn end_session(state: State<'_, TauriState>, token: String) -> Result<(), String> {
    if let Some(s) = state.registry.find_by_token(&token) {
        state.registry.remove(&s);
    }
    let _ = state.manager.shutdown_if_idle().await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_transfer(state: State<'_, TauriState>, token: String) -> Result<(), String> {
    if let Some(s) = state.registry.find_by_token(&token) {
        s.cancel();
    }
    Ok(())
}

/// Set the preferred phone-facing port (0 = OS-assigned ephemeral). If the
/// server is idle it is shut down so the next session rebinds on the new
/// port; live sessions keep their current port until they end.
#[tauri::command]
pub async fn set_preferred_port(state: State<'_, TauriState>, port: u16) -> Result<(), String> {
    state.manager.set_preferred_port(port);
    let _ = state.manager.shutdown_if_idle().await;
    Ok(())
}

/// The folder where received files are saved — opened after a receive
/// completes and shown in Settings.
#[tauri::command]
pub fn get_downloads_dir(state: State<'_, TauriState>) -> Result<String, String> {
    Ok(state.downloads_dir.to_string_lossy().into_owned())
}

/// Open a folder (e.g. the Downloads directory) in the system file manager.
///
/// This intentionally calls the opener plugin's Rust API instead of the JS
/// `openPath` binding. The JS binding is gated behind the plugin's IPC path
/// scope, and an unscoped `opener:allow-open-path` permission produces an
/// empty allow list — the fs scope then denies every path (see
/// `tauri::fs::Scope::is_allowed`: "false if ... neither allowed nor
/// forbidden"), so the call returns `ForbiddenPath` and the button silently
/// did nothing. The Rust `OpenerExt::open_path` applies no such ACL.
#[tauri::command]
pub fn open_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("failed to open folder: {e}"))
}

// ---------------------------------------------------------------------------
// Network / firewall
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_network_info(state: State<'_, TauriState>) -> Result<NetworkInfoResult, String> {
    let lan = net::discover_lan_ipv4().map_err(|e| e.to_string())?;
    let port = state.manager.bound_addr().map(|a| a.port()).unwrap_or(0);
    Ok(NetworkInfoResult {
        lan_ip: lan.ip.to_string(),
        interface_name: lan.name.clone(),
        port,
    })
}

/// Bind (if needed) and self-test. The returned report uses the real bound
/// address, so a stale `:0` from a quiet PC never produces a false failure.
#[tauri::command]
pub async fn verify_reachability(
    state: State<'_, TauriState>,
    addr: String,
    interface_name: String,
) -> Result<ReachReport, String> {
    match ensure_server(&state).await {
        Ok(bound) => Ok(reachability::verify(bound, &interface_name)),
        Err(msg) => Ok(ReachReport::bind_failed(interface_name.clone(), format!("{msg} ({addr})"))),
    }
}

#[tauri::command]
pub fn check_firewall() -> FirewallStatusResult {
    match firewall::status() {
        FirewallStatus::Clean => FirewallStatusResult {
            status: "clean",
            blocks: None,
        },
        FirewallStatus::MissingAllowRule => FirewallStatusResult {
            status: "missingAllowRule",
            blocks: None,
        },
        FirewallStatus::StaleBlockRules { names } => FirewallStatusResult {
            status: "staleBlockRules",
            blocks: Some(names),
        },
        FirewallStatus::Unknown => FirewallStatusResult {
            status: "unknown",
            blocks: None,
        },
    }
}

#[tauri::command]
pub fn repair_firewall() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    firewall::repair(exe.to_str().unwrap_or("")).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Session watcher — turns registry state into UI events
// ---------------------------------------------------------------------------

/// Background loop that emits `transfer-progress`, `session-delivered`, and
/// `session-cancelled`. Runs for the lifetime of the app.
pub async fn watch_sessions(handle: AppHandle, registry: Arc<SessionRegistry>, manager: Arc<ServerManager>) {
    let mut prev: HashMap<u64, (u64, std::time::Instant)> = HashMap::new();
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let now = std::time::Instant::now();
        let live: std::collections::HashSet<u64> = registry.snapshot().iter().map(|s| s.id).collect();
        prev.retain(|id, _| live.contains(id));

        let mut expired = Vec::new();
        for s in registry.snapshot() {
            if s.is_cancelled() {
                let token = s.token.clone();
                registry.remove(&s);
                let _ = manager.shutdown_if_idle().await;
                let _ = handle.emit("session-cancelled", serde_json::json!({ "token": token }));
                continue;
            }
            if s.is_expired(now, registry.timeout()) {
                expired.push((s.id, s.token.clone()));
                continue;
            }
            let files = s.files();
            if files.is_empty() {
                continue;
            }
            let total: u64 = files.iter().map(|f| f.size).sum();
            let sent: u64 = files.iter().map(|f| f.sent_bytes.load(Ordering::Relaxed)).sum();

            if total > 0 && files.iter().all(|f| *f.status.lock().unwrap() == FileStatus::Done) {
                let token = s.token.clone();
                let names: Vec<String> = files.iter().map(|f| f.original_name.clone()).collect();
                let _ = handle.emit(
                    "session-delivered",
                    serde_json::json!({ "token": token, "files": names, "bytes": total }),
                );
                registry.remove(&s);
                let _ = manager.shutdown_if_idle().await;
                continue;
            }
            let speed = match prev.get(&s.id) {
                Some(&(ps, ts)) => {
                    let secs = now.duration_since(ts).as_secs_f64().max(0.001);
                    (sent.saturating_sub(ps)) as f64 / secs
                }
                None => 0.0,
            };
            prev.insert(s.id, (sent, now));
            let file_list: Vec<serde_json::Value> = files.iter().map(|f| {
                let status = match *f.status.lock().unwrap() {
                    FileStatus::Pending => "pending",
                    FileStatus::Active => "active",
                    FileStatus::Done => "done",
                    FileStatus::Failed => "failed",
                };
                serde_json::json!({
                    "id": f.id,
                    "sentBytes": f.sent_bytes.load(Ordering::Relaxed),
                    "status": status,
                })
            }).collect();
            let _ = handle.emit(
                "transfer-progress",
                serde_json::json!({
                    "token": s.token,
                    "sentBytes": sent,
                    "totalBytes": total,
                    "files": file_list,
                    "transferSpeed": speed as u64,
                }),
            );
        }
        for (id, token) in &expired {
            let _ = handle.emit("session-cancelled", serde_json::json!({ "token": token }));
            registry.remove_by_id(*id);
        }
        if !expired.is_empty() {
            let _ = manager.shutdown_if_idle().await;
        }
    }
}