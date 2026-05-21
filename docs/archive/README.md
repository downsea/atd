# Archive

Frozen historical material. **Nothing here is authoritative for ATD 1.0.** It is kept
for provenance — to answer "why was this decided" — not as guidance for current work.

For current, authoritative documentation start at [`../index.md`](../index.md).

## What is here

| Path | What it is | Superseded by |
|---|---|---|
| `superpowers/` | The **Superpowers (SP) design archive** — 33 specs + 32 plans, the phase-by-phase design record of how ATD was built from Phase 0 to 1.0. Each SP shipped under a git tag. | Forward design now lives in [`../adr/`](../adr/) (decisions) and [`../extending/`](../extending/) (how-to). |
| `design.md` | The original Phase 0 design spec (2026-04-21). | [`../architecture.md`](../architecture.md) |
| `validation/` | Three dated milestone validation logs (Hermes E2E, SP-6 capstone, SP-7 MCP bridge) — point-in-time evidence that a milestone worked. | [`../../CHANGELOG.md`](../../CHANGELOG.md) + the conformance suite (`crates/atd-conformance`). |
| `release-plan-v0.3.0.md` | The 0.3.0 release plan. The 0.3.0 line was never published; the project went straight to 1.0. | [`../release-plan-v1.0.md`](../release-plan-v1.0.md) |

## Reading the SP archive

Each SP (Superpowers unit of work) has a **spec** (`superpowers/specs/`) and a **plan**
(`superpowers/plans/`). Specs are *read-only* — they record the design as it was approved,
even where crate names or APIs inside them later changed. Do not edit them; do not treat
them as current. The implementation they describe may have drifted; `CHANGELOG.md` and the
code are the truth for what shipped.

A handful of specs were **designed but never implemented** (no matching git tag):
`sp-agent-identity`, `sp-secret-bootstrap`, `sp-streamable-http`. Their forward-looking
ideas are distilled into [`../roadmap.md`](../roadmap.md); the specs themselves remain here
as the original exploration.

Internal links inside archived files may point at paths that have since moved. That is
expected — the archive is a snapshot, not a maintained tree.
