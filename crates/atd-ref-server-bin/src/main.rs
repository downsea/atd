//! `atd-ref-server` — neutral reference server for the ATD protocol.
//!
//! Runs a Unix-socket server that speaks the standard ATD wire protocol and
//! serves the built-in tool registry. Meant as a fork-friendly reference
//! implementation for third parties writing their own ATD servers.

use std::path::PathBuf;

use atd_ref_server_bin::builtin::builtin_registry;
use atd_ref_server_bin::server::{Server, ServerConfig};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "atd-ref-server",
    version,
    about = "Neutral reference server for the Agent Tool Dispatch (ATD) protocol."
)]
struct Args {
    /// Unix socket path. Default: $HOME/.atd-ref/server.sock
    #[arg(long)]
    sock: Option<PathBuf>,

    /// Working directory for relative-path tools. Default: current directory.
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// Per-call output budget in bytes (advisory; tools honor it).
    #[arg(long, default_value_t = 1_048_576)]
    max_output_bytes: usize,

    /// Per-call deadline in milliseconds.
    #[arg(long, default_value_t = 60_000)]
    timeout_ms: u64,

    /// Capability the server will grant to clients that request it during
    /// `Hello`. Repeatable (e.g. `--grant-capability read --grant-capability exec`).
    /// No flags = fail-closed: clients cannot hold any capability, so tools
    /// with non-empty `required_capabilities` are unreachable.
    #[arg(long = "grant-capability", action = clap::ArgAction::Append)]
    grant_capabilities: Vec<String>,

    /// Override a per-tier budget. Format: `<tier>=<key>=<value>`.
    /// Tiers: hot | warm | cold. Keys: timeout_ms | max_output_bytes.
    /// Repeatable. Example: `--tier-override hot=timeout_ms=300`.
    #[arg(long = "tier-override", action = clap::ArgAction::Append)]
    tier_overrides: Vec<String>,

    /// Enable a result-middleware by name. Repeatable. Known names:
    /// `redact_paths` (default). Unknown names exit 2. Pass
    /// `--middleware none` or don't pass the flag to disable entirely
    /// — the default list below includes `redact_paths`.
    #[arg(
        long = "middleware",
        action = clap::ArgAction::Append,
        default_values_t = vec!["redact_paths".to_string()]
    )]
    middleware: Vec<String>,

    /// Register a test-only conformance tool (ref:conformance.denied_op)
    /// that requires the 'conformance.denied' capability. Used by the
    /// atd-conformance suite to validate the ERR_CAPABILITY_DENIED
    /// (code 1001) wire path. NOT for production use.
    #[arg(long, default_value_t = false)]
    enable_conformance_tool: bool,

    /// Path or keyword for audit log sink. Values: "stdout", "stderr",
    /// or a file path. If omitted, audit logging is disabled (zero
    /// overhead — no events are constructed). SP-operability-v1 C1.
    #[arg(long)]
    audit_log: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();

    // SP-operability-v1 C1: install optional audit sink at startup, before
    // Server::new, so Server::new sees the configured sink via ServerConfig.
    // "stdout" and "stderr" are recognized keywords; anything else is a
    // filesystem path opened for append. Failure to open the file is fatal
    // (exit 2) — silent fallback would be dangerous for audit pipelines.
    let audit_sink: Option<std::sync::Arc<dyn atd_runtime::AuditSink>> =
        match args.audit_log.as_deref() {
            None => None,
            Some("stdout") => Some(std::sync::Arc::new(
                atd_runtime::JsonLinesAuditSink::stdout(),
            )),
            Some("stderr") => Some(std::sync::Arc::new(
                atd_runtime::JsonLinesAuditSink::stderr(),
            )),
            Some(path) => match atd_runtime::JsonLinesAuditSink::file(std::path::Path::new(path)) {
                Ok(s) => Some(std::sync::Arc::new(s)),
                Err(e) => {
                    eprintln!("atd-ref-server: cannot open audit log {path}: {e}");
                    return std::process::ExitCode::from(2);
                }
            },
        };

    let mut config = ServerConfig::default();
    if let Some(p) = args.sock {
        config.socket_path = p;
    }
    if let Some(p) = args.cwd {
        config.cwd = p;
    }
    config.max_output_bytes = args.max_output_bytes;
    config.default_call_timeout_ms = args.timeout_ms;
    config.granted_capabilities = args.grant_capabilities;
    config.audit_sink = audit_sink;

    let registry = builtin_registry(args.enable_conformance_tool);

    // SP-8.2: when the conformance tool family is enabled, permanently
    // saturate ref:conformance.saturate_op so the conformance suite's
    // rate-limited fixture sees a 1002 on every single-shot call. The
    // leaked permit lives for the entire process lifetime —
    // intentional, documented, ~64 bytes.
    if args.enable_conformance_tool {
        let entry = registry
            .get("ref:conformance.saturate_op")
            .expect("ref:conformance.saturate_op registered when --enable-conformance-tool is set");
        let permit = entry
            .semaphore
            .clone()
            .try_acquire_owned()
            .expect("saturate_op semaphore should have its single permit available at startup");
        Box::leak(Box::new(permit));
    }

    let mut server = Server::new(registry, config);

    // Apply tier overrides before run() so they take effect before any
    // connection is accepted. Malformed specs → exit 2 with a clear message.
    let mut policy = atd_runtime::TierPolicy::defaults();
    for spec in &args.tier_overrides {
        if let Err(e) = policy.apply_override(spec) {
            eprintln!("atd-ref-server: --tier-override '{spec}': {e}");
            return std::process::ExitCode::from(2);
        }
    }
    server.set_tier_policy(policy);

    // Resolve middleware names → trait objects. `none` is a sentinel that
    // skips all middleware (useful for debugging). Unknown names exit 2.
    let mut middleware: Vec<std::sync::Arc<dyn atd_runtime::Middleware>> = Vec::new();
    for name in &args.middleware {
        match name.as_str() {
            "none" => { /* explicit opt-out */ }
            "redact_paths" => {
                middleware.push(std::sync::Arc::new(
                    atd_runtime::middleware::RedactPathsMiddleware::with_home_default(),
                ));
            }
            other => {
                eprintln!(
                    "atd-ref-server: --middleware '{other}': unknown (known: redact_paths, none)"
                );
                return std::process::ExitCode::from(2);
            }
        }
    }
    server.set_middleware(middleware);

    match server.run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("atd-ref-server: fatal: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
