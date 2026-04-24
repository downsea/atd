//! Built-in tools.
//!
//! - SP-1: echo test-anchor
//! - SP-2: fs.{read,write,edit} + ReadTracker
//! - SP-3: shell.{exec,pwsh} + shared subprocess handler
//! - SP-4: fs.{glob,grep}
//! - SP-5: web.fetch
//! - SP-12: external.uname — CliBinding demo (unix-only)

pub mod echo;
#[cfg(unix)]
pub mod external;
pub mod fs;
pub mod shell;
pub mod web;
