# atd-mock-weather-server

Mock weather ATD server — companion to `atd-ref-server` for the
**cross-vendor composition demo** (SP-cross-vendor-mock-demo).

## What this is

A standalone ATD-speaking server binary that registers three canned
`mock:weather.*` tools (`current`, `forecast`, `alerts`) with static
responses. Used to demonstrate the cross-vendor composition pattern:

- Boot `atd-ref-server` (Huawei healthkit-style tools) on
  `/tmp/hk.sock`
- Boot `atd-mock-weather-server` on `/tmp/weather.sock`
- Configure two `atd-mcp-bridge` instances pointing at each socket
- An MCP client (Claude Desktop, Cursor, Hermes) sees a merged tool
  catalog from both vendors without either knowing about the other.

See [`scripts/cross-vendor-demo.sh`](https://github.com/downsea/atd-mvp/blob/master/scripts/cross-vendor-demo.sh) and
[`docs/integrations/cross-vendor-pattern.md`](https://github.com/downsea/atd-mvp/blob/master/docs/integrations/cross-vendor-pattern.md)
for the full pattern.

## What this is NOT

- ❌ A real weather service — the returned values are hard-coded fixtures
- ❌ Suitable for production — no rate limiting, no caching, no real source
- ❌ A reference implementation of the ATD protocol — that role belongs
  to `atd-ref-server`

## Usage

```bash
cargo install atd-mock-weather-server
atd-mock-weather-server --sock /tmp/weather.sock
```

Then via any ATD client:

```bash
atd discover --endpoint unix:/tmp/weather.sock
# → mock:weather.current, mock:weather.forecast, mock:weather.alerts

atd call mock:weather.current --endpoint unix:/tmp/weather.sock \
    --args '{"location": "Palo Alto"}'
# → {"temp_c": 18, "condition": "sunny", "humidity": 0.45}
```

## Part of the ATD reference implementation

This crate is part of [ATD — Agent Tool Dispatch](https://github.com/downsea/atd-mvp).
See the [project README](https://github.com/downsea/atd-mvp#readme) for
context.

## License

Apache-2.0.
