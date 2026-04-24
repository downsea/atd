//! Report aggregation and output formatting.

use crate::case::Category;
use crate::runner::{CaseResult, Outcome};
use std::time::Duration;

#[derive(Debug)]
pub struct Report {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cases: Vec<CaseResult>,
    pub total_duration: Duration,
}

impl Report {
    pub fn from_results(cases: Vec<CaseResult>) -> Self {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut total_duration = Duration::ZERO;
        for c in &cases {
            total_duration += c.duration;
            match &c.outcome {
                Outcome::Pass => passed += 1,
                Outcome::Fail { .. } => failed += 1,
                Outcome::Skip { .. } => skipped += 1,
            }
        }
        Self {
            total: cases.len(),
            passed,
            failed,
            skipped,
            cases,
            total_duration,
        }
    }

    /// Human-readable text format; used by the CLI's default output.
    pub fn to_text(&self, target_display: &str) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let version = env!("CARGO_PKG_VERSION");
        writeln!(
            &mut out,
            "atd-conformance {} — target {}",
            version, target_display
        )
        .unwrap();
        writeln!(&mut out).unwrap();

        for category in [Category::Wire, Category::Sanitize, Category::Behavior] {
            let in_cat: Vec<&CaseResult> = self
                .cases
                .iter()
                .filter(|c| c.category == category)
                .collect();
            if in_cat.is_empty() {
                continue;
            }
            let passed = in_cat.iter().filter(|c| c.outcome.is_pass()).count();
            let failed = in_cat.iter().filter(|c| c.outcome.is_fail()).count();
            let marker = if failed == 0 { "✓" } else { "✗" };
            writeln!(
                &mut out,
                "[{:<9}] ({}/{} {})",
                category.as_str(),
                passed,
                in_cat.len(),
                marker
            )
            .unwrap();
            for c in in_cat {
                let (mark, suffix) = match &c.outcome {
                    Outcome::Pass => ("✓".to_string(), String::new()),
                    Outcome::Fail { reason } => ("✗".to_string(), format!("\n      {}", reason)),
                    Outcome::Skip { why } => ("~".to_string(), format!(" (skip: {})", why)),
                };
                writeln!(
                    &mut out,
                    "  {} {:<45} {}ms{}",
                    mark,
                    c.name,
                    c.duration.as_millis(),
                    suffix
                )
                .unwrap();
            }
            writeln!(&mut out).unwrap();
        }

        writeln!(
            &mut out,
            "{} cases: {} passed, {} failed, {} skipped  (total {}ms)",
            self.total,
            self.passed,
            self.failed,
            self.skipped,
            self.total_duration.as_millis()
        )
        .unwrap();

        out
    }

    /// JSON format; used by CI consumers.
    pub fn to_json(&self) -> String {
        let val = serde_json::json!({
            "total": self.total,
            "passed": self.passed,
            "failed": self.failed,
            "skipped": self.skipped,
            "total_duration_ms": self.total_duration.as_millis(),
            "cases": self.cases.iter().map(|c| {
                let outcome = match &c.outcome {
                    Outcome::Pass => serde_json::json!("pass"),
                    Outcome::Fail { reason } => serde_json::json!({
                        "fail": { "reason": reason }
                    }),
                    Outcome::Skip { why } => serde_json::json!({
                        "skip": { "why": why }
                    }),
                };
                serde_json::json!({
                    "name": c.name,
                    "category": c.category.as_str(),
                    "outcome": outcome,
                    "duration_ms": c.duration.as_millis(),
                })
            }).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&val).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_case(name: &str, category: Category, outcome: Outcome, ms: u64) -> CaseResult {
        CaseResult {
            name: name.into(),
            category,
            outcome,
            duration: Duration::from_millis(ms),
        }
    }

    #[test]
    fn from_results_counts() {
        let cases = vec![
            mk_case("a", Category::Wire, Outcome::Pass, 1),
            mk_case("b", Category::Wire, Outcome::Fail { reason: "x".into() }, 2),
            mk_case(
                "c",
                Category::Sanitize,
                Outcome::Skip { why: "y".into() },
                0,
            ),
        ];
        let r = Report::from_results(cases);
        assert_eq!(r.total, 3);
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 1);
        assert_eq!(r.skipped, 1);
    }

    #[test]
    fn text_report_mentions_target_and_counts() {
        let cases = vec![mk_case("a", Category::Wire, Outcome::Pass, 5)];
        let r = Report::from_results(cases);
        let t = r.to_text("unix:/tmp/x.sock");
        assert!(t.contains("unix:/tmp/x.sock"));
        assert!(t.contains("1 cases"));
        assert!(t.contains("1 passed"));
    }

    #[test]
    fn text_report_shows_failure_reason() {
        let cases = vec![mk_case(
            "failing",
            Category::Wire,
            Outcome::Fail {
                reason: "expected X got Y".into(),
            },
            1,
        )];
        let r = Report::from_results(cases);
        let t = r.to_text("unix:/x");
        assert!(t.contains("✗ failing"));
        assert!(t.contains("expected X got Y"));
    }

    #[test]
    fn json_report_parses_back() {
        let cases = vec![
            mk_case("a", Category::Wire, Outcome::Pass, 1),
            mk_case(
                "b",
                Category::Behavior,
                Outcome::Fail { reason: "r".into() },
                2,
            ),
        ];
        let r = Report::from_results(cases);
        let j = r.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed["total"], 2);
        assert_eq!(parsed["passed"], 1);
        assert_eq!(parsed["failed"], 1);
        assert_eq!(parsed["cases"][0]["outcome"], "pass");
        assert_eq!(parsed["cases"][1]["outcome"]["fail"]["reason"], "r");
    }
}
