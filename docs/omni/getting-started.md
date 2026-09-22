# Getting started with the omni endpoint

Three tools cover the whole Longbridge MCP catalogue.

1. `search` — find tools by keywords in English or Chinese, e.g. `{"query": "market temperature"}` or `{"query": "市场温度"}`.
2. `docs` — read a tool's full input/output schema (`{"tool": "quote"}`), a topic guide (`{"topic": "orders"}`), or Longbridge OpenAPI docs (`{"query": "submit order"}`, `{"page": "trade/order/submit"}`).
3. `execute` — run a tool: `{"tool": "quote", "arguments": {"symbols": ["700.HK"]}}`, or a multi-step pipeline (see topic `pipelines`).

Every call accepts a top-level `_jq` filter applied to the response (see topic `jq`).
Typical flow: search → docs (when parameters are non-trivial) → execute.
