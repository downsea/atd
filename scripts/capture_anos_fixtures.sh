#!/usr/bin/env bash
# Capture live responses from a running ANOS daemon so atd-client has a
# contract fixture to test against. Run this when you have an ANOS daemon
# listening on ~/.anos/anos.sock.
#
# Output: crates/atd-client/tests/fixtures/anos_{tool_list,tool_schema_fs_read}.json

set -euo pipefail

SOCK="${ANOS_SOCK:-$HOME/.anos/anos.sock}"
FIXTURE_DIR="$(cd "$(dirname "$0")/.." && pwd)/crates/atd-client/tests/fixtures"

if [ ! -S "$SOCK" ]; then
  echo "error: no ANOS socket at $SOCK" >&2
  exit 1
fi

mkdir -p "$FIXTURE_DIR"

# We use a tiny helper binary compiled on-the-fly because writing
# a length-prefixed Unix-socket client in bash is painful.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cat > "$TMP/Cargo.toml" <<'EOF'
[package]
name = "capture"
version = "0.0.1"
edition = "2021"
[dependencies]
tokio = { version = "1", features = ["net","io-util","rt-multi-thread","macros"] }
EOF
mkdir -p "$TMP/src"
cat > "$TMP/src/main.rs" <<'EOF'
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

async fn call(sock: &str, req: &str) -> std::io::Result<Vec<u8>> {
    let mut s = UnixStream::connect(sock).await?;
    let body = req.as_bytes();
    s.write_all(&(body.len() as u32).to_be_bytes()).await?;
    s.write_all(body).await?;
    s.flush().await?;
    let mut lb = [0u8; 4];
    s.read_exact(&mut lb).await?;
    let n = u32::from_be_bytes(lb) as usize;
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf).await?;
    Ok(buf)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let sock = std::env::args().nth(1).expect("sock path");
    let out = std::env::args().nth(2).expect("out dir");
    let req = std::env::args().nth(3).expect("json request");
    let name = std::env::args().nth(4).expect("out name");
    let bytes = call(&sock, &req).await?;
    std::fs::write(format!("{}/{}", out, name), &bytes)?;
    eprintln!("wrote {}/{} ({} bytes)", out, name, bytes.len());
    Ok(())
}
EOF

(cd "$TMP" && cargo build --quiet --release)

CAPTURE="$TMP/target/release/capture"

"$CAPTURE" "$SOCK" "$FIXTURE_DIR" '{"type":"tool_list"}' anos_tool_list.json
"$CAPTURE" "$SOCK" "$FIXTURE_DIR" '{"type":"tool_schema","tool_id":"anos:fs.read"}' anos_tool_schema_fs_read.json

echo "ok: fixtures in $FIXTURE_DIR"
