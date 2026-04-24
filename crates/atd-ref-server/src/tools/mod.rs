//! Remaining built-in tools shipped inside atd-ref-server-bin after
//! the tool-crate extraction (C5). The echo/fs/shell/web tools live in
//! their own `atd-tools-*` crates; only `external` (SP-12 CliBinding
//! demo) stays local — it's a binding demo, not a reusable tool crate.

#[cfg(unix)]
pub mod external;
