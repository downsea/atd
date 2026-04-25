//! Server transport errors.
//!
//! For v0.2.x this is an alias of [`std::io::Error`]: bind, accept, and
//! per-connection I/O errors all surface as `io::Error` today. A typed enum
//! can replace this alias in a future SP if call sites need to discriminate
//! by failure category — the alias keeps the public name stable across that
//! change.

pub type ServerError = std::io::Error;
