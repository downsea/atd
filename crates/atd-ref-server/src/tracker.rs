//! Per-connection record of which files have been Read + their observed
//! mtime/size at Read time. Edit uses this to enforce "you must Read before
//! Edit, and the file mustn't have changed since then."

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use thiserror::Error;

#[derive(Debug, Clone, Copy)]
struct ReadRecord {
    mtime: SystemTime,
    size: u64,
}

#[derive(Debug, Error)]
pub enum ReadTrackerError {
    #[error("file has not been read in this session: {path}")]
    NotRead { path: PathBuf },

    #[error("file modified since it was read: {path}")]
    Modified { path: PathBuf },
}

pub struct ReadTracker {
    entries: Mutex<HashMap<PathBuf, ReadRecord>>,
}

impl ReadTracker {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record a successful read. `path` should already be canonicalized by
    /// the caller.
    pub fn record(&self, path: PathBuf, mtime: SystemTime, size: u64) {
        let mut g = self.entries.lock().expect("tracker mutex poisoned");
        g.insert(path, ReadRecord { mtime, size });
    }

    /// Verify that `path` has been read in this session AND its current
    /// mtime + size match what was recorded.
    ///
    /// Caller passes the current stat to avoid racing a syscall inside the
    /// lock. `path` must already be canonicalized.
    pub fn check(
        &self,
        path: &Path,
        current_mtime: SystemTime,
        current_size: u64,
    ) -> Result<(), ReadTrackerError> {
        let g = self.entries.lock().expect("tracker mutex poisoned");
        match g.get(path) {
            None => Err(ReadTrackerError::NotRead { path: path.to_path_buf() }),
            Some(rec) => {
                if rec.mtime != current_mtime || rec.size != current_size {
                    Err(ReadTrackerError::Modified { path: path.to_path_buf() })
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Default for ReadTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn check_unrecorded_path_returns_not_read() {
        let tr = ReadTracker::new();
        let err = tr.check(Path::new("/tmp/nope"), t(1), 10).unwrap_err();
        assert!(matches!(err, ReadTrackerError::NotRead { .. }));
    }

    #[test]
    fn record_then_check_same_stat_is_ok() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        tr.check(&p, t(100), 42).unwrap();
    }

    #[test]
    fn check_returns_modified_when_mtime_changed() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        let err = tr.check(&p, t(200), 42).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
    }

    #[test]
    fn check_returns_modified_when_size_changed() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        let err = tr.check(&p, t(100), 100).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
    }

    #[test]
    fn record_overwrites_prior_entry() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        tr.record(p.clone(), t(200), 84);
        // The new record is the one that matches.
        tr.check(&p, t(200), 84).unwrap();
        // The old one doesn't.
        let err = tr.check(&p, t(100), 42).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
    }
}
