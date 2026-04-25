//! atd-mcp-bridge: MCP-over-stdio bridge to an ATD server.
//!
//! Usage:
//!   atd-mcp-bridge [--sock PATH]
//!
//! Speaks MCP (JSON-RPC 2.0) on stdin/stdout, logs to stderr.

use atd_mcp_bridge::bridge::Bridge;
use atd_mcp_bridge::jsonrpc::{Response, read_request, write_response};
use atd_sdk::{AtdClient, Endpoint};
use std::io::{BufReader, BufWriter};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Parse argv: look for "--sock <path>".
    let mut args = std::env::args().skip(1);
    let mut sock_path: Option<std::path::PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sock" => {
                sock_path = args.next().map(std::path::PathBuf::from);
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: atd-mcp-bridge [--sock PATH]\n\nOne of --sock PATH or ATD_SOCK env var is required.\nPoints the bridge at an ATD-speaking Unix socket."
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("atd-mcp-bridge: unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let sock = sock_path.or_else(|| {
        std::env::var("ATD_SOCK")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
    });

    let sock = match sock {
        Some(p) => p,
        None => {
            eprintln!(
                "atd-mcp-bridge: no target socket configured.\n\
             specify --sock PATH or set ATD_SOCK=/path/to/atd-server.sock"
            );
            std::process::exit(2);
        }
    };

    let endpoint = Endpoint::unix(sock);

    eprintln!("atd-mcp-bridge: connecting to {endpoint:?}");
    let client = match AtdClient::connect(endpoint).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("atd-mcp-bridge: connect failed: {e}");
            std::process::exit(1);
        }
    };

    // SP-12 capability handshake: if the operator wants this MCP-bridge
    // session to hold capabilities (so it can reach gated tools), set
    // `ATD_REQUEST_CAPS=cap1,cap2,...`. Capabilities not in the server's
    // allow-list are silently dropped (server returns the granted subset).
    // Empty / unset = no caps requested = bridge can only call tools with
    // `required_capabilities: []` (matches pre-SP-12 behavior).
    if let Ok(caps_str) = std::env::var("ATD_REQUEST_CAPS") {
        let caps: Vec<String> = caps_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !caps.is_empty() {
            match client.hello(Some("atd-mcp-bridge"), caps.clone()).await {
                Ok(granted) => {
                    eprintln!("atd-mcp-bridge: hello requested {caps:?}, granted {granted:?}");
                }
                Err(e) => {
                    eprintln!("atd-mcp-bridge: hello failed (continuing without caps): {e}");
                }
            }
        }
    }

    eprintln!("atd-mcp-bridge: connected; entering stdio loop");

    let bridge = Bridge::new(client);

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    loop {
        match read_request(&mut reader) {
            Ok(None) => {
                eprintln!("atd-mcp-bridge: stdin closed; exiting cleanly");
                return;
            }
            Ok(Some(req)) => {
                let method = req.method.clone();
                let id = req.id.clone();
                let resp = bridge.handle(req).await;
                if let Some(r) = resp {
                    if let Err(e) = write_response(&mut writer, &r) {
                        eprintln!("atd-mcp-bridge: write failed on {method}: {e}");
                        return;
                    }
                } else {
                    eprintln!("atd-mcp-bridge: notification {method} (no reply)");
                }
                let _ = id; // unused; retained for future logging
            }
            Err(e) => {
                eprintln!("atd-mcp-bridge: parse error: {e}");
                // Per JSON-RPC 2.0, send a parse error if we could recover an id,
                // but since parsing failed we can't — just log and exit.
                let err =
                    Response::err(serde_json::Value::Null, -32700, format!("parse error: {e}"));
                let _ = write_response(&mut writer, &err);
                return;
            }
        }
    }
}
