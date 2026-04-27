//! `atd skills sync` — pull skill files from a connected ATD server via
//! the skills meta-tool convention (`<publisher>:<service>.skills.list/get`)
//! and write them to per-platform install paths.
//!
//! See [`docs/protocol/wire-format.md` §11](../../../docs/protocol/wire-format.md)
//! for the convention contract and SP-skills-discovery-convention for the
//! design rationale.

use std::io::Write;
use std::path::PathBuf;

use atd_protocol::AtdError;
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter};
use serde_json::Value;

use crate::cli::{SkillsSyncArgs, SyncTarget};

pub async fn run(
    client: &AtdClient,
    args: SkillsSyncArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let resolved_out_dir = args
        .out_dir
        .clone()
        .or_else(|| args.target.default_out_dir());

    if matches!(args.target, SyncTarget::Stdout) && args.out_dir.is_some() {
        return Err(AtdError::InvalidArguments {
            tool_id: "atd:skills.sync".into(),
            field: "--out-dir".into(),
            reason: "cannot be combined with --target stdout; pipe instead".into(),
        });
    }

    let tools = client
        .discover(None, DiscoverFilter::default())
        .await?;

    let list_ids: Vec<String> = tools
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| id.ends_with(".skills.list"))
        .collect();

    if list_ids.is_empty() {
        writeln!(
            out,
            "no *.skills.list tool found on this server; nothing to sync"
        )
        .ok();
        return Ok(());
    }

    let mut total_synced = 0usize;
    let publishers = list_ids.len();

    for list_id in &list_ids {
        let prefix = list_id
            .strip_suffix(".skills.list")
            .ok_or_else(|| AtdError::ProtocolError {
                expected: "tool id ending in .skills.list".into(),
                got: list_id.clone(),
            })?;
        let get_id = format!("{prefix}.skills.get");

        let entries = call_list(client, list_id).await?;
        let dir_prefix = prefix.replace(':', "-").replace('.', "-");

        for entry in &entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| AtdError::ProtocolError {
                    expected: "skill summary entry with `name` field".into(),
                    got: entry.to_string(),
                })?;

            let content = call_get(client, &get_id, name).await?;
            write_skill(
                args.target,
                resolved_out_dir.as_ref(),
                &dir_prefix,
                name,
                &content,
                args.dry_run,
                out,
            )?;
            total_synced += 1;
        }
    }

    let dest = resolved_out_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "stdout".into());
    writeln!(
        out,
        "{total_synced} skill(s) synced from {publishers} publisher(s) to {dest}"
    )
    .ok();
    Ok(())
}

async fn call_list(client: &AtdClient, list_id: &str) -> Result<Vec<Value>, AtdError> {
    let result = client
        .call(list_id, serde_json::json!({}), CallOptions::default())
        .await?;
    let data = match result {
        atd_protocol::ToolResult::Success { data, .. } => data,
        atd_protocol::ToolResult::Error { code, message, .. } => {
            return Err(AtdError::ToolExecutionFailed {
                tool_id: list_id.into(),
                inner: Box::new(std::io::Error::other(format!("[{code}] {message}"))),
            });
        }
    };
    data.as_array()
        .cloned()
        .ok_or_else(|| AtdError::ProtocolError {
            expected: "Vec<SkillSummary>".into(),
            got: data.to_string(),
        })
}

async fn call_get(client: &AtdClient, get_id: &str, name: &str) -> Result<String, AtdError> {
    let result = client
        .call(
            get_id,
            serde_json::json!({"name": name}),
            CallOptions::default(),
        )
        .await?;
    let data = match result {
        atd_protocol::ToolResult::Success { data, .. } => data,
        atd_protocol::ToolResult::Error { code, message, .. } => {
            return Err(AtdError::ToolExecutionFailed {
                tool_id: get_id.into(),
                inner: Box::new(std::io::Error::other(format!(
                    "[{code}] {message} (skill: {name})"
                ))),
            });
        }
    };
    data.get("content_md")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| AtdError::ProtocolError {
            expected: "skills.get response with content_md field".into(),
            got: data.to_string(),
        })
}

fn write_skill(
    target: SyncTarget,
    out_dir: Option<&PathBuf>,
    dir_prefix: &str,
    name: &str,
    content: &str,
    dry_run: bool,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let safe_name = sanitize_name(name);
    match target {
        SyncTarget::Stdout => {
            writeln!(out, "--- {dir_prefix}-{safe_name} ---").ok();
            write!(out, "{content}").ok();
            if !content.ends_with('\n') {
                writeln!(out).ok();
            }
            Ok(())
        }
        SyncTarget::Hermes | SyncTarget::ClaudeCode => {
            let base = out_dir.ok_or_else(|| AtdError::InvalidArguments {
                tool_id: "atd:skills.sync".into(),
                field: "--out-dir".into(),
                reason: "no install dir resolved (HOME unset?); supply --out-dir explicitly".into(),
            })?;
            let dir = base.join(format!("{dir_prefix}-{safe_name}"));
            let path = dir.join("SKILL.md");
            if dry_run {
                writeln!(
                    out,
                    "[would write] {} ({} bytes)",
                    path.display(),
                    content.len()
                )
                .ok();
            } else {
                std::fs::create_dir_all(&dir).map_err(|e| AtdError::ToolExecutionFailed {
                    tool_id: "atd:skills.sync".into(),
                    inner: Box::new(e),
                })?;
                std::fs::write(&path, content).map_err(|e| AtdError::ToolExecutionFailed {
                    tool_id: "atd:skills.sync".into(),
                    inner: Box::new(e),
                })?;
                writeln!(out, "[wrote] {}", path.display()).ok();
            }
            Ok(())
        }
    }
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_name_strips_unsafe_chars() {
        assert_eq!(sanitize_name("healthkit-heartrate"), "healthkit-heartrate");
        assert_eq!(sanitize_name("a/b\\c d"), "a_b_c_d");
        assert_eq!(sanitize_name("foo.bar_baz-qux"), "foo.bar_baz-qux");
    }

    #[test]
    fn dir_prefix_replaces_colon_and_dot() {
        let prefix = "huawei:hms.healthkit";
        let normalized = prefix.replace(':', "-").replace('.', "-");
        assert_eq!(normalized, "huawei-hms-healthkit");
    }
}
