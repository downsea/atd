//! Shared subprocess handler for shell tools.
//!
//! Responsibilities:
//! - Spawn the given program + args with piped stdout/stderr, null stdin.
//! - Concurrently drain stdout and stderr into byte buffers, each capped at
//!   its respective byte budget. Continue reading past the cap until EOF so
//!   the child doesn't block on full pipe buffers; bytes past the cap are
//!   discarded.
//! - Wait for the child with an optional absolute deadline. On timeout:
//!   Unix: SIGTERM, then sleep grace_ms, then Child::kill (SIGKILL).
//!   Windows: Child::kill directly.
//! - UTF-8-lossy decode both streams; return exit code + truncation flags +
//!   duration.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct RunRequest<'a> {
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub cwd: &'a Path,
    pub deadline: Option<Instant>,
    pub grace_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

#[derive(Debug)]
pub struct RunOutput {
    /// Exit code, or `None` if process was killed by signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stdout_truncated: bool,
    pub stderr: String,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("program not found: {program}")]
    NotFound { program: String },

    #[error("spawn failed: {0}")]
    SpawnFailed(#[source] std::io::Error),

    #[error("command timed out after {after_ms}ms")]
    TimedOut { after_ms: u64 },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn run(req: RunRequest<'_>) -> Result<RunOutput, RunError> {
    let start = Instant::now();

    let mut cmd = tokio::process::Command::new(req.program);
    cmd.args(req.args)
        .current_dir(req.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(RunError::NotFound {
                program: req.program.to_string(),
            });
        }
        Err(e) => return Err(RunError::SpawnFailed(e)),
    };

    let stdout = child.stdout.take().expect("stdout was set to piped");
    let stderr = child.stderr.take().expect("stderr was set to piped");

    let max_stdout = req.max_stdout_bytes;
    let max_stderr = req.max_stderr_bytes;
    let stdout_task = tokio::spawn(read_capped(stdout, max_stdout));
    let stderr_task = tokio::spawn(read_capped(stderr, max_stderr));

    // Wait for the child, honoring the optional deadline.
    let status_result = match req.deadline {
        Some(deadline) => {
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::timeout(remaining, child.wait()).await
        }
        None => Ok(child.wait().await),
    };

    let status = match status_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(RunError::Io(e)),
        Err(_elapsed) => {
            // Deadline hit. SIGTERM → grace → SIGKILL.
            #[cfg(unix)]
            {
                if let Some(pid) = child.id() {
                    // Safe: pid is the actual child's PID, we're sending a
                    // standard termination signal. If the process has already
                    // exited, kill() returns -1 and we fall through.
                    unsafe {
                        libc::kill(pid as libc::pid_t, libc::SIGTERM);
                    }
                    tokio::time::sleep(Duration::from_millis(req.grace_ms)).await;
                }
            }
            // Either the grace expired, or we're on Windows — force kill.
            let _ = child.start_kill();
            let _ = child.wait().await;
            // Drain the readers so their tasks complete.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(RunError::TimedOut {
                after_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    // Process finished; harvest the readers.
    let (stdout_bytes, stdout_truncated) = stdout_task
        .await
        .unwrap_or_else(|_| (Vec::new(), false));
    let (stderr_bytes, stderr_truncated) = stderr_task
        .await
        .unwrap_or_else(|_| (Vec::new(), false));

    Ok(RunOutput {
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stdout_truncated,
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        stderr_truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Drain `reader` into a Vec<u8>. Stop storing once `max` bytes are
/// captured, but keep reading to EOF (so the writer doesn't block on a
/// full pipe buffer) — discard the excess.
async fn read_capped<R>(mut reader: R, max: usize) -> (Vec<u8>, bool)
where
    R: AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() >= max {
                    truncated = true;
                    continue;
                }
                let room = max - buf.len();
                if n <= room {
                    buf.extend_from_slice(&chunk[..n]);
                } else {
                    buf.extend_from_slice(&chunk[..room]);
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    #[tokio::test]
    async fn happy_run_returns_stdout_and_exit_zero() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "echo hello"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout, "hello\n");
        assert_eq!(out.stderr, "");
        assert!(!out.stdout_truncated);
    }

    #[tokio::test]
    async fn nonzero_exit_code_returned_as_ok() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "exit 3"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(3));
    }

    #[tokio::test]
    async fn stderr_captured_separately() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", ">&2 echo oops; echo good"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.stdout, "good\n");
        assert_eq!(out.stderr, "oops\n");
    }

    #[tokio::test]
    async fn stdout_truncated_at_budget() {
        // Print 10 KB to stdout with 1 KB budget.
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "yes x | head -c 10240"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap();
        assert!(out.stdout_truncated);
        assert!(out.stdout.len() <= 1024);
    }

    #[tokio::test]
    async fn stderr_truncated_at_budget() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "yes x | head -c 10240 >&2"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap();
        assert!(out.stderr_truncated);
        assert!(out.stderr.len() <= 1024);
    }

    #[tokio::test]
    async fn timeout_triggers_sigterm_then_sigkill() {
        let start = Instant::now();
        let err = run(RunRequest {
            program: "bash",
            args: &["-c", "sleep 10"],
            cwd: &cwd(),
            deadline: Some(Instant::now() + Duration::from_millis(200)),
            grace_ms: 100,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap_err();
        let elapsed = start.elapsed();
        match err {
            RunError::TimedOut { .. } => {}
            _ => panic!("expected TimedOut, got {err:?}"),
        }
        // Should have killed within ~deadline + grace, certainly less than the 10s sleep.
        assert!(elapsed < Duration::from_secs(2), "took too long: {elapsed:?}");
    }

    #[tokio::test]
    async fn bogus_program_returns_not_found() {
        let err = run(RunRequest {
            program: "this-program-definitely-does-not-exist-xyzzy",
            args: &[],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap_err();
        match err {
            RunError::NotFound { program } => {
                assert!(program.contains("xyzzy"));
            }
            _ => panic!("expected NotFound, got {err:?}"),
        }
    }

    #[tokio::test]
    async fn cwd_is_honored() {
        // Run `pwd` in a tempdir; output should be that tempdir's canonical path.
        let dir = tempfile::tempdir().unwrap();
        let canonical = tokio::fs::canonicalize(dir.path()).await.unwrap();
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "pwd"],
            cwd: &canonical,
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        let printed = out.stdout.trim_end();
        assert_eq!(printed, canonical.to_string_lossy());
    }
}
