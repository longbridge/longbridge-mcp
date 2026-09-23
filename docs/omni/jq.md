# jq filters

Every tool accepts a top-level `_jq` (jaq syntax) applied to the response, e.g. `.data | map({symbol})`. One output is returned as-is, several as an array, none as `[]`. Module imports and `env`/`debug`/`stderr` are unavailable; filters time out after 5 s and are capped at 10,000 outputs / 8 MiB. Inside `execute` pipelines use per-step `jq` and `$from` `jq` instead of `_jq` inside `arguments`. The ~6000-token output budget is applied after `_jq`, so a top-level `_jq` can narrow a response that would otherwise be truncated.
