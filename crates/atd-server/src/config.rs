//! Server configuration.

use std::path::PathBuf;
use std::sync::Arc;

/// Configuration for [`crate::Server`]. All fields are public for direct
/// construction; use `Default::default()` for sensible defaults.
pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub default_call_timeout_ms: u64,
    /// Server-operator allow-list. Capabilities a client may ask for in
    /// `Hello`; anything not in this list is refused. Empty (default) means
    /// no client can hold any capability, so tools with
    /// `required_capabilities != []` cannot be called — matching the fail-
    /// closed policy for SP-12.
    pub granted_capabilities: Vec<String>,
    /// Optional audit sink for per-call observability. `None` (default)
    /// disables audit entirely — no events are constructed, zero overhead.
    /// SP-operability-v1 C1.
    pub audit_sink: Option<Arc<dyn atd_runtime::AuditSink>>,
    /// Identity string returned in the `Hello` handshake's `server_version`
    /// field. Concretely the deployed server's name + version (e.g.
    /// `"atd-ref-server 0.2.1"` or `"healthkit-server 1.4.0"`); the listener
    /// crate's own version is not part of the wire identity.
    /// Default: `concat!("atd-server ", env!("CARGO_PKG_VERSION"))` — used
    /// only when no binary overrides it.
    pub server_version: String,
    /// Optional `TokenBroker` for multi-tenant secret routing. `None`
    /// (default) means the server runs single-tenant — `CallContext::secrets`
    /// is always `None` and tools fall back to env vars / saved file.
    /// SP-token-broker-phase1.
    pub token_broker: Option<Arc<dyn atd_runtime::TokenBroker>>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            socket_path: PathBuf::from(home).join(".atd-ref").join("server.sock"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
            granted_capabilities: vec![],
            audit_sink: None,
            server_version: concat!("atd-server ", env!("CARGO_PKG_VERSION")).to_string(),
            token_broker: None,
        }
    }
}
