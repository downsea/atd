# ToolTier always `Warm`; no hot/cold dispatch

**Layer:** dispatch
**Status:** blocked-by-design
**Effort:** ~2 days (requires design)
**Filed:** 2026-04-24

## Summary

The protocol defines `ToolTier::{Hot, Warm, Cold}` and design.md §3.6
frames tiering as the primary scale lever for large tool registries
("H/W/C tier covers thousands"). At runtime, every registered tool in
`atd-ref-server` is hardcoded to `ToolTier::Warm`. No hot-tier
warmup, no cold-tier lazy-load, no tiered discovery caps.

## Current state

All 9 built-in tools in `atd-ref-server`:

```rust
// crates/atd-ref-server/src/tools/*/ (shared pattern)
ToolSummary { ..., tier: ToolTier::Warm, ... }
```

`Registry::dispatch()` ignores tier. `discover(filter)` can filter
*by* tier in the type surface (`DiscoverFilter::tier: Option<ToolTier>`)
but a query for `Hot` tools will correctly return an empty list —
because nothing is Hot.

## Gap

- No mechanism for a tool to self-classify its tier
- No warmup path for Hot tools at server startup
- No lazy-load path for Cold tools (every tool is eagerly registered)
- No tier-aware discovery (Cold tools not returned by default in design.md
  §2.1's envisioned behavior — but they ARE returned because everything's
  Warm)

## Impact

- **Low for MVP:** 9 tools fit in memory; tier is irrelevant at this
  scale
- **High for the design's credibility:** if the tier type exists in the
  wire protocol without runtime meaning, agents and tool registries
  built against it have to re-discover tier later

## Why blocked-by-design

Real tier implementation requires deciding:

1. **Classification source of truth** — does the tool author set its
   tier? The registry operator? An auto-profiler based on call
   frequency?
2. **Hot-tier contract** — is "Hot" just a hint to the dispatcher, or
   does it imply pre-warmed resources (sub-ms latency budget)?
3. **Cold-tier activation cost** — what's the UX when a Cold tool is
   invoked? Lazy load? Explicit grant required? Different wire code?
4. **Discovery defaults** — does `discover()` without filter return
   Hot+Warm only? Or all? How does an agent find Cold tools?

None of these has a load-bearing use case in the current MVP. The
type is a placeholder for Phase 2.

## Recommended interim

Two choices:

**Choice A:** Keep the type, document it as "informational only in
v0.1.x; real tier semantics in Phase 2." Cheap and honest.

**Choice B:** Remove `ToolTier::{Hot, Cold}` from v0.1.x wire protocol
and keep only `Warm` (or collapse to nothing). Cleaner but a breaking
change if anything downstream is checking tier.

**Recommendation:** Choice A. Document + track here. Revisit when a
registry with 1000+ tools actually appears.

## Related

- `crates/atd-types/src/enums.rs` (ToolTier)
- design.md §3.6 (tier positioning)
- `docs/protocol/wire-format.md` §5 (type table should note status)
