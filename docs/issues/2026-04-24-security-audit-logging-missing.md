# No structured audit log of tool calls

**Layer:** security / observability
**Status:** closed-verified
**Effort:** ~0.5 day
**Filed:** 2026-04-24
**Closed:** 2026-05-12

## Resolution

**Shipped.** Structured per-call audit landed across two SPs: the
`CallEvent` / `AuditSink` model plus the `JsonLinesAuditSink` JSONL writer
(SP-token-broker-phase1 / SP-operability-v1), then rewritten to a
non-blocking bounded `tokio::sync::mpsc` channel + dedicated drain task
in SP-concurrency-baseline (tag `sp-concurrency-baseline`, 2026-05-12).
Every dispatched call emits a `CallEvent` with timestamp, `call_id`,
`tool_id`, `args_hash`, `tier`, `binding`, `outcome`, `duration_ms`,
`secrets_resolved`, and `cursor_page`; the `drops` counter is exposed via
`Server::metrics_snapshot()`. See the `[0.3.0]` entry in
[`CHANGELOG.md`](../../CHANGELOG.md) ("Bounded mpsc audit sink") and
[`docs/atd-architecture.md`](../atd-architecture.md) §6.4. The body below is the
original gap report, kept as a record.

## Summary

`atd-ref-server` has no structured audit trail. There's no record of
which tool was invoked, with what args, by which caller (no caller
identity anyway), at what time, with what result. Without an audit
log, every other security layer — the SSRF guard, the header
allowlist, the must-read-before-edit invariant — is unobservable in
retrospect.

## Current state

- Server logs some info via `eprintln!` in the binary (startup, shutdown,
  occasional errors); this is not structured
- No `tracing` or `slog` subscriber configured
- No per-call correlation id persisted beyond the in-flight `call_id:
  ulid`
- No log of `run_tool` events (tool_id, arg hash, outcome, duration)
- `docs/integrations/hermes.md` notes users can `tail` stderr — which
  only shows the unstructured eprintln stream

## Gap

For every tool call, the server should produce (at minimum):

| Field | Source | Purpose |
|---|---|---|
| timestamp | server clock | correlate with external events |
| call_id | existing `CallContext::call_id` (ULID) | trace across client/server |
| tool_id | request | what was called |
| args_hash | hash of serde_json::Value | privacy-safe arg fingerprint |
| caller_identity | (TBD — currently always `did:anos:system`) | who called it |
| outcome | Success / Error(code) | did it work |
| duration_ms | clock | performance tracking |
| binding | (currently only `Cli`) | which backend ran it |

## Impact

- **Forensics:** after an incident ("a destructive tool was invoked
  yesterday"), there's no way to reconstruct what happened
- **Debugging:** cross-call ordering is lost; flaky e2e tests can't
  blame a specific server-side event
- **Future capability system:** once tokens exist
  (`2026-04-24-security-capability-tokens-deferred.md`), a per-call
  audit showing which token authorized what is essential; the audit
  foundation should exist first
- **Compliance:** any "GDPR/SOC2/HIPAA-ready" claim requires audit
  logs

## Proposed approach

Minimal, in-scope for v0.1.x:

1. Add `tracing` as a direct dep on atd-ref-server (it's already
   transitive via tokio-rustls etc.)
2. Wrap `Registry::dispatch` with `tracing::info_span!` and emit
   `event!(Level::INFO, call_id, tool_id, duration_ms, outcome, ...)`
   on completion
3. Hash args using `blake3` (or SHA-256) — log the hash, not the
   args themselves (args may contain secrets)
4. Subscriber is caller-configurable via `RUST_LOG` / `atd-ref-server
   --log-format json` flag; default is human-readable to stderr
5. Document in `docs/integrations/claude-code.md` how to tail the
   structured log

## Acceptance

- Every `run_tool` call produces one structured audit event
- Event includes timestamp, call_id, tool_id, args_hash, outcome,
  duration_ms
- `--log-format json` emits valid JSON lines
- One integration test spins up ref-server with JSON logging, makes a
  known-shape call, asserts the expected event appears on stderr
- `docs/protocol/error-codes.md` gains a section on audit format

## Related

- `crates/atd-ref-server/src/server.rs` (dispatch point)
- `crates/atd-ref-server/src/context.rs` (CallContext.call_id already
  exists)
- `2026-04-24-resource-limits-not-enforced.md` — shares the need for
  per-caller tracking
- `2026-04-24-security-capability-tokens-deferred.md` — audit is a
  prerequisite for meaningful authz
