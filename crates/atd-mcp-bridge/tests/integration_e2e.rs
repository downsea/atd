//! End-to-end test: MCP client (this test) → atd-mcp-bridge → atd-ref-server.
//!
//! Both binaries must be pre-built in release mode:
//!   cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge
//!
//! Tests pipe raw MCP JSON-RPC through the bridge's stdio and validate the
//! responses. They use no LLM and no external MCP client — just our own
//! deterministic JSON framing, proving the bridge + ref-server pair works
//! end-to-end without help from any agent.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/atd-mcp-bridge should have a grandparent")
        .to_path_buf()
}

fn ref_server_bin() -> PathBuf {
    repo_root().join("target/release/atd-ref-server")
}

fn bridge_bin() -> PathBuf {
    repo_root().join("target/release/atd-mcp-bridge")
}

async fn wait_for_socket(sock: &Path) -> bool {
    for _ in 0..30 {
        if sock.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

struct Harness {
    _tmp: tempfile::TempDir,
    ref_server: Child,
    bridge: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Harness {
    async fn spawn() -> Result<Self, Box<dyn std::error::Error>> {
        let ref_bin = ref_server_bin();
        let br_bin = bridge_bin();
        if !ref_bin.exists() {
            return Err(format!(
                "atd-ref-server release binary missing at {}.\n\
                 build first: cargo build --release -p atd-ref-server-bin",
                ref_bin.display()
            )
            .into());
        }
        if !br_bin.exists() {
            return Err(format!(
                "atd-mcp-bridge release binary missing at {}.\n\
                 build first: cargo build --release -p atd-mcp-bridge",
                br_bin.display()
            )
            .into());
        }

        let tmp = tempfile::tempdir()?;
        let sock = tmp.path().join("sp7.sock");

        let ref_server = Command::new(&ref_bin)
            .arg("--sock")
            .arg(&sock)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        if !wait_for_socket(&sock).await {
            return Err("atd-ref-server didn't bind its socket in 3s".into());
        }

        let mut bridge = Command::new(&br_bin)
            .arg("--sock")
            .arg(&sock)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = bridge.stdin.take().expect("bridge stdin was piped");
        let stdout = BufReader::new(bridge.stdout.take().expect("bridge stdout was piped"));

        Ok(Harness {
            _tmp: tmp,
            ref_server,
            bridge,
            stdin,
            stdout,
            next_id: 0,
        })
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.next_id += 1;
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id,
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;

        let mut resp_line = String::new();
        self.stdout.read_line(&mut resp_line).await?;
        if resp_line.is_empty() {
            return Err("bridge closed stdout (EOF) while awaiting response".into());
        }
        let v: Value = serde_json::from_str(resp_line.trim_end())?;
        Ok(v)
    }

    async fn notify(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Notifications have no id and expect no response.
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&req)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn initialize(&mut self) -> Result<Value, Box<dyn std::error::Error>> {
        let r = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "atd-sp7-e2e", "version": "0.1.0"},
                }),
            )
            .await?;
        // Per MCP spec, client sends notifications/initialized after success.
        self.notify("notifications/initialized", json!({})).await?;
        Ok(r)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // kill_on_drop is set on both children; Drop will reap them.
        // Explicit start_kill is belt-and-suspenders.
        let _ = self.bridge.start_kill();
        let _ = self.ref_server.start_kill();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_initialize_handshake() {
    let mut h = Harness::spawn().await.unwrap();
    let r = h.initialize().await.unwrap();
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["id"], 1);
    let result = r.get("result").cloned().unwrap_or(Value::Null);
    assert!(
        result.get("protocolVersion").is_some(),
        "initialize response must include protocolVersion: {r}"
    );
    assert!(
        result.get("capabilities").is_some(),
        "initialize response must include capabilities: {r}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_tools_list_returns_ref_server_tools() {
    let mut h = Harness::spawn().await.unwrap();
    h.initialize().await.unwrap();

    let r = h.request("tools/list", json!({})).await.unwrap();
    let tools = r["result"]["tools"]
        .as_array()
        .expect("tools/list must return a `result.tools` array");
    let names: std::collections::HashSet<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect();

    // MCP names can differ from ATD ids via sanitization; check both raw and
    // sanitized forms liberally. The bridge's mapping is stable for these:
    // `ref:echo.say` → `ref_echo_say` etc. We accept either.
    let has = |candidates: &[&str]| candidates.iter().any(|c| names.contains(*c));
    assert!(
        has(&["ref:echo.say", "ref_echo_say"]),
        "echo missing: {names:?}"
    );
    assert!(
        has(&["ref:fs.read", "ref_fs_read"]),
        "fs.read missing: {names:?}"
    );
    assert!(
        has(&["ref:shell.exec", "ref_shell_exec"]),
        "shell.exec missing: {names:?}"
    );
    assert!(tools.len() >= 9, "expected >= 9 tools: {}", tools.len());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_tools_call_echo_success() {
    let mut h = Harness::spawn().await.unwrap();
    h.initialize().await.unwrap();

    // Discover the canonical echo name (sanitized or not) and use it.
    let listed = h.request("tools/list", json!({})).await.unwrap();
    let echo_name = listed["result"]["tools"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|t| {
                let n = t.get("name")?.as_str()?;
                if n == "ref:echo.say" || n == "ref_echo_say" {
                    Some(n.to_string())
                } else {
                    None
                }
            })
        })
        .expect("echo tool should be registered");

    let r = h
        .request(
            "tools/call",
            json!({"name": echo_name, "arguments": {"text": "hello from sp7"}}),
        )
        .await
        .unwrap();
    assert!(r.get("error").is_none(), "unexpected tools/call error: {r}");
    let result = &r["result"];
    // MCP tools/call returns content array. isError should be falsy.
    let is_error = result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(!is_error, "tools/call echo reported isError=true: {r}");
    let content_text = serde_json::to_string(&result["content"]).unwrap_or_default();
    assert!(
        content_text.contains("hello from sp7"),
        "echoed text not found in response: {content_text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_tools_call_shell_exec_real_command() {
    let mut h = Harness::spawn().await.unwrap();
    h.initialize().await.unwrap();

    let listed = h.request("tools/list", json!({})).await.unwrap();
    let shell_name = listed["result"]["tools"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|t| {
                let n = t.get("name")?.as_str()?;
                if n == "ref:shell.exec" || n == "ref_shell_exec" {
                    Some(n.to_string())
                } else {
                    None
                }
            })
        })
        .expect("shell.exec should be registered");

    let r = h
        .request(
            "tools/call",
            json!({
                "name": shell_name,
                "arguments": {"command": "echo capstone-ok"},
            }),
        )
        .await
        .unwrap();
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    let is_error = r["result"]
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(!is_error, "tools/call shell reported isError=true: {r}");
    let content_text = serde_json::to_string(&r["result"]["content"]).unwrap_or_default();
    assert!(
        content_text.contains("capstone-ok"),
        "shell stdout missing from content: {content_text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_tools_call_bad_args_signals_error() {
    let mut h = Harness::spawn().await.unwrap();
    h.initialize().await.unwrap();

    let listed = h.request("tools/list", json!({})).await.unwrap();
    let fs_read_name = listed["result"]["tools"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find_map(|t| {
                let n = t.get("name")?.as_str()?;
                if n == "ref:fs.read" || n == "ref_fs_read" {
                    Some(n.to_string())
                } else {
                    None
                }
            })
        })
        .expect("fs.read should be registered");

    let r = h
        .request(
            "tools/call",
            json!({
                "name": fs_read_name,
                "arguments": {},  // missing required `path`
            }),
        )
        .await
        .unwrap();

    // The bridge may surface this as either:
    //   (a) top-level {"error": {...}} JSON-RPC error
    //   (b) {"result": {"isError": true, "content": [...]}}
    // We accept either — the point is SOMETHING signals failure.
    let has_rpc_error = r.get("error").is_some();
    let has_tool_error = r["result"]
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        has_rpc_error || has_tool_error,
        "missing-path should have surfaced an error somewhere: {r}"
    );
}
