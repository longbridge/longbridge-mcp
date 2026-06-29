#!/usr/bin/env python3
"""Benchmark tools/list throughput to verify CPU regression is fixed.

Usage:
    python3 tests/bench_cpu.py --token <bearer> [--url URL] [--n 200] [--concurrency 10]

The script fires N tools/list requests (sequentially or in parallel) against
the target server and reports:
  - total time
  - requests/sec
  - p50 / p95 / p99 latency per request

Before the fix (PR #80): ~380 µs CPU per request → high CPU under load.
After the fix: tool router is cached; per-request overhead should be ~0.
"""

import argparse
import json
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from urllib.error import HTTPError
from urllib.request import Request, urlopen

DEFAULT_URL = "https://mcp.longbridge.com/mcp"

INIT_PAYLOAD = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "bench_cpu", "version": "0.1.0"},
    },
}
LIST_PAYLOAD = {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}


def parse_sse_or_json(data: bytes) -> dict:
    """Parse either plain JSON or SSE (data: {...}) response."""
    text = data.decode()
    if text.startswith("{"):
        return json.loads(text)
    # SSE: find first data: line
    for line in text.splitlines():
        if line.startswith("data:"):
            return json.loads(line[5:].strip())
    raise ValueError(f"unparseable response: {text[:200]}")


def http_post(url: str, payload: dict, token: str, session_id: str | None) -> dict:
    body = json.dumps(payload).encode()
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
        "Authorization": f"Bearer {token}",
    }
    if session_id:
        headers["Mcp-Session-Id"] = session_id
    req = Request(url, data=body, headers=headers, method="POST")
    with urlopen(req, timeout=15) as resp:
        return parse_sse_or_json(resp.read())


def one_tools_list(url: str, token: str) -> float:
    """Initialize + tools/list, return elapsed seconds."""
    t0 = time.perf_counter()
    init = http_post(url, INIT_PAYLOAD, token, None)
    sid = init.get("result", {}).get("sessionId")
    http_post(url, LIST_PAYLOAD, token, sid)
    return time.perf_counter() - t0


def run(url: str, token: str, n: int, concurrency: int) -> None:
    print(f"Target : {url}")
    print(f"Requests: {n}  Concurrency: {concurrency}")
    print("Warming up (1 request) …", flush=True)
    one_tools_list(url, token)  # warm-up

    print(f"Running {n} requests …", flush=True)
    latencies: list[float] = []
    errors = 0
    wall_start = time.perf_counter()

    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        futs = [pool.submit(one_tools_list, url, token) for _ in range(n)]
        for i, fut in enumerate(as_completed(futs), 1):
            try:
                latencies.append(fut.result())
            except Exception as e:
                errors += 1
                print(f"  error: {e}", file=sys.stderr)
            if i % 20 == 0:
                print(f"  {i}/{n} done …", flush=True)

    wall = time.perf_counter() - wall_start
    latencies.sort()

    def pct(p: float) -> str:
        idx = min(int(len(latencies) * p / 100), len(latencies) - 1)
        return f"{latencies[idx]*1000:.1f} ms"

    print("\n── Results ──────────────────────────────")
    print(f"  Total wall time : {wall:.2f} s")
    print(f"  Throughput      : {n / wall:.1f} req/s")
    print(f"  Errors          : {errors}")
    print(f"  Latency p50     : {pct(50)}")
    print(f"  Latency p95     : {pct(95)}")
    print(f"  Latency p99     : {pct(99)}")
    print(f"  Latency mean    : {statistics.mean(latencies)*1000:.1f} ms")
    print(f"  Latency max     : {max(latencies)*1000:.1f} ms")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--token", required=True, help="Bearer token")
    ap.add_argument("--url", default=DEFAULT_URL, help=f"MCP endpoint (default: {DEFAULT_URL})")
    ap.add_argument("--n", type=int, default=100, help="Number of requests (default: 100)")
    ap.add_argument("--concurrency", type=int, default=5, help="Concurrent workers (default: 5)")
    args = ap.parse_args()
    run(args.url, args.token, args.n, args.concurrency)
