//! `Server` — the listener and accept loop.
//!
//! SP-streamable-http §6.3: the dispatch state machine that was inlined
//! here has moved to `atd-runtime::dispatch` so the HTTP listener
//! (`atd-server-http`) can reuse it byte-for-byte. The Unix-socket server
//! still owns the accept loop, socket-permissions setup, and the
//! per-connection task — those are UDS-specific. `ServerState` is now a
//! re-export of `atd_runtime::dispatch::ServerState`; `Server::new`
//! builds a `SharedServerConfig` snapshot from the per-crate `ServerConfig`
//! so HTTP and UDS see the same fields by composition.

use std::sync::Arc;

use tokio::net::UnixListener;

use atd_runtime::dispatch::SharedServerConfig;
use atd_runtime::registry::Registry;

use crate::config::ServerConfig;
use crate::connection::handle_connection;

pub struct Server {
    state: Arc<ServerState>,
    /// Kept verbatim because `run()` reads `socket_path` from it after
    /// `state` has been frozen into an `Arc`. UDS-specific config does not
    /// belong on `SharedServerConfig` (HTTP doesn't need it) so it lives
    /// here on the `Server` itself.
    socket_path: std::path::PathBuf,
}

/// Re-export of `atd_runtime::dispatch::ServerState`. The struct lives in
/// `atd-runtime` so HTTP and UDS listeners hold the **same** state type;
/// this alias preserves the historical
/// `atd_server::server::ServerState` import path for existing call sites
/// (notably `connection.rs::handle_connection`).
pub(crate) type ServerState = atd_runtime::dispatch::ServerState;

impl Server {
    pub fn new(registry: Registry, config: ServerConfig) -> Self {
        let socket_path = config.socket_path.clone();
        let shared = SharedServerConfig {
            cwd: config.cwd,
            max_output_bytes: config.max_output_bytes,
            default_call_timeout_ms: config.default_call_timeout_ms,
            granted_capabilities: config.granted_capabilities,
            audit_sink: config.audit_sink,
            server_version: config.server_version,
            token_broker: config.token_broker,
        };
        Self {
            state: Arc::new(ServerState {
                registry,
                config: shared,
                tier_policy: atd_runtime::TierPolicy::defaults(),
                middleware: Vec::new(),
            }),
            socket_path,
        }
    }

    /// Replace the tier policy. Valid only before `run()` — after the server
    /// starts, `state` has already been handed to connection tasks and is
    /// effectively immutable. Tests and CLI startup call this once.
    pub fn set_tier_policy(&mut self, policy: atd_runtime::TierPolicy) {
        let state = Arc::get_mut(&mut self.state)
            .expect("set_tier_policy must be called before run() hands out Arcs");
        state.tier_policy = policy;
    }

    /// Install the result-middleware chain. Order matters: first registered
    /// runs first. Must be called before `run()` for the same reason as
    /// `set_tier_policy` — `state` becomes shared when connections spawn.
    pub fn set_middleware(&mut self, middleware: Vec<Arc<dyn atd_runtime::Middleware>>) {
        let state = Arc::get_mut(&mut self.state)
            .expect("set_middleware must be called before run() hands out Arcs");
        state.middleware = middleware;
    }

    pub async fn run(self) -> std::io::Result<()> {
        let sock = &self.socket_path;

        // Ensure parent dir exists.
        if let Some(parent) = sock.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Remove stale socket.
        if sock.exists() {
            std::fs::remove_file(sock)?;
        }

        let listener = UnixListener::bind(sock)?;
        // Unix 0600: owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(sock, perms);
        }

        eprintln!(
            "atd-server: listening on {:?} ({} tool(s) registered)",
            sock,
            self.state.registry.count()
        );

        loop {
            let (stream, _) = listener.accept().await?;
            let state = self.state.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(state, stream).await {
                    eprintln!("atd-server: connection error: {e}");
                }
            });
        }
    }
}
