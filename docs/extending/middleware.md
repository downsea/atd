# Adding result middleware

**Purpose:** rewrite, validate, or redact a tool's successful result before it
goes back on the wire, by implementing the `Middleware` trait.

## When to use this

Middleware is the **egress-side hook** between a tool's successful return and
the serialized reply. Use it for cross-cutting concerns that should apply to
many tools without each tool knowing about them:

- output redaction (strip `$HOME` paths, redact PHI),
- egress schema validation (FHIR R4 conformance),
- annotation, compression, field shaping.

Middleware runs **on success only** — error responses flow past untouched.
If you need to change *how a tool executes*, that is a [binding](binding.md);
if you need a brand-new capability, that is a [tool](tool.md).

## The trait

`Middleware` is defined in `crates/atd-runtime/src/middleware.rs`, re-exported
as `atd_runtime::Middleware`:

```rust
pub trait Middleware: Send + Sync {
    /// Short discriminator: "redact_paths", "fhir_egress_validate", …
    fn name(&self) -> &'static str;

    /// Invoked after a tool returns successfully, with a mutable reference
    /// to the result value. Mutate in place. Must be deterministic and
    /// side-effect-free beyond the `result` mutation.
    fn on_result(&self, tool_id: &str, tool_def: &ToolDefinition, result: &mut serde_json::Value);
}
```

`on_result` takes the result by `&mut serde_json::Value`. You can rewrite a
sub-tree, strip fields, annotate, or replace the whole value with an error
envelope (the fail-closed pattern, below).

## Pipeline ordering

A server holds a `Vec<Arc<dyn Middleware>>`. Dispatch runs them **top-down** —
first registered runs first — over the result of every successful call (this
applies to both the initial `RunTool` and every `RunToolContinue` page; see
`run_tool` and `run_tool_continue` in `crates/atd-runtime/src/dispatch.rs`).
Order is therefore semantically significant: validate-then-redact and
redact-then-validate give different outputs. Compose deterministically.

## The two reference implementations

- **`RedactPathsMiddleware`** (`crates/atd-runtime/src/middleware.rs`) — the
  minimal example. Holds `Vec<(regex::Regex, String)>` pattern/replacement
  pairs; `on_result` walks every string leaf of the result tree and applies
  each pair in order. `RedactPathsMiddleware::with_home_default()` builds one
  that masks `$HOME`. `name()` → `"redact_paths"`.
- **`FhirMiddleware`** (`crates/atd-middleware-fhir`) — a real standalone
  middleware crate. `on_result` detects FHIR-shaped JSON (presence of
  `resourceType`), validates resource type / required fields / coding-system
  URIs, and applies a `MismatchPolicy`. `name()` → `"fhir_egress_validate"`.
  Non-FHIR results pass through untouched.

## Fail-closed vs. annotate

`FhirMiddleware` shows the two response postures via its `MismatchPolicy`
(`crates/atd-middleware-fhir/src/config.rs`):

- **Annotate** (`MismatchPolicy::AnnotateAndPass`, the default) — attach
  findings to the result (`_fhir_validation_errors: [...]`) and let it through.
  The caller sees the data *and* the warnings. Use when the consumer can
  tolerate imperfect data.
- **Fail-closed** (`MismatchPolicy::ReplaceWithError`) — overwrite the whole
  result with a structured error envelope. Nothing non-conforming reaches the
  caller. Use when an invariant must hold absolutely (a strict-compliance
  adopter).
- **Strip** (`MismatchPolicy::StripOffending`) — drop only the offending
  sub-tree, keep the rest.

Choose the posture deliberately. A redaction middleware that *annotates* still
ships the unredacted data — for PHI/secrets you almost always want strip or
fail-closed.

## Step by step

1. **Create a crate** `atd-middleware-<topic>` (convention — see below), or a
   module if the middleware is server-local. Depend on `atd-runtime` and
   `atd-protocol`.
2. **Define the struct.** Keep the hot-path lookups cheap — `FhirMiddleware`
   `Arc`-wraps its sets so cloning the middleware per connection is cheap.
3. **`impl Middleware`.** `name()` returns a stable `&'static str`.
4. **Write `on_result`.** Inspect `result`; decide whether it applies (FHIR
   checks for `resourceType`, redaction checks string leaves). Mutate in place.
   Keep it deterministic — no I/O that changes the output, no randomness.
5. **(Optional) make the posture configurable** with a config enum, the way
   `MismatchPolicy` is, so adopters pick annotate vs. fail-closed.

## Wiring it in

Compose the pipeline at server construction, **before `run()`**:

```rust
let mut server = atd_server::Server::new(registry, cfg);
server.set_middleware(vec![
    Arc::new(FhirMiddleware::default()),
    Arc::new(PiiRedactMiddleware::default()),
    Arc::new(RedactPathsMiddleware::with_home_default()),
]);
```

The HTTP transport takes the same vec via `ServerBuilder::middleware(...)`.
`set_middleware` uses `Arc::get_mut` internally — calling it after `run()` has
spawned connection tasks panics. Set it once at startup.

## Testing it

Middleware tests need no socket — call `on_result` directly with a stub
`ToolDefinition` and assert on the mutated value. The `FhirMiddleware` tests are
the template:

```rust
#[test]
fn passes_non_fhir_result_untouched() {
    let mw = FhirMiddleware::default();
    let mut v = serde_json::json!({"echoed": "hi"});
    let snapshot = v.clone();
    mw.on_result("ref:echo.say", &stub_def(), &mut v);
    assert_eq!(v, snapshot);   // no-op on non-matching input
}
```

Cover: a no-op on non-matching input, the rewrite on matching input, and each
configurable posture (annotate / fail-closed / strip).

## The standalone-crate convention

Middleware that pulls heavy or domain-specific dependencies ships as its own
crate named `atd-middleware-<topic>` — `atd-middleware-fhir`,
`atd-middleware-pii-redact-medical`. This keeps adopters who don't need FHIR or
PHI handling from pulling those deps. The `Middleware` trait is `pub` and
stable; a third party can publish `atd-middleware-<topic>` with no coordination.
Server-local middleware with no special deps can live as a module instead.

## Invariants you must preserve

- **Success-only.** `on_result` runs only on successful tool returns; error
  responses bypass middleware. Do not assume you see every call.
- **Deterministic and side-effect-free** beyond the `result` mutation. Same
  input → same output. No hidden state, no nondeterministic I/O.
- **Never panic.** A panic in `on_result` takes down the connection.
- **Pick the posture explicitly.** Annotate ships the data; fail-closed does
  not. For anything sensitive, do not default to annotate.
- **`name()` is stable.**
- **Order matters** — document where in the pipeline your middleware expects to
  sit if it depends on another's output.

## See also

- [`../atd-architecture.md`](../atd-architecture.md) §7 (middleware), §7.2 (the FHIR
  whitelist drift-guard invariant).
- `crates/atd-middleware-pii-redact-medical` — HIPAA Safe Harbor PHI redaction,
  a second standalone-crate example.
