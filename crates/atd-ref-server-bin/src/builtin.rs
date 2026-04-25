//! Built-in tool registration for `atd-ref-server`.

use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_tools_echo::EchoTool;
use atd_tools_fs::{FsEditTool, FsGlobTool, FsGrepTool, FsReadTool, FsWriteTool};
use atd_tools_shell::{ShellExecTool, ShellPwshTool};
use atd_tools_web::WebFetchTool;

/// Build the reference server's built-in tool registry.
///
/// When `enable_conformance_tool` is `true`, additionally registers two
/// test-only tools used by the `atd-conformance` suite:
/// - `ref:conformance.denied_op` — requires the `conformance.denied`
///   capability so the suite can exercise `ERR_CAPABILITY_DENIED`
///   (code 1001).
/// - `ref:conformance.saturate_op` — declares `max_concurrent=1`; its
///   sole permit is leaked at startup by `main.rs`, so any client
///   call returns `ERR_RATE_LIMITED` (code 1002).
///
/// Production deployments pass `false`.
pub fn builtin_registry(enable_conformance_tool: bool) -> Registry {
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

    #[cfg(unix)]
    {
        use crate::external::uname;
        let stub = Arc::new(uname::UnameStub::new());
        let binding = Arc::new(uname::cli_binding());
        reg.register_with_binding(stub, binding);
    }

    if enable_conformance_tool {
        use crate::conformance::{ConformanceDeniedTool, ConformanceSaturatedTool};
        reg.register(Arc::new(ConformanceDeniedTool::new()));
        reg.register(Arc::new(ConformanceSaturatedTool::new()));
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry(false);
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
        assert!(
            r.get("ref:conformance.denied_op").is_none(),
            "conformance tool must NOT be registered by default"
        );
        #[cfg(unix)]
        {
            let entry = r
                .get("ref:external.uname")
                .expect("uname registered on unix");
            assert_eq!(entry.binding.name(), "cli");
        }
    }

    #[test]
    fn builtin_registry_with_conformance_tools_adds_two() {
        let default = builtin_registry(false);
        let extended = builtin_registry(true);
        assert_eq!(
            extended.count(),
            default.count() + 2,
            "enabling conformance tools should add exactly two tools"
        );

        // denied_op (SP-8.1)
        let denied = extended
            .get("ref:conformance.denied_op")
            .expect("denied_op registered when flag is true");
        assert_eq!(
            denied.tool.definition().required_capabilities,
            vec!["conformance.denied".to_string()]
        );

        // saturate_op (SP-8.2)
        let saturate = extended
            .get("ref:conformance.saturate_op")
            .expect("saturate_op registered when flag is true");
        assert_eq!(saturate.tool.definition().resources.max_concurrent, 1);
        assert!(saturate.tool.definition().required_capabilities.is_empty());

        // Default registry must NOT contain either
        assert!(default.get("ref:conformance.denied_op").is_none());
        assert!(default.get("ref:conformance.saturate_op").is_none());
    }

    #[test]
    fn shell_tools_declare_dry_run_true() {
        let reg = builtin_registry(false);
        let exec = reg
            .get("ref:shell.exec")
            .expect("shell.exec registered by default");
        assert!(
            exec.tool.definition().safety.dry_run,
            "shell.exec has side effects → should declare dry_run: true"
        );
        let pwsh = reg
            .get("ref:shell.pwsh")
            .expect("shell.pwsh registered by default");
        assert!(
            pwsh.tool.definition().safety.dry_run,
            "shell.pwsh has side effects → should declare dry_run: true"
        );
    }
}
