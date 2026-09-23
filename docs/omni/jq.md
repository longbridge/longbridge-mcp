# jq filters

Every tool accepts a top-level `_jq` (jaq syntax) applied to the response, e.g. `.data | map({symbol})`. One output is returned as-is, several as an array, none as `[]`. Module imports and `env`/`debug`/`stderr` are unavailable; filters time out after 5 s and are capped at 10,000 outputs / 8 MiB. Inside `execute` pipelines use per-step `jq` and `$from` `jq` instead of `_jq` inside `arguments`. The ~6000-token output budget is applied after `_jq`, so a top-level `_jq` can narrow a response that would otherwise be truncated.

## Compact output builtins

Two extra filters, on top of the jaq standard library, shrink list-shaped responses:

- `table`: an array of objects becomes `{"cols": [...], "rows": [[...], ...]}`. Keys are written once instead of per row, which roughly halves a candlestick or minute-series payload. Columns are the union of keys in first-seen order; a missing key is `null`, so every row lines up with `cols`. Each cell goes through `num`.
- `num`: a decimal string becomes a number (`"332.81"` → `332.81`). Strings with leading zeros (`"00700"`), more than 15 integer digits (order ids), exponents or other text pass through unchanged.

```
map({t: .timestamp[5:10], close, volume}) | table
```

Select and rename fields first, then call `table`. For evenly spaced series, drop the time column and keep a start and interval instead:

```
{start: .[0].timestamp, interval: "1m", inflow: map(.inflow | num)}
```

Both names can be redefined with your own `def`.
