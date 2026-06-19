# MCP Annotation Accuracy Justifications — Group B: Account & Trade-Record Queries

This document justifies the explicit MCP tool annotations for the account and
trade-record query tools in the Longbridge MCP server, for the OpenAI MCP
submission review form. Every tool in this group is declared with:

```
read_only_hint = true
destructive_hint = false
idempotent_hint = true
open_world_hint = true
```

Each section explains why every annotation value is accurate and does not
misrepresent the tool's real behavior. Justifications are grounded in the
verified implementation:

- Tool definitions and annotations: `src/tools/mod.rs`
- Implementations: `src/tools/trade.rs`, `src/tools/statement.rs`, `src/tools/atm.rs`
- The HTTP helper `http_get_tool` (`src/tools/support/http_client.rs`) issues only `Method::GET`.

All upstream calls in this group are read-only: they invoke SDK getters
(`account_balance`, `stock_positions`, `cash_flow`, `statements`,
`statement_download_url`, `estimate_max_purchase_quantity`, etc.) or plain HTTP
GET requests against `/v1/account/...` and `/v1/asset/...` endpoints. None of
them submit, modify, cancel, or transfer anything.

---

### account_balance
- **Read Only = true** — Calls `TradeContext::account_balance(currency)` (`trade.rs:139`), a pure getter that returns cash balance and asset summary. It accepts only an optional currency filter and writes nothing to the account.
- **Destructive = false** — It performs no update of any kind; there is no write path, so no data can be lost or overwritten.
- **Idempotent = true** — Repeating the call with the same currency filter produces no additional side effects; it just re-reads the current balance.
- **Open World = true** — It reaches the external Longbridge brokerage backend over the network. Balances reflect live market valuations and account activity outside this server's control, so successive reads may legitimately return different values.

### stock_positions
- **Read Only = true** — Calls `TradeContext::stock_positions(None)` (`trade.rs:151`), a getter returning current stock holdings. No parameters that mutate state; no write occurs.
- **Destructive = false** — No update or deletion is performed; positions are only read.
- **Idempotent = true** — Re-invoking it has no cumulative effect; it simply re-fetches the holdings snapshot.
- **Open World = true** — Data comes from the remote Longbridge account backend and changes as fills/corporate actions occur outside this server.

### fund_positions
- **Read Only = true** — Calls `TradeContext::fund_positions(None)` (`trade.rs:157`), a getter for current fund holdings. It is parameterless apart from the implicit filter and never writes.
- **Destructive = false** — No mutation; fund positions are only read.
- **Idempotent = true** — Repeated calls re-read the same holdings with no side effects.
- **Open World = true** — Backed by the external Longbridge backend; NAV and holding units update over time independently of this server.

### margin_ratio
- **Read Only = true** — Calls `TradeContext::margin_ratio(symbol)` (`trade.rs:163`), returning initial/maintenance/forced-liquidation margin factors for a symbol. It is a lookup; nothing is changed.
- **Destructive = false** — No write or update; the factors are read-only reference values.
- **Idempotent = true** — Querying the same symbol repeatedly yields the same kind of read with no side effects.
- **Open World = true** — Margin factors are served by the remote Longbridge backend and may change per upstream risk policy, outside this server's control.

### today_orders
- **Read Only = true** — Calls `TradeContext::today_orders(opts)` (`trade.rs:175`) with an optional symbol filter; it lists orders placed today and does not create, modify, or cancel any order.
- **Destructive = false** — Pure listing; no order is altered or removed.
- **Idempotent = true** — Re-running the query (same filter) has no effect on orders or account state.
- **Open World = true** — Reads live order state from the Longbridge backend, which changes as orders are submitted/filled elsewhere.

### order_detail
- **Read Only = true** — Calls `TradeContext::order_detail(order_id)` (`trade.rs:188`), fetching the details of one order by id. It only reads.
- **Destructive = false** — No mutation of the order; status/quantities are reported, not changed.
- **Idempotent = true** — Repeated lookups of the same order_id are side-effect free.
- **Open World = true** — The order record lives on the remote brokerage backend and reflects real-time execution state outside this server.

### today_executions
- **Read Only = true** — Calls `TradeContext::today_executions` and `today_orders` (joined via `tokio::try_join!`, `trade.rs:211`) to list today's fills and annotate each with its order side. Both are getters; no execution or order is created or modified.
- **Destructive = false** — The side annotation is computed in-memory on the returned data only; nothing upstream is written.
- **Idempotent = true** — Re-querying the same filters re-reads fills with no cumulative side effects.
- **Open World = true** — Execution data is fetched live from the Longbridge backend and grows as trades fill during the day.

### history_orders
- **Read Only = true** — Calls `TradeContext::history_orders(opts)` with a date range and optional symbol (`trade.rs:253`). It is a historical read; no order is touched.
- **Destructive = false** — No write/update/delete; orders are only listed.
- **Idempotent = true** — Same date range and filter re-read the same historical set with no side effects.
- **Open World = true** — Historical order data resides on the remote Longbridge backend and is not controlled by this server.

### history_executions
- **Read Only = true** — Calls `TradeContext::history_executions` and `history_orders` (joined, `trade.rs:270`) to list historical fills and annotate side. Both are getters; nothing is mutated.
- **Destructive = false** — Only in-memory enrichment of read data; no upstream write.
- **Idempotent = true** — Repeating the date-range query is side-effect free.
- **Open World = true** — Backed by the external Longbridge backend serving real historical trade records.

### cash_flow
- **Read Only = true** — Calls `TradeContext::cash_flow(opts)` over a date range (`trade.rs:316`), listing cash movement records (deposits, withdrawals, dividends). It reports past transactions; it does not initiate any cash movement.
- **Destructive = false** — Pure listing of existing records; no write or update.
- **Idempotent = true** — Re-querying the same date range re-reads the same records with no side effects.
- **Open World = true** — Records come from the remote Longbridge backend and reflect account activity outside this server's control.

### estimate_max_purchase_quantity
- **Read Only = true** — Calls `TradeContext::estimate_max_purchase_quantity(opts)` (`trade.rs:444`). Despite taking order-like parameters (symbol, side, order_type, optional price), it only *estimates* the maximum buy/sell quantity (returns `cash_max_qty`, `margin_max_qty`). No order is constructed or submitted — confirmed by the implementation, which builds `EstimateMaxPurchaseQuantityOptions` and calls the estimation getter, never `submit_order`. The parameters are inputs to a calculation, not an order placement.
- **Destructive = false** — It is a what-if calculation; no account state, position, or order is created or changed.
- **Idempotent = true** — The same inputs return the same estimate (subject to live data) with no cumulative side effects.
- **Open World = true** — The estimate is computed by the remote Longbridge backend using live buying power and margin data, which vary over time outside this server.

### statement_list
- **Read Only = true** — Calls `AssetContext::statements(options)` (`statement.rs:30`) to list available daily/monthly statements (id, type, date, status). It only enumerates existing statements.
- **Destructive = false** — No statement is generated, deleted, or modified; the list is read-only.
- **Idempotent = true** — Re-listing with the same parameters returns the same available statements with no side effects.
- **Open World = true** — Statement metadata is served by the remote Longbridge backend, where new statements appear over time independently of this server.

### statement_export
- **Read Only = true** — Calls `AssetContext::statement_download_url(options)` (`statement.rs:75`). It only retrieves a pre-signed download URL for an existing statement file (keyed by `file_key` from `statement_list`); it returns `{url}` and does nothing else. It does not generate, alter, or move the statement, and it does not even download the file itself.
- **Destructive = false** — Producing a download URL is non-destructive: the underlying statement data is unchanged, and no account state is touched.
- **Idempotent = true** — Requesting the URL for the same `file_key` repeatedly is side-effect free (it yields a fresh pre-signed link to the same data each time, but no account or statement state changes).
- **Open World = true** — The URL is minted by the remote Longbridge backend / object store, outside this server's control; the link is time-limited and externally managed.

### bank_cards
- **Read Only = true** — Calls `http_get_tool(client, "/v1/account/bank-cards", &[])` (`atm.rs:29`), an HTTP GET that *lists* the linked withdrawal bank cards (masked). It is a query of existing linked cards; it does not add, remove, or modify any card, and it does not initiate any transfer.
- **Destructive = false** — Pure GET listing; no card is created/deleted/changed.
- **Idempotent = true** — Repeating the GET returns the current card list with no side effects.
- **Open World = true** — Card data is served by the remote Longbridge `/v1/account/bank-cards` endpoint, outside this server's control.

### withdrawals
- **Read Only = true** — Calls `http_get_tool(client, "/v1/account/withdrawals", ...)` with page/size/account_channel params (`atm.rs:35`). This is an HTTP GET that *lists withdrawal history*; it does **not** initiate a withdrawal. Confirmed by the implementation: it only issues a GET against the history endpoint with paging parameters.
- **Destructive = false** — Listing past withdrawals changes nothing; no funds are moved.
- **Idempotent = true** — Re-fetching the same page is side-effect free.
- **Open World = true** — History is served by the remote Longbridge backend and grows as real withdrawals occur outside this server.

### deposits
- **Read Only = true** — Calls `http_get_tool(client, "/v1/account/deposits", ...)` with page/size/account_channel and optional states/currencies filters (`atm.rs:56`). This is an HTTP GET that *lists deposit history*; it does **not** initiate a deposit. Confirmed by the implementation: a GET against the history endpoint with filter/paging query params only.
- **Destructive = false** — Listing past deposits changes nothing; no funds are moved.
- **Idempotent = true** — Re-fetching the same filtered page produces no side effects.
- **Open World = true** — History is served by the remote Longbridge backend and reflects real deposit activity outside this server's control.

---

## Verification note

All three fund-related tools (`bank_cards`, `withdrawals`, `deposits`) were
confirmed via source inspection to be **queries**, not fund-movement operations:
each delegates to `http_get_tool`, which issues only `Method::GET`
(`src/tools/support/http_client.rs:29`). There is no POST/PUT/DELETE and no
"initiate withdrawal/deposit" path anywhere in `atm.rs`.

Similarly, `estimate_max_purchase_quantity` was confirmed to call the SDK's
estimation getter and never `submit_order`, and `statement_export` was confirmed
to only return a pre-signed download URL.

No tool in this group has an annotation value that appears inaccurate. All 16
tools are genuine read-only queries and the declared
`read_only_hint=true / destructive_hint=false / idempotent_hint=true /
open_world_hint=true` set is accurate for each.
