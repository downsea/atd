//! ATD reference client SDK (Rust).
//!
//! Zero runtime dependency on any `anos-*` crate. Protocol-level types
//! live in the `atd-types` sibling crate.

pub mod client;
pub mod endpoint;
pub mod options;
pub mod protocol;
pub mod sanitize;
pub mod wire;

#[cfg(any(feature = "openai", feature = "anthropic", feature = "langchain"))]
pub mod adapters;

pub use client::AtdClient;
pub use endpoint::Endpoint;
pub use options::{CallOptions, DiscoverFilter};
pub use sanitize::{desanitize_tool_name, sanitize_tool_name};
