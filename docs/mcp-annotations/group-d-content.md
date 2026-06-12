# MCP Annotation Accuracy Justification — Group D (Content / Community / Screener / IPO / DCA / Sharelist / Alert / Quant)

All tools in this group are read-only query tools. Each declares the identical
annotation set in `src/tools/mod.rs`:

```
read_only_hint = true
destructive_hint = false
idempotent_hint = true
open_world_hint = true
```

Annotation semantics applied here:

- **readOnlyHint = true** — the tool only fetches data; it does not modify any
  state on the Longbridge backend.
- **destructiveHint = false** — the tool performs no write, no deletion, and no
  destructive update.
- **idempotentHint = true** — calling the tool repeatedly with the same
  arguments produces no additional side effects (each call is just another read).
- **openWorldHint = true** — the tool reaches out over the network to the
  external Longbridge backend, whose data (news feeds, community posts, screener
  universe, IPO calendar, market rankings, account-scoped DCA/alert/IPO records)
  changes in real time and is not under this server's control.

Implementation note common to the whole group: every handler builds an HTTP
client via `McpContext::create_http_client` and issues the upstream request,
then serializes the response back to the caller. The handlers hold no local
state and persist nothing. The mutating counterparts of these features
(`topic_create`, `topic_create_reply`, `dca_create`/`dca_update`/`dca_pause`/
`dca_resume`/`dca_stop`, `alert_add`/`alert_delete`/`alert_enable`/
`alert_disable`, `sharelist_create`/`sharelist_delete`/`sharelist_add`/
`sharelist_remove`/`sharelist_sort`) are deliberately **excluded** from this
group and carry `read_only_hint = false` instead.

---

### news
- **Read Only = true** — `content::news` issues a GET-style fetch for a symbol's latest news articles and returns `items[]{id, title, source, publish_time, summary, url, related_symbols}`. It only reads a news feed.
- **Destructive = false** — Fetching articles writes or deletes nothing on the backend.
- **Idempotent = true** — Re-querying the same symbol just re-reads the feed; no cumulative effect from repeated calls.
- **Open World = true** — News content comes from the external Longbridge backend and updates continuously as new articles are published.

### news_search
- **Read Only = true** — `search::news_search` calls `http_get_tool` to run a keyword search and returns `news_list[]{id, title, description, source_name, publish_at, score}`. It is a search/read over the news corpus.
- **Destructive = false** — A search query persists nothing and removes nothing.
- **Idempotent = true** — The same keyword + cursor returns the same page slice; repeating the call adds no side effects.
- **Open World = true** — Results are served by the external Longbridge search backend and shift as news is indexed.

### topic
- **Read Only = true** — `content::topic` lists community discussion topics for a symbol, returning `items[]{id, title, author, created_at, like_count, comment_count, content_summary}`. Read-only listing (creation is the separate `topic_create`, not in this group).
- **Destructive = false** — Listing topics modifies no community data.
- **Idempotent = true** — Repeated listing for the same symbol re-reads the same view; no accumulating effect.
- **Open World = true** — Community topics live on the external Longbridge backend and are updated by other users in real time.

### topic_detail
- **Read Only = true** — `content::topic_detail` fetches one topic by `topic_id`, returning its full content and metadata. Pure read.
- **Destructive = false** — Reading a topic does not edit or delete it.
- **Idempotent = true** — Fetching the same `topic_id` repeatedly yields the same record with no side effects.
- **Open World = true** — Topic data is hosted on the external Longbridge backend and may change as the post is edited or gains likes/comments.

### topic_replies
- **Read Only = true** — `content::topic_replies` reads the paginated reply list under a topic. It only retrieves replies (posting a reply is `topic_create_reply`, excluded from this group).
- **Destructive = false** — Reading replies creates/removes nothing.
- **Idempotent = true** — Same topic + page/size returns the same slice; repeating it has no effect.
- **Open World = true** — Replies are stored on the external Longbridge backend and grow as users respond.

### topic_search
- **Read Only = true** — `search::topic_search` uses `http_get_tool` to search community topics by keyword and returns id/author/time/excerpt. Read-only search.
- **Destructive = false** — A search persists nothing.
- **Idempotent = true** — The same keyword query re-reads the index; no cumulative effect.
- **Open World = true** — Topic search results come from the external Longbridge backend and vary as content is indexed.

### screener_recommend_strategies
- **Read Only = true** — `screener::screener_recommend_strategies` calls `http_get_tool` to list platform-preset screener strategies (`strategys[]{id, name, description, market, three_months_chg, risk}`). It only reads strategy metadata.
- **Destructive = false** — Listing presets changes no state.
- **Idempotent = true** — Repeated listing for the same market re-reads the same catalog.
- **Open World = true** — Strategy presets and their `three_months_chg` figures are served by the external Longbridge backend and updated server-side.

### screener_user_strategies
- **Read Only = true** — `screener::screener_user_strategies` calls `http_get_tool` to list the current user's saved screener strategies. It reads saved-strategy metadata only; it does not create or modify strategies.
- **Destructive = false** — Listing the user's saved strategies modifies nothing.
- **Idempotent = true** — Repeated calls re-read the same saved list (absent independent user changes).
- **Open World = true** — The saved list is account-scoped data held on the external Longbridge backend.

### screener_strategy
- **Read Only = true** — `screener::screener_strategy` calls `http_get_tool` to fetch a single strategy's filter conditions (`market`, `filter{filters[]{key, min, max, tech_values}}`). It inspects a strategy; it does not run or change it.
- **Destructive = false** — Inspecting filter conditions writes nothing.
- **Idempotent = true** — Fetching the same strategy id repeatedly returns the same conditions.
- **Open World = true** — Strategy definitions are stored on the external Longbridge backend.

### screener_search
- **Read Only = true** — `screener::screener_search` runs a screener query and returns `{total, items[]{symbol, name, indicators[]}}`. In Mode A it first does a `GET` to load a strategy, then submits the filter set; the actual screen is a `POST` to `/v1/quote/ai/screener/search`. The POST is purely the transport for the filter conditions of a stateless search computation — it stores no strategy and mutates no account or universe data.
- **Destructive = false** — Executing a screen creates/deletes nothing on the backend.
- **Idempotent = true** — Running the same `strategy_id` or the same `conditions`/`sort`/`page` returns the same result set; repeating the search has no side effect.
- **Open World = true** — The screen runs against the live Longbridge universe and indicator values on the external backend, which change with the market.

### screener_indicators
- **Read Only = true** — `screener::screener_indicators` calls `http_get_tool` against `/v1/quote/ai/screener/indicators` to return indicator metadata (`groups[]{group_name, indicators[]{id, key, name, unit, default_range, tech_values}}`). Pure metadata read.
- **Destructive = false** — Reading the indicator catalog changes nothing.
- **Idempotent = true** — The same (optional symbol) query re-reads the same schema.
- **Open World = true** — Indicator metadata is supplied by the external Longbridge backend.

### rank_categories
- **Read Only = true** — `market::rank_categories` calls `http_get_tool` against `/v1/quote/market/rank/categories` and returns the leaderboard tab configuration (`first_tags[]{key, name, second_tags[]}`). Read-only configuration fetch.
- **Destructive = false** — Reading category config modifies nothing.
- **Idempotent = true** — Repeated calls return the same configuration.
- **Open World = true** — Leaderboard category config is served by the external Longbridge backend.

### rank_list
- **Read Only = true** — `market::rank_list` calls `http_get_tool` to fetch the ranked stock list for a leaderboard tab key (`lists[]{symbol, name, last_done, chg, ...}`). It only reads the leaderboard.
- **Destructive = false** — Reading rankings changes no state.
- **Idempotent = true** — The same tab key + size re-reads the same leaderboard view.
- **Open World = true** — Rankings are computed live on the external Longbridge backend and change with market activity (`updated_at` is returned).

### ipo_subscriptions
- **Read Only = true** — `ipo::ipo_subscriptions` issues two `http_get_tool` reads (`/v1/ipo/subscriptions` and `/v1/ipo/us/subscriptions`) and merges them. It lists IPOs in the subscription stage; it does **not** submit any subscription/application.
- **Destructive = false** — Listing subscription-stage IPOs writes nothing.
- **Idempotent = true** — Repeated calls re-read the same listings.
- **Open World = true** — The subscription pipeline is maintained on the external Longbridge backend and changes as IPOs open/close.

### ipo_calendar
- **Read Only = true** — `ipo::ipo_calendar` calls `http_get_tool_unix` against `/v1/ipo/calendar` and returns scheduled/recent IPOs. Pure calendar read.
- **Destructive = false** — Reading the calendar modifies nothing.
- **Idempotent = true** — Repeated calls re-read the same calendar.
- **Open World = true** — The IPO calendar is served by the external Longbridge backend and updated as schedules change.

### ipo_listed
- **Read Only = true** — `ipo::ipo_listed` issues two `http_get_tool` reads (`/v1/ipo/listed`, `/v1/ipo/us/listed`) for recently listed IPOs. Read-only.
- **Destructive = false** — Listing recently listed IPOs changes nothing.
- **Idempotent = true** — Same parameters re-read the same list.
- **Open World = true** — Listing data and first-day returns come from the external Longbridge backend.

### ipo_detail
- **Read Only = true** — `ipo::ipo_detail` issues `http_get_tool` reads for `/v1/ipo/profile`, `/v1/ipo/timeline`, and `/v1/ipo/eligibility` and assembles the detail view. It only reads IPO information; checking eligibility is informational and submits no order.
- **Destructive = false** — Reading detail/eligibility writes nothing.
- **Idempotent = true** — The same symbol re-reads the same detail.
- **Open World = true** — IPO profile/timeline/eligibility data is hosted on the external Longbridge backend.

### ipo_orders
- **Read Only = true** — `ipo::ipo_orders` issues `http_get_tool` reads against `/v1/ipo/orders` and `/v1/ipo/orders/history` to list existing IPO orders. It **queries** the user's IPO application records; it does not place or amend an order.
- **Destructive = false** — Listing orders modifies no order.
- **Idempotent = true** — Same filters re-read the same order list.
- **Open World = true** — IPO orders are account-scoped records on the external Longbridge backend; their status changes server-side (e.g. allotment).

### ipo_order_detail
- **Read Only = true** — `ipo::ipo_order_detail` does a single `http_get_tool` read for one IPO order by id (`{order_id, symbol, allotted_quantity, status, ...}`). Pure read of an existing order record.
- **Destructive = false** — Reading order detail changes nothing.
- **Idempotent = true** — Fetching the same `order_id` repeatedly returns the same record.
- **Open World = true** — The order record lives on the external Longbridge backend and its status may update server-side.

### ipo_profit_loss
- **Read Only = true** — `ipo::ipo_profit_loss` issues `http_get_tool` reads for `/v1/ipo/profit-loss` and `/v1/ipo/profit-loss/items` and returns a P/L summary and per-stock breakdown. Read-only reporting.
- **Destructive = false** — Reading P/L figures modifies nothing.
- **Idempotent = true** — Same period re-reads the same computed summary.
- **Open World = true** — P/L is derived server-side on the external Longbridge backend from live valuations.

### dca_list
- **Read Only = true** — `dca::dca_list` calls `http_get_tool_unix` to list DCA (recurring investment) plans (`plans[]{plan_id, symbol, amount, frequency, status, next_execution_date}`). It only **lists** plans; creating/changing a plan is `dca_create`/`dca_update`/`dca_pause`/`dca_resume`/`dca_stop` (all excluded from this group with `read_only_hint = false`).
- **Destructive = false** — Listing plans creates/modifies/stops nothing.
- **Idempotent = true** — Same filters re-read the same plan list.
- **Open World = true** — DCA plans are account-scoped records on the external Longbridge backend, advanced server-side on schedule.

### dca_history
- **Read Only = true** — `dca::dca_history` calls `http_get_tool` against `/v1/dailycoins/query-records` to return a plan's execution history (`executions[]{date, quantity, amount, price, status, order_id}`). Read-only history query.
- **Destructive = false** — Reading execution history changes nothing.
- **Idempotent = true** — Same `plan_id` re-reads the same records.
- **Open World = true** — Execution history is stored on the external Longbridge backend and grows as the plan executes.

### dca_stats
- **Read Only = true** — `dca::dca_stats` calls `http_get_tool` against `/v1/dailycoins/statistic` and returns aggregate DCA statistics (`total_invested, total_value, total_return, ...`). Read-only aggregation.
- **Destructive = false** — Reading statistics modifies nothing.
- **Idempotent = true** — Same arguments re-read the same computed stats.
- **Open World = true** — Statistics are computed server-side on the external Longbridge backend from live valuations.

### dca_check
- **Read Only = true** — `dca::dca_check` POSTs `{counter_ids:[...]}` to `/v1/dailycoins/batch-check-support` and returns `items[]{symbol, support_dca, reason}`. Despite the POST, this is a stateless eligibility **check** (a lookup keyed by the symbol list); it creates no plan and stores nothing. The POST is only the transport for the batch of symbols.
- **Destructive = false** — Checking DCA support writes/deletes nothing.
- **Idempotent = true** — The same symbol set returns the same support flags; repeating the check has no side effect.
- **Open World = true** — Support flags are determined by the external Longbridge backend (per-instrument eligibility rules).

### sharelist_list
- **Read Only = true** — `sharelist::sharelist_list` calls `http_get_tool` against `/v1/sharelists` to list the user's own and subscribed community sharelists. Read-only listing (create/delete/add/remove/sort are excluded from this group).
- **Destructive = false** — Listing sharelists modifies none of them.
- **Idempotent = true** — Same count argument re-reads the same list.
- **Open World = true** — Sharelists are community data on the external Longbridge backend, changed by their owners/followers.

### sharelist_detail
- **Read Only = true** — `sharelist::sharelist_detail` calls `http_get_tool` to fetch one sharelist by id, including its constituents and quote data. Pure read.
- **Destructive = false** — Reading a sharelist's detail changes nothing.
- **Idempotent = true** — Fetching the same id repeatedly returns the same view.
- **Open World = true** — Sharelist content (constituents, live quotes, subscription status) is served by the external Longbridge backend.

### sharelist_popular
- **Read Only = true** — `sharelist::sharelist_popular` calls `http_get_tool` against `/v1/sharelists/popular` and returns trending sharelists sorted by popularity. Read-only.
- **Destructive = false** — Reading popular lists modifies nothing.
- **Idempotent = true** — Same count argument re-reads the same ranked view.
- **Open World = true** — Popularity rankings are computed on the external Longbridge backend and shift with community activity.

### alert_list
- **Read Only = true** — `alert::alert_list` calls `http_get_tool` against `/v1/notify/reminders` and returns configured price alerts (`lists[]{counter_id, indicators[]{id, condition, price, frequency, enabled, triggered_at}}`). It only **lists** alerts; adding/deleting/enabling/disabling are `alert_add`/`alert_delete`/`alert_enable`/`alert_disable` (excluded from this group).
- **Destructive = false** — Listing alerts changes no alert.
- **Idempotent = true** — Repeated calls re-read the same alert configuration.
- **Open World = true** — Alerts are account-scoped records on the external Longbridge backend; their `triggered_at`/`enabled` state updates server-side.

### quant_run
- **Read Only = true** — `quant::run_script` fetches historical K-line data for the symbol/period/date-range and runs the supplied indicator **script** server-side, returning the computed indicator/plot values as JSON. Although it issues a `POST` to `/v1/quant/run_script`, the request body is just the input (counter_id, time range, line_type, the script source, and `input_json`); the call is a stateless computation that reads K-line data and returns derived values. It persists nothing and modifies no account, watchlist, plan, or market state. The script runs against historical data only.
- **Destructive = false** — Running the indicator script writes/deletes nothing on the backend; it produces a computed result and discards all working state.
- **Idempotent = true** — Re-running the same script with the same symbol/period/range/input returns the same computed values; there is no cumulative effect (the only variability is normal backend data updates, which is the open-world property, not a side effect of the call).
- **Open World = true** — The computation depends on live/historical K-line data served by the external Longbridge backend, which is not under this server's control.
