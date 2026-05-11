//! SP-capability-v2 Phase B.1 — JWT compact-form structural decoder.
//!
//! Parses a UCAN-lite token (`<header>.<payload>.<signature>`) into a
//! [`UcanPayload`]. **Does not verify the signature** — that's Phase B.2
//! (`ucan::verify::verify_chain`). Pure structural validation: alg / typ /
//! ucv / cmd / DID-method-prefix only.
//!
//! Spec: [`specs/2026-05-11-sp-capability-v2-design.md`] §4.1, §4.3, §4.4, §4.5

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use super::error::UcanParseError;
use super::types::{UcanHeader, UcanPayload};

/// Decode a JWT compact-form UCAN-lite token into its payload.
///
/// Returns the [`UcanPayload`] on structural success. The header is
/// discarded after validation (the caller never needs `alg` etc. once
/// the parser has accepted it). Signature bytes are also discarded by
/// this entry point; Phase B.2's verifier takes the raw token and
/// re-splits to obtain the signed bytes.
///
/// # Errors
///
/// [`UcanParseError`] — every variant maps to wire code
/// `ERR_UCAN_INVALID = 1010`. Always `retryable: false`.
///
/// # Out of scope
///
/// - Signature verification (Phase B.2)
/// - Chain attenuation, audience pinning, expiry, depth limit (Phase B.2)
/// - Revocation store consultation (Phase B.2)
pub fn parse_jwt(token: &str) -> Result<UcanPayload, UcanParseError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(UcanParseError::MalformedJwt(parts.len()));
    }
    let (header_b64, payload_b64, _sig_b64) = (parts[0], parts[1], parts[2]);

    // Header: base64url-decode → JSON → validate alg / typ / ucv.
    let header_bytes = URL_SAFE_NO_PAD
        .decode(header_b64)
        .map_err(|e| UcanParseError::base64("header", e))?;
    let header: UcanHeader =
        serde_json::from_slice(&header_bytes).map_err(|e| UcanParseError::json("header", e))?;

    if header.alg != "EdDSA" {
        return Err(UcanParseError::UnsupportedAlg(header.alg));
    }
    if header.typ != "ucan/1.0+jwt" {
        return Err(UcanParseError::UnsupportedTyp(header.typ));
    }
    if header.ucv != "1.0" {
        return Err(UcanParseError::UnsupportedUcv(header.ucv));
    }

    // Payload: base64url-decode → JSON → validate cmd + DID-method prefixes.
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|e| UcanParseError::base64("payload", e))?;
    let payload: UcanPayload =
        serde_json::from_slice(&payload_bytes).map_err(|e| UcanParseError::json("payload", e))?;

    if payload.cmd != "atd-cap" {
        return Err(UcanParseError::NonAtdCap(payload.cmd));
    }
    require_did_key(&payload.iss, "iss")?;
    require_did_key(&payload.aud, "aud")?;

    Ok(payload)
}

/// `did:key:z<base58btc multibase>` is the only DID method accepted in
/// v1 (spec §4.4). `did:web` / `did:plc` / `did:agent` are explicit
/// non-goals deferred to follow-up SPs.
fn require_did_key(did: &str, field: &'static str) -> Result<(), UcanParseError> {
    if did.starts_with("did:key:z") {
        Ok(())
    } else {
        Err(UcanParseError::UnsupportedDidMethod {
            field,
            did: did.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Phase B.1 parse-stage tests. Spec §8.1 — first 4 unit cases.
    //!
    //! Each test constructs the JWT compact form via the local
    //! `build_jwt` helper. Signatures are arbitrary bytes (`b"sig"`)
    //! because Phase B.1 does not verify them — that's Phase B.2.

    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    fn build_jwt(header: serde_json::Value, payload: serde_json::Value) -> String {
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let s = URL_SAFE_NO_PAD.encode(b"signature-placeholder");
        format!("{h}.{p}.{s}")
    }

    fn canonical_header() -> serde_json::Value {
        json!({ "alg": "EdDSA", "typ": "ucan/1.0+jwt", "ucv": "1.0" })
    }

    fn canonical_payload() -> serde_json::Value {
        json!({
            "iss":  "did:key:z6MkA_abbreviated",
            "aud":  "did:key:z6MkB_abbreviated",
            "sub":  "did:key:z6MkU_abbreviated",
            "cmd":  "atd-cap",
            "args": { "caps": ["records:read"], "with": [{"patient": "Patient/X"}] },
            "nonce": "Mv3K0000000000000000",
            "exp":  9999999999_i64
        })
    }

    #[test]
    fn parse_well_formed_token_succeeds() {
        let tok = build_jwt(canonical_header(), canonical_payload());
        let parsed = parse_jwt(&tok).expect("well-formed UCAN should parse");
        assert_eq!(parsed.iss, "did:key:z6MkA_abbreviated");
        assert_eq!(parsed.aud, "did:key:z6MkB_abbreviated");
        assert_eq!(parsed.cmd, "atd-cap");
        assert_eq!(parsed.args.caps, vec!["records:read"]);
        assert_eq!(parsed.exp, 9999999999_i64);
    }

    #[test]
    fn parse_unsupported_alg_rejects() {
        let mut h = canonical_header();
        h["alg"] = json!("RS256");
        let tok = build_jwt(h, canonical_payload());
        match parse_jwt(&tok) {
            Err(UcanParseError::UnsupportedAlg(alg)) => assert_eq!(alg, "RS256"),
            other => panic!("expected UnsupportedAlg, got {other:?}"),
        }
    }

    #[test]
    fn parse_non_did_key_issuer_rejects() {
        let mut p = canonical_payload();
        p["iss"] = json!("did:web:example.org");
        let tok = build_jwt(canonical_header(), p);
        match parse_jwt(&tok) {
            Err(UcanParseError::UnsupportedDidMethod { field, did }) => {
                assert_eq!(field, "iss");
                assert_eq!(did, "did:web:example.org");
            }
            other => panic!("expected UnsupportedDidMethod(iss), got {other:?}"),
        }
    }

    #[test]
    fn parse_malformed_jwt_rejects() {
        // Only 2 dot-segments — should fail before any decode.
        let tok = "abc.def";
        match parse_jwt(tok) {
            Err(UcanParseError::MalformedJwt(n)) => assert_eq!(n, 2),
            other => panic!("expected MalformedJwt(2), got {other:?}"),
        }

        // 4 segments — also rejected.
        let tok4 = "a.b.c.d";
        match parse_jwt(tok4) {
            Err(UcanParseError::MalformedJwt(n)) => assert_eq!(n, 4),
            other => panic!("expected MalformedJwt(4), got {other:?}"),
        }
    }

    // ---- Additional defensive tests (not in spec §8.1 but valuable) ----

    #[test]
    fn parse_non_atd_cap_command_rejects() {
        let mut p = canonical_payload();
        p["cmd"] = json!("/bsky/post");
        let tok = build_jwt(canonical_header(), p);
        match parse_jwt(&tok) {
            Err(UcanParseError::NonAtdCap(c)) => assert_eq!(c, "/bsky/post"),
            other => panic!("expected NonAtdCap, got {other:?}"),
        }
    }

    #[test]
    fn parse_wrong_typ_rejects() {
        let mut h = canonical_header();
        h["typ"] = json!("JWT");
        let tok = build_jwt(h, canonical_payload());
        assert!(matches!(
            parse_jwt(&tok),
            Err(UcanParseError::UnsupportedTyp(_))
        ));
    }

    #[test]
    fn parse_wrong_ucv_rejects() {
        let mut h = canonical_header();
        h["ucv"] = json!("0.9");
        let tok = build_jwt(h, canonical_payload());
        assert!(matches!(
            parse_jwt(&tok),
            Err(UcanParseError::UnsupportedUcv(_))
        ));
    }

    #[test]
    fn parse_garbage_base64_in_header_rejects() {
        let tok = "!!!.aaaa.bbbb";
        assert!(matches!(
            parse_jwt(tok),
            Err(UcanParseError::Base64Decode {
                segment: "header",
                ..
            })
        ));
    }

    #[test]
    fn parse_audience_non_did_key_rejects() {
        let mut p = canonical_payload();
        p["aud"] = json!("did:plc:abcdef");
        let tok = build_jwt(canonical_header(), p);
        match parse_jwt(&tok) {
            Err(UcanParseError::UnsupportedDidMethod { field, .. }) => assert_eq!(field, "aud"),
            other => panic!("expected UnsupportedDidMethod(aud), got {other:?}"),
        }
    }
}
