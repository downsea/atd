//! Built-in tool registration for `atd-ref-server`.

use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_tools_echo::EchoTool;
use atd_tools_fs::{FsEditTool, FsGlobTool, FsGrepTool, FsReadTool, FsWriteTool};
use atd_tools_shell::{ShellExecTool, ShellPwshTool};
use atd_tools_web::WebFetchTool;

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
