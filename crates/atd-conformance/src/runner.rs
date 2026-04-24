//! Per-case runner dispatch.
//!
//! Each case category has its own runner path; this module exposes
//! `run_case` which dispatches by category. Higher-level orchestration
//! (loading fixtures, aggregating results) lives in `lib.rs::run_conformance`.

use crate::case::{Category, ConformanceCase, SanitizeCase};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CaseResult {
    pub name: String,
    pub category: Category,
    pub outcome: Outcome,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Pass,
    Fail { reason: String },
    Skip { why: String },
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass)
    }
    pub fn is_fail(&self) -> bool {
        matches!(self, Outcome::Fail { .. })
    }
    pub fn is_skip(&self) -> bool {
        matches!(self, Outcome::Skip { .. })
    }
}

/// Execute a single case. Wire/behavior cases connect to `target`;
/// sanitize cases ignore `target` and run purely locally.
pub async fn run_case(case: &ConformanceCase, target: &atd_sdk::Endpoint) -> CaseResult {
    let name = case.name().to_string();
    let category = case.category();
    let start = Instant::now();

    let outcome = match case {
        ConformanceCase::Sanitize(s) => run_sanitize_case(s),
        ConformanceCase::Wire(w) => {
            let path = target_to_path(target);
            crate::wire::run_wire_case(w, &path).await
        }
        // Behavior path implemented in Task 5:
        ConformanceCase::Behavior(_) => Outcome::Skip {
            why: "behavior runner not yet implemented (Task 5)".into(),
        },
    };

    CaseResult {
        name,
        category,
        outcome,
        duration: start.elapsed(),
    }
}

fn run_sanitize_case(case: &SanitizeCase) -> Outcome {
    let actual = atd_protocol::sanitize::sanitize_tool_name(&case.input);
    if actual == case.expect_sanitized {
        Outcome::Pass
    } else {
        Outcome::Fail {
            reason: format!(
                "sanitize_tool_name({:?}) = {:?}, expected {:?}",
                case.input, actual, case.expect_sanitized
            ),
        }
    }
}

/// Extract the Unix socket path from an atd_sdk::Endpoint.
/// The conformance suite is Unix-socket-only in v1. If new Endpoint
/// variants are added upstream, the exhaustive match forces us to
/// decide here.
fn target_to_path(endpoint: &atd_sdk::Endpoint) -> std::path::PathBuf {
    match endpoint {
        atd_sdk::Endpoint::UnixSocket(p) => p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Must;

    #[tokio::test]
    async fn sanitize_pass() {
        let case = ConformanceCase::Sanitize(SanitizeCase {
            name: "basic".into(),
            description: "x".into(),
            must: Must::Pass,
            input: "ref:fs.read".into(),
            expect_sanitized: "ref_fs_read".into(),
        });
        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        let r = run_case(&case, &target).await;
        assert!(r.outcome.is_pass(), "unexpected outcome: {:?}", r.outcome);
    }

    #[tokio::test]
    async fn sanitize_fail_reports_mismatch() {
        let case = ConformanceCase::Sanitize(SanitizeCase {
            name: "wrong".into(),
            description: "x".into(),
            must: Must::Pass,
            input: "ref:fs.read".into(),
            expect_sanitized: "definitely_wrong".into(),
        });
        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        let r = run_case(&case, &target).await;
        match r.outcome {
            Outcome::Fail { reason } => {
                assert!(reason.contains("ref_fs_read"));
                assert!(reason.contains("definitely_wrong"));
            }
            other => panic!("expected Fail, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn all_sanitize_fixtures_pass_against_reference() {
        let fixtures_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("sanitize");
        let cases = crate::case::load_fixtures(&fixtures_root).expect("load sanitize fixtures");
        assert!(!cases.is_empty(), "no sanitize fixtures found");

        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        for case in &cases {
            let r = run_case(case, &target).await;
            assert!(
                r.outcome.is_pass(),
                "sanitize case {} failed: {:?}",
                case.name(),
                r.outcome
            );
        }
    }
}
