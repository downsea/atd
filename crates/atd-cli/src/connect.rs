//! Endpoint resolution + AtdClient connect helper.

use atd_sdk::{AtdClient, Endpoint};
use atd_protocol::AtdError;
use std::path::PathBuf;

/// Resolve the endpoint from an optional explicit `--sock PATH` override.
/// Falls back to `Endpoint::default_anos()` which reads `$HOME/.anos/anos.sock`.
pub fn resolve_endpoint(sock: Option<PathBuf>) -> Endpoint {
    match sock {
        Some(p) => Endpoint::unix(p),
        None => Endpoint::default_anos(),
    }
}

/// Connect to the configured ATD server. On failure returns an `AtdError`
/// suitable for the top-level error formatter; the caller owns printing.
pub async fn connect(sock: Option<PathBuf>) -> Result<AtdClient, AtdError> {
    let endpoint = resolve_endpoint(sock);
    AtdClient::connect(endpoint).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_endpoint_uses_override_when_provided() {
        let e = resolve_endpoint(Some(PathBuf::from("/tmp/custom.sock")));
        match e {
            Endpoint::UnixSocket(p) => assert_eq!(p, PathBuf::from("/tmp/custom.sock")),
        }
    }

    #[test]
    fn resolve_endpoint_falls_back_to_default_anos() {
        let e = resolve_endpoint(None);
        match e {
            Endpoint::UnixSocket(p) => assert!(
                p.to_string_lossy().ends_with(".anos/anos.sock"),
                "default should point at ~/.anos/anos.sock, got {p:?}"
            ),
        }
    }
}
