//! `atd-ref-server` — neutral reference server for the ATD protocol.
//!
//! Runs a Unix-socket server that speaks the standard ATD wire protocol and
//! serves the built-in tool registry. Meant as a fork-friendly reference
//! implementation for third parties writing their own ATD servers.

use std::path::PathBuf;

use atd_ref_server::builtin::builtin_registry;
use atd_ref_server::server::{Server, ServerConfig};

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
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();

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

    let registry = builtin_registry();
    let mut server = Server::new(registry, config);

    // Apply tier overrides before run() so they take effect before any
    // connection is accepted. Malformed specs → exit 2 with a clear message.
    let mut policy = atd_ref_server::tier::TierPolicy::defaults();
    for spec in &args.tier_overrides {
        if let Err(e) = policy.apply_override(spec) {
            eprintln!("atd-ref-server: --tier-override '{spec}': {e}");
            return std::process::ExitCode::from(2);
        }
    }
    server.set_tier_policy(policy);

    match server.run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("atd-ref-server: fatal: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
