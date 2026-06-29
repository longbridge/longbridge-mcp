#!/usr/bin/env python3
"""Full MCP tool verification — tests each tool with incremental params, validates
descriptions, counter_id conversion, and date format consistency.

Usage:
    # Start server first (production):
    cargo run

    # Run (tests all tools one by one):
    python3 tests/test_tools_full.py --token-file /tmp/prod_oauth_token.json

    # Single tool:
    python3 tests/test_tools_full.py --token-file /tmp/prod_oauth_token.json --tool candlesticks
"""

import argparse
import json
import re
import sys
import time
from urllib.request import Request, urlopen

# ── Tool test definitions ─────────────────────────────────────────────
# Each tool has:
#   required: dict of required params
#   optional: list of (param_name, value) — tested incrementally one at a time
#   skip_call: True if tool has side effects
#   expected_fields: list of fields that should appear in response (for description validation)

TOOL_TESTS = {
    # ═══ Quote ═══
    "now": {"required": {}},
    "static_info": {
        "required": {"symbols": ["700.HK"]},
        "expected_fields": ["name_cn", "exchange", "lot_size"],
    },
    "quote": {
        "required": {"symbols": ["700.HK", "AAPL.US"]},
        "expected_fields": ["last_done", "open", "high", "low", "volume", "turnover"],
    },
    "option_quote": {
        "required": {"symbols": ["AAPL250620C00200000.US"]},
    },
    "warrant_quote": {
        "required": {"symbols": ["13157.HK"]},
    },
    "depth": {
        "required": {"symbol": "700.HK"},
        "expected_fields": ["asks", "bids"],
    },
    "brokers": {
        "required": {"symbol": "700.HK"},
    },
    "participants": {"required": {}},
    "trades": {
        "required": {"symbol": "700.HK", "count": 10},
        "expected_fields": ["price", "volume"],
    },
    "intraday": {
        "required": {"symbol": "700.HK"},
        "optional": [("trade_sessions", "all")],
    },
    "candlesticks": {
        "required": {"symbol": "700.HK", "period": "day", "count": 5, "forward_adjust": False, "trade_sessions": "intraday"},
        "optional": [("period", "week"), ("count", 10), ("forward_adjust", True), ("trade_sessions", "all")],
        "expected_fields": ["open", "high", "low", "close", "volume", "turnover"],
    },
    "history_candlesticks_by_offset": {
        "required": {"symbol": "700.HK", "period": "day", "forward_adjust": False, "forward": False, "count": 5, "trade_sessions": "intraday"},
        "optional": [("time", "2025-01-15T00:00:00"), ("forward", True), ("count", 3)],
    },
    "history_candlesticks_by_date": {
        "required": {"symbol": "700.HK", "period": "day", "forward_adjust": False, "trade_sessions": "intraday"},
        "optional": [("start", "2025-01-01"), ("end", "2025-01-31")],
    },
    "trading_days": {
        "required": {"market": "HK", "start": "2025-01-01", "end": "2025-01-31"},
    },
    "option_chain_expiry_date_list": {
        "required": {"symbol": "AAPL.US"},
    },
    "option_chain_info_by_date": {
        "required": {"symbol": "AAPL.US", "date": "2025-06-20"},
    },
    "capital_flow": {
        "required": {"symbol": "700.HK"},
    },
    "capital_distribution": {
        "required": {"symbol": "700.HK"},
    },
    "trading_session": {"required": {}},
    "market_temperature": {
        "required": {"market": "HK"},
    },
    "history_market_temperature": {
        "required": {"market": "HK", "start": "2025-01-01", "end": "2025-01-31"},
    },
    "watchlist": {"required": {}},
    "filings": {
        "required": {"symbol": "AAPL.US"},
    },
    "warrant_issuers": {"required": {}},
    "warrant_list": {
        "required": {"symbol": "700.HK", "sort_by": "LastDone", "sort_order": "Descending"},
    },
    "calc_indexes": {
        "required": {"symbols": ["700.HK"], "indexes": ["PeTtmRatio", "PbRatio"]},
        "optional": [("indexes", ["PeTtmRatio", "PbRatio", "TurnoverRate", "TotalMarketValue"])],
    },
    "create_watchlist_group": {"required": {"name": "mcp_full_test"}, "skip_call": True},
    "delete_watchlist_group": {"required": {"id": 0, "purge": False}, "skip_call": True},
    "update_watchlist_group": {"required": {"id": 0}, "skip_call": True},
    "security_list": {
        "required": {"market": "US", "category": "Overnight"},
    },
    # ═══ Trade ═══
    "account_balance": {
        "required": {},
        "optional": [("currency", "USD")],
    },
    "stock_positions": {"required": {}},
    "fund_positions": {"required": {}},
    "margin_ratio": {
        "required": {"symbol": "700.HK"},
        "expected_fields": ["im_factor", "mm_factor", "fm_factor"],
    },
    "today_orders": {
        "required": {},
        "optional": [("symbol", "700.HK")],
    },
    "order_detail": {"required": {"order_id": "FAKE"}, "skip_call": True},
    "cancel_order": {"required": {"order_id": "FAKE"}, "skip_call": True},
    "today_executions": {
        "required": {},
        "optional": [("symbol", "700.HK")],
    },
    "history_orders": {
        "required": {"start_at": "2025-01-01T00:00:00Z", "end_at": "2025-04-01T00:00:00Z"},
        "optional": [("symbol", "700.HK")],
    },
    "history_executions": {
        "required": {"start_at": "2025-01-01T00:00:00Z", "end_at": "2025-04-01T00:00:00Z"},
        "optional": [("symbol", "700.HK")],
    },
    "cash_flow": {
        "required": {"start_at": "2025-01-01T00:00:00Z", "end_at": "2025-04-01T00:00:00Z"},
    },
    "submit_order": {"required": {"symbol": "700.HK", "order_type": "LO", "side": "Buy", "submitted_quantity": "100", "time_in_force": "Day", "submitted_price": "1.00"}, "skip_call": True},
    "replace_order": {"required": {"order_id": "FAKE", "quantity": "100"}, "skip_call": True},
    "estimate_max_purchase_quantity": {
        "required": {"symbol": "700.HK", "side": "Buy", "order_type": "LO"},
        "optional": [("price", "100")],
    },
    # ═══ Fundamental ═══
    "financial_report": {
        "required": {"symbol": "AAPL.US"},
        "optional": [("report_type", "annual")],
    },
    "institution_rating": {"required": {"symbol": "AAPL.US"}},
    "institution_rating_detail": {"required": {"symbol": "AAPL.US"}},
    "dividend": {"required": {"symbol": "AAPL.US"}},
    "dividend_detail": {"required": {"symbol": "AAPL.US"}},
    "forecast_eps": {"required": {"symbol": "AAPL.US"}},
    "consensus": {"required": {"symbol": "AAPL.US"}},
    "valuation": {"required": {"symbol": "AAPL.US"}},
    "valuation_history": {"required": {"symbol": "AAPL.US"}},
    "industry_valuation": {"required": {"symbol": "AAPL.US"}},
    "industry_valuation_dist": {"required": {"symbol": "AAPL.US"}},
    "company": {
        "required": {"symbol": "AAPL.US"},
        "expected_fields": ["name"],
    },
    "executive": {"required": {"symbol": "AAPL.US"}},
    "shareholder": {"required": {"symbol": "AAPL.US"}},
    "fund_holder": {"required": {"symbol": "AAPL.US"}},
    "corp_action": {"required": {"symbol": "700.HK"}},
    "invest_relation": {"required": {"symbol": "700.HK"}},
    "operating": {"required": {"symbol": "AAPL.US"}},
    # ═══ Market ═══
    "market_status": {"required": {}},
    "broker_holding": {"required": {"symbol": "700.HK"}},
    "broker_holding_detail": {"required": {"symbol": "700.HK"}},
    "broker_holding_daily": {"required": {"symbol": "700.HK", "broker_id": "B01224"}},
    "ah_premium": {"required": {"symbol": "939.HK"}},
    "ah_premium_intraday": {"required": {"symbol": "939.HK"}},
    "trade_stats": {"required": {"symbol": "700.HK"}},
    "anomaly": {"required": {"market": "HK"}},
    "constituent": {"required": {"symbol": "HSI.HK"}},
    # ═══ Calendar ═══
    "finance_calendar": {
        "required": {"start": "2025-04-01", "end": "2025-04-30", "category": "dividend", "market": "HK"},
    },
    # ═══ Portfolio ═══
    "exchange_rate": {"required": {}},
    "profit_analysis": {"required": {}},
    "profit_analysis_detail": {"required": {"symbol": "700.HK"}},
    # ═══ Alert ═══
    "alert_list": {"required": {}},
    "alert_add": {"required": {"symbol": "700.HK", "condition": "price_rise", "price": "9999"}, "skip_call": True},
    "alert_delete": {"required": {"alert_id": "0"}, "skip_call": True},
    "alert_enable": {"required": {"alert_id": "0"}, "skip_call": True},
    "alert_disable": {"required": {"alert_id": "0"}, "skip_call": True},
    # ═══ Content ═══
    "news": {"required": {"symbol": "AAPL.US"}},
    "topic": {"required": {"symbol": "AAPL.US"}},
    "topic_detail": {"required": {"topic_id": "0"}, "skip_call": True},
    "topic_replies": {"required": {"topic_id": "0"}, "skip_call": True},
    "topic_create": {"required": {"title": "test", "body": "test"}, "skip_call": True},
    "topic_create_reply": {"required": {"topic_id": "0", "body": "test"}, "skip_call": True},
    # ═══ Statement ═══
    "statement_list": {
        "required": {},
        "optional": [("statement_type", "daily"), ("limit", 5)],
    },
    "statement_export": {"required": {"file_key": "FAKE"}, "skip_call": True},
    # ═══ DCA ═══
    "dca_list": {
        "required": {},
        "optional": [("status", "Active"), ("symbol", "AAPL.US"), ("page", 1), ("limit", 5)],
    },
    "dca_create": {"required": {"symbol": "AAPL.US", "amount": "100", "frequency": "Monthly"}, "skip_call": True},
    "dca_update": {"required": {"plan_id": "FAKE"}, "skip_call": True},
    "dca_pause": {"required": {"plan_id": "FAKE"}, "skip_call": True},
    "dca_resume": {"required": {"plan_id": "FAKE"}, "skip_call": True},
    "dca_stop": {"required": {"plan_id": "FAKE"}, "skip_call": True},
    "dca_history": {"required": {"plan_id": "FAKE"}, "skip_call": True},
    "dca_stats": {
        "required": {},
        "optional": [("symbol", "AAPL.US")],
    },
    "dca_check": {
        "required": {"symbols": ["AAPL.US", "700.HK"]},
    },
    # ═══ Option Volume ═══
    "option_volume": {
        "required": {"symbol": "AAPL.US"},
    },
    "option_volume_daily": {
        "required": {"symbol": "AAPL.US"},
        "optional": [("count", 10)],
    },
    # ═══ Short Positions ═══
    "short_positions": {
        "required": {"symbol": "AAPL.US"},
        "optional": [("count", 10)],
    },
    # ═══ Sharelist ═══
    "sharelist_list": {
        "required": {},
        "optional": [("count", 5)],
    },
    "sharelist_popular": {
        "required": {},
        "optional": [("count", 5)],
    },
    "sharelist_detail": {"required": {"id": "0"}, "skip_call": True},
    "sharelist_create": {"required": {"name": "test"}, "skip_call": True},
    "sharelist_delete": {"required": {"id": "0"}, "skip_call": True},
    "sharelist_add": {"required": {"id": "0", "symbols": ["AAPL.US"]}, "skip_call": True},
    "sharelist_remove": {"required": {"id": "0", "symbols": ["AAPL.US"]}, "skip_call": True},
    "sharelist_sort": {"required": {"id": "0", "symbols": ["AAPL.US"]}, "skip_call": True},
    # ═══ Quant ═══
    "quant_run": {"required": {"symbol": "AAPL.US"}, "skip_call": True},
    # ═══ Search ═══
    "news_search": {
        "required": {"keyword": "NVIDIA"},
        "expected_fields": ["title"],
    },
    "topic_search": {
        "required": {"keyword": "AAPL"},
        "expected_fields": ["id", "title"],
    },
    # ═══ Fundamental (new) ═══
    "financial_statement": {
        "required": {"symbol": "AAPL.US", "kind": "IS", "report_type": "af"},
        "expected_fields": ["currency", "list"],
    },
    "financial_report_latest": {
        "required": {"symbol": "700.HK"},
        "expected_fields": ["report"],
    },
    "valuation_rank": {
        "required": {"symbol": "AAPL.US", "start_date": "20250101", "end_date": "20250501"},
        "expected_fields": ["pe"],
    },
    "analyst_estimates": {
        "required": {"symbol": "AAPL.US"},
        "expected_fields": ["estimate"],
    },
    "institution_rating_history": {
        "required": {"symbol": "AAPL.US"},
        "expected_fields": ["target_history", "evaluate_history"],
    },
    "institution_rating_industry_rank": {
        "required": {"symbol": "AAPL.US"},
        "expected_fields": ["industry_id", "industry_name"],
    },
    # ═══ Asset (new) ═══
    "short_margin": {"required": {}, "expected_fields": ["short_list"]},
    # ═══ ATM ═══
    "bank_cards": {"required": {}},
    "withdrawals": {"required": {}},
    "deposits": {"required": {}},
    # ═══ IPO ═══
    "ipo_subscriptions": {"required": {}},
    "ipo_calendar": {"required": {}},
    "ipo_listed": {"required": {"page": 1, "size": 5}},
    "ipo_detail": {"required": {"symbol": "6871.HK"}, "skip_call": True},
    "ipo_orders": {"required": {}},
    "ipo_order_detail": {"required": {"order_id": "0"}, "skip_call": True},
    "ipo_profit_loss": {"required": {"period": "1y"}},
}

# ── Counter ID patterns to detect ────────────────────────────────────
COUNTER_ID_PATTERN = re.compile(r'"counter_id[s]?"\s*:\s*"?\d+"?')
# Date formats: expect RFC3339 or yyyy-mm-dd, flag unix timestamps
UNIX_TS_PATTERN = re.compile(r'"(?:timestamp|time|date|created_at|updated_at|published_at|ex_date|pay_date|start|end)[^"]*"\s*:\s*"?\d{10,13}"?')
RFC3339_PATTERN = re.compile(r'\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2})?')


class McpClient:
    def __init__(self, base_url, token):
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.session_id = None
        self.req_id = 0

    def _post(self, body):
        self.req_id += 1
        body["id"] = self.req_id
        data = json.dumps(body).encode()
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
            "Authorization": f"Bearer {self.token}",
        }
        if self.session_id:
            headers["Mcp-Session-Id"] = self.session_id
        req = Request(f"{self.base_url}/mcp", data=data, headers=headers, method="POST")
        resp = urlopen(req, timeout=60)
        self.session_id = self.session_id or resp.headers.get("mcp-session-id")
        raw = resp.read().decode()
        for line in raw.split("\n"):
            if line.startswith("data: {"):
                return json.loads(line[6:])
        raise RuntimeError(f"no data in SSE: {raw[:200]}")

    def initialize(self):
        return self._post({"jsonrpc": "2.0", "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "full-test", "version": "0.1"}}})

    def list_tools(self):
        r = self._post({"jsonrpc": "2.0", "method": "tools/list", "params": {}})
        return r["result"]["tools"]

    def call_tool(self, name, arguments):
        return self._post({"jsonrpc": "2.0", "method": "tools/call", "params": {"name": name, "arguments": arguments}})


def check_counter_id(text, tool_name):
    """Check if counter_id appears in response (should be converted to symbol)."""
    issues = []
    matches = COUNTER_ID_PATTERN.findall(text)
    if matches:
        issues.append(f"counter_id 未转换: {matches[:3]}")
    return issues


def check_date_format(text, tool_name):
    """Check for inconsistent date formats (unix timestamps should be RFC3339)."""
    issues = []
    matches = UNIX_TS_PATTERN.findall(text)
    if matches:
        issues.append(f"发现 unix 时间戳: {matches[:3]}")
    return issues


def check_expected_fields(text, expected_fields):
    """Check if expected fields appear in the response."""
    missing = []
    for field in expected_fields:
        if f'"{field}"' not in text:
            missing.append(field)
    return missing


def truncate(s, n=200):
    return s[:n] + "..." if len(s) > n else s


def run_tests(args):
    client = McpClient(args.base_url, args.token)

    print("=" * 70)
    print("MCP Tool Full Verification")
    print(f"Server: {args.base_url}")
    print("=" * 70)

    # Initialize
    init = client.initialize()
    info = init.get("result", {}).get("serverInfo", {})
    print(f"Server: {info.get('name')} v{info.get('version')}")
    print(f"Session: {client.session_id}\n")

    # Get schemas
    tools = client.list_tools()
    tool_map = {t["name"]: t for t in tools}
    print(f"Total tools: {len(tools)}\n")

    # Filter
    if args.tool:
        test_names = [args.tool]
    else:
        test_names = list(TOOL_TESTS.keys())

    stats = {"pass": 0, "fail": 0, "skip": 0, "warn": 0}
    all_issues = []

    for name in test_names:
        test = TOOL_TESTS.get(name)
        if not test:
            continue

        schema = tool_map.get(name)
        if not schema:
            print(f"{'─'*70}")
            print(f"✗ {name}: 在服务器中不存在!")
            stats["fail"] += 1
            all_issues.append((name, "tool not found in server"))
            continue

        print(f"{'─'*70}")
        print(f"Tool: {name}")
        print(f"  描述: {schema.get('description', '(none)')}")

        input_schema = schema.get("inputSchema", {})
        required = set(input_schema.get("required", []))
        properties = input_schema.get("properties", {})
        print(f"  参数: {list(properties.keys()) or '(none)'}")
        print(f"  必填: {list(required) or '(none)'}")

        if test.get("skip_call"):
            print(f"  [SKIP] 有副作用，跳过调用")
            stats["skip"] += 1
            continue

        tool_issues = []

        # ── Test 1: Call with required params only ──
        req_args = dict(test["required"])
        print(f"\n  [调用 1] 仅必填参数")
        print(f"    入参: {json.dumps(req_args, ensure_ascii=False)}")

        try:
            r = client.call_tool(name, req_args)
            if "error" in r:
                err = r["error"]
                print(f"    出参: ERROR [{err['code']}] {err['message'][:150]}")
                tool_issues.append(f"必填参数调用失败: {err['message'][:80]}")
            else:
                content = r.get("result", {}).get("content", [])
                text = content[0].get("text", "") if content else ""
                print(f"    出参: {truncate(text, 300)}")

                # Check counter_id
                cid_issues = check_counter_id(text, name)
                tool_issues.extend(cid_issues)

                # Check date format
                date_issues = check_date_format(text, name)
                tool_issues.extend(date_issues)

                # Check expected fields
                expected = test.get("expected_fields", [])
                missing = check_expected_fields(text, expected)
                if missing:
                    tool_issues.append(f"描述中提到但响应中缺少的字段: {missing}")
        except Exception as e:
            print(f"    出参: EXCEPTION {e}")
            tool_issues.append(f"调用异常: {e}")

        # ── Test 2+: Add optional params one at a time ──
        optional_params = test.get("optional", [])
        for i, (opt_name, opt_value) in enumerate(optional_params):
            call_args = dict(req_args)
            call_args[opt_name] = opt_value
            print(f"\n  [调用 {i+2}] +{opt_name}={json.dumps(opt_value, ensure_ascii=False)}")
            print(f"    入参: {json.dumps(call_args, ensure_ascii=False)}")

            try:
                r = client.call_tool(name, call_args)
                if "error" in r:
                    err = r["error"]
                    print(f"    出参: ERROR [{err['code']}] {err['message'][:150]}")
                    tool_issues.append(f"可选参数 {opt_name} 调用失败: {err['message'][:80]}")
                else:
                    content = r.get("result", {}).get("content", [])
                    text = content[0].get("text", "") if content else ""
                    print(f"    出参: {truncate(text, 300)}")

                    cid_issues = check_counter_id(text, name)
                    tool_issues.extend(cid_issues)

                    date_issues = check_date_format(text, name)
                    tool_issues.extend(date_issues)
            except Exception as e:
                print(f"    出参: EXCEPTION {e}")
                tool_issues.append(f"可选参数 {opt_name} 异常: {e}")

        # ── Summary for this tool ──
        if tool_issues:
            print(f"\n  ⚠ 问题 ({len(tool_issues)}):")
            for issue in tool_issues:
                print(f"    - {issue}")
            stats["warn"] += 1
            all_issues.append((name, tool_issues))
        else:
            print(f"\n  ✓ PASS")
            stats["pass"] += 1
        print()

    # ── Final summary ──
    print("=" * 70)
    print("最终结果")
    print("=" * 70)
    total = sum(stats.values())
    print(f"  合计: {total}")
    print(f"  PASS: {stats['pass']}")
    print(f"  WARN: {stats['warn']} (有问题但能调用)")
    print(f"  SKIP: {stats['skip']} (有副作用)")
    print(f"  FAIL: {stats['fail']} (tool 不存在或无法调用)")

    # Check for tools in server but not in test cases
    untested = set(tool_map.keys()) - set(TOOL_TESTS.keys())
    if untested:
        print(f"\n  未覆盖的 tool ({len(untested)}): {sorted(untested)}")

    if all_issues:
        print(f"\n{'─'*70}")
        print("问题汇总:")
        for name, issues in all_issues:
            if isinstance(issues, list):
                for issue in issues:
                    print(f"  {name:40s} {issue}")
            else:
                print(f"  {name:40s} {issues}")

    return 0 if stats["fail"] == 0 else 1


def main():
    parser = argparse.ArgumentParser(description="Full MCP tool verification")
    parser.add_argument("--token-file", required=True, help="JSON file with access_token")
    parser.add_argument("--base-url", default="http://127.0.0.1:8000")
    parser.add_argument("--tool", help="Test single tool")
    args = parser.parse_args()

    with open(args.token_file) as f:
        ti = json.load(f)
    args.token = ti["access_token"]

    sys.exit(run_tests(args))


if __name__ == "__main__":
    main()
