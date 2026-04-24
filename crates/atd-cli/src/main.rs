//! `atd` — reference command-line client for the ATD protocol.

use atd_cli::cli::{Cli, Command};
use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::List(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::list::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
        Command::Schema(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::schema::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
        Command::Call(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::call::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
        Command::Doctor(args) => {
            let endpoint = atd_cli::connect::resolve_endpoint(cli.sock);
            let sock = match &endpoint {
                atd_client::Endpoint::UnixSocket(p) => p.clone(),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::doctor::run(sock, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
    }
}

fn fail(e: atd_protocol::AtdError) -> std::process::ExitCode {
    eprintln!("atd: {e}");
    if let Some(hint) = e.suggest_fix() {
        eprintln!("hint: {hint}");
    }
    std::process::ExitCode::from(1)
}
