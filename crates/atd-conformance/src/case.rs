//! Types describing a single conformance case, plus the JSON loader.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A single conformance case. Three variants keyed by `category`.
#[derive(Debug, Deserialize)]
#[serde(tag = "category")]
pub enum ConformanceCase {
    #[serde(rename = "wire")]
    Wire(WireCase),
    #[serde(rename = "sanitize")]
    Sanitize(SanitizeCase),
    #[serde(rename = "behavior")]
    Behavior(BehaviorCase),
}

impl ConformanceCase {
    pub fn name(&self) -> &str {
        match self {
            Self::Wire(c) => &c.name,
            Self::Sanitize(c) => &c.name,
            Self::Behavior(c) => &c.name,
        }
    }

    pub fn category(&self) -> Category {
        match self {
            Self::Wire(_) => Category::Wire,
            Self::Sanitize(_) => Category::Sanitize,
            Self::Behavior(_) => Category::Behavior,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Wire(c) => &c.description,
            Self::Sanitize(c) => &c.description,
            Self::Behavior(c) => &c.description,
        }
    }

    pub fn must(&self) -> Must {
        match self {
            Self::Wire(c) => c.must,
            Self::Sanitize(c) => c.must,
            Self::Behavior(c) => c.must,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Wire,
    Sanitize,
    Behavior,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Sanitize => "sanitize",
            Self::Behavior => "behavior",
        }
    }
}

/// Whether a case is required to pass or is optional.
/// Only `Pass` is used in the v1 suite; `Skip` is reserved for future
/// use when optional-capability distinctions arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Must {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "skip")]
    Skip,
}

fn default_must_pass() -> Must {
    Must::Pass
}

/// Wire-frame round-trip case.
#[derive(Debug, Deserialize)]
pub struct WireCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    /// JSON value matching the `atd_protocol::Request` enum shape.
    pub send: serde_json::Value,
    /// Expected subset of the server's `atd_protocol::Response`.
    /// Deep-subset match: every key in expect must appear in actual.
    #[serde(default)]
    pub expect_response_matches: Option<serde_json::Value>,
    /// Optional raw-byte prefix assertion (hex-encoded), used for
    /// frame-codec correctness (BE u32 length, etc.). Rare.
    #[serde(default)]
    pub expect_wire_bytes_prefix_hex: Option<String>,
    /// Optional Hello handshake to perform before the main send.
    #[serde(default)]
    pub setup: Option<SetupStep>,
}

/// Pure-function sanitize case. Doesn't contact any server.
#[derive(Debug, Deserialize)]
pub struct SanitizeCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    pub input: String,
    pub expect_sanitized: String,
}

/// Behavior case — like Wire but typically with a Hello handshake setup
/// and assertion on semantics like error codes.
#[derive(Debug, Deserialize)]
pub struct BehaviorCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    #[serde(default)]
    pub setup: Option<SetupStep>,
    pub send: serde_json::Value,
    pub expect_response_matches: serde_json::Value,
}

/// Pre-send setup — currently only Hello handshake.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupStep {
    Hello {
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        requested_capabilities: Vec<String>,
    },
}

/// Error type returned by the loader when a fixture file is malformed.
#[derive(Debug)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for LoadError {}

/// Load every `.json` fixture under `fixtures_root` recursively.
/// Returns the loaded cases, or a list of per-file errors.
/// Fails fast on the first malformed file — `cases` is always empty on error.
pub fn load_fixtures(fixtures_root: &Path) -> Result<Vec<ConformanceCase>, LoadError> {
    let mut cases = Vec::new();
    load_dir_recursive(fixtures_root, &mut cases)?;
    cases.sort_by(|a, b| a.name().cmp(b.name()));
    Ok(cases)
}

fn load_dir_recursive(dir: &Path, out: &mut Vec<ConformanceCase>) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(dir).map_err(|e| LoadError {
        path: dir.to_path_buf(),
        message: format!("read_dir failed: {}", e),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| LoadError {
            path: dir.to_path_buf(),
            message: format!("read_dir entry failed: {}", e),
        })?;
        let path = entry.path();

        if path.is_dir() {
            load_dir_recursive(&path, out)?;
        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
            let content = std::fs::read_to_string(&path).map_err(|e| LoadError {
                path: path.clone(),
                message: format!("read failed: {}", e),
            })?;
            let case: ConformanceCase = serde_json::from_str(&content).map_err(|e| LoadError {
                path: path.clone(),
                message: format!("JSON parse failed: {}", e),
            })?;
            out.push(case);
        }
        // Non-JSON files (e.g., .gitkeep, README.md) are silently skipped.
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn mk_tempdir_with(cases: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in cases {
            let p = dir.path().join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn load_empty_dir_returns_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 0);
    }

    #[test]
    fn load_sanitize_case_parses() {
        let dir = mk_tempdir_with(&[(
            "sanitize/basic.json",
            r#"{
                "category": "sanitize",
                "name": "basic",
                "description": "basic test",
                "input": "ref:fs.read",
                "expect_sanitized": "ref_fs_read"
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        match &cases[0] {
            ConformanceCase::Sanitize(s) => {
                assert_eq!(s.name, "basic");
                assert_eq!(s.input, "ref:fs.read");
                assert_eq!(s.expect_sanitized, "ref_fs_read");
                assert_eq!(s.must, Must::Pass);
            }
            _ => panic!("expected Sanitize variant"),
        }
    }

    #[test]
    fn load_wire_case_parses() {
        let dir = mk_tempdir_with(&[(
            "wire/ping.json",
            r#"{
                "category": "wire",
                "name": "ping",
                "description": "ping test",
                "send": {"type": "ping"},
                "expect_response_matches": {"type": "pong"}
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        assert!(matches!(cases[0], ConformanceCase::Wire(_)));
    }

    #[test]
    fn load_behavior_case_with_setup_parses() {
        let dir = mk_tempdir_with(&[(
            "behavior/cap_denied.json",
            r#"{
                "category": "behavior",
                "name": "cap_denied",
                "description": "capability denial",
                "setup": {
                    "kind": "hello",
                    "client_id": "test",
                    "requested_capabilities": []
                },
                "send": {"type": "run_tool", "tool_id": "x", "args": {}, "dry_run": false},
                "expect_response_matches": {"type": "error", "code": 1001}
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        match &cases[0] {
            ConformanceCase::Behavior(b) => {
                assert!(b.setup.is_some());
            }
            _ => panic!("expected Behavior variant"),
        }
    }

    #[test]
    fn load_malformed_json_returns_error() {
        let dir = mk_tempdir_with(&[("wire/bad.json", r#"{this is not valid json"#)]);
        let err = load_fixtures(dir.path()).unwrap_err();
        assert!(err.message.contains("JSON parse failed"));
    }

    #[test]
    fn load_unknown_category_returns_error() {
        let dir = mk_tempdir_with(&[(
            "wire/weird.json",
            r#"{"category": "unknown", "name": "x", "description": "x"}"#,
        )]);
        let err = load_fixtures(dir.path()).unwrap_err();
        assert!(err.message.contains("JSON parse failed"));
    }

    #[test]
    fn load_recursive_traversal() {
        let dir = mk_tempdir_with(&[
            (
                "wire/a.json",
                r#"{"category": "wire", "name": "a", "description": "a",
                    "send": {"type": "ping"}}"#,
            ),
            (
                "behavior/b.json",
                r#"{"category": "behavior", "name": "b", "description": "b",
                    "send": {"type": "ping"},
                    "expect_response_matches": {"type": "pong"}}"#,
            ),
        ]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 2);
        // Alphabetical sort by name
        assert_eq!(cases[0].name(), "a");
        assert_eq!(cases[1].name(), "b");
    }
}
