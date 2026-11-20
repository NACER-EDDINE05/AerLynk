//! Server lifecycle: start when the first session is created, stop when the
//! last one ends (FR-14). The phone-facing port is only ever open while a
//! session is live — nothing listens on a quiet PC.
//!
//! SEC-7: the listener binds to the specific LAN IPv4 address discovered by
//! `net`, never `0.0.0.0`, so the server is not exposed on VPN tunnels, host-
//! only adapters, or other interfaces the phone could not reach anyway.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::RwLock;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::net;
use crate::server::{app, AppState};
use crate::session::SessionRegistry;

pub struct ServerManager {
    registry: Arc<SessionRegistry>,
    /// Destination for received files. Read at bind time, so a change applies
    /// to the next session while a live transfer keeps the folder it started
    /// with (a half-delivered file never straddles two directories).
    downloads_dir: RwLock<PathBuf>,
    /// The OS Downloads folder, restored by "use default".
    default_downloads_dir: PathBuf,
    quarantine_dir: PathBuf,
    /// SEC-7: the LAN address to bind (ephemeral port). Never 0.0.0.0. It is
    /// refreshed when a stale address is detected at bind time (Wi-Fi switch).
    bind_addr: RwLock<SocketAddr>,
    /// Interface chosen in Settings (None = automatic discovery).
    preferred_ip: RwLock<Option<Ipv4Addr>>,
    /// Abort handle for the running server task, if any.
    server: tokio::sync::Mutex<Option<RunningServer>>,
    /// User-preferred phone-facing port (0 = OS-assigned ephemeral).
    preferred_port: AtomicU16,
}

struct RunningServer {
    /// The address actually bound (LAN IP + the OS-assigned port).
    addr: SocketAddr,
    abort: tokio::task::JoinHandle<()>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl ServerManager {
    pub fn new(
        registry: Arc<SessionRegistry>,
        downloads_dir: PathBuf,
        quarantine_dir: PathBuf,
        bind_addr: SocketAddr,
    ) -> Self {
        Self {
            registry,
            downloads_dir: RwLock::new(downloads_dir.clone()),
            default_downloads_dir: downloads_dir,
            quarantine_dir,
            bind_addr: RwLock::new(bind_addr),
            preferred_ip: RwLock::new(None),
            server: tokio::sync::Mutex::new(None),
            preferred_port: AtomicU16::new(0),
        }
    }

    /// The folder received files are currently written to.
    pub fn downloads_dir(&self) -> PathBuf {
        self.downloads_dir.read().unwrap().clone()
    }

    /// Point received files at `dir`, or back at the OS Downloads folder when
    /// `None`. Creates the folder when needed and returns the effective path.
    pub fn set_downloads_dir(&self, dir: Option<&std::path::Path>) -> std::io::Result<PathBuf> {
        let target = match dir {
            Some(path) => path.to_path_buf(),
            None => self.default_downloads_dir.clone(),
        };
        if !target.is_absolute() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "folder path must be absolute",
            ));
        }
        std::fs::create_dir_all(&target)?;
        *self.downloads_dir.write().unwrap() = target.clone();
        Ok(target)
    }

    /// The IPv4 address the next session will bind to: the interface chosen in
    /// Settings when it is still present on this PC, otherwise the automatically
    /// discovered one. A stale choice (unplugged adapter, left the network)
    /// degrades to automatic discovery instead of breaking the app.
    pub fn effective_ip(&self) -> Option<Ipv4Addr> {
        if let Some(chosen) = *self.preferred_ip.read().unwrap() {
            if net::has_ipv4(chosen) {
                return Some(chosen);
            }
        }
        match self.bind_addr.read().unwrap().ip() {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        }
    }

    /// The user's chosen interface, if any (may be currently unavailable).
    pub fn preferred_ip(&self) -> Option<Ipv4Addr> {
        *self.preferred_ip.read().unwrap()
    }

    /// Choose the interface sessions bind to (None = automatic). Takes effect
    /// the next time the server binds; live sessions keep their address.
    pub fn set_preferred_ip(&self, ip: Option<Ipv4Addr>) {
        *self.preferred_ip.write().unwrap() = ip;
    }

    /// Ensure the HTTP server is listening. Starts it on first use; returns
    /// the bound address (LAN IP + port). Call on every session creation.
    pub async fn ensure_running(&self) -> std::io::Result<SocketAddr> {
        let mut guard = self.server.lock().await;
        if let Some(running) = guard.as_ref() {
            return Ok(running.addr);
        }
        let state = AppState {
            registry: self.registry.clone(),
            downloads_dir: self.downloads_dir(),
            quarantine_dir: self.quarantine_dir.clone(),
        };
        let router = app(state);
        // SEC-7: bind to the specific LAN address, never INADDR_ANY. Loopback
        // still answers when the LAN address is used locally. The port is the
        // user's preferred one (Settings) when set, else OS-assigned ephemeral.
        let port = self.preferred_port.load(Ordering::Relaxed);
        let ip = self
            .effective_ip()
            .unwrap_or(Ipv4Addr::LOCALHOST);
        let listener = match TcpListener::bind(SocketAddr::new(IpAddr::V4(ip), port)).await {
            Ok(listener) => listener,
            Err(first) => {
                // The remembered address may have gone away (Wi-Fi switched,
                // hotspot joined, VPN dropped). Re-discover once before failing
                // the session: a QR on the current address beats an error.
                let rediscovered = tokio::task::spawn_blocking(net::discover_lan_ipv4)
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .map(|lan| lan.ip);
                match rediscovered {
                    Some(fresh) if fresh != ip => {
                        let listener =
                            TcpListener::bind(SocketAddr::new(IpAddr::V4(fresh), port)).await?;
                        *self.bind_addr.write().unwrap() =
                            SocketAddr::new(IpAddr::V4(fresh), 0);
                        listener
                    }
                    _ => return Err(first),
                }
            }
        };
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = server.await;
        });
        *guard = Some(RunningServer {
            addr,
            abort: task,
            shutdown: Some(tx),
        });
        Ok(addr)
    }

    /// The user's preferred phone-facing port (0 = OS-assigned ephemeral).
    pub fn preferred_port(&self) -> u16 {
        self.preferred_port.load(Ordering::Relaxed)
    }

    /// Set the preferred phone-facing port (0 = automatic). Takes effect the
    /// next time the server binds — live sessions keep their current port.
    pub fn set_preferred_port(&self, port: u16) {
        self.preferred_port.store(port, Ordering::Relaxed);
    }

    /// The address the server is bound to, if it is running.
    pub fn bound_addr(&self) -> Option<SocketAddr> {
        let guard = self.server.try_lock();
        guard.ok().and_then(|g| g.as_ref().map(|r| r.addr))
    }

    /// Stop the server when the registry is empty (FR-14). Returns true when
    /// a server was actually stopped.
    pub async fn shutdown_if_idle(&self) -> bool {
        let mut guard = self.server.lock().await;
        if self.registry.is_empty() {
            if let Some(running) = guard.take() {
                if let Some(tx) = running.shutdown {
                    let _ = tx.send(());
                }
                let _ = running.abort.await;
                return true;
            }
        }
        false
    }

    /// Force-stop (app exit). Idempotent.
    pub async fn shutdown(&self) {
        let mut guard = self.server.lock().await;
        if let Some(running) = guard.take() {
            if let Some(tx) = running.shutdown {
                let _ = tx.send(());
            }
            let _ = running.abort.await;
        }
    }

    pub fn is_running(&self) -> bool {
        // Best-effort sync read for UI state; the lock is short.
        let guard = self.server.try_lock();
        guard.map(|g| g.is_some()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::DEFAULT_INACTIVITY_TIMEOUT;

    /// Tests bind to loopback: they exercise lifecycle, not binding security.
    /// SEC-7 is verified by `binds_to_the_requested_address`.
    fn loopback_bind() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    fn tmp_dirs(tag: &str) -> (PathBuf, PathBuf) {
        let pid = std::process::id();
        let d = std::env::temp_dir().join(format!("aerlynk-mgr-{tag}-{pid}"));
        let q = std::env::temp_dir().join(format!("aerlynk-mgr-q-{tag}-{pid}"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::create_dir_all(&q).unwrap();
        (d, q)
    }

    #[tokio::test]
    async fn starts_on_first_session_and_stops_with_last() {
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("lifecycle");
        let mgr = ServerManager::new(registry.clone(), dl, q, loopback_bind());

        assert!(!mgr.is_running());
        let addr = mgr.ensure_running().await.unwrap();
        assert!(mgr.is_running());

        // Real socket: the server answers on the bound port.
        let session = registry.create_receive_session();
        let token = session.token.clone();
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(
                format!(
                    "GET /r/{token} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let head = String::from_utf8_lossy(&buf);
        assert!(head.starts_with("HTTP/1.1 200"), "got: {head}");

        // Registry empty -> idle shutdown.
        assert!(registry.remove(&session));
        assert!(mgr.shutdown_if_idle().await);
        assert!(!mgr.is_running());

        // Idempotent: second shutdown is a no-op.
        assert!(!mgr.shutdown_if_idle().await);
    }

    #[tokio::test]
    async fn stays_up_while_sessions_exist() {
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("stayup");
        let mgr = ServerManager::new(registry.clone(), dl, q, loopback_bind());
        let _ = mgr.ensure_running().await.unwrap();
        let s1 = registry.create_receive_session();
        let s2 = registry.create_receive_session();
        assert!(!mgr.shutdown_if_idle().await, "must stay up while sessions live");
        registry.remove(&s1);
        assert!(!mgr.shutdown_if_idle().await, "one session still live");
        registry.remove(&s2);
        assert!(mgr.shutdown_if_idle().await);
    }

    #[tokio::test]
    async fn ensure_running_is_idempotent() {
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("idem");
        let mgr = ServerManager::new(registry.clone(), dl, q, loopback_bind());
        let a1 = mgr.ensure_running().await.unwrap();
        let a2 = mgr.ensure_running().await.unwrap();
        assert_eq!(a1, a2, "second call must not rebind");
        assert_eq!(mgr.bound_addr(), Some(a1));
        mgr.shutdown().await;
        assert!(!mgr.is_running());
        assert_eq!(mgr.bound_addr(), None);
        mgr.shutdown().await; // idempotent
    }

    #[tokio::test]
    async fn downloads_dir_can_be_changed_and_reset() {
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("dlchoose");
        let mgr = ServerManager::new(registry, dl.clone(), q, loopback_bind());
        assert_eq!(mgr.downloads_dir(), dl);

        let chosen = dl.join("received");
        let effective = mgr.set_downloads_dir(Some(&chosen)).unwrap();
        assert_eq!(effective, chosen);
        assert!(chosen.is_dir(), "the chosen folder is created if missing");
        assert_eq!(mgr.downloads_dir(), chosen);

        // A relative path is refused rather than silently resolved.
        assert!(mgr
            .set_downloads_dir(Some(std::path::Path::new("relative\\dir")))
            .is_err());

        // None restores the OS default captured at startup.
        assert_eq!(mgr.set_downloads_dir(None).unwrap(), dl);
        assert_eq!(mgr.downloads_dir(), dl);
    }

    #[tokio::test]
    async fn preferred_interface_overrides_the_default_bind() {
        // The preference is honoured only while that address exists on this
        // PC; loopback is never offered by the UI, so a stale choice must fall
        // back to the default bind exactly as the test's loopback default does.
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("prefip");
        let mgr = ServerManager::new(registry, dl, q, loopback_bind());
        assert_eq!(mgr.preferred_ip(), None);
        mgr.set_preferred_ip(Some(Ipv4Addr::new(203, 0, 113, 9)));
        assert_eq!(mgr.preferred_ip(), Some(Ipv4Addr::new(203, 0, 113, 9)));
        assert_eq!(
            mgr.effective_ip(),
            Some(Ipv4Addr::LOCALHOST),
            "an address this PC does not own must not be used"
        );
        mgr.set_preferred_ip(None);
        assert_eq!(mgr.effective_ip(), Some(Ipv4Addr::LOCALHOST));
    }

    #[tokio::test]
    async fn binds_to_the_requested_address() {
        // SEC-7: the listener must answer on the requested LAN address, never
        // 0.0.0.0. Bind to a specific non-loopback-ish IP (a private IP on
        // loopback would fail; use loopback IP but assert the port is real).
        let registry = Arc::new(SessionRegistry::new(DEFAULT_INACTIVITY_TIMEOUT));
        let (dl, q) = tmp_dirs("sec7");
        let mgr = ServerManager::new(registry.clone(), dl, q, loopback_bind());
        let addr = mgr.ensure_running().await.unwrap();
        assert_ne!(addr.ip().to_string(), "0.0.0.0", "never bind INADDR_ANY");
        assert_ne!(addr.port(), 0, "OS must assign a real port");
    }
}
