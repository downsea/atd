# ATD · Tracked Issues

Honest gap-tracking for the ATD reference implementation. Each issue
documents a discrepancy between what the protocol promises (or what
the type surface implies) and what the runtime actually delivers.

## Status vocabulary

- **tracked** — real gap, to be fixed in a future SP
- **blocked-by-design** — design rationale still evolving; waits for concrete
  adopter use case
- **deferred-phase-2** — intentionally scoped out of MVP per design.md;
  tracked for Phase 2 planning
- **accepted-trade-off** — acknowledged limitation, no planned fix
- **ready-for-{adopter}** — work-tracking issue waiting on an external repo
- **closed-verified** — adopter / external delivery confirmed; left in tree as a record

## Currently open

From the three-layer audit (2026-04-24, schema / dispatch / security):

| # | Layer | Status | Summary |
|---|---|---|---|
| [2026-04-24-dispatch-binding-single-impl.md](2026-04-24-dispatch-binding-single-impl.md) | dispatch | tracked | `ToolBinding` + `BindingProtocol` types imply multi-binding; runtime routes only to a single Rust `impl Tool` |
| [2026-04-24-dispatch-tier-hardcoded-warm.md](2026-04-24-dispatch-tier-hardcoded-warm.md) | dispatch | blocked-by-design | Every registered tool is `ToolTier::Warm`; no hot/cold dispatch |
| [2026-04-24-dispatch-preferred-binding-ignored.md](2026-04-24-dispatch-preferred-binding-ignored.md) | dispatch | tracked | `CallOptions::preferred_binding` reaches the server but is dropped |
| [2026-04-24-dispatch-session-cancel-not-implemented.md](2026-04-24-dispatch-session-cancel-not-implemented.md) | dispatch | blocked-by-design | Design §3.1 lists `session.start`/`session.end`/`cancel` as Phase 0 scope; not implemented |
| [2026-04-24-resource-limits-not-enforced.md](2026-04-24-resource-limits-not-enforced.md) | dispatch / security | tracked | `ToolResources.max_concurrent` is now enforced (per-tool semaphore); `rate_limit_per_min` is still declarative-only |
| [2026-04-24-security-trust-signature-unverified.md](2026-04-24-security-trust-signature-unverified.md) | security | deferred-phase-2 | `ToolTrust.signature` field permanently `None`; `TrustLevel::L3Audited` is honor system |
| [2026-04-24-security-dry-run-inconsistent.md](2026-04-24-security-dry-run-inconsistent.md) | security | tracked | Dispatch now short-circuits all `dry_run: true` calls uniformly; per-tool dry-run preview semantics remain unbuilt |

From adopter requirements (2026-05):

| # | Layer | Status | Summary |
|---|---|---|---|
| [2026-05-19-cbrain-adopter-requirements.md](2026-05-19-cbrain-adopter-requirements.md) | adopter (cbrain) | **P0-1 + P2-8 shipped 2026-05-19** | 11 项 triage 见 issue §9；**P0-1 (`SP-server-py-v1`) + P2-8 (bundled) shipped** in same session (Phase A-H, 8 commits, 72 tests, 22/24 conformance fixtures, 96% coverage). cbrain swap-over ready. Queued: P0-2 / P1-3+4 / P1-6 / P2-7 / P2-9 / P2-11. Deferred: P1-5 / P2-10. |

## Recently closed

| # | Layer | Closed | Summary |
|---|---|---|---|
| [2026-04-24-schema-protocol-machine-readable-missing.md](2026-04-24-schema-protocol-machine-readable-missing.md) | schema | 2026-04-25 | `/atd-protocol-schema.json` shipped (SP-protocol-schema; CI drift gate + 2020-12 meta-schema validity) |
| [2026-04-24-security-capability-tokens-deferred.md](2026-04-24-security-capability-tokens-deferred.md) | security | 2026-05-11 | UCAN-lite capability tokens shipped (SP-capability-v2; `Hello.ucan_tokens`, error codes 1010–1013, revocation store) |
| [2026-04-24-security-audit-logging-missing.md](2026-04-24-security-audit-logging-missing.md) | security | 2026-05-12 | Structured `CallEvent` audit + non-blocking mpsc `JsonLinesAuditSink` shipped (SP-operability-v1 + SP-concurrency-baseline) |
| [2026-05-12-celia-concurrency-adopter-validation.md](2026-05-12-celia-concurrency-adopter-validation.md) | adopter (celia_phr) | 2026-05-12 | celia `atd-mcp-opt iter-4` 120Q SHARP baseline (0 errors, 0 rate-limit) is the integration-level proof; 60% session-init failure mode gone |
| [2026-05-12-healthkit-perf-v1-adopter-validation.md](2026-05-12-healthkit-perf-v1-adopter-validation.md) | adopter (healthkit_cli) | 2026-05-12 | `healthkit_cli/docs/sp-pagination-v1-adopter.md` — Activities + HealthRecord helpers paginate, 4 unit + 2 integration tests green |

## Historical

Prior ANOS-specific gap notes were removed during SP-9 workspace polish
(they were confusing for external readers and lived in git history).

## How to file a new issue

Copy an existing file, update the frontmatter, commit. One file per
gap; keep each focused enough that a single PR could close it. If a
gap is genuinely unbounded in scope, split it into sub-issues.
