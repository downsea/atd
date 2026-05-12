//! `ref:fs.glob` — glob pattern → paths, honoring .gitignore + skipping hidden.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

const DEFAULT_MAX_MATCHES: usize = 1000;

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.glob".into(),
        name: "File Glob".into(),
        description: "Find files matching a glob pattern. Walks the tree honoring .gitignore and skipping hidden files/dirs. Returns paths relative to the searched root, lexicographically sorted.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["glob".into()],
            tags: vec!["fs".into(), "search".into(), "glob".into()],
            intent_examples: vec![
                "find all .rs files under src/".into(),
                "list Cargo manifests in the repo".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern":     { "type": "string", "minLength": 1 },
                "path":        { "type": "string" },
                "max_matches": { "type": "integer", "minimum": 1 }
            },
            "required": ["pattern"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "paths":       { "type": "array", "items": { "type": "string" } },
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
            data_sensitivity: Some("directory layout".into()),
        },
        resources: ToolResources {
            timeout_ms: 30_000,
            max_concurrent: 10,
            rate_limit_per_min: None,
            estimated_tokens: Some(300),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: None,
        errors: vec![],
    })
}

pub struct FsGlobTool;

impl FsGlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsGlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct GlobArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_matches: Option<usize>,
}

/// Resolve `path` against `ctx.cwd` and canonicalize.
/// Returns `NOT_A_DIRECTORY` if the result isn't an existing directory.
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

fn build_globset(pattern: &str) -> Result<GlobSet, ToolCallError> {
    let glob = Glob::new(pattern)
        .map_err(|e| ToolCallError::InvalidArgs(format!("invalid glob `{pattern}`: {e}")))?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    builder
        .build()
        .map_err(|e| ToolCallError::InvalidArgs(format!("glob build failed: {e}")))
}

fn walk_and_collect(
    root: &Path,
    globs: &GlobSet,
    max_matches: usize,
    max_output_bytes: usize,
) -> (Vec<String>, bool) {
    let mut results: Vec<String> = Vec::new();
    let mut byte_budget = max_output_bytes;
    let mut truncated = false;

    // .require_git(false) so `.gitignore` is honored even when `root` is
    // not inside a git repository — the tool's description promises
    // "honoring .gitignore" unconditionally, and the prior default
    // (`require_git = true`) silently dropped that promise for
    // non-git working dirs (incl. the test fixture's tempdir).
    for entry in WalkBuilder::new(root).require_git(false).build().flatten() {
        let path = entry.path();
        // Skip the root itself and any directory entries.
        if path == root {
            continue;
        }
        let file_type = entry.file_type();
        if !matches!(file_type, Some(ft) if ft.is_file()) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if !globs.is_match(rel) {
            continue;
        }
        let rel_str = rel.to_string_lossy().into_owned();
        let cost = rel_str.len() + 2; // rough JSON overhead
        if cost > byte_budget {
            truncated = true;
            break;
        }
        byte_budget -= cost;
        results.push(rel_str);
        if results.len() >= max_matches {
            truncated = true;
            break;
        }
    }

    results.sort();
    (results, truncated)
}

impl Tool for FsGlobTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: GlobArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.pattern.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "pattern is empty or whitespace-only".into(),
                ));
            }
            let max_matches = args.max_matches.unwrap_or(DEFAULT_MAX_MATCHES).max(1);
            let root = resolve_root(ctx, args.path.as_deref())?;
            let globs = build_globset(&args.pattern)?;
            let max_bytes = ctx.max_output_bytes;

            let start = Instant::now();
            let (paths, truncated, root_str) = tokio::task::spawn_blocking(move || {
                let root_str = root.to_string_lossy().into_owned();
                let (paths, truncated) = walk_and_collect(&root, &globs, max_matches, max_bytes);
                (paths, truncated, root_str)
            })
            .await
            .map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("walker task failed: {e}"),
                retryable: true,
            })?;
            let duration_ms = start.elapsed().as_millis() as u64;

            Ok(serde_json::json!({
                "paths": paths,
                "truncated": truncated,
                "root": root_str,
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
    async fn basic_pattern_returns_matching_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.rs"), "");
        write_file(&dir.path().join("b.rs"), "");
        write_file(&dir.path().join("c.txt"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
        assert_eq!(r["truncated"], false);
    }

    #[tokio::test]
    async fn recursive_pattern() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("src/main.rs"), "");
        write_file(&dir.path().join("src/lib/util.rs"), "");
        write_file(&dir.path().join("README.md"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(paths.iter().any(|p| p.ends_with("util.rs")));
    }

    #[tokio::test]
    async fn gitignore_respected() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join(".gitignore"), "target/\n");
        write_file(&dir.path().join("src/main.rs"), "");
        write_file(&dir.path().join("target/debug/out.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(
            !paths.iter().any(|p| p.contains("target")),
            "target/ should be ignored: {paths:?}"
        );
    }

    #[tokio::test]
    async fn hidden_skipped_by_default() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join(".hidden/foo.rs"), "");
        write_file(&dir.path().join("visible.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["visible.rs".to_string()]);
    }

    #[tokio::test]
    async fn max_matches_cap_sets_truncated() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..50 {
            write_file(&dir.path().join(format!("f{i:02}.rs")), "");
        }
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "*.rs", "max_matches": 10}),
                &ctx,
            )
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths.len(), 10);
        assert_eq!(r["truncated"], true);
    }

    #[tokio::test]
    async fn path_scoping_honored() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("outside.rs"), "");
        write_file(&dir.path().join("sub/inside.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "*.rs", "path": "sub"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> = serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["inside.rs".to_string()]);
    }

    #[tokio::test]
    async fn invalid_glob_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let err = t
            .call(serde_json::json!({"pattern": "["}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }
}
