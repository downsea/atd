//! ATD reference client SDK (Rust).
//!
//! Zero runtime dependency on any `anos-*` crate. Protocol-level types
//! live in the `atd-protocol` sibling crate.

pub mod client;
pub mod endpoint;
pub mod options;

#[cfg(any(feature = "openai", feature = "anthropic", feature = "langchain"))]
pub mod adapters;

pub use client::AtdClient;
pub use endpoint::Endpoint;
pub use options::{CallOptions, ConnectOptions, DiscoverFilter};

pub use atd_protocol::sanitize;
pub use atd_protocol::wire;
pub use atd_protocol::{desanitize_tool_name, sanitize_tool_name};
