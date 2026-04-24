//! `ref:fs.read` — read a UTF-8 file with line numbers.

use std::sync::OnceLock;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{format_with_line_numbers, resolve_path};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.read".into(),
        name: "Read File".into(),
        description: "Read a UTF-8 text file with 1-indexed line numbers. Supports offset/limit and honors ctx.max_output_bytes via byte-budget truncation at line boundaries.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["read".into()],
            tags: vec!["file".into(), "filesystem".into(), "read".into()],
            intent_examples: vec![
                "read /etc/hostname".into(),
                "show me the file at src/main.rs".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":   { "type": "string", "minLength": 1 },
                "offset": { "type": "integer", "minimum": 1 },
                "limit":  { "type": "integer", "minimum": 1 }
            },
            "required": ["path"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":        { "type": "string" },
                "content":     { "type": "string" },
                "line_count":  { "type": "integer" },
                "total_lines": { "type": "integer" },
                "truncated":   { "type": "boolean" }
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
            data_sensitivity: Some("file contents".into()),
        },
        resources: ToolResources {
            timeout_ms: 10_000,
            max_concurrent: 50,
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

pub struct FsReadTool;

impl FsReadTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

impl Tool for FsReadTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: ReadArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if matches!(args.offset, Some(0)) || matches!(args.limit, Some(0)) {
                return Err(ToolCallError::InvalidArgs(
                    "offset/limit must be >= 1".into(),
                ));
            }

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let canonical = match tokio::fs::canonicalize(&resolved).await {
                Ok(p) => p,
                Err(e) => return Err(io_to_tool_err(&resolved, e)),
            };

            let meta = match tokio::fs::metadata(&canonical).await {
                Ok(m) => m,
                Err(e) => return Err(io_to_tool_err(&canonical, e)),
            };
            if meta.is_dir() {
                return Err(ToolCallError::ExecutionFailed {
                    code: "IS_DIR".into(),
                    message: format!("path is a directory: {}", canonical.display()),
                    retryable: false,
                });
            }
            let size = meta.len();
            let mtime = meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("mtime: {e}"),
                retryable: true,
            })?;

            let bytes = match tokio::fs::read(&canonical).await {
                Ok(b) => b,
                Err(e) => return Err(io_to_tool_err(&canonical, e)),
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "ENCODING".into(),
                        message: format!("not valid UTF-8 at byte {}", e.valid_up_to()),
                        retryable: false,
                    });
                }
            };

            let offset = args.offset.unwrap_or(1);
            let formatted = format_with_line_numbers(&text, offset, args.limit, ctx.max_output_bytes);

            // Record in tracker (if any).
            if let Some(tracker) = &ctx.read_tracker {
                tracker.record(canonical.clone(), mtime, size);
            }

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "content": formatted.content,
                "line_count": formatted.lines_shown,
                "total_lines": formatted.total_lines,
                "truncated": formatted.truncated,
            }))
        })
    }
}

fn io_to_tool_err(path: &std::path::Path, e: std::io::Error) -> ToolCallError {
    use std::io::ErrorKind;
    let (code, retryable) = match e.kind() {
        ErrorKind::NotFound => ("NOT_FOUND", false),
        ErrorKind::PermissionDenied => ("EACCES", false),
        _ => ("IO", true),
    };
    ToolCallError::ExecutionFailed {
        code: code.into(),
        message: format!("{}: {}", path.display(), e),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write_tmp(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn read_happy_path() {
        let (_dir, path) = write_tmp("hello\nworld\n").await;
        let t = FsReadTool::new();
        let (ctx, _tr) = CallContext::for_test_with_tracker();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        assert_eq!(r["total_lines"], 2);
        assert!(r["content"].as_str().unwrap().contains("   1\thello"));
        assert!(r["content"].as_str().unwrap().contains("   2\tworld"));
        assert_eq!(r["truncated"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn read_with_offset_skips_leading_lines() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 3}),
                &ctx,
            )
            .await
            .unwrap();
        let content = r["content"].as_str().unwrap();
        assert!(!content.contains("   1\ta"));
        assert!(content.contains("   3\tc"));
        assert!(content.contains("   4\td"));
    }

    #[tokio::test]
    async fn read_with_limit_caps_lines() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        assert_eq!(r["total_lines"], 4);
    }

    #[tokio::test]
    async fn read_with_offset_and_limit() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 2, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        let content = r["content"].as_str().unwrap();
        assert!(content.contains("   2\tb"));
        assert!(content.contains("   3\tc"));
        assert!(!content.contains("   1\ta"));
        assert!(!content.contains("   4\td"));
    }

    #[tokio::test]
    async fn read_nonexistent_returns_not_found() {
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": "/tmp/atd-ref-does-not-exist-xxxxx"}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "NOT_FOUND");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_directory_returns_is_dir() {
        let dir = tempfile::tempdir().unwrap();
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": dir.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "IS_DIR");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_non_utf8_returns_encoding_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, &[0xff, 0xfe, 0xfd]).unwrap();
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "ENCODING");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_offset_zero_is_invalid_args() {
        let (_dir, path) = write_tmp("x\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 0}),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn read_records_in_tracker() {
        let (_dir, path) = write_tmp("one\n").await;
        let t = FsReadTool::new();
        let (ctx, tr) = CallContext::for_test_with_tracker();
        t.call(
            serde_json::json!({"path": path.to_string_lossy()}),
            &ctx,
        )
        .await
        .unwrap();
        // After Read, tracker.check with current stat should succeed.
        let canonical = tokio::fs::canonicalize(&path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        tr.check(&canonical, meta.modified().unwrap(), meta.len())
            .unwrap();
    }

    #[tokio::test]
    async fn read_truncates_when_over_max_output_bytes() {
        let big = "x".repeat(200);
        let (_dir, path) = write_tmp(&format!("{big}\n{big}\n")).await;
        let t = FsReadTool::new();
        // Budget tiny so second line can't fit.
        let mut ctx = CallContext::for_test();
        ctx.max_output_bytes = 220;
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["truncated"], serde_json::json!(true));
        assert!(r["line_count"].as_u64().unwrap() < r["total_lines"].as_u64().unwrap());
    }
}
