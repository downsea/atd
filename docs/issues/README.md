# ATD · Tracked Issues

Honest gap-tracking for the ATD reference implementation. Each issue
documents a discrepancy between what `docs/design.md` promises (or
what the type surface implies) and what the runtime actually delivers.

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
| [2026-04-24-schema-protocol-machine-readable-missing.md](2026-04-24-schema-protocol-machine-readable-missing.md) | schema | **resolved** (sp-protocol-schema) | Closed inline in body; left in tree as historical record |
| [2026-04-24-dispatch-binding-single-impl.md](2026-04-24-dispatch-binding-single-impl.md) | dispatch | tracked | `ToolBinding` + `BindingProtocol` types imply multi-binding; runtime routes only to a single Rust `impl Tool` |
| [2026-04-24-dispatch-tier-hardcoded-warm.md](2026-04-24-dispatch-tier-hardcoded-warm.md) | dispatch | blocked-by-design | Every registered tool is `ToolTier::Warm`; no hot/cold dispatch |
| [2026-04-24-dispatch-preferred-binding-ignored.md](2026-04-24-dispatch-preferred-binding-ignored.md) | dispatch | tracked | `CallOptions::preferred_binding` reaches the server but is dropped |
| [2026-04-24-dispatch-session-cancel-not-implemented.md](2026-04-24-dispatch-session-cancel-not-implemented.md) | dispatch | blocked-by-design | Design §3.1 lists `session.start`/`session.end`/`cancel` as Phase 0 scope; not implemented |
| [2026-04-24-resource-limits-not-enforced.md](2026-04-24-resource-limits-not-enforced.md) | dispatch / security | tracked | `ToolResources.rate_limit_per_min` + `.max_concurrent` declared; server ignores both |
| [2026-04-24-security-capability-tokens-deferred.md](2026-04-24-security-capability-tokens-deferred.md) | security | deferred-phase-2 (UCAN-lite shipped via sp-capability-v2 — needs restatus) | No `CapabilityToken` / UCAN types; no token-scoped access |
| [2026-04-24-security-trust-signature-unverified.md](2026-04-24-security-trust-signature-unverified.md) | security | deferred-phase-2 | `ToolTrust.signature` field permanently `None`; `TrustLevel::L3Audited` is honor system |
| [2026-04-24-security-audit-logging-missing.md](2026-04-24-security-audit-logging-missing.md) | security | tracked (audit mpsc shipped via sp-concurrency-baseline — needs restatus) | No structured audit trail of tool calls |
| [2026-04-24-security-dry-run-inconsistent.md](2026-04-24-security-dry-run-inconsistent.md) | security | tracked | `CallOptions.dry_run` honored by some tools, silently ignored by others |

From adopter requirements (2026-05):

| # | Layer | Status | Summary |
|---|---|---|---|
| [2026-05-19-cbrain-adopter-requirements.md](2026-05-19-cbrain-adopter-requirements.md) | adopter (cbrain) | triaged-2026-05-19 | 11 项已全部 ACK + 排期 (见 issue §9)；**P0-1 → `SP-server-py-v1`** (spec landed `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`)；P0-2 → SP-release-binaries-v1；P1-3+P1-4 → SP-cancel-streaming-v1；P1-6 → SP-error-namespace-v1；P2-8 bundled into P0-1；P2-10 deferred |

## Recently closed

| # | Layer | Closed | Summary |
|---|---|---|---|
| [2026-05-12-celia-concurrency-adopter-validation.md](2026-05-12-celia-concurrency-adopter-validation.md) | adopter (celia_phr) | 2026-05-12 | celia `atd-mcp-opt iter-4` 120Q SHARP baseline (0 errors, 0 rate-limit) is the integration-level proof; 60% session-init failure mode gone |
| [2026-05-12-healthkit-perf-v1-adopter-validation.md](2026-05-12-healthkit-perf-v1-adopter-validation.md) | adopter (healthkit_cli) | 2026-05-12 | `healthkit_cli/docs/sp-pagination-v1-adopter.md` — Activities + HealthRecord helpers paginate, 4 unit + 2 integration tests green |

## Historical

Prior ANOS-specific gap notes were removed during SP-9 workspace polish
(they were confusing for external readers and lived in git history).

## How to file a new issue

Copy an existing file, update the frontmatter, commit. One file per
gap; keep each focused enough that a single PR could close it. If a
gap is genuinely unbounded in scope, split it into sub-issues.
