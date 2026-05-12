//! ATD runtime — server-side abstractions.
//!
//! `Tool` trait, `Registry`, dispatch, `Binding`, `Middleware`, capability
//! gate, tier policy, read tracker. Depends only on `atd-protocol`.

pub mod audit;
pub mod binding;
pub mod capability;
pub mod context;
pub mod cursor;
pub mod dispatch;
pub mod error;
pub mod metrics;
pub mod middleware;
pub mod registry;
pub mod runtime;
pub mod secrets;
pub mod tier;
pub mod tracker;
pub mod ucan;

pub use audit::{AuditSink, CallEvent, JsonLinesAuditSink, Outcome, SCHEMA_VERSION};
pub use binding::{Binding, CliBinding, NativeBinding};
pub use capability::CapabilitySet;
pub use context::CallContext;
pub use cursor::{CursorError, CursorIssuer, CursorPayload, args_fingerprint};
pub use dispatch::{ServerState, SharedServerConfig, dispatch_request, run_tool};
pub use error::ToolCallError;
pub use metrics::{MetricsCounters, MetricsSnapshot};
pub use middleware::{Middleware, RedactPathsMiddleware};
pub use registry::{RegisteredTool, Registry, Tool};
pub use runtime::default_worker_threads;
pub use secrets::{
    BearerIdentity, BrokerError, InMemoryTokenBroker, RedactedString, ResolveBearerFuture,
    ResolveFuture, SecretBundle, TokenBroker,
};
pub use tier::{TierPolicy, tier_as_str, tier_from_opt_str};
pub use tracker::{ReadTracker, ReadTrackerError};
