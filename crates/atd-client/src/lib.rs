//! ATD reference client SDK (Rust).
//!
//! Zero runtime dependency on any `anos-*` crate. Protocol-level types
//! live in the `atd-types` sibling crate.

pub mod client;
pub mod endpoint;
pub mod protocol;
pub mod wire;

pub use client::AtdClient;
pub use endpoint::Endpoint;
