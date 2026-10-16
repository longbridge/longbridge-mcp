#!/usr/bin/env python3
"""Compare each tool's real response against its published output schema.

The output schemas (exposed as `lb://tools/<name>/output-schema` resources) are
hand-written and are never checked against upstream in CI. This script calls
every read-only tool whose only required argument is `symbol` or `symbols`
against a running server and reports:

- response fields missing from the schema (two levels deep);
- snake_case field names the tool description mentions that never appear in
  the response (the description is what models read when writing `_jq`).

Needs a real OAuth token, so it is a pre-release check, not a CI test.

Usage:
    # Start the server first (production):
    cargo run

    python3 scripts/schema_drift.py --token-file /tmp/prod_oauth_token.json
    # or reuse the token Claude Code holds for an MCP server entry (macOS):
    python3 scripts/schema_drift.py --keychain longbridge-local
    # one tool, and save the raw responses for schema work:
    python3 scripts/schema_drift.py --keychain longbridge-local --tool dividend --dump /tmp/drift

Both are drift and set the exit status to 1. Schema-only fields are reported
as warnings: the samples come from one account, so a field can legitimately be
absent (an optional or US-account-only field, or an array that happened to be
empty).
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from urllib.request import Request, urlopen

SYMBOLS = ("NVDA.US", "700.HK")
MAX_DEPTH = 2


class Client:
    def __init__(self, base_url, token):
        self.url = base_url.rstrip("/") + "/mcp"
        self.token = token
        self.next_id = 0

    def rpc(self, method, params):
        self.next_id += 1
        body = json.dumps(
            {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        ).encode()
        req = Request(
            self.url,
            data=body,
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json, text/event-stream",
                "Authorization": f"Bearer {self.token}",
            },
        )
        with urlopen(req, timeout=60) as resp:
            text = resp.read().decode()
        events = [line[5:] for line in text.splitlines() if line.startswith("data:")]
        return json.loads(events[0] if events else text)


def token_from_keychain(server_name):
    raw = subprocess.run(
        ["security", "find-generic-password", "-s", "Claude Code-credentials", "-w"],
        capture_output=True,
        check=True,
    ).stdout
    entries = json.loads(raw)["mcpOAuth"].values()
    for entry in entries:
        if entry.get("serverName") == server_name and entry.get("accessToken"):
            return entry["accessToken"]
    sys.exit(f"no Claude Code OAuth token stored for MCP server `{server_name}`")


def resolve(node, root):
    for _ in range(8):
        if not isinstance(node, dict):
            return node
        ref = node.get("$ref")
        if ref:
            defs = root.get("$defs") or root.get("definitions") or {}
            node = defs.get(ref.rsplit("/", 1)[-1], {})
            continue
        arms = next((node[k] for k in ("anyOf", "oneOf", "allOf") if node.get(k)), None)
        if arms:
            node = next((a for a in arms if a.get("type") != "null"), arms[0])
            continue
        return node
    return node


def schema_paths(node, root, maps, prefix="", depth=0):
    """Field paths the schema declares. Paths of map-typed objects (dynamic keys,
    declared via `additionalProperties`) are collected into `maps` instead."""
    node = resolve(node, root)
    paths = set()
    if depth >= MAX_DEPTH or not isinstance(node, dict):
        return paths
    if "properties" not in node and isinstance(node.get("additionalProperties"), dict):
        maps.add(prefix)
        return paths
    if "properties" in node:
        for key, child in node["properties"].items():
            path = f"{prefix}.{key}" if prefix else key
            paths.add(path)
            paths |= schema_paths(child, root, maps, path, depth + 1)
    elif "items" in node:
        paths |= schema_paths(node["items"], root, maps, prefix + "[]", depth)
    return paths


def value_paths(value, prefix="", depth=0):
    paths = set()
    if depth >= MAX_DEPTH:
        return paths
    if isinstance(value, dict):
        for key, child in value.items():
            path = f"{prefix}.{key}" if prefix else key
            paths.add(path)
            paths |= value_paths(child, path, depth + 1)
    elif isinstance(value, list):
        # Union over elements: optional fields may be absent from the first.
        for item in value:
            paths |= value_paths(item, prefix + "[]", depth)
    return paths


def all_keys(value):
    """Every object key at any depth of a response."""
    keys = set()
    if isinstance(value, dict):
        for key, child in value.items():
            keys.add(key)
            keys |= all_keys(child)
    elif isinstance(value, list):
        for item in value:
            keys |= all_keys(item)
    return keys


# Description fields the probe accounts/symbols cannot show, with the reason.
# Keep this short: an entry here is a field nobody has verified live.
DESCRIPTION_UNOBSERVABLE = {
    "company": {"detail_url", "market_cap", "top_rank_tags", "share_list", "ccy_symbol"},  # US-account variant
    "consensus": {"ai_summary"},  # US-account variant
    "valuation": {"ai_summary"},  # US-account variant
    "dividend": {"dividend_yield_ttm"},  # US-account variant
    "security_facts": {"key_fact_id"},  # names a field of `signals`, not of this tool
    "grid_symbol_info": {"order_id"},  # "takes a symbol, not an order_id"
    # Empty unless the probe account traded today; same shape as history_executions.
    "today_executions": {"trade_done_at", "trade_id"},
    # Empty unless the probe account placed orders today; fields from the SDK Order.
    "today_orders": {
        "attached_orders", "executed_quantity", "order_type", "submitted_at",
        "stock_name", "time_in_force", "updated_at",
    },
    # Empty order arrays for the probe account; fields from the SDK type.
    "ipo_orders": {"order_id", "submitted_at", "total_amount"},
    # Upstream returned {chain: null, top: null} for every probe symbol.
    "industry_peers": {"stock_num", "ytd_chg"},
    # macd_day is an example indicator key; tech_key names the map key in
    # tech_values{tech_key: [...]}, not a literal field.
    "screener_indicators": {"macd_day", "tech_key"},
    # bid/ask arrays were empty for every probe; fields from the BrokerLevel the
    # handler builds.
    "brokers": {"broker_ids"},
    # The probe account has no DCA plans; plan fields from the SDK DcaPlan.
    "dca_list": {
        "plan_id", "stock_name", "per_invest_amount", "invest_frequency",
        "invest_day_of_week", "invest_day_of_month", "next_trd_date", "cum_amount",
        "cum_profit", "average_cost", "issue_number",
    },
    # US-account kind=ALL variant; the probe account is AP.
    "financial_statement": {"income_statement", "balance_sheet", "cash_flow", "empty_fields"},
    "financial_report": {"is_list", "bs_list", "cf_list"},  # US-account overview variant
}

FIELD_LIKE = re.compile(r"\b[a-z][a-z0-9]*(?:_[a-z0-9]+)+\b")


def described_fields(tool, tool_names):
    """snake_case identifiers the description presents as response fields —
    excluding the tool's own parameters and other tools' names."""
    params = set(tool["inputSchema"].get("properties", {}))
    words = set(FIELD_LIKE.findall(tool.get("description") or ""))
    return words - params - tool_names - DESCRIPTION_UNOBSERVABLE.get(tool["name"], set())


# Tools whose data only exists for a particular kind of security.
PROBE_SYMBOLS = {
    "constituent": ("HSI.HK", "QQQ.US"),  # an index and an ETF
    "ah_premium": ("939.HK",),  # A/H dual-listed
    "ah_premium_intraday": ("939.HK",),
    "warrant_quote": ("14055.HK",),  # a Tencent call warrant, expires 2027-04-30
}


def catalogue_tool_names():
    """All tool names from data/scopes.json, when run from the repo root."""
    path = os.path.join(os.path.dirname(__file__), "..", "data", "scopes.json")
    try:
        with open(path) as f:
            return {t for scope in json.load(f)["scopes"] for t in scope["tools"]}
    except (OSError, ValueError, KeyError):
        return set()


def probe_arguments(tool):
    """Arguments for each probe, or None when the tool needs more than a symbol."""
    symbols = PROBE_SYMBOLS.get(tool["name"], SYMBOLS)
    schema = tool["inputSchema"]
    props = schema.get("properties", {})
    required = set(schema.get("required", []))
    if "symbol" in props and not required - {"symbol"}:
        return [({"symbol": s}, s) for s in symbols]
    if "symbols" in props and not required - {"symbols"}:
        return [({"symbols": list(symbols)}, "multi")]
    return None


def real_responses(client, name, probes, dump_dir):
    """Successful, non-placeholder responses for each probe."""
    values = []
    for arguments, label in probes:
        out = client.rpc("tools/call", {"name": name, "arguments": arguments})
        time.sleep(0.4)  # stay under the upstream per-second limit
        result = out.get("result") or {}
        if result.get("isError"):
            continue
        # A trimmed result leads with a "[TRUNCATED ...]" notice block; the
        # data is the block that parses as JSON.
        value = None
        for block in result.get("content") or []:
            try:
                value = json.loads(block.get("text", ""))
                break
            except ValueError:
                continue
        if value is None:
            continue
        if isinstance(value, dict):
            value.pop("_truncated", None)
        if isinstance(value, dict) and "error_code" in value and "note" in value:
            continue  # terminal no-access placeholder, not real data
        values.append(value)
        if dump_dir:
            path = os.path.join(dump_dir, f"{name}.{label}.json")
            with open(path, "w") as f:
                json.dump(value, f, ensure_ascii=False, indent=2)
    return values


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--token-file", help="JSON file with access_token")
    source.add_argument("--keychain", metavar="SERVER", help="Claude Code MCP server name")
    parser.add_argument("--base-url", default="http://127.0.0.1:8000")
    parser.add_argument("--tool", help="Check a single tool")
    parser.add_argument("--dump", metavar="DIR", help="Save raw responses here")
    args = parser.parse_args()

    if args.token_file:
        with open(args.token_file) as f:
            token = json.load(f)["access_token"]
    else:
        token = token_from_keychain(args.keychain)
    if args.dump:
        os.makedirs(args.dump, exist_ok=True)

    client = Client(args.base_url, token)
    tools = client.rpc("tools/list", {})["result"]["tools"]
    drifted = 0
    warned = 0
    checked = 0
    # Every tool name, including ones hidden for this token's scopes, so a
    # description that mentions another tool is not read as a field.
    tool_names = {t["name"] for t in tools} | catalogue_tool_names()
    for tool in sorted(tools, key=lambda t: t["name"]):
        name = tool["name"]
        if args.tool and name != args.tool:
            continue
        if not (tool.get("annotations") or {}).get("readOnlyHint"):
            continue
        probes = probe_arguments(tool)
        if probes is None:
            continue
        values = real_responses(client, name, probes, args.dump)
        if not values:
            print(f"[no-data] {name}")
            continue
        checked += 1
        problems = []

        # The description is what the model reads: a field it names that the
        # response never carries makes a `_jq` filter on it fail.
        keys = set().union(*(all_keys(v) for v in values))
        phantom = sorted(described_fields(tool, tool_names) - keys)
        if phantom:
            problems.append(f"   description names missing fields: {', '.join(phantom)}")

        schema_only = []
        res = client.rpc("resources/read", {"uri": f"lb://tools/{name}/output-schema"})
        if "result" in res:
            schema = json.loads(res["result"]["contents"][0]["text"])
            maps = set()
            want = schema_paths(schema, schema, maps)
            got = {
                path
                for path in set().union(*(value_paths(v) for v in values))
                if not any(path.startswith(m + ".") for m in maps)
            }
            schema_only = sorted(want - got)
            response_only = sorted(got - want)
            if response_only:
                problems.append(f"   schema missing response fields: {', '.join(response_only)}")

        if problems:
            drifted += 1
            print(f"[DRIFT] {name}")
            print("\n".join(problems))
        elif schema_only:
            warned += 1
            print(f"[warn]  {name}")
        if schema_only:
            print("   schema-only (unobserved):", ", ".join(schema_only))
    print(f"checked {checked} tools, {drifted} drifted, {warned} with unobserved schema fields")
    sys.exit(1 if drifted else 0)


if __name__ == "__main__":
    main()
