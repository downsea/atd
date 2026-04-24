//! Shell tools: shell.exec (/bin/sh) and shell.pwsh (PowerShell).
//!
//! Subprocess execution with configurable timeouts; shared capture helper
//! reused across both tools.

pub mod exec;
pub mod pwsh;
pub mod shared;

pub use exec::ShellExecTool;
pub use pwsh::ShellPwshTool;
