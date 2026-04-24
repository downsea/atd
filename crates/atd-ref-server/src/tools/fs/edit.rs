//! `ref:fs.edit` — exact-string find-and-replace with must-read-first invariant.

use std::sync::OnceLock;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{atomic_write, resolve_path};
use crate::tracker::ReadTrackerError;

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.edit".into(),
        name: "Edit File".into(),
        description: "Exact-string find-and-replace in a UTF-8 file. Requires the file to have been Read in this session and unchanged since. Ambiguous (multi-match) edits without replace_all=true are rejected.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["edit".into()],
            tags: vec!["file".into(), "filesystem".into(), "edit".into()],
            intent_examples: vec!["change 'old_name' to 'new_name' in main.rs".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":        { "type": "string", "minLength": 1 },
                "old_string":  { "type": "string", "minLength": 1 },
                "new_string":  { "type": "string" },
                "replace_all": { "type": "boolean", "default": false }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":          { "type": "string" },
                "replacements":  { "type": "integer" },
                "bytes_written": { "type": "integer" }
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
            estimated_tokens: Some(300),
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

pub struct FsEditTool;

impl FsEditTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsEditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct EditArgs {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

impl Tool for FsEditTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: EditArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;

            let tracker = ctx.read_tracker.as_ref().ok_or_else(|| {
                ToolCallError::InternalError(
                    "server did not attach a read_tracker to CallContext".into(),
                )
            })?;

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let canonical = tokio::fs::canonicalize(&resolved).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: match e.kind() {
                        std::io::ErrorKind::NotFound => "NOT_FOUND",
                        _ => "IO",
                    }
                    .into(),
                    message: format!("{}: {e}", resolved.display()),
                    retryable: matches!(e.kind(), std::io::ErrorKind::Interrupted),
                }
            })?;

            let meta = tokio::fs::metadata(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("metadata: {e}"),
                    retryable: true,
                }
            })?;
            let size = meta.len();
            let mtime = meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("mtime: {e}"),
                retryable: true,
            })?;

            // Must-read-before-edit + unchanged-since-read checks.
            match tracker.check(&canonical, mtime, size) {
                Ok(()) => {}
                Err(ReadTrackerError::NotRead { .. }) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "NOT_READ".into(),
                        message: format!(
                            "call ref:fs.read on {} first",
                            canonical.display()
                        ),
                        retryable: false,
                    });
                }
                Err(ReadTrackerError::Modified { .. }) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "FILE_MODIFIED".into(),
                        message: format!(
                            "file {} changed since it was read; call ref:fs.read again",
                            canonical.display()
                        ),
                        retryable: false,
                    });
                }
            }

            // Read current contents.
            let bytes = tokio::fs::read(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("read: {e}"),
                    retryable: true,
                }
            })?;
            let text = std::str::from_utf8(&bytes).map_err(|e| ToolCallError::ExecutionFailed {
                code: "ENCODING".into(),
                message: format!("not valid UTF-8 at byte {}", e.valid_up_to()),
                retryable: false,
            })?;

            // Count matches.
            let match_count = text.matches(&args.old_string).count();
            if match_count == 0 {
                return Err(ToolCallError::InvalidArgs(
                    "old_string not found in file".into(),
                ));
            }
            if match_count >= 2 && !args.replace_all {
                return Err(ToolCallError::InvalidArgs(format!(
                    "{match_count} occurrences of old_string; supply more context or set replace_all=true"
                )));
            }

            // Replace.
            let new_text = if args.replace_all {
                text.replace(&args.old_string, &args.new_string)
            } else {
                // exactly one match
                text.replacen(&args.old_string, &args.new_string, 1)
            };

            // Atomic write.
            let wr = atomic_write(&canonical, new_text.as_bytes())
                .await
                .map_err(|e| ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("write: {e}"),
                    retryable: true,
                })?;

            // Re-record the post-write state so immediate subsequent Edits on
            // the same file don't hit FILE_MODIFIED.
            let new_meta = tokio::fs::metadata(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("post-write metadata: {e}"),
                    retryable: true,
                }
            })?;
            let new_mtime = new_meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("post-write mtime: {e}"),
                retryable: true,
            })?;
            tracker.record(canonical.clone(), new_mtime, new_meta.len());

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "replacements": match_count,
                "bytes_written": wr.bytes_written,
            }))
        })
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

    async fn ctx_with_read(path: &std::path::Path) -> (CallContext, std::sync::Arc<crate::tracker::ReadTracker>) {
        let (ctx, tr) = CallContext::for_test_with_tracker();
        let canonical = tokio::fs::canonicalize(path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        tr.record(canonical, meta.modified().unwrap(), meta.len());
        (ctx, tr)
    }

    #[tokio::test]
    async fn edit_single_match_replaces() {
        let (_dir, path) = write_tmp("hello world\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "HI"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["replacements"], 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "HI world\n");
    }

    #[tokio::test]
    async fn edit_without_prior_read_returns_not_read() {
        let (_dir, path) = write_tmp("hello\n").await;
        // Tracker empty — no Read was recorded.
        let (ctx, _tr) = CallContext::for_test_with_tracker();
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "hi"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "NOT_READ"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_multi_match_without_replace_all_is_invalid_args() {
        let (_dir, path) = write_tmp("foo foo foo\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::InvalidArgs(msg) => {
                assert!(msg.contains("3"));
                assert!(msg.contains("replace_all"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_multi_match_with_replace_all_succeeds() {
        let (_dir, path) = write_tmp("foo foo foo\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["replacements"], 3);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "bar bar bar\n");
    }

    #[tokio::test]
    async fn edit_zero_match_is_invalid_args() {
        let (_dir, path) = write_tmp("hello\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "nope",
                    "new_string": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn edit_detects_external_modification_after_read() {
        let (_dir, path) = write_tmp("hello\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        // Simulate external change after Read: overwrite + sleep briefly so
        // mtime moves (filesystems with 1s resolution need the sleep).
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        std::fs::write(&path, "externally changed\n").unwrap();
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "externally",
                    "new_string": "xxx"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "FILE_MODIFIED"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_non_utf8_returns_encoding_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, &[0xff, 0xfe, 0xfd]).unwrap();
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "x",
                    "new_string": "y"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "ENCODING"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_re_records_so_second_edit_works() {
        let (_dir, path) = write_tmp("aaa bbb\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "aaa",
                "new_string": "AAA"
            }),
            &ctx,
        )
        .await
        .unwrap();
        // Without re-recording, this second edit would see FILE_MODIFIED.
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "bbb",
                "new_string": "BBB"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "AAA BBB\n");
    }

    #[tokio::test]
    async fn edit_without_tracker_attached_is_internal_error() {
        let (_dir, path) = write_tmp("hello\n").await;
        let ctx = CallContext::for_test(); // no tracker
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "hi"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InternalError(_)));
    }

    #[tokio::test]
    async fn edit_bytes_written_matches_new_content() {
        let (_dir, path) = write_tmp("abcdef\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "abc",
                    "new_string": "ABCDEF"
                }),
                &ctx,
            )
            .await
            .unwrap();
        // New content: "ABCDEFdef\n" = 10 bytes
        assert_eq!(r["bytes_written"], 10);
    }

    #[tokio::test]
    async fn edit_at_start_of_file_works() {
        let (_dir, path) = write_tmp("start middle end\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "start",
                "new_string": "BEGIN"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "BEGIN middle end\n");
    }

    #[tokio::test]
    async fn edit_at_end_of_file_works() {
        let (_dir, path) = write_tmp("start middle end").await; // no trailing newline
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "end",
                "new_string": "FINISH"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "start middle FINISH");
    }

    #[tokio::test]
    async fn edit_with_empty_new_string_deletes() {
        let (_dir, path) = write_tmp("keep_me_remove_me\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "_remove_me",
                "new_string": ""
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep_me\n");
    }
}
