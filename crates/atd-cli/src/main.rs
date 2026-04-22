//! `atd` — reference command-line client for the ATD protocol.

use atd_cli::cli::{Cli, Command};
use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::List(_) => {
            eprintln!("atd list: not yet implemented (Task 4)");
            std::process::ExitCode::from(2)
        }
        Command::Schema(_) => {
            eprintln!("atd schema: not yet implemented (Task 5)");
            std::process::ExitCode::from(2)
        }
        Command::Call(_) => {
            eprintln!("atd call: not yet implemented (Task 6)");
            std::process::ExitCode::from(2)
        }
        Command::Doctor(_) => {
            eprintln!("atd doctor: not yet implemented (Task 7)");
            std::process::ExitCode::from(2)
        }
    }
}
