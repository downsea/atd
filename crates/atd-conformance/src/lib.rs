//! ATD conformance test suite.
//!
//! Drives a target ATD server through wire-format, sanitize, and
//! behavioral conformance cases loaded from JSON fixtures. Reports
//! pass/fail per case. Implementation-agnostic: any server that
//! speaks ATD over a Unix socket can be validated.
//!
//! See `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`
//! for the design.

// Modules are populated in subsequent tasks:
// pub mod case;      (Task 2)
// pub mod runner;    (Tasks 3-5)
// pub mod wire;      (Task 4)
// pub mod report;    (Task 6)

// run_conformance entry is added in Task 7.
