//! Streamable-HTTP transport for ATD — sibling of `atd-server`.
//!
//! This crate implements the SP-streamable-http design
//! (`docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md`):
//! a one-endpoint MCP JSON-RPC 2.0 listener that delegates every
//! `tools/call` to the **same** `atd-runtime::dispatch::run_tool` the
//! Unix-socket transport uses. Browser PWAs, Cursor, Claude.ai, OpenAI
//! Functions over HTTP — anything that speaks MCP Streamable HTTP — can
//! reach an ATD-registered tool through this transport without any
//! bespoke ATD wire knowledge.
//!
//! Composition (SP-streamable-http §4.5):
//!
//! ```rust,ignore
//! let registry: atd_runtime::Registry = my_tools();
//! let cfg = atd_server_http::HttpServerConfig::default();
//! let (router, server) = atd_server_http::Server::builder(registry)
//!     .config(cfg)
//!     .build();
//! // Adopters may extend the router with their own routes here.
//! server.serve(router).await?;
//! ```
//!
//! What is **not** in this crate (deferred to future SPs — SP §9):
//! - SSE for `tools/call` (single response per request — adopter routes
//!   layer SSE on top of the returned `Router`).
//! - `Mcp-Session-Id` sessions / resumability / `Last-Event-ID`.
//! - TLS termination (operators front with nginx / Caddy / Tauri).
//! - OAuth 2.1 token issuance (bearer validated, not minted).

pub mod bearer;
pub mod config;
pub mod error;
pub mod mcp;
pub mod origin;
pub mod server;
pub mod sse_refresh;

pub use config::HttpServerConfig;
pub use error::HttpServerError;
pub use server::{Server, ServerBuilder};
pub use sse_refresh::{
    AuthLostReason, DEFAULT_REFRESH_CADENCE, RefreshEvent, spawn_bearer_refresh,
};
