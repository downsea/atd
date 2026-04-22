//! CLI surface: every clap-derive struct in one place.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "atd",
    version,
    about = "Reference client for the Agent Tool Dispatch (ATD) protocol."
)]
pub struct Cli {
    /// Override the Unix socket path. Default: $HOME/.anos/anos.sock
    #[arg(long, global = true)]
    pub sock: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List available tools (wraps the ATD `discover` API).
    List(ListArgs),
    /// Show a tool's full schema (wraps `describe`).
    Schema(SchemaArgs),
    /// Invoke a tool (wraps `call`).
    Call(CallArgs),
    /// Check connectivity to the ATD server.
    Doctor(DoctorArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Substring match against id/name/description.
    #[arg(short, long)]
    pub query: Option<String>,
    /// Filter by domain (e.g. "fs", "web").
    #[arg(short, long)]
    pub domain: Option<String>,
    /// Filter by tier.
    #[arg(long, value_parser = ["hot", "warm", "cold"])]
    pub tier: Option<String>,
    /// Filter by visibility.
    #[arg(long, value_parser = ["read", "write", "dangerous", "system"])]
    pub visibility: Option<String>,
    /// Cap the number of results.
    #[arg(short, long)]
    pub limit: Option<usize>,
    /// Emit structured JSON instead of human output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Tool id, e.g. "anos:fs.read".
    pub tool_id: String,
    /// Emit raw JSON instead of pretty-printed JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct CallArgs {
    /// Tool id, e.g. "anos:fs.read".
    pub tool_id: String,
    /// JSON arguments object, e.g. '{"path":"/tmp/x"}'. Defaults to `{}`.
    #[arg(long, default_value = "{}")]
    pub args: String,
    /// Run in dry-run mode (server may return a preview).
    #[arg(long)]
    pub dry_run: bool,
    /// Emit the result as raw JSON instead of pretty-printed.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct DoctorArgs {
    /// Emit structured JSON instead of human output.
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parses_list_with_flags() {
        let cli = Cli::try_parse_from([
            "atd", "list", "--query", "fs", "--limit", "5", "--json",
        ])
        .unwrap();
        match cli.command {
            Command::List(args) => {
                assert_eq!(args.query.as_deref(), Some("fs"));
                assert_eq!(args.limit, Some(5));
                assert!(args.json);
            }
            _ => panic!("expected List variant"),
        }
    }

    #[test]
    fn cli_parses_schema_with_positional_tool_id() {
        let cli = Cli::try_parse_from(["atd", "schema", "anos:fs.read"]).unwrap();
        match cli.command {
            Command::Schema(args) => assert_eq!(args.tool_id, "anos:fs.read"),
            _ => panic!("expected Schema variant"),
        }
    }

    #[test]
    fn cli_parses_call_with_args_and_dry_run() {
        let cli = Cli::try_parse_from([
            "atd", "call", "anos:fs.read",
            "--args", r#"{"path":"/tmp/x"}"#,
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Command::Call(args) => {
                assert_eq!(args.tool_id, "anos:fs.read");
                assert_eq!(args.args, r#"{"path":"/tmp/x"}"#);
                assert!(args.dry_run);
            }
            _ => panic!("expected Call variant"),
        }
    }

    #[test]
    fn sock_flag_is_global_and_parses_before_subcommand() {
        let cli = Cli::try_parse_from([
            "atd", "--sock", "/tmp/x.sock", "list",
        ])
        .unwrap();
        assert_eq!(cli.sock.as_deref().map(|p| p.to_string_lossy().into_owned()),
                   Some("/tmp/x.sock".to_string()));
    }

    #[test]
    fn invalid_tier_value_is_rejected() {
        let err = Cli::try_parse_from(["atd", "list", "--tier", "lukewarm"]).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("lukewarm"), "error should mention bad value, got: {s}");
    }

    #[test]
    fn cli_is_wellformed() {
        // Fails at compile / factory time if the derive macros produce an
        // invalid command tree (e.g. overlapping short flags).
        Cli::command().debug_assert();
    }
}
