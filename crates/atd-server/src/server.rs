//! `Server` — the listener and accept loop.

use std::sync::Arc;

use tokio::net::UnixListener;

use atd_runtime::registry::Registry;

use crate::config::ServerConfig;
use crate::connection::handle_connection;

pub struct Server {
    state: Arc<ServerState>,
}

pub(crate) struct ServerState {
    pub(crate) registry: Registry,
    pub(crate) config: ServerConfig,
    pub(crate) tier_policy: atd_runtime::TierPolicy,
    pub(crate) middleware: Vec<Arc<dyn atd_runtime::Middleware>>,
}

impl Server {
    pub fn new(registry: Registry, config: ServerConfig) -> Self {
        Self {
            state: Arc::new(ServerState {
                registry,
                config,
                tier_policy: atd_runtime::TierPolicy::defaults(),
                middleware: Vec::new(),
            }),
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
        let sock = &self.state.config.socket_path;

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
