# MCP 工具参数审计 — 以 longbridge-terminal CLI 为参考

- **审计日期**: 2026-04-27
- **被审计**: `/Users/hogan/work/longbridge/longbridge-mcp` (`src/tools/`)
- **参考实现**: `/Users/hogan/work/longbridge/longbridge-terminal` (`src/cli/`)
- **触发点**: `topic_create` 通过 MCP 时少传了参数,且 MCP tool schema 里少了字段
- **方法**: 逐工具对照 MCP 的 `Param` 结构体 vs CLI 的 clap 子命令 flag,以及二者调用 SDK 时是否硬编码 `None`

## 分类标记

- **A** — MCP 缺字段:CLI 暴露了但 MCP `Param` 结构体没收(最严重)
- **B** — MCP 字段描述不全:doc comment 缺默认值/可选值/单位等关键信息
- **C** — MCP 多于 CLI:可能 CLI 落后,信息不对称
- **D** — SDK 双失:SDK options 里有的字段,CLI 和 MCP 两边都硬编码 `None`(此类不是 CLI/MCP 差距,但值得统一记录)

## 优先级

- **P0** — 功能缺失,影响日常使用
- **P1** — 缺可选参数,降低可用性
- **P2** — 仅文档/描述问题

---

## 一、需要修复的工具

### 🔴 `topic_create`(P0,A 类)

**MCP 现状** (`src/tools/content.rs:24-32`):
```rust
pub struct TopicCreateParam {
    pub title: String,
    pub body: String,
    pub symbols: Option<Vec<String>>,
}
```

业务实现里(`content.rs:92-94`)硬编码 `topic_type: None, hashtags: None`。

**CLI 现状** (`longbridge-terminal/src/cli/mod.rs:1918-1931`):
```
topic create
  --title <Optional>
  --body  <Required>
  --type  <Optional>   (post | article)  ← MCP 缺
  --tickers <Vec, comma-separated>
```

**差距**:
- MCP **缺 `topic_type`** 字段(post / article)。post 是默认纯文本,article 支持 Markdown,这是产品级核心区分。
- `hashtags` CLI 自己也没暴露(`topic.rs:294` 也是 `hashtags: None`),归为 D 类(SDK 双失,不阻塞当前修复)。

**建议修复**:
```rust
pub struct TopicCreateParam {
    /// Topic title. Required when topic_type is "article", optional for "post".
    pub title: String,
    /// Topic body. "post" type is plain text only;
    /// "article" type accepts Markdown.
    pub body: String,
    /// Related security symbols (e.g. ["700.HK", "TSLA.US"], max 10).
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub symbols: Option<Vec<String>>,
    /// Topic type: "post" (default, plain text) | "article" (Markdown, title required).
    pub topic_type: Option<String>,
}
```

并在 `topic_create()` 把 `topic_type: p.topic_type` 传进 `CreateTopicOptions`。

---

### 🔴 `topic_create_reply`(P1,A 类)

**MCP 现状** (`src/tools/content.rs:34-40`):
```rust
pub struct TopicCreateReplyParam {
    pub topic_id: String,
    pub body: String,
}
```

业务实现里(`content.rs:107`)硬编码 `reply_to_id: None`。

**CLI 现状** (`longbridge-terminal/src/cli/mod.rs:1956-1965`):
```
topic create-reply <topic_id>
  --body     <Required>
  --reply-to <Optional>   ← MCP 缺
```

CLI help 文本:`Nest under this reply ID (get IDs from topic-replies). Omit for a top-level reply.`

**差距**: MCP 不支持嵌套回复(reply-to-reply),用户只能发顶层回复。

**建议修复**:
```rust
pub struct TopicCreateReplyParam {
    /// Topic ID to reply to.
    pub topic_id: String,
    /// Reply body (plain text only).
    pub body: String,
    /// Optional parent reply ID for nested replies.
    /// Get from `topic_replies`. Omit for a top-level reply.
    pub reply_to_id: Option<String>,
}
```

`topic_create_reply()` 把 `reply_to_id: p.reply_to_id` 传进 `CreateReplyOptions`。

---

### 🔴 `alert_add`(P1,A 类)

**MCP 现状** (`src/tools/alert.rs:11-20`):
```rust
pub struct AlertAddParam {
    pub symbol: String,
    pub condition: String,    // "price_rise" | "price_fall" | "percent_rise" | "percent_fall"
    pub price: String,
    pub frequency: Option<String>,
}
```

实现把 `condition` 字符串自己解析成 `indicator_id`(`alert.rs:39-44`)。**没有发送 `note`**(`alert.rs:55-63` 的 body 里没这个字段)。

**CLI 现状** (`longbridge-terminal/src/cli/mod.rs:1287-1305`):
```
alert add <symbol>
  --price      <Required>
  --direction  <rise|fall>     default "rise"      ← MCP 把它和 alert_type 拼成 condition
  --alert-type <price|percent> default "price"     ← 同上
  --frequency  <once|daily|every> default "once"
  --note       <Optional>      ← MCP 缺
```

**差距**:
1. MCP **缺 `note`** 字段,且业务实现也没把 note 发给上游。
2. MCP 把 CLI 的两个独立字段(`direction` + `alert_type`)折叠成一个 `condition` 枚举。这没坏,但对客户端模型意图不清晰,容易拼错。**建议保留 `condition` 不动**(改了对客户端是 breaking change),只补 `note`。

**建议修复**(最小改动):
```rust
pub struct AlertAddParam {
    pub symbol: String,
    /// Alert condition: "price_rise" | "price_fall" | "percent_rise" | "percent_fall".
    pub condition: String,
    /// Threshold price (for price_*) or percentage value (for percent_*).
    pub price: String,
    /// Alert frequency: "once" (trigger once then disable), "daily" (once per day),
    /// "every" (every time condition is met). Default: "once".
    pub frequency: Option<String>,
    /// Optional user-defined note shown in the alert list.
    pub note: Option<String>,
}
```

并在 `alert_add()` 的 body JSON 里把 note 加上(API 字段名待确认,需查 longbridge-terminal 上游或抓包)。

> ⚠️ 实现 note 时需要先确认上游 `/v1/notify/reminders` POST 的 note 字段名 — CLI 只把 note 收下来了,但用的是 SDK/API,具体字段名得 grep `note` 在 `longbridge-terminal/src/openapi/` 里追溯。

---

## 二、第二轮深审新发现(A 类,P1)

第二轮对原本被标「一致」的工具做逐字段比对,又发现 5 个 CLI 暴露但 MCP 漏掉的字段。

### 🔴 `account_balance`(P1,A 类)

**MCP 现状** (`src/tools/trade.rs:96-100`):
```rust
pub async fn account_balance(mctx: &crate::tools::McpContext) -> Result<...> {
    let result = ctx.account_balance(None).await...  // ← 硬编码 None
}
```

**CLI 现状** (`longbridge-terminal/src/cli/trade.rs:498-502`):
```rust
pub async fn cmd_assets(currency: Option<String>, format: &OutputFormat) -> Result<()> {
    let balances = ctx.account_balance(currency.as_deref()).await?;
}
```
CLI flag 在 `mod.rs` 的 `Assets { #[arg(long)] currency: Option<String> }`。

**修复**:加 `AccountBalanceParam { currency: Option<String> }`,转发给 SDK。

---

### 🔴 `today_orders`(P1,A 类)

**MCP 现状** (`src/tools/trade.rs:126-130`):
```rust
pub async fn today_orders(mctx: &...) -> Result<...> {
    let result = ctx.today_orders(None).await...  // ← 硬编码 None
}
```

**CLI 现状** (`cli/trade.rs:73-83`):
```rust
let opts = GetTodayOrdersOptions::new();
let opts = if let Some(s) = symbol { opts.symbol(s) } else { opts };
```
CLI 也只暴露 `--symbol`(其他 SDK 支持的 status/side/market/order_id **CLI 也没用**,归 D 类)。

**修复**:加 `TodayOrdersParam { symbol: Option<String> }`。SDK 支持的 status/side/market/order_id 不强求,标记到 D 类。

---

### 🔴 `today_executions`(P1,A 类)

**MCP 现状** (`src/tools/trade.rs:155-162`):同样硬编码 `None`。

**CLI 现状** (`cli/trade.rs:198-202`):
```rust
let mut opts = GetTodayExecutionsOptions::new();
if let Some(s) = symbol { opts = opts.symbol(s); }
```
CLI 暴露 `--symbol` + `--order-id`(从 mod.rs 子命令找)。

**修复**:加 `TodayExecutionsParam { symbol: Option<String>, order_id: Option<String> }`。

---

### 🔴 `intraday`(P1,A 类)

**MCP 现状** (`src/tools/quote.rs:271-281`):
```rust
ctx.intraday(p.symbol, longbridge::quote::TradeSessions::Intraday)  // ← 硬编码
```

**CLI 现状** (`cli/quote.rs:478-481`):
```rust
pub async fn cmd_intraday(symbol: String, session: &str, format: &OutputFormat) -> Result<()> {
    let trade_sessions = parse_trade_sessions(session)?;  // "intraday" | "all"
}
```
**MCP 用户无法访问盘前/盘后数据**。

**修复**:把 `SymbolParam` 换成 `IntradayParam`,加 `trade_sessions: Option<String>`(默认 `"intraday"`)。

---

### 🔴 `topic_replies`(P1,A 类)

**MCP 现状** (`src/tools/content.rs:72-82`):
```rust
ctx.list_topic_replies(p.topic_id, Default::default())  // ← 不分页
```

**CLI 现状** (`cli/topic.rs:176-185`):接受 `page: i32, size: i32`,默认 1 / 20。

**修复**:`TopicIdParam` 不够用,新建 `TopicRepliesParam`:
```rust
pub struct TopicRepliesParam {
    pub topic_id: String,
    pub page: Option<i32>,    // default 1
    pub size: Option<i32>,    // default 20, range 1-50
}
```

---

## 三、Agent 误判修正(已澄清)

第二轮 agent 报告中以下两个被错标为 A 类,实际是 D 类(CLI 自己也没暴露,SDK 双失,不阻塞):

- `stock_positions` — CLI `cli/trade.rs:557` 也是 `stock_positions(None)`
- `fund_positions` — 同上

不算 CLI/MCP 差距,移到 D 类。

---

## 四、文档/描述需补完(P2)

这些 MCP 字段功能上正确,但 doc comment 缺关键信息(默认值、可选值、单位、约束),客户端拿到 schema 后不易使用。**仅文档改动**,不影响业务逻辑。

| 工具 | 字段 | 应在 doc comment 里说明 | 文件位置 |
|---|---|---|---|
| `financial_report` | `kind`、`report_type` | 完整可选值列表("IS/BS/CF/ALL"、"af/saf/q1..q4/qf") | `fundamental.rs:16-24` |
| `ah_premium` | `period`、`count` | 默认值("day"、100)、period 可选枚举 | `market.rs:42-49` |
| `sharelist_create` | `description` | 默认值是 `name`(实现层有此 fallback) | `sharelist.rs:26-31` |
| `statement_list` | `start_date`、`limit` | start_date 缺省时回溯 30 天/12 月、limit 默认 30/12 | `statement.rs:14-22` |
| `profit_analysis` | `start`、`end` | 必须成对传(已在 tool description 里写了,但 Param 字段 doc 没说) | `mod.rs:987-1001` |
| `finance_calendar` | `market` | 缺省含义(全市场?)、`category` 仅接受单个值的设计是否符合上游期望 | `calendar.rs:8-24` |

**前一版 P2 的勘误**(实际 doc 已写明,误判):
- ~~`update_watchlist_group.mode`~~ — `quote.rs:178` 已写 `(default: "replace")`
- ~~`security_list`~~ — `quote.rs:186` 描述已经很详细
- ~~`dca_create.allow_margin`~~ — `dca.rs:41` 已写 `(default false)`

---

## 五、信息不对称(C 类,P2,需团队确认)

### `replace_order` — MCP 多于 CLI

**MCP 现状** (`src/tools/trade.rs:58-66`):
```rust
pub struct ReplaceOrderParam {
    pub order_id: String,
    pub quantity: String,
    pub price: Option<String>,
    pub trigger_price: Option<String>,
    pub limit_offset: Option<String>,
    pub trailing_amount: Option<String>,
    pub trailing_percent: Option<String>,
}
```

**CLI 现状** (`cmd_replace_order` in `trade.rs:367-390`): 只支持 `order_id`、`qty`、`price`。

**疑点**: SDK `ReplaceOrderOptions` 是否真的支持 `trigger_price` / `trailing_*`?

- 若**真支持**: CLI 落后,MCP 是对的,建议给 CLI 团队补完。
- 若**不支持**: MCP 这些字段是无效字段,会被 SDK 丢弃或报错,应删掉避免误导客户端。

**行动**: 查 `longbridge` SDK crate 的 `ReplaceOrderOptions` 定义后再决定。

---

## 六、SDK 双失(D 类,可选)

CLI 和 MCP 都没用上的 SDK 字段。不阻塞当前修复;若产品要补完,两边一起改。

| SDK 字段 | 出处 | 备注 |
|---|---|---|
| `CreateTopicOptions::hashtags` | `topic_create` | 两边都硬编码 `None` |
| `CreateReplyOptions` 之外的字段(若有) | `topic_create_reply` | 待 SDK 文档确认 |
| `GetStockPositionsOptions::symbols` | `stock_positions` | CLI 也是 `None` |
| `GetFundPositionsOptions::symbols` | `fund_positions` | CLI 也是 `None` |
| `GetTodayOrdersOptions::{status, side, market, order_id}` | `today_orders` | CLI 只用了 `symbol` |
| `GetTodayExecutionsOptions` 中其他字段 | `today_executions` | 待确认 |

---

## 七、第二轮已覆盖范围

第二轮 agent 扫描覆盖了下列工具,经手工抽查校正后纳入本报告:

- ✅ trade: `submit_order`、`replace_order`、`cancel_order`、`order_detail`、`history_orders`、`history_executions`、`cash_flow`、`estimate_max_purchase_quantity`、`margin_ratio` — 字段齐全,doc 充分
- 🔴 trade: `account_balance`、`today_orders`、`today_executions` — 见第二节
- 🟡 trade: `stock_positions`、`fund_positions` — D 类,SDK 双失
- ✅ quote 大部分工具(candlesticks、history_*、option_quote、warrant_quote、depth、brokers、trades、warrant_list、calc_indexes、create/update/delete_watchlist_group、security_list 等)
- 🔴 quote: `intraday` — 见第二节
- ✅ content: `news`、`topic`、`topic_detail`
- 🔴 content: `topic_replies` — 见第二节
- ✅ dca: `dca_list`、`dca_create`、`dca_update`、`dca_pause`、`dca_resume`、`dca_stop`、`dca_history`、`dca_stats`、`dca_check` — 字段齐全
- ✅ sharelist 全部
- ✅ alert: `alert_list`、`alert_delete`、`alert_enable`、`alert_disable`
- ✅ statement、portfolio、market、fundamental、calendar 大部分

---

## 八、汇总表

| 工具 | 类别 | 优先级 | 一句话 |
|---|---|---|---|
| `topic_create` | A | **P0** | 缺 `topic_type`(post/article) |
| `topic_create_reply` | A | **P1** | 缺 `reply_to_id`,无法嵌套回复 |
| `alert_add` | A | **P1** | 缺 `note`,业务实现也未发送 |
| `account_balance` | A | **P1** | 缺 `currency` 过滤 |
| `today_orders` | A | **P1** | 缺 `symbol` 过滤 |
| `today_executions` | A | **P1** | 缺 `symbol`、`order_id` 过滤 |
| `intraday` | A | **P1** | 缺 `trade_sessions`,无法访问盘前/盘后 |
| `topic_replies` | A | **P1** | 缺 `page`/`size`,无分页 |
| `replace_order` | C | P2 | MCP 比 CLI 多 4 个字段,需确认 SDK 是否真支持 |
| `financial_report` | B | P2 | kind/report_type 可选值未在 doc 列出 |
| `ah_premium` | B | P2 | period/count 默认值未说明 |
| `sharelist_create` | B | P2 | description fallback 行为未说明 |
| `statement_list` | B | P2 | start_date/limit 默认值逻辑未说明 |
| `profit_analysis` | B | P2 | start/end 配对约束应在字段 doc 重申 |
| `finance_calendar` | B | P2 | market 缺省含义、category 是否应支持多值 |

## 九、建议工期

- **P0**(`topic_create`): ~30 min(单字段透传)
- **P1 — 内容/告警**(`topic_create_reply`、`alert_add`、`topic_replies`): ~2 h
  - `alert_add` 需先确认上游 note 字段名
- **P1 — 交易过滤**(`account_balance`、`today_orders`、`today_executions`): ~1 h(模式相同,Param 加可选字段透传)
- **P1 — 行情**(`intraday`): ~30 min(用 `parse_trade_sessions` 已有逻辑)
- **P2 文档批改**: ~1 h(纯改 doc comment)
- **C 类 `replace_order`**: ~30 min 翻 SDK 验证后决定

合计 **~5.5 h**。
- **P2 文档批改**: ~1 h(纯改 doc comment)
- **C 类 `replace_order`**: ~30 min 翻 SDK 验证后决定

合计 ~3.5 h。
