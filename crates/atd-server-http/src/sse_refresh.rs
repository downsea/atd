//! SP-token-broker-phase2 §4.7 — SSE long-connection bearer refresh.
//!
//! For HTTP routes that hold a stream open (Celia's `/chat/stream`,
//! healthkit's bulk-export progress, etc.), the bearer must be
//! re-validated periodically — otherwise consent revocations propagate
//! only at the next request, which on a long-lived stream might be
//! never. This helper spawns a background task that polls
//! [`TokenBroker::resolve_bearer`] on a heartbeat and emits
//! [`RefreshEvent`]s the adopter consumes from their SSE handler.
//!
//! ## Cadence (spec §4.7)
//!
//! - Default cadence is `60s` — matches the bound under which "consent
//!   withdrawals get honoured" the spec contracts for.
//! - If `BearerIdentity.expires_at` is `Some(t)`, the next sleep is
//!   `min(t - now, cadence)` so we never sleep past the advertised TTL.
//! - The cadence is recomputed every iteration from the most recent
//!   identity (a shrunk `expires_at` from a revalidation immediately
//!   tightens the loop).
//!
//! ## What the helper does NOT do
//!
//! - It does not cancel in-flight tool dispatches. Per spec §4.7, a
//!   tool call that began authorised must complete with that authority.
//!   Adopters propagate the cap change to the *next* request only.
//! - It does not emit SSE bytes. The adopter's route handler maps
//!   [`RefreshEvent::AuthLost`] to its own application-level frame
//!   (Celia chose `event: auth_lost` per spec §4.7).
//! - It does not auto-reconnect. When the helper emits `AuthLost`,
//!   the loop terminates; the adopter route closes the stream; the
//!   client decides whether to reconnect with a fresh bearer.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use atd_runtime::secrets::{BearerIdentity, BrokerError, TokenBroker};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Default heartbeat cadence per spec §4.7 ("matches the typical SSE
/// keep-alive ping cadence; revocation window adopters accept").
pub const DEFAULT_REFRESH_CADENCE: Duration = Duration::from_secs(60);

/// Channel buffer for [`RefreshEvent`]s. Adopters typically read
/// one-at-a-time inside an SSE select! arm; 8 absorbs bursts without
/// back-pressuring the broker.
const CHANNEL_BUFFER: usize = 8;

/// Events the refresh task emits to the adopter's SSE route.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RefreshEvent {
    /// Broker re-validated the bearer. `caps` is the latest authorised
    /// capability set — may be the same as before, or smaller (shrunk
    /// by a partial revoke). The adopter updates its per-stream cap
    /// reference for future requests but **does not** cancel in-flight
    /// tool dispatches.
    Refreshed {
        caps: Vec<String>,
        expires_at: Option<SystemTime>,
    },
    /// Bearer is no longer valid (expired, revoked, or
    /// broker-recognised-bad). Adopter route should emit its
    /// application-level `auth_lost` frame and close the stream. The
    /// refresh task exits after sending this; the channel closes.
    AuthLost { reason: AuthLostReason },
}

/// Why the bearer was invalidated. Maps 1:1 to the broker error
/// variants per spec §4.4 — adopter UX can surface "session expired"
/// vs "agent revoked" vs "server hiccup" distinctly.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthLostReason {
    Expired,
    Revoked(String),
    /// Broker started returning `Ok(None)` — bearer no longer
    /// recognised (admin deleted the pairing? token rotation gone
    /// wrong?). Treated the same as expired/revoked at the client.
    Unknown,
    /// Broker hard error (`Err(Internal)` or `Err(Lookup)`). Includes
    /// the broker's reason string. Distinct from `Expired`/`Revoked`
    /// so the adopter can decide whether to retry the validation
    /// (transient) or close the stream (persistent).
    BrokerError {
        reason: String,
        transient: bool,
    },
}

/// Spawn a background task that periodically re-validates `bearer` via
/// `broker` and emits [`RefreshEvent`]s on the returned channel. The
/// task exits after emitting one [`RefreshEvent::AuthLost`]; dropping
/// the [`JoinHandle`] aborts it sooner.
///
/// Per spec §4.7, the initial `resolve_bearer` call already happened at
/// stream open (so the route handler has the live `BearerIdentity`);
/// this task picks up *after* that. The caller supplies `initial` so
/// the helper can compute the first sleep duration from the recorded
/// `expires_at`.
pub fn spawn_bearer_refresh(
    broker: Arc<dyn TokenBroker>,
    bearer: String,
    initial: BearerIdentity,
    cadence: Duration,
) -> (JoinHandle<()>, mpsc::Receiver<RefreshEvent>) {
    let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);

    let handle = tokio::spawn(async move {
        let mut current_expires_at = initial.expires_at;
        loop {
            let sleep_for = next_sleep_duration(cadence, current_expires_at);
            tokio::time::sleep(sleep_for).await;

            match broker.resolve_bearer(&bearer).await {
                Ok(Some(id)) => {
                    current_expires_at = id.expires_at;
                    let evt = RefreshEvent::Refreshed {
                        caps: id.granted_capabilities,
                        expires_at: id.expires_at,
                    };
                    if tx.send(evt).await.is_err() {
                        // Adopter dropped the receiver; stream is gone.
                        break;
                    }
                }
                Ok(None) => {
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::Unknown,
                        })
                        .await;
                    break;
                }
                Err(BrokerError::Expired) => {
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::Expired,
                        })
                        .await;
                    break;
                }
                Err(BrokerError::Revoked(msg)) => {
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::Revoked(msg),
                        })
                        .await;
                    break;
                }
                Err(BrokerError::Lookup(msg)) => {
                    // Transient — emit AuthLost(transient) but exit
                    // anyway; adopter decides whether to reconnect.
                    // SSE long-connection design (spec §4.7) prefers
                    // close-and-reconnect over silent retry.
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::BrokerError {
                                reason: msg,
                                transient: true,
                            },
                        })
                        .await;
                    break;
                }
                Err(BrokerError::Internal(msg)) => {
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::BrokerError {
                                reason: msg,
                                transient: false,
                            },
                        })
                        .await;
                    break;
                }
                Err(BrokerError::NotConfigured) => {
                    let _ = tx
                        .send(RefreshEvent::AuthLost {
                            reason: AuthLostReason::BrokerError {
                                reason: "broker NotConfigured".into(),
                                transient: false,
                            },
                        })
                        .await;
                    break;
                }
            }
        }
    });

    (handle, rx)
}

/// Compute the next sleep duration: `min(expires_at - now, cadence)`.
/// If `expires_at` is `None`, sleeps the full cadence. If `expires_at`
/// is in the past, returns the cadence (the next `resolve_bearer` will
/// confirm the expiry); we don't burn CPU on a zero-duration sleep loop.
fn next_sleep_duration(cadence: Duration, expires_at: Option<SystemTime>) -> Duration {
    match expires_at {
        Some(t) => {
            let now = SystemTime::now();
            match t.duration_since(now) {
                Ok(until_expiry) => cadence.min(until_expiry),
                Err(_) => cadence, // already expired; next tick discovers
            }
        }
        None => cadence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_runtime::secrets::{ResolveBearerFuture, ResolveFuture};
    use std::sync::Mutex;

    /// Broker whose `resolve_bearer` response is mutated by tests.
    /// Wrapped in `Arc<Mutex<...>>` so the test thread can flip the
    /// behaviour mid-refresh and the spawned task picks it up on its
    /// next iteration.
    #[derive(Debug, Default)]
    struct MutBroker {
        state: Mutex<MutState>,
    }
    #[derive(Default)]
    enum MutState {
        #[default]
        Ok,
        OkShrunk,
        Expired,
        Revoked(String),
        Unknown,
        Internal(String),
        Lookup(String),
    }
    // Impl Debug for the enum so `derive(Debug)` on the broker works.
    impl std::fmt::Debug for MutState {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Ok => write!(f, "Ok"),
                Self::OkShrunk => write!(f, "OkShrunk"),
                Self::Expired => write!(f, "Expired"),
                Self::Revoked(s) => write!(f, "Revoked({s})"),
                Self::Unknown => write!(f, "Unknown"),
                Self::Internal(s) => write!(f, "Internal({s})"),
                Self::Lookup(s) => write!(f, "Lookup({s})"),
            }
        }
    }
    impl MutBroker {
        fn set(&self, st: MutState) {
            *self.state.lock().unwrap() = st;
        }
    }
    impl TokenBroker for MutBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
            Box::pin(async { Ok(None) })
        }
        fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
            let st = std::mem::take(&mut *self.state.lock().unwrap());
            // After read, default back to `Ok` so subsequent ticks find
            // a fresh state if the test hasn't reset it explicitly.
            // (Tests that need a sticky state reset before each tick.)
            *self.state.lock().unwrap() = match &st {
                MutState::Ok => MutState::Ok,
                MutState::OkShrunk => MutState::OkShrunk,
                _ => MutState::default(),
            };
            // Snapshot value we'll respond with
            let snapshot = match st {
                MutState::Ok => MutState::Ok,
                MutState::OkShrunk => MutState::OkShrunk,
                MutState::Expired => MutState::Expired,
                MutState::Revoked(s) => MutState::Revoked(s),
                MutState::Unknown => MutState::Unknown,
                MutState::Internal(s) => MutState::Internal(s),
                MutState::Lookup(s) => MutState::Lookup(s),
            };
            Box::pin(async move {
                match snapshot {
                    MutState::Ok => Ok(Some(BearerIdentity {
                        caller_id: "agent-A".into(),
                        granted_capabilities: vec!["records:read".into(), "summary:read".into()],
                        secrets: None,
                        expires_at: None,
                        cache_until: None,
                    })),
                    MutState::OkShrunk => Ok(Some(BearerIdentity {
                        caller_id: "agent-A".into(),
                        granted_capabilities: vec!["records:read".into()],
                        secrets: None,
                        expires_at: None,
                        cache_until: None,
                    })),
                    MutState::Expired => Err(BrokerError::Expired),
                    MutState::Revoked(s) => Err(BrokerError::Revoked(s)),
                    MutState::Unknown => Ok(None),
                    MutState::Internal(s) => Err(BrokerError::Internal(s)),
                    MutState::Lookup(s) => Err(BrokerError::Lookup(s)),
                }
            })
        }
    }

    fn ok_identity() -> BearerIdentity {
        BearerIdentity {
            caller_id: "agent-A".into(),
            granted_capabilities: vec!["records:read".into(), "summary:read".into()],
            secrets: None,
            expires_at: None,
            cache_until: None,
        }
    }

    // ---- next_sleep_duration ----

    #[test]
    fn no_expires_at_sleeps_full_cadence() {
        let dur = next_sleep_duration(Duration::from_secs(60), None);
        assert_eq!(dur, Duration::from_secs(60));
    }

    #[test]
    fn expires_at_in_the_past_returns_cadence_not_zero() {
        let past = SystemTime::now() - Duration::from_secs(100);
        let dur = next_sleep_duration(Duration::from_secs(60), Some(past));
        assert_eq!(dur, Duration::from_secs(60));
    }

    #[test]
    fn expires_at_sooner_than_cadence_clamps_sleep() {
        let soon = SystemTime::now() + Duration::from_secs(5);
        let dur = next_sleep_duration(Duration::from_secs(60), Some(soon));
        // 5 +/- 1s scheduling slack
        assert!(dur < Duration::from_secs(7));
        assert!(dur > Duration::from_secs(3));
    }

    #[test]
    fn expires_at_far_in_future_keeps_cadence() {
        let far = SystemTime::now() + Duration::from_secs(3600);
        let dur = next_sleep_duration(Duration::from_secs(60), Some(far));
        assert_eq!(dur, Duration::from_secs(60));
    }

    // ---- spawn_bearer_refresh ----
    //
    // Tests use very short cadences (50-200ms) so the suite stays fast.

    #[tokio::test]
    async fn happy_refresh_emits_refreshed_with_current_caps() {
        let broker: Arc<dyn TokenBroker> = Arc::new(MutBroker::default());
        let (handle, mut rx) = spawn_bearer_refresh(
            broker.clone(),
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event within 500ms")
            .expect("non-closed channel");

        match evt {
            RefreshEvent::Refreshed { caps, .. } => {
                assert!(caps.contains(&"records:read".to_string()));
                assert!(caps.contains(&"summary:read".to_string()));
            }
            other => panic!("expected Refreshed, got {other:?}"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn shrunk_caps_propagate_via_refreshed_event() {
        let broker_concrete = Arc::new(MutBroker::default());
        broker_concrete.set(MutState::OkShrunk);
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(), // initial = 2 caps
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event within 500ms")
            .expect("non-closed channel");

        match evt {
            RefreshEvent::Refreshed { caps, .. } => {
                assert_eq!(caps, vec!["records:read".to_string()]);
            }
            other => panic!("expected Refreshed(shrunk), got {other:?}"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn mid_stream_revoke_emits_auth_lost_revoked_and_closes_channel() {
        let broker_concrete = Arc::new(MutBroker::default());
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        // First tick should be a happy Refreshed.
        let evt1 = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("first event")
            .expect("non-closed");
        assert!(matches!(evt1, RefreshEvent::Refreshed { .. }));

        // Now flip the broker to revoked; next tick should emit AuthLost.
        broker_concrete.set(MutState::Revoked("user revoked".into()));

        let evt2 = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("second event")
            .expect("non-closed before AuthLost");
        match evt2 {
            RefreshEvent::AuthLost {
                reason: AuthLostReason::Revoked(msg),
            } => assert_eq!(msg, "user revoked"),
            other => panic!("expected AuthLost(Revoked), got {other:?}"),
        }

        // Channel should close after AuthLost — next recv returns None.
        let evt3 = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("channel-close detection within 500ms");
        assert!(evt3.is_none(), "channel must close after AuthLost");

        // Task exited; handle is await-able to confirm completion.
        let _ = handle.await;
    }

    #[tokio::test]
    async fn mid_stream_expired_emits_auth_lost_expired() {
        let broker_concrete = Arc::new(MutBroker::default());
        broker_concrete.set(MutState::Expired);
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event within 500ms")
            .expect("non-closed");
        assert!(matches!(
            evt,
            RefreshEvent::AuthLost {
                reason: AuthLostReason::Expired
            }
        ));
        handle.abort();
    }

    #[tokio::test]
    async fn broker_internal_error_emits_persistent_auth_lost() {
        let broker_concrete = Arc::new(MutBroker::default());
        broker_concrete.set(MutState::Internal("oh no".into()));
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("non-closed");
        match evt {
            RefreshEvent::AuthLost {
                reason: AuthLostReason::BrokerError { transient, .. },
            } => assert!(!transient),
            other => panic!("expected persistent BrokerError, got {other:?}"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn broker_lookup_error_emits_transient_auth_lost() {
        let broker_concrete = Arc::new(MutBroker::default());
        broker_concrete.set(MutState::Lookup("sqlite locked".into()));
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("non-closed");
        match evt {
            RefreshEvent::AuthLost {
                reason: AuthLostReason::BrokerError { transient, .. },
            } => assert!(transient),
            other => panic!("expected transient BrokerError, got {other:?}"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn unknown_bearer_emits_auth_lost_unknown() {
        let broker_concrete = Arc::new(MutBroker::default());
        broker_concrete.set(MutState::Unknown);
        let broker: Arc<dyn TokenBroker> = broker_concrete.clone();

        let (handle, mut rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("non-closed");
        assert!(matches!(
            evt,
            RefreshEvent::AuthLost {
                reason: AuthLostReason::Unknown
            }
        ));
        handle.abort();
    }

    #[tokio::test]
    async fn dropping_receiver_aborts_refresh_loop() {
        let broker: Arc<dyn TokenBroker> = Arc::new(MutBroker::default());
        let (handle, rx) = spawn_bearer_refresh(
            broker,
            "tok".into(),
            ok_identity(),
            Duration::from_millis(50),
        );

        // Drop the receiver immediately.
        drop(rx);
        // Wait for the next tick, then the next send fails and the task exits.
        let exited = tokio::time::timeout(Duration::from_millis(500), handle).await;
        assert!(
            exited.is_ok(),
            "task must exit within ~1 cadence after receiver drop"
        );
    }
}
