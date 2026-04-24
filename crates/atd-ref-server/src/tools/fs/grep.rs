//! `ref:fs.grep` — regex search across files, honoring .gitignore + skipping hidden/binary.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

const DEFAULT_MAX_MATCHES: usize = 1000;

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.grep".into(),
        name: "File Grep".into(),
        description: "Regex search across files under a root. Honors .gitignore, skips hidden files and binary files. Optional glob filter narrows the walked files. Returns (path, 1-indexed line, line text) triples sorted by path then line.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["grep".into()],
            tags: vec!["fs".into(), "search".into(), "grep".into(), "regex".into()],
            intent_examples: vec![
                "find all TODO comments in src/".into(),
                "search for `fn foo` in Rust sources".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern":          { "type": "string", "minLength": 1, "maxLength": 10000 },
                "path":             { "type": "string" },
                "glob":             { "type": "string" },
                "case_insensitive": { "type": "boolean" },
                "max_matches":      { "type": "integer", "minimum": 1 }
            },
            "required": ["pattern"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "matches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "line": { "type": "integer" },
                            "text": { "type": "string" }
                        }
                    }
                },
                "truncated":   { "type": "boolean" },
                "root":        { "type": "string" },
                "duration_ms": { "type": "integer" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: Some("file contents (matched lines)".into()),
        },
        resources: ToolResources {
            timeout_ms: 30_000,
            max_concurrent: 10,
            rate_limit_per_min: None,
            estimated_tokens: Some(500),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: None,
    })
}

pub struct FsGrepTool;

impl FsGrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsGrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct GrepArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    case_insensitive: Option<bool>,
    #[serde(default)]
    max_matches: Option<usize>,
}

#[derive(Clone)]
struct MatchRow {
    path: String,
    line: u64,
    text: String,
}

fn resolve_root(ctx: &CallContext, path: Option<&str>) -> Result<PathBuf, ToolCallError> {
    let raw = match path {
        Some(p) if !p.is_empty() => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                ctx.cwd.join(pb)
            }
        }
        _ => ctx.cwd.clone(),
    };
    let canonical = std::fs::canonicalize(&raw).map_err(|_| ToolCallError::ExecutionFailed {
        code: "NOT_A_DIRECTORY".into(),
        message: format!("path does not exist: {}", raw.display()),
        retryable: false,
    })?;
    if !canonical.is_dir() {
        return Err(ToolCallError::ExecutionFailed {
            code: "NOT_A_DIRECTORY".into(),
            message: format!("not a directory: {}", canonical.display()),
            retryable: false,
        });
    }
    Ok(canonical)
}

fn build_optional_globset(glob: Option<&str>) -> Result<Option<GlobSet>, ToolCallError> {
    match glob {
        None => Ok(None),
        Some(g) if g.is_empty() => Ok(None),
        Some(g) => {
            let glob = Glob::new(g)
                .map_err(|e| ToolCallError::InvalidArgs(format!("invalid glob `{g}`: {e}")))?;
            let mut b = GlobSetBuilder::new();
            b.add(glob);
            let set = b.build().map_err(|e| {
                ToolCallError::InvalidArgs(format!("glob build failed: {e}"))
            })?;
            Ok(Some(set))
        }
    }
}

/// Sink that collects matches from one file, honoring a remaining-match budget.
struct CollectSink<'a> {
    rel_path: String,
    out: &'a mut Vec<MatchRow>,
    /// Budget in MATCH ROWS; decremented as we push.
    remaining: &'a mut usize,
    /// Budget in BYTES; we charge path.len() + text.len() + overhead per row.
    remaining_bytes: &'a mut usize,
    /// Set to true if any limit was hit while in this sink.
    truncated: &'a mut bool,
}

impl<'a> Sink for CollectSink<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        if *self.remaining == 0 {
            *self.truncated = true;
            return Ok(false);
        }
        let line = mat.line_number().unwrap_or(0);
        let raw = String::from_utf8_lossy(mat.bytes());
        let text = raw.trim_end_matches('\n').trim_end_matches('\r').to_string();
        let cost = self.rel_path.len() + text.len() + 40; // rough JSON overhead
        if cost > *self.remaining_bytes {
            *self.truncated = true;
            return Ok(false);
        }
        *self.remaining_bytes -= cost;
        *self.remaining -= 1;
        self.out.push(MatchRow {
            path: self.rel_path.clone(),
            line,
            text,
        });
        // Intentional: we set `truncated = true` the moment the match-count cap
        // is reached, even if there happen to be no further matches. Detecting
        // "are there more matches beyond the cap" requires searching past the
        // cap, which defeats the cap. So `truncated: true` means "we hit the
        // cap" — not strictly "we dropped results." Callers needing an exact
        // count should raise max_matches.
        if *self.remaining == 0 {
            *self.truncated = true;
            return Ok(false);
        }
        Ok(true)
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_and_search(
    root: &Path,
    matcher: &grep_regex::RegexMatcher,
    glob_filter: Option<&GlobSet>,
    max_matches: usize,
    max_output_bytes: usize,
) -> (Vec<MatchRow>, bool) {
    let mut results: Vec<MatchRow> = Vec::new();
    let mut remaining = max_matches;
    let mut remaining_bytes = max_output_bytes;
    let mut truncated = false;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build();

    'walker: for entry in WalkBuilder::new(root).build().flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }
        if !matches!(entry.file_type(), Some(ft) if ft.is_file()) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if let Some(g) = glob_filter {
            if !g.is_match(rel) {
                continue;
            }
        }
        let rel_str = rel.to_string_lossy().into_owned();
        let mut sink = CollectSink {
            rel_path: rel_str,
            out: &mut results,
            remaining: &mut remaining,
            remaining_bytes: &mut remaining_bytes,
            truncated: &mut truncated,
        };
        // Per-file search. Individual IO errors are swallowed (don't fail
        // the whole grep for one unreadable file).
        let _ = searcher.search_path(matcher, path, &mut sink);
        if remaining == 0 || truncated {
            break 'walker;
        }
    }

    results.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    (results, truncated)
}

impl Tool for FsGrepTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: GrepArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.pattern.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "pattern is empty or whitespace-only".into(),
                ));
            }
            const MAX_PATTERN_BYTES: usize = 10_000;
            if args.pattern.len() > MAX_PATTERN_BYTES {
                return Err(ToolCallError::InvalidArgs(format!(
                    "pattern exceeds {MAX_PATTERN_BYTES} bytes"
                )));
            }
            let case_insensitive = args.case_insensitive.unwrap_or(false);
            let matcher = RegexMatcherBuilder::new()
                .case_insensitive(case_insensitive)
                .build(&args.pattern)
                .map_err(|e| {
                    ToolCallError::InvalidArgs(format!(
                        "invalid regex `{}`: {e}",
                        args.pattern
                    ))
                })?;
            let glob_set = build_optional_globset(args.glob.as_deref())?;
            let max_matches = args.max_matches.unwrap_or(DEFAULT_MAX_MATCHES).max(1);
            let root = resolve_root(ctx, args.path.as_deref())?;
            let max_bytes = ctx.max_output_bytes;

            let start = Instant::now();
            let root_for_task = root.clone();
            let (rows, truncated) = tokio::task::spawn_blocking(move || {
                walk_and_search(
                    &root_for_task,
                    &matcher,
                    glob_set.as_ref(),
                    max_matches,
                    max_bytes,
                )
            })
            .await
            .map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("grep task failed: {e}"),
                retryable: true,
            })?;
            let duration_ms = start.elapsed().as_millis() as u64;

            let matches_json: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "path": m.path,
                        "line": m.line,
                        "text": m.text,
                    })
                })
                .collect();

            Ok(serde_json::json!({
                "matches": matches_json,
                "truncated": truncated,
                "root": root.to_string_lossy(),
                "duration_ms": duration_ms,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(p: &Path, contents: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    fn ctx_for(dir: &Path) -> CallContext {
        let mut c = CallContext::for_test();
        c.cwd = dir.to_path_buf();
        c
    }

    #[tokio::test]
    async fn basic_regex_finds_line() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("src/main.rs"),
            "use std::io;\nfn foo() {}\nfn main() {}\n",
        );
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "fn\\s+\\w+"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 2);
        assert_eq!(matches[0]["text"], "fn foo() {}");
        assert_eq!(matches[1]["line"], 3);
    }

    #[tokio::test]
    async fn case_insensitive_flag() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "Hello\nhello\nworld\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "hello", "case_insensitive": true}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn glob_filter_narrows_search() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("main.rs"), "TODO rs\n");
        write_file(&dir.path().join("main.py"), "TODO py\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "TODO", "glob": "*.rs"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "main.rs");
    }

    #[tokio::test]
    async fn binary_files_skipped() {
        let dir = tempfile::tempdir().unwrap();
        // Construct a file with a NUL byte AND a literal match pattern; grep
        // should skip the whole file due to BinaryDetection::quit.
        let bytes: Vec<u8> = b"text before\x00matches here\n".to_vec();
        fs::write(dir.path().join("data.bin"), &bytes).unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "matches"}), &ctx)
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 0, "binary file should be skipped");
    }

    #[tokio::test]
    async fn no_matches_returns_empty_array() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "hello\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "zzzzzz_not_present"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert!(matches.is_empty());
        assert_eq!(r["truncated"], false);
    }

    #[tokio::test]
    async fn max_matches_cap_sets_truncated() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            write_file(
                &dir.path().join(format!("f{i:02}.txt")),
                "TODO 1\nTODO 2\nTODO 3\nTODO 4\nTODO 5\n",
            );
        }
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "TODO", "max_matches": 10}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 10);
        assert_eq!(r["truncated"], true);
    }

    #[tokio::test]
    async fn line_numbers_are_1_indexed() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "hit\nmiss\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "hit"}), &ctx)
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["line"], 1, "first line is line 1, not line 0");
    }

    #[tokio::test]
    async fn invalid_regex_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let err = t
            .call(serde_json::json!({"pattern": "["}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }
}
