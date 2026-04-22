//! Built-in tools.
//!
//! - SP-1: echo test-anchor
//! - SP-2: fs.{read,write,edit} + ReadTracker
//! - SP-3: shell.{exec,pwsh} + shared subprocess handler

pub mod echo;
pub mod fs;
pub mod shell;
