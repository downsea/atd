//! Disk-backed [`TokenBroker`] for adopters that need cross-process
//! persistence (Phase L.0 deliverable, `docs/issues/`
//! 2026-04-24-security-capability-tokens-deferred path, and the
//! `atd-mvp#6` GitHub issue).
//!
//! ## Layout
//!
//! ```text
//! ${root}/
//!   ${bearer_id}/                  (mode 0700)
//!     access_token.json            (mode 0600)
//!     refresh_token.json           (mode 0600)
//!     expires_at.json              (mode 0600)
//! ```
//!
//! Each JSON file holds a single string. The split-file shape matches
//! the per-tenant on-disk layout the adopters already use in
//! `healthkit_cli` (single-tenant since v1.2.0), so the disk migration
//! when adopters switch from `InMemoryTokenBroker` to this is purely
//! additive — drop the existing three files into a `${bearer_id}/`
//! subdir.
//!
//! ## Refresh coordination
//!
//! [`FileTokenBroker::lock_refresh`] hands out a per-bearer mutex so
//! parallel OAuth-refresh attempts for the same bearer can't both
//! round-trip to the upstream provider. Adopters acquire the guard,
//! re-check whether refresh is still needed (some other task may have
//! finished while we were waiting), call their provider, then
//! `put()` the new record. Other concurrent `resolve()` calls for
//! the same bearer continue to serve the pre-refresh secrets — the
//! lock guards the refresh *flow*, not the read path. Drop the guard
//! to release.
//!
//! [`FileTokenBroker::is_near_expiry`] is the predicate adopters call
//! to decide whether to take the refresh path at all. It's a no-IO
//! check against the in-memory cache populated by `put`.
//!
//! ## What this is NOT
//!
//! - Not an OAuth client. Vendor-specific refresh flows live in adopter
//!   code; the broker is purely the persistent token store + refresh
//!   serialization point.
//! - Not encrypted at rest. File permissions (0600) are the access
//!   control. Adopters running on a multi-user host should layer an
//!   FS-encryption story (`fscrypt`, LUKS, etc.) underneath.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

use crate::secrets::{BrokerError, RedactedString, ResolveFuture, SecretBundle, TokenBroker};

/// Default refresh window — `resolve()` callers should treat a bearer
/// as "needs refresh" if `expires_at - now < this`. Matches
/// `healthkit_cli`'s single-tenant value (`auth::REFRESH_GUARD`).
pub const DEFAULT_REFRESH_WINDOW: Duration = Duration::from_secs(5 * 60);

/// On-disk shape persisted to `${root}/${bearer_id}/...`. Public so
/// adopters can construct records for `put()` and compare what they
/// read back from `read_record()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTokenRecord {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix epoch seconds at which `access_token` ceases to be valid.
    /// Stored as `u64` (no JSON Number precision loss for any
    /// realistic OAuth expiry).
    pub expires_at_unix: u64,
}

impl FileTokenRecord {
    pub fn expires_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.expires_at_unix)
    }

    pub fn from_expires_at(
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
        expires_at: SystemTime,
    ) -> Self {
        let expires_at_unix = expires_at
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            access_token: access_token.into(),
            refresh_token: refresh_token.into(),
            expires_at_unix,
        }
    }
}

#[derive(Clone)]
struct CachedEntry {
    bundle: Arc<SecretBundle>,
    expires_at: SystemTime,
}

pub struct FileTokenBroker {
    root: PathBuf,
    refresh_window: Duration,
    cache: RwLock<HashMap<String, CachedEntry>>,
    refresh_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl FileTokenBroker {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            refresh_window: DEFAULT_REFRESH_WINDOW,
            cache: RwLock::new(HashMap::new()),
            refresh_locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_refresh_window(mut self, window: Duration) -> Self {
        self.refresh_window = window;
        self
    }

    pub fn refresh_window(&self) -> Duration {
        self.refresh_window
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn bearer_dir(&self, bearer_id: &str) -> PathBuf {
        self.root.join(bearer_id)
    }

    /// Write (or overwrite) the three-file record for `bearer_id`.
    pub async fn put(&self, bearer_id: &str, rec: FileTokenRecord) -> io::Result<()> {
        let dir = self.bearer_dir(bearer_id);
        tokio::fs::create_dir_all(&dir).await?;
        #[cfg(unix)]
        set_unix_mode(&dir, 0o700).await?;

        write_secret_file(&dir.join("access_token.json"), &rec.access_token).await?;
        write_secret_file(&dir.join("refresh_token.json"), &rec.refresh_token).await?;
        write_secret_file(
            &dir.join("expires_at.json"),
            &rec.expires_at_unix.to_string(),
        )
        .await?;

        let bundle = bundle_from_record(&rec);
        self.cache.write().await.insert(
            bearer_id.to_string(),
            CachedEntry {
                bundle: Arc::new(bundle),
                expires_at: rec.expires_at(),
            },
        );
        Ok(())
    }

    /// Read the three-file record for `bearer_id` from disk. Does NOT
    /// touch the in-memory cache; useful for verifying persisted state
    /// in tests or for adopters that want to force a re-read.
    pub async fn read_record(&self, bearer_id: &str) -> io::Result<Option<FileTokenRecord>> {
        let dir = self.bearer_dir(bearer_id);
        if !tokio::fs::try_exists(&dir).await? {
            return Ok(None);
        }
        let access_token = read_secret_file(&dir.join("access_token.json")).await?;
        let refresh_token = read_secret_file(&dir.join("refresh_token.json")).await?;
        let expires_at_unix = read_secret_file(&dir.join("expires_at.json"))
            .await?
            .parse::<u64>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(FileTokenRecord {
            access_token,
            refresh_token,
            expires_at_unix,
        }))
    }

    /// Cache-only predicate. Returns true when the cached `expires_at`
    /// for this bearer is `<= now + refresh_window`. Returns false if
    /// the bearer is not in cache (the caller should `resolve()` first
    /// to populate it, or treat unknown as "not near expiry").
    pub async fn is_near_expiry(&self, bearer_id: &str) -> bool {
        let now = SystemTime::now();
        match self.cache.read().await.get(bearer_id) {
            Some(entry) => entry
                .expires_at
                .duration_since(now)
                .map(|remaining| remaining <= self.refresh_window)
                .unwrap_or(true),
            None => false,
        }
    }

    /// Acquire the per-bearer refresh mutex. Other concurrent acquirers
    /// for the same `bearer_id` block until this guard drops; acquirers
    /// for *different* bearers are unaffected. `resolve()` is NOT
    /// gated on this lock — readers continue to serve pre-refresh
    /// secrets.
    pub async fn lock_refresh(&self, bearer_id: &str) -> OwnedMutexGuard<()> {
        let arc = {
            let mut map = self.refresh_locks.lock().await;
            map.entry(bearer_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        arc.lock_owned().await
    }
}

impl TokenBroker for FileTokenBroker {
    fn resolve<'a>(&'a self, caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async move {
            let Some(id) = caller_id else {
                return Ok(None);
            };
            if let Some(entry) = self.cache.read().await.get(id).cloned() {
                return Ok(Some(entry.bundle));
            }
            match self.read_record(id).await {
                Ok(Some(rec)) => {
                    let bundle = Arc::new(bundle_from_record(&rec));
                    self.cache.write().await.insert(
                        id.to_string(),
                        CachedEntry {
                            bundle: bundle.clone(),
                            expires_at: rec.expires_at(),
                        },
                    );
                    Ok(Some(bundle))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(BrokerError::Lookup(format!(
                    "file broker read failed for {id}: {e}"
                ))),
            }
        })
    }

    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &["opaque"]
    }
}

fn bundle_from_record(rec: &FileTokenRecord) -> SecretBundle {
    let mut b = SecretBundle::new();
    b.insert(
        "access_token".to_string(),
        RedactedString::new(rec.access_token.clone()),
    );
    b.insert(
        "refresh_token".to_string(),
        RedactedString::new(rec.refresh_token.clone()),
    );
    b
}

async fn write_secret_file(path: &Path, contents: &str) -> io::Result<()> {
    let json = serde_json::to_string(contents).map_err(io::Error::other)?;
    tokio::fs::write(path, json.as_bytes()).await?;
    #[cfg(unix)]
    set_unix_mode(path, 0o600).await?;
    Ok(())
}

async fn read_secret_file(path: &Path) -> io::Result<String> {
    let raw = tokio::fs::read_to_string(path).await?;
    serde_json::from_str::<String>(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(unix)]
async fn set_unix_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(mode);
    tokio::fs::set_permissions(path, perms).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn now_plus(secs: u64) -> SystemTime {
        SystemTime::now() + Duration::from_secs(secs)
    }

    #[tokio::test]
    async fn put_then_resolve_returns_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("acc-A", "ref-A", now_plus(3600)),
            )
            .await
            .unwrap();
        let bundle = broker
            .resolve(Some("agent-A"))
            .await
            .unwrap()
            .expect("bundle present");
        assert_eq!(bundle.get("access_token").unwrap().expose(), "acc-A");
        assert_eq!(bundle.get("refresh_token").unwrap().expose(), "ref-A");
    }

    #[tokio::test]
    async fn resolve_unknown_bearer_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        assert!(
            broker
                .resolve(Some("does-not-exist"))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn resolve_anonymous_caller_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("a", "r", now_plus(3600)),
            )
            .await
            .unwrap();
        assert!(broker.resolve(None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn read_record_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        let rec = FileTokenRecord::from_expires_at("aaa", "rrr", now_plus(7200));
        broker.put("agent-A", rec.clone()).await.unwrap();
        let back = broker.read_record("agent-A").await.unwrap().unwrap();
        assert_eq!(back, rec);
    }

    #[tokio::test]
    async fn persistence_survives_broker_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let b1 = FileTokenBroker::new(dir.path());
            b1.put(
                "agent-A",
                FileTokenRecord::from_expires_at("acc1", "ref1", now_plus(3600)),
            )
            .await
            .unwrap();
        }
        let b2 = FileTokenBroker::new(dir.path());
        let bundle = b2.resolve(Some("agent-A")).await.unwrap().unwrap();
        assert_eq!(bundle.get("access_token").unwrap().expose(), "acc1");
    }

    #[tokio::test]
    async fn cache_isolated_between_bearers() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("acc-A", "ref-A", now_plus(3600)),
            )
            .await
            .unwrap();
        broker
            .put(
                "agent-B",
                FileTokenRecord::from_expires_at("acc-B", "ref-B", now_plus(3600)),
            )
            .await
            .unwrap();
        let a = broker.resolve(Some("agent-A")).await.unwrap().unwrap();
        let b = broker.resolve(Some("agent-B")).await.unwrap().unwrap();
        assert_eq!(a.get("access_token").unwrap().expose(), "acc-A");
        assert_eq!(b.get("access_token").unwrap().expose(), "acc-B");
    }

    #[tokio::test]
    async fn is_near_expiry_true_inside_window() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path()).with_refresh_window(Duration::from_secs(300));
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("a", "r", now_plus(120)),
            )
            .await
            .unwrap();
        assert!(broker.is_near_expiry("agent-A").await);
    }

    #[tokio::test]
    async fn is_near_expiry_false_outside_window() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path()).with_refresh_window(Duration::from_secs(300));
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("a", "r", now_plus(3600)),
            )
            .await
            .unwrap();
        assert!(!broker.is_near_expiry("agent-A").await);
    }

    #[tokio::test]
    async fn is_near_expiry_true_when_already_expired() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        // expires_at = UNIX_EPOCH; firmly in the past.
        broker
            .put(
                "agent-A",
                FileTokenRecord {
                    access_token: "a".into(),
                    refresh_token: "r".into(),
                    expires_at_unix: 0,
                },
            )
            .await
            .unwrap();
        assert!(broker.is_near_expiry("agent-A").await);
    }

    #[tokio::test]
    async fn is_near_expiry_unknown_bearer_false() {
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        assert!(!broker.is_near_expiry("never-seen").await);
    }

    #[tokio::test]
    async fn refresh_lock_serialises_same_bearer() {
        use tokio::sync::Notify;

        let dir = tempfile::tempdir().unwrap();
        let broker = Arc::new(FileTokenBroker::new(dir.path()));

        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let broker_a = broker.clone();
        let started_a = started.clone();
        let release_a = release.clone();

        // Task A grabs the lock and waits to be released.
        let task_a = tokio::spawn(async move {
            let _guard = broker_a.lock_refresh("agent-A").await;
            started_a.notify_one();
            release_a.notified().await;
        });

        started.notified().await;

        // Task B must NOT acquire while A holds.
        let broker_b = broker.clone();
        let task_b = tokio::spawn(async move {
            let _guard = broker_b.lock_refresh("agent-A").await;
            "got-it"
        });

        // Give B a real shot at acquiring; it shouldn't.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task_b.is_finished(), "task B acquired before A released");

        release.notify_one();
        task_a.await.unwrap();
        assert_eq!(task_b.await.unwrap(), "got-it");
    }

    #[tokio::test]
    async fn refresh_lock_independent_across_bearers() {
        let dir = tempfile::tempdir().unwrap();
        let broker = Arc::new(FileTokenBroker::new(dir.path()));

        let _g_a = broker.lock_refresh("agent-A").await;
        // Holding A's lock must not block B.
        let started_b = std::time::Instant::now();
        let _g_b = broker.lock_refresh("agent-B").await;
        assert!(
            started_b.elapsed() < Duration::from_millis(50),
            "lock_refresh blocked across bearers"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_file_permissions_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let broker = FileTokenBroker::new(dir.path());
        broker
            .put(
                "agent-A",
                FileTokenRecord::from_expires_at("a", "r", now_plus(3600)),
            )
            .await
            .unwrap();
        let bearer_dir = dir.path().join("agent-A");
        let dir_mode = std::fs::metadata(&bearer_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "bearer dir should be 0700, got {dir_mode:o}"
        );
        for name in ["access_token.json", "refresh_token.json", "expires_at.json"] {
            let mode = std::fs::metadata(bearer_dir.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{name} should be 0600, got {mode:o}");
        }
    }
}
