//! atd-conformance CLI — runs the conformance suite against a target
//! ATD server over a Unix socket.

use atd_conformance::case::Category;
use atd_conformance::{Opts, run_conformance};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "atd-conformance",
    version,
    about = "ATD protocol conformance suite"
)]
struct Args {
    /// Target server endpoint. Example: `unix:/tmp/atd.sock`.
    #[arg(long)]
    target: String,

    /// Substring filter on case name.
    #[arg(long)]
    filter: Option<String>,

    /// Restrict to one or more categories. Repeatable. Default: all.
    #[arg(long, value_enum)]
    category: Vec<CategoryArg>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
    report: ReportFormat,

    /// Exit on first failure.
    #[arg(long)]
    stop_on_first_fail: bool,

    /// Override fixtures directory. Defaults to the bundled fixtures.
    #[arg(long)]
    fixtures_root: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CategoryArg {
    Wire,
    Sanitize,
    Behavior,
}

impl From<CategoryArg> for Category {
    fn from(c: CategoryArg) -> Self {
        match c {
            CategoryArg::Wire => Category::Wire,
            CategoryArg::Sanitize => Category::Sanitize,
            CategoryArg::Behavior => Category::Behavior,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ReportFormat {
    Text,
    Json,
}

fn parse_target(s: &str) -> Result<atd_sdk::Endpoint, String> {
    let s = s.strip_prefix("unix:").unwrap_or(s);
    Ok(atd_sdk::Endpoint::unix(s))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let target = match parse_target(&args.target) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("atd-conformance: invalid --target: {}", e);
            std::process::exit(2);
        }
    };
    let target_display = args.target.clone();

    let fixtures_root = args
        .fixtures_root
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures"));

    let opts = Opts {
        target,
        filter: args.filter,
        categories: args.category.into_iter().map(Into::into).collect(),
        stop_on_first_fail: args.stop_on_first_fail,
        fixtures_root,
    };

    let report = run_conformance(opts).await;

    match args.report {
        ReportFormat::Text => {
            print!("{}", report.to_text(&target_display));
        }
        ReportFormat::Json => {
            println!("{}", report.to_json());
        }
    }

    if report.failed > 0 {
        std::process::exit(1);
    }
}
