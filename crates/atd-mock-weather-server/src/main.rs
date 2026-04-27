//! `atd-mock-weather-server` — small bin that registers 3 mock weather
//! tools onto an [`atd_server::Server`]. Used by the cross-vendor demo
//! to prove client-side composition: an agent connects to BOTH this
//! server AND `healthkit serve` in one session, and sees both vendors'
//! tool ids in a single `discover()` call.
//!
//! Static canned data, never hits a real weather API. See
//! `docs/integrations/cross-vendor-pattern.md` and
//! SP-cross-vendor-mock-demo for the design rationale.

use std::path::PathBuf;
use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_server::{Server, ServerConfig};
use clap::Parser;

mod tools;

#[derive(Parser, Debug)]
#[command(
    name = "atd-mock-weather-server",
    about = "Mock weather ATD server — cross-vendor composition demo.",
    version
)]
struct Args {
    /// Unix socket path to bind. Default: /tmp/atd-weather.sock
    #[arg(long, default_value = "/tmp/atd-weather.sock")]
    sock: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();

    // Best-effort cleanup so a stale socket from a crashed prior run
    // doesn't make `bind` fail.
    let _ = std::fs::remove_file(&args.sock);

    let mut reg = Registry::new();
    reg.register(Arc::new(tools::WeatherNowTool::new()));
    reg.register(Arc::new(tools::WeatherForecastHourlyTool::new()));
    reg.register(Arc::new(tools::WeatherSummaryTool::new()));

    let cfg = ServerConfig {
        socket_path: args.sock.clone(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        max_output_bytes: 65_536,
        default_call_timeout_ms: 1000,
        granted_capabilities: vec![],
        audit_sink: None,
        server_version: concat!("atd-mock-weather-server ", env!("CARGO_PKG_VERSION")).into(),
        token_broker: None,
    };

    eprintln!(
        "atd-mock-weather-server: 3 tool(s) registered (mock:weather.*); listening on {}",
        args.sock.display()
    );

    match Server::new(reg, cfg).run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
