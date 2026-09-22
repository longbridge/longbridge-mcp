# Multi-step execute

```json
{
  "steps": [
    {"id": "picks", "tool": "screener_search", "arguments": {"strategy_id": "…", "limit": 20}, "jq": ".data | map(.symbol)"},
    {"id": "quotes", "tool": "quote", "arguments": {"symbols": {"$from": "picks"}}, "jq": "map({symbol, last_done, change_rate})"},
    {"id": "top", "tool": "quote", "arguments": {"symbols": {"$from": "quotes", "jq": "sort_by(-.change_rate) | .[:3] | map(.symbol)"}}}
  ],
  "return": ["top"]
}
```

- `id`: `[a-z0-9_]{1,32}`, unique. `tool`: any tool name. `arguments`: the tool's schema.
- Any argument value may be `{"$from": "<id>", "jq": "<expr>"}`: the referenced step's (already projected) result, optionally projected again.
- `jq` on a step projects its result; later references and the final return see the projected value. Use it to keep intermediates out of your context.
- `return`: ids to return data for (default: steps nobody references). Other steps return `status` and `elapsed_ms` only.
- Limits: 10 steps, 4 concurrent, 15 jq projections (step `jq` plus `jq` on `$from` references), 30 s total. A failed step marks its dependents `skipped`; independent steps still run.
- Write tools (orders, alerts, watchlists): at most one per pipeline, nothing may depend on it, and **none of its arguments may be a `$from` reference** — anywhere, including inside arrays and nested objects. Write literal values the user can confirm in the dry-run preview.
