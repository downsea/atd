//! Healthcare PHI redaction middleware for `atd-runtime`.
//!
//! See `README.md` and `SP-medical-middleware-design.md` §4.5 + §4.6
//! for design rationale. Mount via
//! `Server::set_middleware(vec![Arc::new(FhirMiddleware::default()),
//!  Arc::new(PiiRedactMiddleware::default())])`.

pub mod config;
pub mod middleware;
pub mod paths;
pub mod redact;
pub mod strategy;

pub use config::PiiRedactConfig;
pub use middleware::PiiRedactMiddleware;
pub use paths::DEFAULT_PHI_PATHS;
pub use redact::redact_value;
pub use strategy::RedactionStrategy;
