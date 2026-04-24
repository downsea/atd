"""JSON-RPC-less protocol envelope used by atd-client ↔ ANOS daemon.

Matches the Rust `atd-client::protocol`. Requests/responses are plain JSON
objects with a ``type`` tag. We keep the tags as string constants because the
message set is small and we don't want the overhead of separate classes for
what are essentially dict shapes.
"""

from __future__ import annotations

from typing import Any

# Request types (client → server).
REQ_PING = "ping"
REQ_HELLO = "hello"
REQ_TOOL_LIST = "tool_list"
REQ_TOOL_SCHEMA = "tool_schema"
REQ_RUN_TOOL = "run_tool"

# Response types (server → client).
RESP_PONG = "pong"
RESP_HELLO_ACK = "hello_ack"
RESP_TOOL_LIST = "tool_list"
RESP_TOOL_SCHEMA = "tool_schema"
RESP_TOOL_RESULT = "tool_result"
RESP_ERROR = "error"

# SP-12: error code on `Response::Error.code` when the server's capability
# gate refuses a tool call.
ERR_CAPABILITY_DENIED = 1001


def ping_request() -> dict[str, Any]:
    return {"type": REQ_PING}


def hello_request(
    client_id: str | None, requested_capabilities: list[str]
) -> dict[str, Any]:
    """Build a Hello handshake request. SP-12."""
    out: dict[str, Any] = {
        "type": REQ_HELLO,
        "requested_capabilities": list(requested_capabilities),
    }
    if client_id is not None:
        out["client_id"] = client_id
    return out


def tool_list_request() -> dict[str, Any]:
    return {"type": REQ_TOOL_LIST}


def tool_schema_request(tool_id: str) -> dict[str, Any]:
    return {"type": REQ_TOOL_SCHEMA, "tool_id": tool_id}


def run_tool_request(tool_id: str, args: Any, dry_run: bool) -> dict[str, Any]:
    return {
        "type": REQ_RUN_TOOL,
        "tool_id": tool_id,
        "args": args,
        "dry_run": dry_run,
    }
