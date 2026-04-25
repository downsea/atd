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
        }
    }
}
