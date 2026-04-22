//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from `tools/mod.rs` (and `tools/fs/mod.rs` for fs tools).
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{edit::FsEditTool, read::FsReadTool, write::FsWriteTool};

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        assert_eq!(r.count(), 4);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
    }
}
