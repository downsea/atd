//! ATD runtime — server-side abstractions.
//!
//! `Tool` trait, `Registry`, dispatch, `Binding`, `Middleware`, capability
//! gate, tier policy, read tracker. Depends only on `atd-protocol`.

pub mod binding;
pub mod capability;
pub mod context;
pub mod error;
pub mod middleware;
pub mod registry;
pub mod tier;
pub mod tracker;

pub use binding::{Binding, CliBinding, NativeBinding};
pub use capability::CapabilitySet;
pub use context::CallContext;
pub use error::ToolCallError;
pub use middleware::{Middleware, RedactPathsMiddleware};
pub use registry::{RegisteredTool, Registry, Tool};
pub use tier::{TierPolicy, tier_from_opt_str};
pub use tracker::{ReadTracker, ReadTrackerError};
