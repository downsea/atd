//! Reference server binary — wires atd-runtime + atd-tools-* + SP-12
//! CliBinding demo (external::uname, unix-only) into an executable.

pub mod builtin;
pub mod conformance;

#[cfg(unix)]
pub mod external;

// Listener layer extracted to atd-server in SP-listener-extract. Re-export
// here so legacy import paths through `atd_ref_server::Server` /
// `atd_ref_server::ServerConfig` keep working without forcing every consumer
// to add a direct atd-server dep. The module path
// `atd_ref_server::server::Server` no longer exists — direct migration is
// `atd_server::{Server, ServerConfig}`.
pub use atd_server::{Server, ServerConfig};
