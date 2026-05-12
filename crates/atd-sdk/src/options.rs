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

/// SP-pagination-v1 §4.8 — one page of a paginated tool result.
///
/// Returned by `AtdClient::call_page`. Clients pass `next_cursor` verbatim
/// back to the SDK on the follow-up `call_page` call; the SDK doesn't
/// parse it. `None` means terminal page.
#[derive(Debug, Clone)]
pub struct PaginatedSdkResult {
    pub value: serde_json::Value,
    pub next_cursor: Option<String>,
}

/// SP-pagination-v1 §4.8 — controls `AtdClient::call_all`'s auto-loop.
///
/// Sanity bounds against runaway loops (misbehaving server keeps issuing
/// cursors) and against accidentally swallowing more memory than the
/// caller expected. Either cap triggers `AtdError::PaginationLimitExceeded`
/// with `pages_fetched` + `bytes_fetched` so callers can decide whether to
/// treat the partial as success or retry with narrower args.
#[derive(Debug, Clone)]
pub struct CallAllOptions {
    /// Maximum number of pages (including the initial call). Default 100.
    pub max_pages: u32,
    /// Maximum cumulative serialized bytes across pages. Default 32 MiB.
    pub max_total_bytes: usize,
    /// How to combine multiple pages into one Value. See [`MergePolicy`].
    pub merge_policy: MergePolicy,
}

impl Default for CallAllOptions {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_total_bytes: 32 * 1024 * 1024,
            merge_policy: MergePolicy::ConcatArray,
        }
    }
}

/// SP-pagination-v1 §4.8 — strategy for merging pages.
///
/// Different paginating tools return different shapes:
/// - `healthkit:query_observations` returns `[Observation, ...]` per page → `ConcatArray`
/// - `celia:list_observations` returns `{patient: "x", observations: [...], total: N}` per page → `ConcatField("observations")`
/// - A summary tool that only honors the first page → `FirstPageOnly`
#[derive(Debug, Clone)]
pub enum MergePolicy {
    /// Each page is a JSON array; concat across pages.
    ConcatArray,
    /// Each page is a JSON object; concat the named array field across
    /// pages and keep the last page's other fields (e.g., metadata totals
    /// that don't change across pages).
    ConcatField(String),
    /// First page wins; subsequent pages are dropped silently.
    FirstPageOnly,
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
