//! HTTP-listener errors.
//!
//! Mirrors `atd-server::ServerError` (an `std::io::Error` alias) but
//! wraps `axum`'s `BoxError` so bind + accept-loop failures surface
//! through a single discriminant. The variants are deliberately coarse —
//! finer-grained categorisation (per route, per phase) can be added
//! through a future SP without breaking the public name.

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpServerError {
    /// Failed to bind the listen socket (port in use, permission denied,
    /// etc.).
    #[error("bind {addr}: {source}")]
    Bind {
        addr: std::net::SocketAddr,
        #[source]
        source: io::Error,
    },

    /// axum::serve returned an error after binding succeeded — typically
    /// a transient `accept()` failure or a fatal hyper error.
    #[error("serve: {0}")]
    Serve(#[from] io::Error),
}
