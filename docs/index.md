# Documentation Index

The map of `atd`'s documentation. Every document, what it is for, and when to
read it. If you are an AI agent, start here, then read
[`../AGENTS.md`](../AGENTS.md).

---

## Authority hierarchy

When two documents disagree, the higher tier wins.

| Tier | Documents | Role |
|---|---|---|
| **Normative** | [`architecture.md`](architecture.md) · [`protocol/wire-format.md`](protocol/wire-format.md) · [`protocol/error-codes.md`](protocol/error-codes.md) · [`protocol/dry-run-contract.md`](protocol/dry-run-contract.md) · [`/atd-protocol-schema.json`](../atd-protocol-schema.json) | The protocol contract. Source of truth for behaviour and wire shape. |
| **Policy** | [`../README.md`](../README.md) · [`../CONTRIBUTING.md`](../CONTRIBUTING.md) · [`../AGENTS.md`](../AGENTS.md) · [`release-plan-v1.0.md`](release-plan-v1.0.md) | How the project is built, tested, and released. |
| **How-to** | [`extending/`](extending/) · [`quickstart/`](quickstart/) · [`integrations/`](integrations/) · [`cli.md`](cli.md) | Task-oriented guides. Subordinate to Normative docs. |
| **Context** | [`atd-design-philosophy.md`](atd-design-philosophy.md) · [`atd-introduction.md`](atd-introduction.md) · [`roadmap.md`](roadmap.md) · [`adr/`](adr/) · [`issues/`](issues/) | Rationale, evolution scope, decisions, tracked gaps. |
| **Archive** | [`archive/`](archive/) | Frozen history. **Never authoritative; never edit.** |

---

## I want to…

| Goal | Read |
|---|---|
| Understand what ATD is and why | [`atd-introduction.md`](atd-introduction.md), then [`architecture.md`](architecture.md) §1–§2 |
| Implement an ATD SDK or server in another language | [`protocol/wire-format.md`](protocol/wire-format.md) + [`protocol/error-codes.md`](protocol/error-codes.md) + [`/atd-protocol-schema.json`](../atd-protocol-schema.json) |
| Call ATD tools from a Rust / Python agent | [`quickstart/rust.md`](quickstart/rust.md) · [`quickstart/python.md`](quickstart/python.md) |
| Wire ATD into an existing agent framework | [`integrations/overview.md`](integrations/overview.md) |
| Add a built-in tool | [`extending/tool.md`](extending/tool.md) |
| Add a binding, middleware, transport, auth, or audit sink | [`extending/`](extending/) |
| Change the wire format or add an error code | [`extending/protocol-and-schema.md`](extending/protocol-and-schema.md) |
| Build, test, and verify a change | [`../CONTRIBUTING.md`](../CONTRIBUTING.md) |
| Use the `atd` CLI | [`cli.md`](cli.md) |
| Know what is deferred / out of scope / coming later | [`roadmap.md`](roadmap.md) + [`architecture.md`](architecture.md) §10 |
| Understand a past design decision | [`adr/`](adr/), then [`archive/superpowers/`](archive/superpowers/) |
| Know what shipped when | [`../CHANGELOG.md`](../CHANGELOG.md) |
| Release the project | [`release-plan-v1.0.md`](release-plan-v1.0.md) |

---

## Directory guide

| Path | Contents |
|---|---|
| [`architecture.md`](architecture.md) | The normative architecture — layers, dispatch, security, middleware, crate map, non-goals. |
| [`protocol/`](protocol/) | Byte-level wire format, the `AtdError` taxonomy, the dry-run contract. |
| [`extending/`](extending/) | One how-to per extension point: tool, binding, middleware, transport, token-broker, audit-sink, protocol-and-schema. |
| [`quickstart/`](quickstart/) | First-call guides — Rust, Python, TypeScript. |
| [`integrations/`](integrations/) | Per-framework wiring — LangChain, Hermes, Claude Code, OpenClaw — plus adopter case studies and the cross-vendor pattern. |
| [`adr/`](adr/) | Architecture Decision Records — the live decision log. |
| [`issues/`](issues/) | Tracked gaps and adopter-validation records. |
| [`atd-design-philosophy.md`](atd-design-philosophy.md) | Seven principles for building ATD tool servers that hold up across vendors. |
| [`roadmap.md`](roadmap.md) | Evolution scope — deferred features, known limitations, post-1.0 direction. |
| [`release-plan-v1.0.md`](release-plan-v1.0.md) | The 1.0 release contract, checklist, and publish procedure. |
| [`preview/`](preview/) | The ATD 技术预览 — a five-part Chinese slide series generated from these docs. See [`preview/README.md`](preview/README.md). |
| [`archive/`](archive/) | Frozen history — the SP design archive, the Phase 0 spec, validation logs. See [`archive/README.md`](archive/README.md). |

---

## Reading order for a new contributor

1. [`atd-introduction.md`](atd-introduction.md) — what problem ATD solves.
2. [`architecture.md`](architecture.md) — how the system is built.
3. [`../AGENTS.md`](../AGENTS.md) + [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — how to build, test, and contribute.
4. The [`extending/`](extending/) guide for your task.
