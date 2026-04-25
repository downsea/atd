//! ATD server transport — Unix-socket listener and per-connection task.
//!
//! Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) to build a
//! server. See README for a 30-line example.

pub mod config;
pub mod connection;
pub mod error;
pub mod server;

pub use config::ServerConfig;
pub use error::ServerError;
pub use server::Server;
