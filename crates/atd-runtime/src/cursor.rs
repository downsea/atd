//! Stateless HMAC-signed cursors for paginated tool results.
//!
//! SP-pagination-v1 §4.2 + §4.5. The reference cursor implementation is
//! deliberately stateless: the server doesn't keep a cursor table — it
//! HMAC-signs the payload and trusts the verified bytes on each
//! `RunToolContinue`. Trade-off: 512-byte wire cap limits embedded
//! state (~256B for `opaque_state` after fixed-field overhead).
//!
//! Adopters writing their own paginating tools can use [`CursorIssuer`]
//! to issue + verify cursors without touching keys directly — call
//! `ctx.cursor_issuer()` from inside `Tool::call_paginated` (Phase D wiring).
//!
//! ## Cursor wire shape
//!
//! ```text
//! base64url( CBOR(CursorPayload) || HMAC-SHA256(key, CBOR(CursorPayload)) )
//! ```
//!
//! - CBOR encoding is ~80B for the fixed fields, leaving ~250B for
//!   `opaque_state` within the 512-byte cap.
//! - HMAC tag is 32 bytes (SHA-256 output, constant-time verified).
//! - base64url-no-pad keeps the cursor safe to ship inside JSON
//!   `arguments.__cursor` (the HTTP / MCP-bridge surface, Phases F-G).
//!
//! ## Cursor scope
//!
//! Cursors are bound to `(tool_id, caller_id, args_fingerprint, server_session)`:
//!
//! - `tool_id` — server rejects `RunToolContinue` whose `tool_id` ≠ embedded.
//! - `caller_id` — if the connection's identity changes (UDS Hello vs HTTP
//!   bearer), the cursor is invalidated (mismatch returns `ERR_CURSOR_INVALID`).
//! - `args_fingerprint` — SHA-256 of canonical-JSON-serialized original args;
//!   continuing with mutated args is a protocol violation.
//! - `server_session` — random nonce minted at issuer construction; server
//!   restart → new nonce → all outstanding cursors invalidated (returns
//!   `ERR_CURSOR_EXPIRED` so adopters can distinguish from forgery).

use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Maximum encoded cursor length on the wire. Bounded so cursors can
/// safely ride inside HTTP headers, JSON-RPC `arguments.__cursor` fields,
/// and audit log lines without truncation.
pub const MAX_CURSOR_BYTES: usize = 512;

/// Maximum opaque-state size inside a cursor payload. Operators who need
/// more should store state server-side keyed by a 16-byte cursor ID and
/// put just the ID in `opaque_state`.
pub const MAX_OPAQUE_STATE_BYTES: usize = 256;

/// Decoded cursor payload. Server-internal; clients never see the
/// individual fields, only the base64url-encoded signed blob.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CursorPayload {
    /// Canonical tool id the cursor is bound to. Mismatching tool_id on
    /// `RunToolContinue` returns `ERR_CURSOR_INVALID`.
    pub tool_id: String,
    /// Connection caller identity at issuance time. `None` for anonymous
    /// pre-Hello connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    /// SHA-256 of the canonical-JSON-serialized original `RunTool.args`.
    /// Integrity check, not storage: tools that need to replay args put
    /// a copy inside `opaque_state` (within the 256-byte budget).
    pub args_fingerprint: [u8; 32],
    /// 1-based page index. Useful for audit-trail correlation and
    /// abuse detection (a client requesting page=10000 is sus).
    pub page_index: u32,
    /// UNIX timestamp (seconds) at issuance. Server checks `now - issued_at
    /// <= ttl_seconds`.
    pub issued_at_unix: u64,
    /// Per-process random nonce from [`CursorIssuer::session_nonce`]. Server
    /// restart → new nonce → old cursors expire.
    pub server_session: [u8; 8],
    /// Tool-defined per-cursor state. Capped at `MAX_OPAQUE_STATE_BYTES`.
    /// Typical use: a database keyset cursor, an offset/limit pair, or
    /// an encrypted continuation token from an upstream API.
    #[serde(default, with = "serde_bytes")]
    pub opaque_state: Vec<u8>,
}

/// Errors from cursor issue / verify operations.
#[derive(thiserror::Error, Debug)]
pub enum CursorError {
    /// Cursor's `issued_at_unix` is older than `ttl_seconds`, or its
    /// `server_session` doesn't match the current issuer (server restarted).
    /// Maps to `ERR_CURSOR_EXPIRED` on the wire.
    #[error("cursor expired")]
    Expired,
    /// HMAC verification failed AND the cursor's embedded `server_session`
    /// matches the current issuer's nonce. This narrows the cause to real
    /// forgery / tampering: an attacker constructed a body whose nonce
    /// happened to match (or, with probability 2⁻⁶⁴, key rotation that
    /// preserved the nonce). Maps to `ERR_CURSOR_INVALID` on the wire —
    /// adopters should surface as a security signal.
    ///
    /// Server restart (random key AND random nonce both rotated) is the
    /// expected lifecycle of single-instance deployments and is reported
    /// as [`Self::Expired`] instead, so adopters can transparently drop
    /// the persisted cursor and re-issue.
    #[error("cursor signature invalid")]
    InvalidSignature,
    /// Malformed encoding (bad base64, bad CBOR). Maps to `ERR_CURSOR_INVALID`.
    #[error("cursor format invalid: {0}")]
    Format(String),
    /// Encoded cursor exceeds `MAX_CURSOR_BYTES`. Returned only on `issue`
    /// (verify checks the same cap before decoding).
    #[error("cursor too large: {0} bytes (max 512)")]
    TooLarge(usize),
    /// `opaque_state` exceeds `MAX_OPAQUE_STATE_BYTES`. Caught at issue
    /// time so a tool can't accidentally embed a 1MB blob.
    #[error("opaque_state too large: {0} bytes (max 256)")]
    OpaqueStateTooLarge(usize),
}

/// HMAC-SHA256 cursor issuer + verifier. One per server process; constructed
/// at startup with `SharedServerConfig.cursor_signing_key`. Multi-instance
/// deployments behind a load balancer can share a key via env
/// (`ATD_CURSOR_SIGNING_KEY=base64...`); single-instance deployments use
/// a fresh random key per startup (default).
pub struct CursorIssuer {
    key: [u8; 32],
    session_nonce: [u8; 8],
}

impl CursorIssuer {
    /// Build with an explicit signing key + a fresh-random session nonce.
    /// The nonce changes on each construction so server restart invalidates
    /// outstanding cursors even if the key is reused across restarts.
    pub fn new(key: [u8; 32]) -> Self {
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce).expect("OS RNG");
        Self {
            key,
            session_nonce: nonce,
        }
    }

    /// The per-process random session nonce. Cursors carry this in their
    /// payload; if the verifier sees a non-matching nonce, the cursor is
    /// from a prior server process (or a different process entirely) —
    /// treated as expired.
    pub fn session_nonce(&self) -> [u8; 8] {
        self.session_nonce
    }

    /// Sign + encode a [`CursorPayload`]. The encoded result is suitable for
    /// stuffing into `Response::ToolResultResponse.next_cursor`.
    pub fn issue(&self, payload: CursorPayload) -> Result<String, CursorError> {
        if payload.opaque_state.len() > MAX_OPAQUE_STATE_BYTES {
            return Err(CursorError::OpaqueStateTooLarge(payload.opaque_state.len()));
        }
        let mut body = Vec::with_capacity(256);
        ciborium::into_writer(&payload, &mut body)
            .map_err(|e| CursorError::Format(e.to_string()))?;
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC accepts arbitrary key lengths");
        mac.update(&body);
        let tag = mac.finalize().into_bytes();
        let mut combined = body;
        combined.extend_from_slice(&tag);
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&combined);
        if encoded.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::TooLarge(encoded.len()));
        }
        Ok(encoded)
    }

    /// Verify HMAC, decode CBOR, check TTL + session nonce. Returns the
    /// decoded payload on success; the dispatch layer then checks
    /// `tool_id` / `caller_id` / `args_fingerprint` against the
    /// continuation request.
    pub fn verify(&self, cursor: &str, ttl_seconds: u64) -> Result<CursorPayload, CursorError> {
        if cursor.len() > MAX_CURSOR_BYTES {
            return Err(CursorError::TooLarge(cursor.len()));
        }
        let combined = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(cursor)
            .map_err(|e| CursorError::Format(e.to_string()))?;
        if combined.len() < 32 {
            return Err(CursorError::Format("missing HMAC tag".into()));
        }
        let (body, tag) = combined.split_at(combined.len() - 32);
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC accepts arbitrary key lengths");
        mac.update(body);
        // Constant-time tag comparison via the hmac crate.
        if mac.verify_slice(tag).is_err() {
            // HMAC verification failed. Two physical causes share this code
            // path:
            //   (a) Forgery / tampering — someone produced a cursor whose
            //       body doesn't authenticate under our key.
            //   (b) Key rotation — this process was restarted, the issuer's
            //       random key changed, and an adopter is replaying a
            //       cursor signed by a previous incarnation of the same
            //       logical server. This is the **expected** lifecycle for
            //       single-instance deployments where `ATD_CURSOR_SIGNING_KEY`
            //       is not set.
            //
            // Adopters want to differentiate: (a) is a security signal that
            // should be surfaced loudly, (b) is "drop the persisted cursor
            // and re-issue from the initial RunTool". Probe-decode the body
            // *unverified* to read its `server_session` nonce and split:
            //
            //   - nonce ≠ self.session_nonce → key + nonce both rotated at
            //     startup → return `Expired` (maps to ERR_CURSOR_EXPIRED on
            //     the wire); adopters know to drop and re-issue.
            //   - nonce == self.session_nonce → either real tampering, or
            //     the astronomically unlikely 2⁻⁶⁴ nonce collision after a
            //     key rotation → return `InvalidSignature` (ERR_CURSOR_INVALID);
            //     adopters surface as a security signal.
            //
            // The probe-decode is `ciborium::from_reader`; CBOR is panic-safe
            // by design and a malformed body simply collapses to Format-style
            // failure → fall through to `InvalidSignature` (we can't tell
            // (a) from (b) in that case, so we're conservative and pick the
            // security-signal variant).
            if let Ok(probe) = ciborium::from_reader::<CursorPayload, _>(body)
                && probe.server_session != self.session_nonce
            {
                return Err(CursorError::Expired);
            }
            return Err(CursorError::InvalidSignature);
        }
        let payload: CursorPayload =
            ciborium::from_reader(body).map_err(|e| CursorError::Format(e.to_string()))?;
        if payload.server_session != self.session_nonce {
            return Err(CursorError::Expired);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now.saturating_sub(payload.issued_at_unix) > ttl_seconds {
            return Err(CursorError::Expired);
        }
        Ok(payload)
    }
}

/// Generate a fresh 32-byte cursor signing key from the OS RNG. Used by
/// listener crates (`atd-server` / `atd-server-http`) at `Server::new`
/// so they don't have to take a direct `getrandom` dep.
pub fn random_signing_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).expect("OS RNG");
    k
}

/// SP-pagination-v1 §4.2 — pick a cursor signing key by precedence:
///
/// 1. `ATD_CURSOR_SIGNING_KEY` env (base64-url or standard base64 of
///    exactly 32 bytes) — for multi-instance deployments behind a load
///    balancer where every instance must verify cursors any sibling
///    issued. Operators are responsible for keeping the value secret;
///    the env is read once at `Server::new` and never logged.
/// 2. Random 32-byte key from `random_signing_key()` — single-instance
///    default. Server restart → new key → outstanding cursors fail
///    with `ERR_CURSOR_EXPIRED`.
///
/// Invalid env values (non-base64, wrong length) fall back to random with
/// a warning on stderr. This is deliberate fail-closed: a misconfigured
/// shared deployment becomes single-instance rather than insecure.
pub fn signing_key_from_env_or_random() -> [u8; 32] {
    if let Ok(value) = std::env::var("ATD_CURSOR_SIGNING_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(trimmed)
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(trimmed));
            match decoded {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&bytes);
                    return k;
                }
                Ok(bytes) => {
                    eprintln!(
                        "atd-runtime: ATD_CURSOR_SIGNING_KEY decoded to {} bytes; \
                         expected 32. Falling back to random per-process key.",
                        bytes.len()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "atd-runtime: ATD_CURSOR_SIGNING_KEY base64 decode failed: {e}. \
                         Falling back to random per-process key."
                    );
                }
            }
        }
    }
    random_signing_key()
}

/// Compute the canonical `args_fingerprint` for a `RunTool.args` value.
/// Used at issue time (server) and could be used at verify time (server)
/// if the dispatch layer wants to check that a continuation's args
/// match the original. The reference dispatch (Phase D) passes
/// `serde_json::Value::Null` on continuations and binds args via the
/// embedded opaque_state, so this is mainly an integrity tool.
pub fn args_fingerprint(args: &serde_json::Value) -> [u8; 32] {
    use sha2::Digest;
    // Serialize via the standard serde_json writer — non-canonical, but
    // round-trip-stable for the same in-memory value, which is what we need.
    let bytes = serde_json::to_vec(args).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn fresh_issuer() -> CursorIssuer {
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key).unwrap();
        CursorIssuer::new(key)
    }

    fn mk_payload(issuer: &CursorIssuer, page: u32) -> CursorPayload {
        CursorPayload {
            tool_id: "celia:fhir.list_observations".into(),
            caller_id: Some("test-caller".into()),
            args_fingerprint: args_fingerprint(&json!({"patient": "p1"})),
            page_index: page,
            issued_at_unix: now(),
            server_session: issuer.session_nonce(),
            opaque_state: vec![],
        }
    }

    #[test]
    fn issue_round_trips() {
        let issuer = fresh_issuer();
        let payload = mk_payload(&issuer, 2);
        let cursor = issuer.issue(payload.clone()).expect("issue");
        let back = issuer.verify(&cursor, 300).expect("verify");
        assert_eq!(back.tool_id, payload.tool_id);
        assert_eq!(back.page_index, 2);
        assert_eq!(back.args_fingerprint, payload.args_fingerprint);
    }

    #[test]
    fn verify_rejects_tampered_signature() {
        let issuer = fresh_issuer();
        let cursor = issuer.issue(mk_payload(&issuer, 1)).unwrap();
        // Flip a base64 character ~16 chars from the end so we land inside
        // the HMAC tag region without breaking base64 framing (the last
        // few chars often have alignment-sensitive padding bits).
        let mut bytes: Vec<u8> = cursor.bytes().collect();
        let target = bytes.len() - 16;
        bytes[target] = if bytes[target] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();
        match issuer.verify(&tampered, 300) {
            Err(CursorError::InvalidSignature) => {}
            other => panic!("expected InvalidSignature, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_after_ttl() {
        let issuer = fresh_issuer();
        let mut payload = mk_payload(&issuer, 1);
        payload.issued_at_unix = now().saturating_sub(400); // 400s ago
        let cursor = issuer.issue(payload).unwrap();
        match issuer.verify(&cursor, 300) {
            Err(CursorError::Expired) => {}
            other => panic!("expected Expired, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_wrong_session_nonce() {
        // Two issuers sharing a key but different nonces — simulates server restart
        // with a persistent key.
        let key = {
            let mut k = [0u8; 32];
            getrandom::getrandom(&mut k).unwrap();
            k
        };
        let issuer_a = CursorIssuer::new(key);
        let issuer_b = CursorIssuer::new(key);
        assert_ne!(issuer_a.session_nonce(), issuer_b.session_nonce());
        let cursor = issuer_a.issue(mk_payload(&issuer_a, 1)).unwrap();
        match issuer_b.verify(&cursor, 300) {
            Err(CursorError::Expired) => {}
            other => panic!("expected Expired (cross-session), got {other:?}"),
        }
    }

    #[test]
    fn verify_treats_server_restart_as_expired_not_forgery() {
        // The real-world server-restart shape: both `key` and
        // `session_nonce` are freshly minted by `random_signing_key()` /
        // OS RNG, so they differ from the previous incarnation. Adopters
        // replaying an old cursor must see `Expired` (1020), not
        // `InvalidSignature` (1021), so they know to drop the persisted
        // cursor and re-issue the original `RunTool`.
        let issuer_a = fresh_issuer();
        let issuer_b = fresh_issuer(); // different random key AND nonce
        let cursor = issuer_a.issue(mk_payload(&issuer_a, 1)).unwrap();
        match issuer_b.verify(&cursor, 300) {
            Err(CursorError::Expired) => {}
            other => panic!("expected Expired (server restart), got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_real_forgery_with_invalid_signature() {
        // Attacker holds a legitimate cursor and tampers with the body
        // (flipping the page_index byte by re-encoding). The nonce
        // embedded in the forged body necessarily matches our current
        // session (since we re-encode under our own nonce); HMAC must
        // still fail because the attacker doesn't have our key. The
        // probe-decode path sees nonce-match and correctly classifies
        // this as `InvalidSignature` (1021) — the security-signal code.
        let issuer = fresh_issuer();
        let original = issuer.issue(mk_payload(&issuer, 1)).unwrap();

        // Synthesize a body with a different page_index but the *same*
        // (current) nonce, then concatenate someone else's HMAC tag
        // (use the original tag — guaranteed not to match the tampered
        // body).
        let combined = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&original)
            .unwrap();
        let (_orig_body, orig_tag) = combined.split_at(combined.len() - 32);

        let tampered_payload = CursorPayload {
            tool_id: "test:tool".into(),
            caller_id: Some("test-caller".into()),
            args_fingerprint: [0u8; 32],
            page_index: 999, // ← attacker's chosen value
            issued_at_unix: now(),
            server_session: issuer.session_nonce(), // ← current nonce
            opaque_state: vec![],
        };
        let mut tampered_body = Vec::new();
        ciborium::into_writer(&tampered_payload, &mut tampered_body).unwrap();

        let mut forged = tampered_body;
        forged.extend_from_slice(orig_tag);
        let forged_cursor = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&forged);

        match issuer.verify(&forged_cursor, 300) {
            Err(CursorError::InvalidSignature) => {}
            other => panic!("expected InvalidSignature (real forgery), got {other:?}"),
        }
    }

    #[test]
    fn verify_unparseable_body_after_hmac_fail_is_invalid_signature() {
        // Garbage CBOR + garbage HMAC tag (combined long enough to pass
        // the length check). The probe-decode fails; we fall through to
        // `InvalidSignature` rather than guessing — conservative posture.
        let issuer = fresh_issuer();
        let garbage = vec![0xffu8; 64]; // 32B body + 32B tag, all 0xff
        let garbage_cursor = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&garbage);
        match issuer.verify(&garbage_cursor, 300) {
            Err(CursorError::InvalidSignature) => {}
            other => panic!("expected InvalidSignature for unparseable, got {other:?}"),
        }
    }

    #[test]
    fn issue_rejects_oversized_opaque_state() {
        let issuer = fresh_issuer();
        let mut payload = mk_payload(&issuer, 1);
        payload.opaque_state = vec![0u8; MAX_OPAQUE_STATE_BYTES + 1];
        match issuer.issue(payload) {
            Err(CursorError::OpaqueStateTooLarge(n)) => {
                assert_eq!(n, MAX_OPAQUE_STATE_BYTES + 1)
            }
            other => panic!("expected OpaqueStateTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn issue_rejects_oversized_payload_via_long_tool_id() {
        let issuer = fresh_issuer();
        // Pad opaque_state to its max + a long tool_id to push the
        // encoded blob past 512 bytes.
        let mut payload = mk_payload(&issuer, 1);
        payload.tool_id = "x".repeat(400);
        payload.opaque_state = vec![0u8; MAX_OPAQUE_STATE_BYTES];
        match issuer.issue(payload) {
            Err(CursorError::TooLarge(n)) => {
                assert!(n > MAX_CURSOR_BYTES, "got {n}");
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_malformed_base64() {
        let issuer = fresh_issuer();
        match issuer.verify("not!base64!", 300) {
            Err(CursorError::Format(_)) => {}
            other => panic!("expected Format error, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_too_short_combined() {
        let issuer = fresh_issuer();
        // 8 bytes of valid base64 → 6 raw bytes → less than 32-byte HMAC tag.
        let cursor = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"abcdef");
        match issuer.verify(&cursor, 300) {
            Err(CursorError::Format(_)) => {}
            other => panic!("expected Format (missing HMAC tag), got {other:?}"),
        }
    }

    #[test]
    fn args_fingerprint_stable_for_same_value() {
        let a = args_fingerprint(&json!({"x": 1, "y": [2, 3]}));
        let b = args_fingerprint(&json!({"x": 1, "y": [2, 3]}));
        assert_eq!(a, b);
    }

    #[test]
    fn args_fingerprint_differs_for_different_value() {
        let a = args_fingerprint(&json!({"x": 1}));
        let b = args_fingerprint(&json!({"x": 2}));
        assert_ne!(a, b);
    }

    #[test]
    fn cursor_under_cap_for_typical_payload() {
        let issuer = fresh_issuer();
        let payload = mk_payload(&issuer, 1);
        let cursor = issuer.issue(payload).unwrap();
        // Typical payload (no opaque_state, ~30-char tool_id, ~11-char
        // caller_id) measured at ~334 base64 chars in practice. The hard
        // cap is MAX_CURSOR_BYTES (512); anything well under that is fine.
        assert!(
            cursor.len() < MAX_CURSOR_BYTES,
            "typical cursor over cap: {} > {}",
            cursor.len(),
            MAX_CURSOR_BYTES
        );
    }

    #[test]
    fn opaque_state_round_trips() {
        let issuer = fresh_issuer();
        let mut payload = mk_payload(&issuer, 1);
        payload.opaque_state = b"keyset:last_id=42,page=3".to_vec();
        let cursor = issuer.issue(payload.clone()).unwrap();
        let back = issuer.verify(&cursor, 300).unwrap();
        assert_eq!(back.opaque_state, payload.opaque_state);
    }

    /// Env-key wiring must be serialised across tests reading the same
    /// process-global var; we use a static mutex instead of pulling in
    /// `serial_test` so the crate stays dep-tight.
    fn signing_key_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn signing_key_from_env_reads_base64url_no_pad() {
        let _g = signing_key_env_lock();
        let want = [7u8; 32];
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(want);
        unsafe { std::env::set_var("ATD_CURSOR_SIGNING_KEY", &encoded) };
        let got = signing_key_from_env_or_random();
        unsafe { std::env::remove_var("ATD_CURSOR_SIGNING_KEY") };
        assert_eq!(got, want, "env key must round-trip verbatim");
    }

    #[test]
    fn signing_key_from_env_falls_back_on_wrong_length() {
        let _g = signing_key_env_lock();
        // 16-byte key, base64-encoded — wrong length, must fall back.
        let bad = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 16]);
        unsafe { std::env::set_var("ATD_CURSOR_SIGNING_KEY", &bad) };
        let got = signing_key_from_env_or_random();
        unsafe { std::env::remove_var("ATD_CURSOR_SIGNING_KEY") };
        // Random fallback never equals all-1s.
        assert_ne!(got, [1u8; 32]);
    }

    #[test]
    fn signing_key_from_env_falls_back_on_garbage() {
        let _g = signing_key_env_lock();
        unsafe { std::env::set_var("ATD_CURSOR_SIGNING_KEY", "not-base64!!") };
        let got = signing_key_from_env_or_random();
        unsafe { std::env::remove_var("ATD_CURSOR_SIGNING_KEY") };
        // Just assert it returned something (random); shape-validity via
        // construction.
        let _: [u8; 32] = got;
    }

    #[test]
    fn signing_key_from_env_falls_back_when_unset() {
        let _g = signing_key_env_lock();
        unsafe { std::env::remove_var("ATD_CURSOR_SIGNING_KEY") };
        let a = signing_key_from_env_or_random();
        let b = signing_key_from_env_or_random();
        assert_ne!(a, b, "random fallback should yield distinct keys");
    }

    /// Wait one second so `issued_at_unix` advances; verifies TTL semantics
    /// without flakiness from clock granularity.
    #[test]
    fn ttl_zero_rejects_freshly_issued() {
        let issuer = fresh_issuer();
        let mut payload = mk_payload(&issuer, 1);
        payload.issued_at_unix = now().saturating_sub(1); // 1s ago
        let cursor = issuer.issue(payload).unwrap();
        // TTL = 0 means "anything older than 0s is expired"
        std::thread::sleep(Duration::from_millis(10));
        match issuer.verify(&cursor, 0) {
            Err(CursorError::Expired) => {}
            other => panic!("expected Expired with ttl=0, got {other:?}"),
        }
    }
}
