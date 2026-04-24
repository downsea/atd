//! Shared helpers for the fs toolset.

use std::path::{Path, PathBuf};

/// Resolve an input string as a filesystem path. Absolute paths are returned
/// as-is; relative paths are joined with `cwd`. No canonicalization here —
/// the caller does that at the right moment (after existence is known).
pub fn resolve_path(cwd: &Path, input: &str) -> PathBuf {
    let p = Path::new(input);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// Output of line-numbered formatting.
pub struct LineFormatResult {
    pub content: String,
    pub lines_shown: usize,
    pub total_lines: usize,
    pub truncated: bool,
}

/// Format `text` with `"   N\tline\n"` prefixes (N right-padded to 4 chars min).
/// Honors optional 1-indexed `offset` (skip offset-1 leading lines) and `limit`.
/// If appending a line would push output beyond `max_output_bytes`, stop at
/// the current line boundary and set `truncated=true`.
pub fn format_with_line_numbers(
    text: &str,
    offset: usize,
    limit: Option<usize>,
    max_output_bytes: usize,
) -> LineFormatResult {
    let lines: Vec<&str> = text.split('\n').collect();
    // If text ends with \n, the split produces a trailing empty string we
    // should not count as a "line."
    let total_lines = if text.is_empty() {
        0
    } else if text.ends_with('\n') {
        lines.len().saturating_sub(1)
    } else {
        lines.len()
    };

    let start = offset.saturating_sub(1); // 0-indexed
    let iter = lines
        .iter()
        .take(if text.ends_with('\n') {
            lines.len().saturating_sub(1)
        } else {
            lines.len()
        })
        .enumerate()
        .skip(start);

    let mut out = String::new();
    let mut lines_shown = 0usize;
    let mut truncated = false;
    for (zero_idx, line) in iter {
        if let Some(lim) = limit {
            if lines_shown >= lim {
                break;
            }
        }
        let n = zero_idx + 1;
        let prefix = format!("{:>4}\t", n);
        let line_bytes = prefix.len() + line.len() + 1; // + '\n'
        if out.len() + line_bytes > max_output_bytes {
            truncated = true;
            break;
        }
        out.push_str(&prefix);
        out.push_str(line);
        out.push('\n');
        lines_shown += 1;
    }

    LineFormatResult {
        content: out,
        lines_shown,
        total_lines,
        truncated,
    }
}

/// Result of an atomic write.
#[derive(Debug)]
pub struct AtomicWriteResult {
    pub bytes_written: usize,
    pub created: bool,
}

/// Atomic write: create a tempfile in `path`'s parent, write bytes, rename
/// over `path`. Caller is responsible for parent-directory existence checks
/// — this function surfaces the underlying `std::io::Error` if the parent
/// doesn't exist.
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<AtomicWriteResult> {
    use std::io::ErrorKind;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidInput, "path has no parent"))?;

    let created = !path.exists();
    let tmp_name = format!(".atd-ref-write-{}.tmp", ulid::Ulid::new());
    let tmp = parent.join(tmp_name);

    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(AtomicWriteResult {
            bytes_written: bytes.len(),
            created,
        }),
        Err(e) => {
            // Clean up the tempfile on rename failure.
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_absolute_path_unchanged() {
        let cwd = Path::new("/home/u");
        assert_eq!(
            resolve_path(cwd, "/etc/hostname"),
            PathBuf::from("/etc/hostname")
        );
    }

    #[test]
    fn resolve_relative_path_joined_to_cwd() {
        let cwd = Path::new("/home/u");
        assert_eq!(
            resolve_path(cwd, "proj/foo.txt"),
            PathBuf::from("/home/u/proj/foo.txt")
        );
    }

    #[test]
    fn format_with_line_numbers_basic() {
        let r = format_with_line_numbers("a\nb\nc\n", 1, None, 1_000_000);
        assert_eq!(r.content, "   1\ta\n   2\tb\n   3\tc\n");
        assert_eq!(r.lines_shown, 3);
        assert_eq!(r.total_lines, 3);
        assert!(!r.truncated);
    }

    #[test]
    fn format_with_line_numbers_no_trailing_newline() {
        let r = format_with_line_numbers("a\nb", 1, None, 1_000_000);
        assert_eq!(r.total_lines, 2);
        assert_eq!(r.lines_shown, 2);
        assert!(r.content.contains("   2\tb"));
    }

    #[test]
    fn format_with_line_numbers_offset() {
        let r = format_with_line_numbers("a\nb\nc\nd\n", 3, None, 1_000_000);
        assert_eq!(r.content, "   3\tc\n   4\td\n");
        assert_eq!(r.lines_shown, 2);
        assert_eq!(r.total_lines, 4);
    }

    #[test]
    fn format_with_line_numbers_limit() {
        let r = format_with_line_numbers("a\nb\nc\nd\n", 1, Some(2), 1_000_000);
        assert_eq!(r.content, "   1\ta\n   2\tb\n");
        assert_eq!(r.lines_shown, 2);
    }

    #[test]
    fn format_with_line_numbers_truncation_at_byte_budget() {
        let r = format_with_line_numbers("xxxxx\nyyyyy\n", 1, None, 12);
        assert!(r.truncated);
        assert_eq!(r.lines_shown, 1);
        assert!(r.content.starts_with("   1\t"));
    }

    #[test]
    fn format_with_line_numbers_offset_beyond_total_returns_empty() {
        let r = format_with_line_numbers("a\nb\n", 10, None, 1_000_000);
        assert_eq!(r.content, "");
        assert_eq!(r.lines_shown, 0);
        assert_eq!(r.total_lines, 2);
    }

    #[tokio::test]
    async fn atomic_write_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let r = atomic_write(&path, b"hello").await.unwrap();
        assert_eq!(r.bytes_written, 5);
        assert!(r.created);
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn atomic_write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, b"old").unwrap();
        let r = atomic_write(&path, b"new!").await.unwrap();
        assert_eq!(r.bytes_written, 4);
        assert!(!r.created);
        assert_eq!(std::fs::read(&path).unwrap(), b"new!");
    }

    #[tokio::test]
    async fn atomic_write_fails_when_parent_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_such_dir").join("f.txt");
        let err = atomic_write(&path, b"x").await.unwrap_err();
        assert!(
            err.kind() == std::io::ErrorKind::NotFound
                || err.kind() == std::io::ErrorKind::InvalidInput
        );
    }
}
