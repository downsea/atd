# atd-mock-weather-server

Mock weather ATD server — a companion to
[`atd-ref-server`](../atd-ref-server/README.md) for the **cross-vendor
composition demo**. This crate is `publish = false`; it is a demo helper, not a
shipped artifact.

## What this is

A standalone ATD-speaking server binary that registers three canned
`mock:weather.*` tools (`current`, `forecast`, `alerts`) with static
responses. It demonstrates the cross-vendor composition pattern:

- Boot `atd-ref-server` on `/tmp/hk.sock`.
- Boot `atd-mock-weather-server` on `/tmp/weather.sock`.
- Point two `atd-mcp-bridge` instances at the two sockets.
- An MCP client (Claude Desktop, Cursor, Hermes) sees a merged tool catalog
  from both vendors without either knowing about the other.

See
[`scripts/cross-vendor-demo.sh`](https://github.com/downsea/atd/blob/master/scripts/cross-vendor-demo.sh)
and
[`docs/integrations/cross-vendor-pattern.md`](https://github.com/downsea/atd/blob/master/docs/integrations/cross-vendor-pattern.md)
for the full pattern.

## What this is NOT

- Not a real weather service — the returned values are hard-coded fixtures.
- Not suitable for production — no rate limiting, no caching, no real source.
- Not a reference implementation of the ATD protocol — that role belongs to
  `atd-ref-server`.

## Usage

```bash
cargo run -p atd-mock-weather-server -- --sock /tmp/weather.sock
```

Then via any ATD client:

```bash
atd list --sock /tmp/weather.sock
# → mock:weather.current, mock:weather.forecast, mock:weather.alerts

atd call mock:weather.current --sock /tmp/weather.sock \
    --args '{"location": "Palo Alto"}'
```

## Part of the ATD reference implementation

This crate is part of
[ATD — Agent Tool Dispatch](https://github.com/downsea/atd). See the
[project README](https://github.com/downsea/atd#readme) for context.

## License

Apache-2.0.
</content>
