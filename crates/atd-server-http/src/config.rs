//! `HttpServerConfig` — HTTP-listener-specific configuration.
//!
//! SP-streamable-http §6.2: HTTP-specific fields (listen socket, extra
//! origins, body cap, bearer policy) live here; the shared dispatch
//! fields (cwd, audit_sink, token_broker, server_version,
//! granted_capabilities, max_output_bytes) live on
//! `atd_runtime::dispatch::SharedServerConfig`. Adopters pass both into
//! the `Server::builder` chain.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use atd_runtime::dispatch::SharedServerConfig;

/// Default body-size cap for `POST /mcp`. Mirrors the
/// `atd-protocol::wire` 10 MiB frame cap so HTTP and UDS share the same
/// upper bound — there is no MCP scenario where the JSON-RPC envelope
/// should exceed an ATD frame.
pub const DEFAULT_MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// Configuration for [`crate::Server`]. All fields are public so adopters
/// can use struct-literal construction; `Default::default()` returns
/// loopback `127.0.0.1:0` with anonymous-mode bearer policy (matches the
/// Celia http_server.rs precedent in `celia-cli/src/http_server.rs:114-130`).
#[derive(Clone)]
pub struct HttpServerConfig {
    /// Address the listener binds to. SP-streamable-http §4.6 mandates
    /// loopback default — fail-closed against DNS rebinding per MCP
    /// Security Warning. Use `127.0.0.1:0` in tests to let the kernel
    /// pick a free port.
    pub listen: SocketAddr,

    /// Origin patterns added to the default allow-list
    /// (`http://127.0.0.1*`, `http://localhost*`, `https://127.0.0.1*`,
    /// `https://localhost*`, `tauri://*`). Matched against the request's
    /// `Origin` header verbatim — full string equality, not prefix.
    /// Operators surface this from the `--allow-origin` CLI flag,
    /// repeatable.
    pub extra_origins: Vec<String>,

    /// When `true`, requests without `Authorization: Bearer …` are
    /// rejected with HTTP 401 / JSON-RPC `-32002`. When `false` (default),
    /// bearer-less requests fall through to anonymous mode with empty
    /// `CapabilitySet` — matching `celia-cli/src/http_server.rs:295-306`
    /// Tier-0 trust. Bearer-bearing requests are always validated
    /// regardless of this flag.
    pub require_bearer: bool,

    /// Body-size cap for `POST /mcp`. Larger requests rejected with HTTP
    /// 413. Defaults to [`DEFAULT_MAX_BODY_BYTES`] (10 MiB) to match the
    /// `atd-protocol::wire::MAX_FRAME_BYTES` cap.
    pub max_body_bytes: usize,

    /// Shared dispatch-side configuration. Adopters who run UDS + HTTP
    /// from the same process build one `SharedServerConfig`, wrap it in
    /// `Arc`, and pass clones to both transports so they share the audit
    /// sink, token broker, capability allow-list, and server identity by
    /// reference.
    pub shared: Arc<SharedServerConfig>,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            extra_origins: Vec::new(),
            require_bearer: false,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            shared: Arc::new(SharedServerConfig::for_test()),
        }
    }
}
