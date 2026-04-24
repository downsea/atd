//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new native tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from the appropriate `tools/*/mod.rs`.
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.
//!
//! To add a CLI-backed tool (SP-12): provide a stub `Tool` (carrying only
//! the definition) and a `CliBinding`, then call
//! `reg.register_with_binding(stub, binding)`. See
//! `tools::external::uname` for the pattern.

use std::sync::Arc;

use atd_runtime::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{
    edit::FsEditTool, glob::FsGlobTool, grep::FsGrepTool, read::FsReadTool, write::FsWriteTool,
};
use crate::tools::shell::{exec::ShellExecTool, pwsh::ShellPwshTool};
use crate::tools::web::fetch::WebFetchTool;

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg.register(Arc::new(FsGlobTool::new()));
    reg.register(Arc::new(FsGrepTool::new()));
    reg.register(Arc::new(ShellExecTool::new()));
    reg.register(Arc::new(ShellPwshTool::new()));
    reg.register(Arc::new(WebFetchTool::new()));

    // SP-12: CliBinding demo. Gated on unix since /usr/bin/uname is not
    // guaranteed on Windows. Registration is a single `register_with_binding`
    // call — the stub Tool holds only the definition; dispatch runs the
    // CliBinding, so Tool::call here is unreachable in practice.
    #[cfg(unix)]
    {
        use crate::tools::external::uname;
        let stub = Arc::new(uname::UnameStub::new());
        let binding = Arc::new(uname::cli_binding());
        reg.register_with_binding(stub, binding);
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        // 9 native + 1 CLI-binding tool on unix; 9 on windows.
        #[cfg(unix)]
        assert_eq!(r.count(), 10);
        #[cfg(not(unix))]
        assert_eq!(r.count(), 9);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
        assert!(r.get("ref:fs.glob").is_some());
        assert!(r.get("ref:fs.grep").is_some());
        assert!(r.get("ref:shell.exec").is_some());
        assert!(r.get("ref:shell.pwsh").is_some());
        assert!(r.get("ref:web.fetch").is_some());
        #[cfg(unix)]
        {
            let entry = r
                .get("ref:external.uname")
                .expect("uname registered on unix");
            assert_eq!(entry.binding.name(), "cli");
        }
    }
}
