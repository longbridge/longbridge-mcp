# Macrodata Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two MCP tools — `macrodata_list` and `macrodata` — that expose the Longbridge macro-economic indicator API (`/v1/quote/macrodata`).

**Architecture:** A new `src/tools/macrodata.rs` file implements both tool functions following the same HTTP-get pattern as `fundamental.rs`. Tool registration goes in `mod.rs`; locale strings go in both `zh-CN` and `zh-HK` JSON files.

**Tech Stack:** Rust, `rmcp`, `http_get_tool` / `http_get_tool_unix`, `convert_unix_paths`, `parse_rfc3339` from `support/parse.rs`.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `src/tools/macrodata.rs` | Param structs + `macrodata_list` + `macrodata` functions |
| Modify | `src/tools/mod.rs` | Add `mod macrodata;` and register two tools |
| Modify | `locales/zh-CN/tools.json` | zh-CN titles/descriptions |
| Modify | `locales/zh-HK/tools.json` | zh-HK titles/descriptions |

---

## Task 1: Create `macrodata.rs` with failing test

**Files:**
- Create: `src/tools/macrodata.rs`

- [ ] **Step 1: Write the failing test**

Add this file:

```rust
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::convert_unix_paths;
use crate::tools::support::http_client::http_get_tool_unix;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataListParam {
    /// Pagination offset, default 0.
    pub offset: Option<i32>,
    /// Maximum number of indicators to return (default 100, max 1000).
    /// There are ~619 indicators total; pass 1000 to fetch all at once.
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataParam {
    /// Indicator code from `macrodata_list`, e.g. `"USCPI"`.
    pub indicator_code: String,
    /// Earliest release time to include (RFC3339, e.g. `"2024-01-01T00:00:00Z"`).
    pub start_time: Option<String>,
    /// Latest release time to include (RFC3339).
    pub end_time: Option<String>,
    /// Maximum number of data points to return (default 100, max 100).
    pub limit: Option<i32>,
}

/// Convert an RFC3339 string to a unix-seconds string for use as a query param.
fn rfc3339_to_unix(s: &str) -> Result<String, McpError> {
    let dt = crate::tools::support::parse::parse_rfc3339(s)?;
    Ok(dt.unix_timestamp().to_string())
}

pub async fn macrodata_list(
    mctx: &crate::tools::McpContext,
    p: MacrodataListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let offset = p.offset.unwrap_or(0).to_string();
    let limit = p.limit.unwrap_or(100).to_string();
    let params = [("offset", offset.as_str()), ("limit", limit.as_str())];
    http_get_tool_unix(&client, "/v1/quote/macrodata", &params, &["items[*].start_date"]).await
}

pub async fn macrodata(
    mctx: &crate::tools::McpContext,
    p: MacrodataParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/quote/macrodata/{}", p.indicator_code);

    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(ref s) = p.start_time {
        params.push(("start_time", rfc3339_to_unix(s)?));
    }
    if let Some(ref e) = p.end_time {
        params.push(("end_time", rfc3339_to_unix(e)?));
    }
    if let Some(l) = p.limit {
        params.push(("limit", l.to_string()));
    }
    let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let unix_paths = &[
        "info.start_date",
        "data[*].release_at",
        "data[*].next_release_at",
    ];
    http_get_tool_unix(&client, &path, &params_ref, unix_paths).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_to_unix_known_value() {
        let result = rfc3339_to_unix("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(result, "1704067200");
    }

    #[test]
    fn rfc3339_to_unix_rejects_invalid() {
        assert!(rfc3339_to_unix("not-a-date").is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -q -- macrodata 2>&1
```

Expected: compile error — `mod macrodata` not declared yet.

- [ ] **Step 3: Add `mod macrodata;` to `src/tools/mod.rs`**

In `src/tools/mod.rs`, insert after `mod market;`:

```rust
mod macrodata;
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -q -- macrodata 2>&1
```

Expected:
```
running 2 tests
..
test result: ok. 2 passed; 0 failed
```

- [ ] **Step 5: Commit**

```bash
cargo +nightly fmt
git add src/tools/macrodata.rs src/tools/mod.rs
git commit -m "feat(macrodata): add macrodata_list and macrodata tool functions with tests"
```

---

## Task 2: Register tools in `mod.rs`

**Files:**
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Add tool registrations**

In `src/tools/mod.rs`, add the two tool methods inside the `impl Longbridge` block (near other market/fundamental tools, e.g. after `market_temperature`):

```rust
    /// List all supported macro-economic indicators.
    #[tool(
        title = "Macro Indicator List",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List all supported macro-economic indicators (~619 total). Returns items[]{indicator_code, source_org, country, name, periodicity, category, importance, start_date}. Pass limit=1000 to fetch all at once."
    )]
    async fn macrodata_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<macrodata::MacrodataListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("macrodata_list", || macrodata::macrodata_list(&mctx, p)).await
    }

    /// Get historical data for a macro-economic indicator.
    #[tool(
        title = "Macro Indicator Data",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical data for a macro-economic indicator by its code (from macrodata_list). Returns info{name, periodicity, country, unit} and data[]{period, release_at, actual_value, previous_value, forecast_value, revised_value, next_release_at}. limit max 100."
    )]
    async fn macrodata(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<macrodata::MacrodataParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("macrodata", || macrodata::macrodata(&mctx, p)).await
    }
```

- [ ] **Step 2: Build to confirm no compile errors**

```bash
cargo build -q 2>&1
```

Expected: no errors.

- [ ] **Step 3: Run full test suite**

```bash
cargo test -q 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 4: Run clippy**

```bash
cargo clippy --all-features --all-targets -q 2>&1
```

Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
cargo +nightly fmt
git add src/tools/mod.rs
git commit -m "feat(macrodata): register macrodata_list and macrodata MCP tools"
```

---

## Task 3: Add locale strings

**Files:**
- Modify: `locales/zh-CN/tools.json`
- Modify: `locales/zh-HK/tools.json`

- [ ] **Step 1: Add zh-CN entries**

In `locales/zh-CN/tools.json`, inside the `"tools"` object (alphabetical order, near `"market_temperature"`):

```json
    "macrodata_list": {
      "title": "宏观指标列表",
      "description": "列出所有支持的宏观经济指标（约 619 条）。返回 items[]{indicator_code, source_org, country, name, periodicity, category, importance, start_date}。传 limit=1000 可一次获取全部。",
      "properties": {
        "offset": "分页偏移量，默认 0",
        "limit": "返回条数上限，默认 100，最大 1000"
      }
    },
    "macrodata": {
      "title": "宏观指标历史数据",
      "description": "按指标代码（来自 macrodata_list）查询历史数据。返回 info{name, periodicity, country, unit} 和 data[]{period, release_at, actual_value, previous_value, forecast_value, revised_value, next_release_at}。limit 最大 100。",
      "properties": {
        "indicator_code": "指标代码，例如 `\"USCPI\"`，来自 macrodata_list",
        "start_time": "数据起始发布时间（RFC3339，例如 `\"2024-01-01T00:00:00Z\"`）",
        "end_time": "数据结束发布时间（RFC3339）",
        "limit": "返回条数上限，默认 100，最大 100"
      }
    },
```

- [ ] **Step 2: Add zh-HK entries**

In `locales/zh-HK/tools.json`, inside the `"tools"` object:

```json
    "macrodata_list": {
      "title": "宏觀指標列表",
      "description": "列出所有支援的宏觀經濟指標（約 619 條）。返回 items[]{indicator_code, source_org, country, name, periodicity, category, importance, start_date}。傳 limit=1000 可一次獲取全部。",
      "properties": {
        "offset": "分頁偏移量，預設 0",
        "limit": "返回條數上限，預設 100，最大 1000"
      }
    },
    "macrodata": {
      "title": "宏觀指標歷史數據",
      "description": "按指標代碼（來自 macrodata_list）查詢歷史數據。返回 info{name, periodicity, country, unit} 和 data[]{period, release_at, actual_value, previous_value, forecast_value, revised_value, next_release_at}。limit 最大 100。",
      "properties": {
        "indicator_code": "指標代碼，例如 `\"USCPI\"`，來自 macrodata_list",
        "start_time": "數據起始發布時間（RFC3339，例如 `\"2024-01-01T00:00:00Z\"`）",
        "end_time": "數據結束發布時間（RFC3339）",
        "limit": "返回條數上限，預設 100，最大 100"
      }
    },
```

- [ ] **Step 3: Build to confirm locale files load correctly**

```bash
cargo build -q 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add locales/zh-CN/tools.json locales/zh-HK/tools.json
git commit -m "feat(macrodata): add zh-CN and zh-HK locale strings for macrodata tools"
```
