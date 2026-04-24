//! ATD conformance test suite.
//!
//! Drives a target ATD server through wire-format, sanitize, and
//! behavioral conformance cases loaded from JSON fixtures. Reports
//! pass/fail per case. Implementation-agnostic: any server that
//! speaks ATD over a Unix socket can be validated.
//!
//! See `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`
//! for the design.

pub mod case;
pub mod report;
pub mod runner;
pub mod wire;

use crate::case::{Category, ConformanceCase};
use crate::report::Report;
use crate::runner::{CaseResult, Outcome, run_case};
use std::path::PathBuf;

/// Options controlling a conformance run.
pub struct Opts {
    /// Target server endpoint. Unix socket only in v1.
    pub target: atd_sdk::Endpoint,
    /// Optional substring filter on case name.
    pub filter: Option<String>,
    /// Only run these categories. Empty Vec = run all.
    pub categories: Vec<Category>,
    /// Stop after the first failing case.
    pub stop_on_first_fail: bool,
    /// Path to the fixtures directory. Default: `fixtures/` relative to
    /// `CARGO_MANIFEST_DIR`. Callers in a consuming-crate test should
    /// pass the path explicitly because `CARGO_MANIFEST_DIR` won't
    /// point here.
    pub fixtures_root: PathBuf,
}

impl Opts {
    /// Construct Opts with fixtures_root defaulted to the atd-conformance
    /// crate's fixtures/ directory. Only valid when called from within
    /// atd-conformance itself (e.g., the CLI binary or unit tests).
    pub fn with_default_fixtures(target: atd_sdk::Endpoint) -> Self {
        Self {
            target,
            filter: None,
            categories: Vec::new(),
            stop_on_first_fail: false,
            fixtures_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
        }
    }
}

/// Run the full suite against the target. Returns a Report.
///
/// Loader errors (malformed JSON) are surfaced as a single synthetic
/// "loader" case with Outcome::Fail. This keeps the Report type
/// simple — callers should still check `report.failed == 0`.
pub async fn run_conformance(opts: Opts) -> Report {
    let cases = match case::load_fixtures(&opts.fixtures_root) {
        Ok(c) => c,
        Err(e) => {
            let loader_fail = CaseResult {
                name: "_fixture_loader".into(),
                category: Category::Wire,
                outcome: Outcome::Fail {
                    reason: format!("fixture loader failed: {}", e),
                },
                duration: std::time::Duration::ZERO,
            };
            return Report::from_results(vec![loader_fail]);
        }
    };

    let mut results = Vec::with_capacity(cases.len());
    for case in &cases {
        if let Some(skip_reason) = should_skip(case, &opts) {
            results.push(CaseResult {
                name: case.name().to_string(),
                category: case.category(),
                outcome: Outcome::Skip { why: skip_reason },
                duration: std::time::Duration::ZERO,
            });
            continue;
        }

        let r = run_case(case, &opts.target).await;

        let should_stop = opts.stop_on_first_fail && r.outcome.is_fail();
        results.push(r);
        if should_stop {
            break;
        }
    }

    Report::from_results(results)
}

fn should_skip(case: &ConformanceCase, opts: &Opts) -> Option<String> {
    if !opts.categories.is_empty() && !opts.categories.contains(&case.category()) {
        return Some(format!(
            "category filter excludes {}",
            case.category().as_str()
        ));
    }
    if let Some(filter) = &opts.filter {
        if !case.name().contains(filter.as_str()) {
            return Some(format!("name filter {:?} does not match", filter));
        }
    }
    None
}
