//! Thin shim over atd-protocol::wire plus deep-subset JSON matching.

use crate::case::{BehaviorCase, SetupStep, WireCase};
use crate::runner::Outcome;
use atd_protocol::wire;
use serde_json::Value;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::net::UnixStream;

/// Default per-case wire deadline. Cases are expected to complete in
/// well under 1s; this is a protective upper bound.
pub const WIRE_TIMEOUT: Duration = Duration::from_secs(3);

/// Open a new Unix socket connection to `target` and, if `setup` is
/// present, perform its handshake. Returns the open stream ready for
/// the case's main send.
pub async fn open_and_setup(target: &Path, setup: &Option<SetupStep>) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(target).await?;
    if let Some(SetupStep::Hello {
        client_id,
        requested_capabilities,
    }) = setup
    {
        let hello = serde_json::json!({
            "type": "hello",
            "client_id": client_id,
            "requested_capabilities": requested_capabilities,
        });
        wire::write_frame(&mut stream, &hello).await?;
        // Drain the hello_ack response; we don't assert on it here — the
        // assertion is about the main send/response pair.
        let _ack: Value = wire::read_frame(&mut stream).await?;
    }
    Ok(stream)
}

/// Run a wire case end-to-end against the target socket.
pub async fn run_wire_case(case: &WireCase, target: &Path) -> Outcome {
    let res = tokio::time::timeout(WIRE_TIMEOUT, async {
        let mut stream = open_and_setup(target, &case.setup).await?;

        if let Some(hex) = &case.expect_wire_bytes_prefix_hex {
            let body = serde_json::to_vec(&case.send)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let len = u32::try_from(body.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
            let mut framed = Vec::with_capacity(4 + body.len());
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&body);
            let prefix_bytes = hex.len() / 2;
            let got_hex = encode_hex(&framed[..prefix_bytes]);
            if got_hex != hex.to_lowercase() {
                return Ok::<Outcome, io::Error>(Outcome::Fail {
                    reason: format!(
                        "wire-byte prefix mismatch: expected {}, got {}",
                        hex.to_lowercase(),
                        got_hex
                    ),
                });
            }
            use tokio::io::AsyncWriteExt;
            stream.write_all(&framed).await?;
            stream.flush().await?;
        } else {
            wire::write_frame(&mut stream, &case.send).await?;
        }

        let response: Value = wire::read_frame(&mut stream).await?;

        if let Some(expect) = &case.expect_response_matches {
            if let Err(reason) = json_matches_subset(expect, &response) {
                return Ok(Outcome::Fail { reason });
            }
        }
        Ok(Outcome::Pass)
    })
    .await;

    match res {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(io_err)) => Outcome::Fail {
            reason: format!("io error: {}", io_err),
        },
        Err(_elapsed) => Outcome::Fail {
            reason: format!("wire timeout after {:?}", WIRE_TIMEOUT),
        },
    }
}

/// Run a behavior case. Behavior ≈ wire with required
/// expect_response_matches and (typically) a Hello setup.
pub async fn run_behavior_case(case: &BehaviorCase, target: &Path) -> Outcome {
    let res = tokio::time::timeout(WIRE_TIMEOUT, async {
        let mut stream = open_and_setup(target, &case.setup).await?;
        wire::write_frame(&mut stream, &case.send).await?;
        let response: Value = wire::read_frame(&mut stream).await?;
        if let Err(reason) = json_matches_subset(&case.expect_response_matches, &response) {
            return Ok::<Outcome, io::Error>(Outcome::Fail { reason });
        }
        if let Some(excluded_ids) = &case.expect_tools_exclude {
            let tools_array = match response.get("tools").and_then(|t| t.as_array()) {
                Some(a) => a,
                None => {
                    return Ok::<Outcome, io::Error>(Outcome::Fail {
                        reason: "expect_tools_exclude requires a `tools` array in the response"
                            .into(),
                    });
                }
            };
            let present_ids: Vec<&str> = tools_array
                .iter()
                .filter_map(|t| t.get("id").and_then(|i| i.as_str()))
                .collect();
            for excluded in excluded_ids {
                if present_ids.iter().any(|id| id == excluded) {
                    return Ok::<Outcome, io::Error>(Outcome::Fail {
                        reason: format!(
                            "tool id '{excluded}' was expected to be EXCLUDED from tool_list, but appeared"
                        ),
                    });
                }
            }
        }
        Ok(Outcome::Pass)
    })
    .await;

    match res {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(io_err)) => Outcome::Fail {
            reason: format!("io error: {}", io_err),
        },
        Err(_elapsed) => Outcome::Fail {
            reason: format!("behavior timeout after {:?}", WIRE_TIMEOUT),
        },
    }
}

/// Deep-subset match: every key in `expect` must appear in `actual`
/// with a matching value (recursively). Extra keys in `actual` are
/// allowed. The literal string `"*"` in `expect` matches any value
/// except null.
///
/// Arrays require length equality and element-wise subset matching.
pub fn json_matches_subset(expect: &Value, actual: &Value) -> Result<(), String> {
    match (expect, actual) {
        (Value::String(s), a) if s == "*" => {
            if a.is_null() {
                Err("wildcard '*' matched null (null should be explicit)".into())
            } else {
                Ok(())
            }
        }
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(()),
        (Value::Number(a), Value::Number(b)) if a == b => Ok(()),
        (Value::String(a), Value::String(b)) if a == b => Ok(()),
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Err(format!(
                    "array length mismatch: expect {}, actual {}",
                    e.len(),
                    a.len()
                ));
            }
            for (i, (ei, ai)) in e.iter().zip(a.iter()).enumerate() {
                json_matches_subset(ei, ai).map_err(|r| format!("[{}]: {}", i, r))?;
            }
            Ok(())
        }
        (Value::Object(e), Value::Object(a)) => {
            for (key, ev) in e {
                let av = a
                    .get(key)
                    .ok_or_else(|| format!("missing key {:?} in actual", key))?;
                json_matches_subset(ev, av).map_err(|r| format!("{}: {}", key, r))?;
            }
            Ok(())
        }
        (e, a) => Err(format!("mismatch: expect {}, got {}", e, a)),
    }
}

/// Minimal lowercase hex encoder. Avoids pulling in the `hex` crate
/// for one call site.
fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subset_matches_identical() {
        assert!(json_matches_subset(&json!({"type": "pong"}), &json!({"type": "pong"})).is_ok());
    }

    #[test]
    fn subset_allows_extra_keys_in_actual() {
        assert!(
            json_matches_subset(
                &json!({"type": "pong"}),
                &json!({"type": "pong", "extra_field": "ok"})
            )
            .is_ok()
        );
    }

    #[test]
    fn subset_rejects_missing_key() {
        let err = json_matches_subset(
            &json!({"type": "pong", "required": true}),
            &json!({"type": "pong"}),
        )
        .unwrap_err();
        assert!(err.contains("missing key"));
    }

    #[test]
    fn subset_rejects_value_mismatch() {
        let err =
            json_matches_subset(&json!({"type": "pong"}), &json!({"type": "error"})).unwrap_err();
        assert!(err.contains("mismatch"));
    }

    #[test]
    fn subset_array_length_enforced() {
        let err = json_matches_subset(&json!([1, 2, 3]), &json!([1, 2])).unwrap_err();
        assert!(err.contains("array length mismatch"));
    }

    #[test]
    fn subset_wildcard_matches_any_non_null() {
        assert!(json_matches_subset(&json!({"id": "*"}), &json!({"id": 42}),).is_ok());
        assert!(json_matches_subset(&json!({"id": "*"}), &json!({"id": "arbitrary"}),).is_ok());
    }

    #[test]
    fn subset_wildcard_rejects_null() {
        let err = json_matches_subset(&json!({"id": "*"}), &json!({"id": null})).unwrap_err();
        assert!(err.contains("wildcard"));
    }

    #[test]
    fn subset_nested() {
        assert!(
            json_matches_subset(
                &json!({"type": "error", "inner": {"code": 1001}}),
                &json!({"type": "error", "inner": {"code": 1001, "extra": 1}, "x": 2}),
            )
            .is_ok()
        );
    }
}
