use atd_protocol::{ToolTier, ToolVisibility};

#[derive(Debug, Clone, Default)]
pub struct DiscoverFilter {
    pub tier: Option<ToolTier>,
    pub visibility: Option<ToolVisibility>,
    pub domain: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub dry_run: bool,
    pub preferred_binding: Option<atd_protocol::BindingProtocol>,
}

/// SP-concurrency-baseline §5.3 — controls `AtdClient::connect` retry behaviour.
///
/// Defaults are read from env (`ATD_CONNECT_RETRIES`,
/// `ATD_CONNECT_BACKOFF_BASE_MS`, `ATD_CONNECT_BACKOFF_CAP_MS`,
/// `ATD_CONNECT_TIMEOUT_MS`) so adopters tune deployments without code
/// edits. Construct manually for explicit control:
///
/// ```no_run
/// # use atd_sdk::{AtdClient, ConnectOptions, Endpoint};
/// # async fn ex() {
/// let opts = ConnectOptions { max_attempts: 3, backoff_base_ms: 100, backoff_cap_ms: 1000, connect_timeout_ms: 5000 };
/// let _c = AtdClient::connect_with_options(Endpoint::unix("/tmp/x.sock"), opts).await;
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Total connect attempts before giving up. Includes the initial try.
    pub max_attempts: u32,
    /// Initial backoff after the first failed attempt, in ms.
    pub backoff_base_ms: u64,
    /// Backoff is doubled per failure but capped at this value (ms).
    pub backoff_cap_ms: u64,
    /// Per-attempt deadline wrapping `UnixStream::connect` + `ping`.
    pub connect_timeout_ms: u64,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            max_attempts: env_u32("ATD_CONNECT_RETRIES", 5),
            backoff_base_ms: env_u64("ATD_CONNECT_BACKOFF_BASE_MS", 50),
            backoff_cap_ms: env_u64("ATD_CONNECT_BACKOFF_CAP_MS", 800),
            connect_timeout_ms: env_u64("ATD_CONNECT_TIMEOUT_MS", 10_000),
        }
    }
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
