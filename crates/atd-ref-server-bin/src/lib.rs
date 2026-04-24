//! Reference server binary — wires atd-runtime + atd-tools-* + SP-12
//! CliBinding demo (external::uname, unix-only) into an executable.

pub mod builtin;
pub mod server;

#[cfg(unix)]
pub mod external;
