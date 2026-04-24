//! Built-in tool registration for `atd-ref-server`.

use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_tools_echo::EchoTool;
use atd_tools_fs::{FsEditTool, FsGlobTool, FsGrepTool, FsReadTool, FsWriteTool};
use atd_tools_shell::{ShellExecTool, ShellPwshTool};
use atd_tools_web::WebFetchTool;

/// Build the reference server's built-in tool registry.
///
/// When `enable_conformance_tool` is `true`, additionally registers
/// `ref:conformance.denied_op` — a test-only tool that requires the
/// `conformance.denied` capability. This exists solely so the
/// `atd-conformance` suite can exercise the `ERR_CAPABILITY_DENIED`
/// (code 1001) wire path. Production deployments pass `false`.
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
        use crate::conformance::ConformanceDeniedTool;
        reg.register(Arc::new(ConformanceDeniedTool::new()));
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
    fn builtin_registry_with_conformance_tool_adds_one() {
        let default = builtin_registry(false);
        let extended = builtin_registry(true);
        assert_eq!(
            extended.count(),
            default.count() + 1,
            "enabling conformance tool should add exactly one tool"
        );
        let entry = extended
            .get("ref:conformance.denied_op")
            .expect("conformance tool registered when flag is true");
        assert_eq!(
            entry.tool.definition().required_capabilities,
            vec!["conformance.denied".to_string()]
        );
    }
}
