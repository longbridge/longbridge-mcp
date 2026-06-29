#!/usr/bin/env python3
"""Minimal MCP client — supports streamable-HTTP and stdio transports.

Performs the standard initialize → tools/list handshake and prints results.

Usage (HTTP, requires token):
    python3 tests/mcp_client.py --token <bearer>
    python3 tests/mcp_client.py --url https://openapi.longbridge.com/mcp --token <bearer>

Usage (stdio, no token needed):
    python3 tests/mcp_client.py --stdio
    python3 tests/mcp_client.py --stdio --bin ./target/debug/longbridge-mcp

Options:
    --tool NAME     Print full JSON schema for a single tool and exit
"""

import argparse
import json
import subprocess
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen

DEFAULT_URL = "https://mcp.longbridge.com"
DEFAULT_BIN = "./target/debug/longbridge-mcp"

INIT_PAYLOAD = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "mcp-client", "version": "0.1.0"},
    },
}
LIST_PAYLOAD = {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}


# ── HTTP transport ────────────────────────────────────────────────────


def http_post(
    url: str, payload: dict, token: str | None, session_id: str | None
) -> dict:
    body = json.dumps(payload).encode()
    headers = {"Content-Type": "application/json", "Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if session_id:
        headers["Mcp-Session-Id"] = session_id
    req = Request(url, data=body, headers=headers, method="POST")
    try:
        with urlopen(req, timeout=10) as resp:
            return json.loads(resp.read())
    except HTTPError as e:
        print(f"HTTP {e.code}: {e.read().decode()}", file=sys.stderr)
        sys.exit(1)


def http_session(url: str, token: str | None) -> tuple[dict, list[dict]]:
    init = http_post(url, INIT_PAYLOAD, token, None)
    session_id = init.get("result", {}).get("sessionId")
    listing = http_post(url, LIST_PAYLOAD, token, session_id)
    return init["result"], listing["result"]["tools"]


# ── Stdio transport ───────────────────────────────────────────────────


def stdio_session(binary: str) -> tuple[dict, list[dict]]:
    proc = subprocess.Popen(
        [binary, "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    assert proc.stdin and proc.stdout
    input_lines = "\n".join([json.dumps(INIT_PAYLOAD), json.dumps(LIST_PAYLOAD)]) + "\n"
    stdout, _ = proc.communicate(input_lines.encode(), timeout=10)

    results: dict[int, dict] = {}
    for line in stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        if "id" in msg:
            results[msg["id"]] = msg

    return results[1]["result"], results[2]["result"]["tools"]


# ── Output ────────────────────────────────────────────────────────────


def print_results(
    server_info: dict, tools: list[dict], tool_filter: str | None
) -> None:
    info = server_info.get("serverInfo", {})
    print(f"Server:   {info.get('name')} {info.get('version')}")
    print(f"Protocol: {server_info.get('protocolVersion')}")
    print()

    if tool_filter:
        for t in tools:
            if t["name"] == tool_filter:
                print(json.dumps(t, indent=2, ensure_ascii=False))
                return
        print(f"Tool '{tool_filter}' not found.", file=sys.stderr)
        sys.exit(1)

    print(f"Tools ({len(tools)}):")
    for t in sorted(tools, key=lambda x: x["name"]):
        desc = t.get("description", "").split("\n")[0][:72]
        print(f"  {t['name']:<40} {desc}")


# ── Entry point ───────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--url",
        default=DEFAULT_URL,
        help="MCP server URL for HTTP mode (default: %(default)s)",
    )
    parser.add_argument("--token", default=None, help="Bearer token (HTTP mode)")
    parser.add_argument(
        "--stdio", action="store_true", help="Use stdio transport instead of HTTP"
    )
    parser.add_argument(
        "--bin",
        default=DEFAULT_BIN,
        help="Server binary path for stdio mode (default: %(default)s)",
    )
    parser.add_argument(
        "--tool",
        default=None,
        metavar="NAME",
        help="Print full schema for a single tool and exit",
    )
    args = parser.parse_args()

    if args.stdio:
        server_info, tools = stdio_session(args.bin)
    else:
        server_info, tools = http_session(args.url, args.token)

    print_results(server_info, tools, args.tool)


if __name__ == "__main__":
    main()
