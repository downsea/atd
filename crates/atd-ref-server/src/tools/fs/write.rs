//! `ref:fs.write` — atomic write of a UTF-8 file.

use std::sync::OnceLock;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{atomic_write, resolve_path};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.write".into(),
        name: "Write File".into(),
        description: "Atomically write text content to a file (tempfile + rename). Parent directory must already exist.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["write".into()],
            tags: vec!["file".into(), "filesystem".into(), "write".into()],
            intent_examples: vec!["write config.toml".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":    { "type": "string", "minLength": 1 },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":          { "type": "string" },
                "bytes_written": { "type": "integer" },
                "created":       { "type": "boolean" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Write,
            dry_run: true,
            side_effects: vec!["filesystem".into()],
            data_sensitivity: Some("file contents".into()),
        },
        resources: ToolResources {
            timeout_ms: 10_000,
            max_concurrent: 20,
            rate_limit_per_min: None,
            estimated_tokens: Some(200),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Write,
        required_capabilities: vec![],
        tier: None,
    })
}

pub struct FsWriteTool;

impl FsWriteTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
}

impl Tool for FsWriteTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: WriteArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let parent = resolved.parent().ok_or_else(|| ToolCallError::ExecutionFailed {
                code: "NO_PARENT".into(),
                message: format!("path has no parent: {}", resolved.display()),
                retryable: false,
            })?;
            if !parent.exists() {
                return Err(ToolCallError::ExecutionFailed {
                    code: "NO_PARENT".into(),
                    message: format!("parent directory does not exist: {}", parent.display()),
                    retryable: false,
                });
            }

            let result = atomic_write(&resolved, args.content.as_bytes())
                .await
                .map_err(|e| {
                    use std::io::ErrorKind;
                    let (code, retryable) = match e.kind() {
                        ErrorKind::PermissionDenied => ("EACCES", false),
                        _ => ("IO", true),
                    };
                    ToolCallError::ExecutionFailed {
                        code: code.into(),
                        message: format!("{}: {e}", resolved.display()),
                        retryable,
                    }
                })?;

            // Canonicalize AFTER the write so the result's `path` is stable.
            let canonical = tokio::fs::canonicalize(&resolved).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("canonicalize after write: {e}"),
                    retryable: true,
                }
            })?;

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "bytes_written": result.bytes_written,
                "created": result.created,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "hello world"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["bytes_written"], 11);
        assert_eq!(r["created"], serde_json::json!(true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "old").unwrap();
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "new content"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["created"], serde_json::json!(false));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
    }

    #[tokio::test]
    async fn write_fails_when_parent_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_such_dir").join("f.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "NO_PARENT"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn write_bytes_written_matches_content_len() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("size.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let content = "héllo"; // 6 UTF-8 bytes
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": content
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["bytes_written"], 6);
    }

    #[tokio::test]
    async fn write_does_not_record_in_tracker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.txt");
        let t = FsWriteTool::new();
        let (ctx, tr) = CallContext::for_test_with_tracker();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "content": "x"
            }),
            &ctx,
        )
        .await
        .unwrap();
        // Tracker should NOT have recorded anything — Write doesn't satisfy
        // the "read before edit" contract.
        let canonical = tokio::fs::canonicalize(&path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        let err = tr
            .check(&canonical, meta.modified().unwrap(), meta.len())
            .unwrap_err();
        assert!(matches!(
            err,
            crate::tracker::ReadTrackerError::NotRead { .. }
        ));
    }

    #[tokio::test]
    async fn write_returns_canonical_path() {
        let dir = tempfile::tempdir().unwrap();
        // Path via cwd resolution
        let cwd = dir.path().to_path_buf();
        let t = FsWriteTool::new();
        let mut ctx = CallContext::for_test();
        ctx.cwd = cwd.clone();
        let r = t
            .call(
                serde_json::json!({
                    "path": "rel.txt",
                    "content": "rel"
                }),
                &ctx,
            )
            .await
            .unwrap();
        // The returned path should be canonical (absolute, no components).
        let ret = r["path"].as_str().unwrap();
        assert!(std::path::Path::new(ret).is_absolute());
        assert!(ret.ends_with("rel.txt"));
    }
}
