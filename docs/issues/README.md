# atd-mvp · Tracked Issues

Honest gap-tracking for atd-mvp. Each issue documents a discrepancy between
what `docs/design.md` promises (or what the type surface implies) and what
the runtime actually delivers.

## Status vocabulary

- **tracked** — real gap, to be fixed in a future SP
- **blocked-by-design** — design rationale still evolving; waits for concrete
  adopter use case
- **deferred-phase-2** — intentionally scoped out of MVP per design.md;
  tracked for Phase 2 planning
- **accepted-trade-off** — acknowledged limitation, no planned fix

## Current issues (2026-04-24)

From the three-layer audit (schema / dispatch / security):

| # | Layer | Status | Summary |
|---|---|---|---|
| [2026-04-24-schema-protocol-machine-readable-missing.md](2026-04-24-schema-protocol-machine-readable-missing.md) | schema | tracked | No `atd-protocol-schema.json`; third-party implementers must read Rust source |
| [2026-04-24-dispatch-binding-single-impl.md](2026-04-24-dispatch-binding-single-impl.md) | dispatch | tracked | `ToolBinding` + `BindingProtocol` types imply multi-binding; runtime routes only to a single Rust `impl Tool` |
| [2026-04-24-dispatch-tier-hardcoded-warm.md](2026-04-24-dispatch-tier-hardcoded-warm.md) | dispatch | blocked-by-design | Every registered tool is `ToolTier::Warm`; no hot/cold dispatch |
| [2026-04-24-dispatch-preferred-binding-ignored.md](2026-04-24-dispatch-preferred-binding-ignored.md) | dispatch | tracked | `CallOptions::preferred_binding` reaches the server but is dropped |
| [2026-04-24-dispatch-session-cancel-not-implemented.md](2026-04-24-dispatch-session-cancel-not-implemented.md) | dispatch | blocked-by-design | Design §3.1 lists `session.start`/`session.end`/`cancel` as Phase 0 scope; not implemented |
| [2026-04-24-resource-limits-not-enforced.md](2026-04-24-resource-limits-not-enforced.md) | dispatch / security | tracked | `ToolResources.rate_limit_per_min` + `.max_concurrent` declared; server ignores both |
| [2026-04-24-security-capability-tokens-deferred.md](2026-04-24-security-capability-tokens-deferred.md) | security | deferred-phase-2 | No `CapabilityToken` / UCAN types; no token-scoped access |
| [2026-04-24-security-trust-signature-unverified.md](2026-04-24-security-trust-signature-unverified.md) | security | deferred-phase-2 | `ToolTrust.signature` field permanently `None`; `TrustLevel::L3Audited` is honor system |
| [2026-04-24-security-audit-logging-missing.md](2026-04-24-security-audit-logging-missing.md) | security | tracked | No structured audit trail of tool calls |
| [2026-04-24-security-dry-run-inconsistent.md](2026-04-24-security-dry-run-inconsistent.md) | security | tracked | `CallOptions.dry_run` honored by some tools, silently ignored by others |

## Historical

No historical closed issues yet — this directory is fresh as of
`sp11-docs`. Prior ANOS-specific gap notes were removed during SP-9
workspace polish (they were confusing for external readers and lived
in git history).

## How to file a new issue

Copy an existing file, update the frontmatter, commit. One file per
gap; keep each focused enough that a single PR could close it. If a
gap is genuinely unbounded in scope, split it into sub-issues.
