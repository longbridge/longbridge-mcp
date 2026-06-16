# Group A — Market & Quote Data Tools: Annotation Accuracy Rationale

This document justifies the explicit MCP tool annotations for the read-only
market-data / quote tools. Every tool in this group declares the identical
annotation set:

```
readOnlyHint    = true
destructiveHint  = false
idempotentHint   = true
openWorldHint    = true
```

Annotation semantics applied throughout:

- **readOnlyHint = true** — the tool only reads; it never modifies any
  environment or state.
- **destructiveHint = false** — the tool performs no destructive
  (irreversible / overwriting / deleting) update. A read-only tool writes
  nothing at all, so it is non-destructive by construction.
- **idempotentHint = true** — repeated calls with the same arguments produce no
  additional side effects (reading is inherently idempotent).
- **openWorldHint = true** — the tool interacts with entities outside this
  server: it reaches the Longbridge real-time quote / market backend over the
  network, and the values returned change with the live market and are not
  controlled by this server.

Each tool dispatches to the SDK's `QuoteContext` / market endpoints via
`extract_context(...)` and only ever issues a fetch; none of them call any
mutating SDK method.

---

### now
- **Read Only = true** — Returns the current UTC time as an RFC3339 string; it reads the system clock and writes nothing.
- **Destructive = false** — Reading the clock changes no state, so nothing can be overwritten or deleted.
- **Idempotent = true** — Calling it repeatedly only re-reads the clock; no side effects accumulate (the returned timestamp simply advances with real time).
- **Open World = true** — The value reflects real wall-clock time outside this server's control, advancing independently of the server.

### static_info
- **Read Only = true** — Fetches static security metadata (symbol, names, exchange, type, lot_size, listed_date, delisted) for the given symbols; pure lookup, no writes.
- **Destructive = false** — Only retrieves reference data; nothing is modified or removed.
- **Idempotent = true** — Repeated calls with the same symbols return the same metadata and leave no side effects.
- **Open World = true** — Reads listing/reference data from the Longbridge backend over the network; the data is maintained externally by exchanges/Longbridge.

### quote
- **Read Only = true** — Returns the latest price quote fields (last_done, open/high/low, volume, turnover, change, trade_status, timestamp) per symbol; it only reads live quotes.
- **Destructive = false** — Reading quotes does not write, overwrite, or delete anything.
- **Idempotent = true** — Re-querying the same symbols has no cumulative effect; each call just reads the current snapshot.
- **Open World = true** — Quote values come from the live Longbridge quote feed over the network and change continuously with the market, outside server control.

### option_quote
- **Read Only = true** — Reads option quote data including Greeks (delta, gamma, theta, vega, rho), IV, and open_interest for up to 500 symbols; lookup only.
- **Destructive = false** — Pure read of option market data; no state is altered.
- **Idempotent = true** — Repeated identical queries return current values with no side effects.
- **Open World = true** — Option quotes and Greeks are sourced live from the Longbridge backend over the network and vary with the market.

### warrant_quote
- **Read Only = true** — Reads warrant quote fields (price, volume, IV, delta, leverage_ratio, effective_leverage) per symbol; no writes.
- **Destructive = false** — Retrieves warrant market data only; nothing is modified.
- **Idempotent = true** — Repeated calls only re-read live data, with no accumulating effect.
- **Open World = true** — Warrant quotes are fetched live from the Longbridge backend over the network and change with the market.

### depth
- **Read Only = true** — Returns the order-book depth (bid/ask price levels with volume and order_num) for a symbol; it only reads the order book.
- **Destructive = false** — Reading the book places no order and changes no state.
- **Idempotent = true** — Re-reading the depth has no side effects; each call is an independent snapshot read.
- **Open World = true** — Order-book depth is live exchange data delivered through the Longbridge backend over the network, controlled by the market.

### brokers
- **Read Only = true** — Returns the HK broker queue (bid/ask broker IDs by position) for a symbol; a pure read of broker-queue data.
- **Destructive = false** — Only reads queue data; nothing is written or deleted.
- **Idempotent = true** — Repeated calls re-read the same live queue with no side effects.
- **Open World = true** — Broker-queue data originates from HKEX via the Longbridge backend over the network and changes with the market.

### participants
- **Read Only = true** — Returns the HK market participant directory (broker_ids mapped to names); a static reference lookup.
- **Destructive = false** — Read-only reference fetch; no modification of any state.
- **Idempotent = true** — The same reference list is returned on repeat calls with no side effects.
- **Open World = true** — The participant directory is maintained externally and retrieved from the Longbridge backend over the network.

### trades
- **Read Only = true** — Returns recent trade ticks (price, volume, timestamp, trade_type, direction; up to 1000) for a symbol; read-only.
- **Destructive = false** — Reads historical/recent ticks only; no state is altered.
- **Idempotent = true** — Repeated identical queries just re-read tick data, with no cumulative effect.
- **Open World = true** — Trade ticks are live exchange data served by the Longbridge backend over the network.

### intraday
- **Read Only = true** — Returns intraday minute-by-minute price/volume series for a symbol; pure read.
- **Destructive = false** — Reading the intraday line writes nothing and removes nothing.
- **Idempotent = true** — Repeated calls re-read the series; no side effects accumulate.
- **Open World = true** — Intraday data is sourced live from the Longbridge backend over the network and updates with the trading session.

### candlesticks
- **Read Only = true** — Returns OHLCV candlestick data for a symbol/period; it only reads K-line data.
- **Destructive = false** — Pure read of price history; no state is modified.
- **Idempotent = true** — Re-querying the same parameters returns the same/updated candles with no side effects.
- **Open World = true** — Candlestick data comes from the Longbridge backend over the network and reflects external market activity.

### history_candlesticks_by_offset
- **Read Only = true** — Reads historical candlesticks anchored by an offset from a reference time; lookup only.
- **Destructive = false** — Only retrieves historical K-line data; nothing is written.
- **Idempotent = true** — Identical parameters yield the same historical window with no side effects.
- **Open World = true** — Historical candles are fetched from the Longbridge backend over the network; the underlying data is externally maintained.

### history_candlesticks_by_date
- **Read Only = true** — Reads historical candlesticks for a given date range; pure read.
- **Destructive = false** — Retrieves historical price data only; no modification of state.
- **Idempotent = true** — The same date range returns the same data on repeat calls, with no side effects.
- **Open World = true** — Historical data is retrieved from the Longbridge backend over the network, controlled by the exchange/Longbridge.

### trading_days
- **Read Only = true** — Returns trading and half-trading days for a market between two dates; a calendar lookup.
- **Destructive = false** — Reading the trading calendar changes no state.
- **Idempotent = true** — The same market and date range return the same calendar with no side effects.
- **Open World = true** — The trading calendar is maintained externally and served from the Longbridge backend over the network.

### option_chain_expiry_date_list
- **Read Only = true** — Returns the list of option-chain expiry dates for a symbol; pure lookup.
- **Destructive = false** — Only reads available expiries; nothing is written or deleted.
- **Idempotent = true** — Repeated calls return the same expiry list with no side effects.
- **Open World = true** — Expiry data is fetched from the Longbridge backend over the network and reflects externally listed contracts.

### option_chain_info_by_date
- **Read Only = true** — Returns the option chain (strikes with call/put quotes and Greeks) for an expiry date; read-only.
- **Destructive = false** — Retrieves chain data only; no state is modified.
- **Idempotent = true** — Identical queries re-read the chain with no cumulative effect.
- **Open World = true** — Chain quotes and Greeks are live data from the Longbridge backend over the network and change with the market.

### capital_flow
- **Read Only = true** — Returns the same-day capital inflow/outflow/net-flow time series for a symbol; pure read.
- **Destructive = false** — Reading capital-flow data writes nothing.
- **Idempotent = true** — Repeated calls re-read the series with no side effects.
- **Open World = true** — Capital-flow data is computed externally and served from the Longbridge backend over the network.

### capital_distribution
- **Read Only = true** — Returns capital-in/capital-out broken down by large/medium/small order size for a symbol; read-only.
- **Destructive = false** — Pure read of distribution data; no modification.
- **Idempotent = true** — The same symbol returns the current distribution on repeat calls, with no side effects.
- **Open World = true** — Distribution data is sourced from the Longbridge backend over the network and reflects live market flow.

### trading_session
- **Read Only = true** — Returns the trading-session schedule (session windows and types) for all markets; a schedule lookup.
- **Destructive = false** — Reading the schedule changes no state.
- **Idempotent = true** — Returns the same schedule on repeat calls with no side effects.
- **Open World = true** — Session schedules are defined by the exchanges and retrieved from the Longbridge backend over the network.

### market_temperature
- **Read Only = true** — Returns the current market sentiment temperature (temperature, valuation, sentiment, description, timestamp) for a market; read-only.
- **Destructive = false** — Reads a computed sentiment metric; nothing is written.
- **Idempotent = true** — Re-querying the same market re-reads the metric with no side effects.
- **Open World = true** — The temperature is computed externally and served live from the Longbridge backend over the network.

### history_market_temperature
- **Read Only = true** — Returns the historical market-temperature time series for a market and date range; pure read.
- **Destructive = false** — Retrieves historical sentiment data only; no state change.
- **Idempotent = true** — The same market/date range returns the same series with no side effects.
- **Open World = true** — Historical sentiment data is fetched from the Longbridge backend over the network.

### watchlist
- **Read Only = true** — Returns the user's existing watchlist groups and their securities; it only reads the watchlist and never modifies it. (Mutation is handled by the separate create/update/delete_watchlist_group tools, which are not in this group.)
- **Destructive = false** — Reading the watchlist neither adds, removes, nor reorders any group or security.
- **Idempotent = true** — Repeated calls return the current watchlist with no side effects.
- **Open World = true** — The watchlist is stored in the user's Longbridge account and retrieved from the backend over the network.

### filings
- **Read Only = true** — Returns regulatory filings (8-K, 10-Q, 10-K, etc.) metadata and URLs for a symbol; pure lookup.
- **Destructive = false** — Reads filing listings only; nothing is written or deleted.
- **Idempotent = true** — Repeated identical queries return the same listing with no side effects.
- **Open World = true** — Filing data originates from regulators and is served from the Longbridge backend over the network.

### warrant_issuers
- **Read Only = true** — Returns the HK warrant issuer directory (id and names); a static reference lookup.
- **Destructive = false** — Read-only reference fetch; no modification.
- **Idempotent = true** — Returns the same issuer list on repeat calls with no side effects.
- **Open World = true** — The issuer directory is maintained externally and fetched from the Longbridge backend over the network.

### warrant_list
- **Read Only = true** — Returns a filtered list of warrants for an underlying symbol with their quote/term fields; read-only.
- **Destructive = false** — Only reads the warrant list; no state is altered.
- **Idempotent = true** — The same filter returns matching warrants on repeat calls with no side effects.
- **Open World = true** — Warrant listings and quotes are live data from the Longbridge backend over the network.

### calc_indexes
- **Read Only = true** — Computes/returns requested financial indexes (e.g. PE, PB, LastDone, TurnoverRate) for the given symbols by reading market data; it performs no writes.
- **Destructive = false** — The "calculation" is a derived read over market data; no environment state is modified.
- **Idempotent = true** — The same symbols and indexes yield the same computed values with no side effects.
- **Open World = true** — Inputs are live market values fetched from the Longbridge backend over the network, so outputs change with the market.

### security_list
- **Read Only = true** — Returns a paginated security list for a market/category; a directory lookup.
- **Destructive = false** — Reads the listing only; nothing is written or removed.
- **Idempotent = true** — The same market/category/page returns the same page with no side effects.
- **Open World = true** — The security universe is maintained externally and served from the Longbridge backend over the network.

### market_status
- **Read Only = true** — Returns the current trading status (Trading, Closed, Mid-Day Break, Pre-Market, etc.) per market; pure read.
- **Destructive = false** — Reading market status changes no state.
- **Idempotent = true** — Repeated calls re-read the current status with no side effects.
- **Open World = true** — Market status reflects live exchange state delivered by the Longbridge backend over the network.

### exchange_rate
- **Read Only = true** — Returns current exchange rates for supported currency pairs; a pure read.
- **Destructive = false** — Reading FX rates writes nothing.
- **Idempotent = true** — Repeated calls re-read current rates with no side effects.
- **Open World = true** — FX rates are sourced externally and served live from the Longbridge backend over the network.

### ah_premium
- **Read Only = true** — Returns the A/H share premium historical K-line (OHLC of the premium percentage); read-only.
- **Destructive = false** — Retrieves historical premium data only; no modification.
- **Idempotent = true** — The same parameters return the same series with no side effects.
- **Open World = true** — A/H premium data is derived from live cross-market prices via the Longbridge backend over the network.

### ah_premium_intraday
- **Read Only = true** — Returns the intraday A/H premium time-share series (timestamp, premium_rate); pure read.
- **Destructive = false** — Reads intraday premium data only; no state change.
- **Idempotent = true** — Repeated calls re-read the series with no side effects.
- **Open World = true** — Intraday premium values reflect live market prices from the Longbridge backend over the network.

### trade_stats
- **Read Only = true** — Returns the buy/sell/neutral volume distribution (price-volume profile) for a symbol; read-only.
- **Destructive = false** — Reads statistics only; nothing is written.
- **Idempotent = true** — The same symbol returns the current profile on repeat calls with no side effects.
- **Open World = true** — Trade statistics are computed from live market data and served from the Longbridge backend over the network.

### anomaly
- **Read Only = true** — Returns market anomaly alerts (unusual price/volume changes) for a market or symbol; a pure read of detected anomalies.
- **Destructive = false** — Reading alerts changes no state.
- **Idempotent = true** — The same filter returns the current alert set on repeat calls with no side effects.
- **Open World = true** — Anomaly detection runs externally and results are served live from the Longbridge backend over the network.

### constituent
- **Read Only = true** — Returns index constituents or an ETF's asset allocation (holdings, regional, asset-class, industry breakdowns); pure lookup.
- **Destructive = false** — Reads constituent/allocation data only; nothing is modified.
- **Idempotent = true** — The same index/ETF returns the same composition on repeat calls with no side effects.
- **Open World = true** — Constituent and allocation data are maintained externally and fetched from the Longbridge backend over the network.

### short_positions
- **Read Only = true** — Returns short-interest history (open short positions, ratio, days_to_cover, etc.) for HK/US stocks; read-only.
- **Destructive = false** — Reads short-interest data only; no state change.
- **Idempotent = true** — The same symbol/count returns the same history on repeat calls with no side effects.
- **Open World = true** — Short-interest data originates from FINRA (US) and HKEX (HK) and is served from the Longbridge backend over the network.

### option_volume
- **Read Only = true** — Returns real-time call/put volume stats (call/put volume, put_call_ratio, open interest, top contracts) for a US stock; pure read.
- **Destructive = false** — Reads option-volume stats only; nothing is written.
- **Idempotent = true** — Repeated calls re-read the current stats with no side effects.
- **Open World = true** — Option-volume stats are live data from the Longbridge backend over the network and change with the market.

### option_volume_daily
- **Read Only = true** — Returns daily historical option volume/open-interest stats for a US stock; read-only.
- **Destructive = false** — Retrieves historical option stats only; no modification.
- **Idempotent = true** — The same symbol returns the same daily series on repeat calls with no side effects.
- **Open World = true** — Daily option stats are fetched from the Longbridge backend over the network and reflect external market data.

### top_movers
- **Read Only = true** — Returns stocks whose price move exceeds the 20-trading-day standard deviation, with correlated news; a pure read of computed events.
- **Destructive = false** — Reading the movers list changes no state.
- **Idempotent = true** — The same parameters (markets/sort/limit/cursor) return the same page with no side effects; pagination is driven by the supplied cursor, not by mutation.
- **Open World = true** — Top-mover events are computed externally from live market data and served from the Longbridge backend over the network.
