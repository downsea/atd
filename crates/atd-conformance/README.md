# atd-conformance

Cross-implementation conformance suite for the
[ATD (Agent Tool Dispatch) protocol](https://github.com/downsea/atd).

## What it proves

ATD is a neutral protocol with more than one implementation (Rust, Python, and
vendor servers). This suite is the executable definition of "speaks ATD
correctly". It replays a fixed set of fixtures against any target server and
asserts the responses match — proving wire-format, sanitization, and
core-behaviour equivalence with the reference SDK. An adopter who passes the
suite can claim ATD conformance; a regression in any implementation shows up
here.

The suite covers three fixture categories:

| Category | Verifies |
|---|---|
| `wire` | Byte-level framing and message shapes — `ping` round-trip, `Hello` handshake, `run_tool` / `tool_list` / `tool_schema` response shapes, error responses. |
| `sanitize` | Tool-id sanitization rules — `ref:fs.read` → `ref_fs_read`, separator handling, collisions. Pure-function checks, no server needed. |
| `behavior` | Dispatch semantics — capability gating (code `1001`), rate limiting (code `1002`), `dry_run` flag preservation, `Hidden` visibility filtering, unknown-tool handling. |

## Running it

As a binary, against a running server:

```bash
atd-conformance --target unix:/path/to/server.sock
```

Useful flags: `--filter <substring>` (run cases by name), `--category wire`
(repeatable; restrict categories), `--report json`, `--stop-on-first-fail`,
`--fixtures-root <dir>` (override the bundled fixtures).

As a library, from an adopter's integration test — dev-dep on this crate and
call `run_conformance`:

```rust
use atd_conformance::{Opts, run_conformance};
use atd_sdk::Endpoint;
use std::path::PathBuf;

let opts = Opts {
    target: Endpoint::unix("/tmp/my-server.sock"),
    filter: None,
    categories: Vec::new(),          // empty = all
    stop_on_first_fail: false,
    fixtures_root: PathBuf::from("path/to/atd-conformance/fixtures"),
};
let report = run_conformance(opts).await;
assert_eq!(report.failed, 0);
```

A consuming crate must pass `fixtures_root` explicitly — `CARGO_MANIFEST_DIR`
points at the consumer, not here. `Opts::with_default_fixtures` is only valid
from within `atd-conformance` itself.

## Fixture directory layout

```
fixtures/
├── behavior/   one JSON file per behaviour case
├── sanitize/   one JSON file per sanitization case
└── wire/       one JSON file per wire-shape case
```

Each file is a single JSON object whose `category` field selects the schema:

- **wire / behavior** — `send` (a request) + `expect_response_matches` (the
  expected response; `"*"` is a wildcard). Behavior cases may also carry
  `setup` and `expect_tools_exclude` (tool ids that MUST NOT appear in a
  `tool_list` response — used to verify `ToolVisibility::Hidden` filtering).
- **sanitize** — `input` (a raw tool id) + `expect_sanitized` (the expected
  sanitized form).

## Adding a fixture

1. Drop a new JSON file into the matching `fixtures/<category>/` directory with
   a descriptive `name`.
2. Fill in `category`, `name`, `description`, and the category-specific fields
   above.
3. Run `cargo test -p atd-conformance` (the suite is loaded and replayed
   against the reference server in this crate's tests).

Malformed JSON surfaces as a single synthetic "loader" failure, so a bad
fixture fails loudly rather than being skipped.

## License

Apache-2.0.
</content>
