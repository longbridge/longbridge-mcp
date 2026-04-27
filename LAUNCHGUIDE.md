# Longbridge MCP

## Tagline
Trade US & HK stocks, manage positions, and analyse markets through your Longbridge brokerage.

## Description
Longbridge MCP is the official Model Context Protocol server from [Longbridge](https://longbridge.com), a licensed brokerage operating across Hong Kong, the United States, Singapore, Japan and New Zealand. It exposes the Longbridge OpenAPI as **110 composable tools** so any MCP-capable AI assistant (Claude Desktop, Claude Code, Cursor, Cline, Windsurf) can read markets, route orders and analyse portfolios on the user's behalf.

The server is hosted at `https://openapi.longbridge.com/mcp` (streamable-http), stateless, and uses OAuth 2.1 with RFC 9728 protected-resource-metadata — clients auto-discover the Longbridge authorization server, so end users don't configure API keys. Self-hosting is also supported via the `ghcr.io/longbridge/longbridge-mcp` Docker image.

## Setup Requirements
No environment variables or API keys required for the hosted endpoint. Authentication is handled by an OAuth 2.1 flow that the MCP client kicks off automatically the first time a tool is invoked — the user logs in to their Longbridge account in the browser and grants access. No tokens to copy/paste, no secrets to configure.

For **self-hosting** (optional):
- `LONGBRIDGE_HTTP_URL` (optional, default `https://openapi.longbridge.com`): Longbridge OpenAPI base URL — used for OAuth metadata discovery.
- `LONGBRIDGE_QUOTE_WS_URL` (optional, default `wss://openapi-quote.longbridge.com/v2`): quote WebSocket endpoint.
- `LONGBRIDGE_TRADE_WS_URL` (optional, default `wss://openapi-trade.longbridge.com/v2`): trade WebSocket endpoint.

## Category
Finance

## Use Cases
Real-time quotes, options chain analysis, order routing, portfolio analytics, DCA plan management, earnings calendar tracking, price alerts, fundamental research, analyst rating lookups, multi-currency P&L, account statement export, community sharelists, market temperature, capital flow, broker queue analysis, A/H premium, index constituents, dividend tracking, EPS forecasts, valuation metrics

## Features
- 110 MCP tools spanning the full broker workflow
- Real-time and historical quotes for US & HK equities (32 tools)
- Order submission, cancellation and replacement; positions, balance, executions, cash flow, margin (14 tools)
- Fundamentals: financial reports, analyst ratings, dividends, EPS forecasts, valuations, shareholder structure, corporate actions (18 tools)
- Market extensions: market status, A/H premium, broker holdings, index constituents, anomalies, trade statistics (9 tools)
- DCA plan lifecycle: create, update, pause, resume, stop, execution history, statistics (9 tools)
- Content: news, discussion topics, filing details (8 tools)
- Community sharelists: CRUD, member management, popular lists (8 tools)
- Price alerts: add, delete, enable, disable, list (5 tools)
- Portfolio analytics: multi-currency FX, P&L analysis (3 tools)
- Account statements: list & export (2 tools)
- Earnings, dividend, IPO, macro and market-closure calendars
- OAuth 2.1 via RFC 9728 — no API keys for the hosted endpoint
- Stateless architecture; no server-side sessions or database
- JSON normalised: `snake_case` keys, RFC 3339 timestamps, internal counter_id values mapped to human-readable symbols
- Prometheus metrics for tool calls, latency and errors
- Self-hosting via `ghcr.io/longbridge/longbridge-mcp` Docker image
- MIT licensed; source on GitHub

## Getting Started
- "Show me the current AAPL quote with bid, ask, last price, and today's range."
- "What are the highest open-interest call options on TSLA expiring this Friday?"
- "Submit a limit buy for 100 shares of HK.0700 at 380, time-in-force day."
- "List all my open positions across US and HK accounts and compute total P&L."
- "Pause my DCA plan for VOO until next month."
- Tool: `realtime_quote` — Get real-time bid/ask, last price, volume and 52-week range for any US/HK symbol.
- Tool: `option_chain_expiry_date_list` — List option expiry dates for a given underlying.
- Tool: `submit_order` — Place a market or limit order with full TIF / outside-RTH controls.
- Tool: `stock_positions` — List all stock positions across your linked accounts.
- Tool: `analyst_rating` — Pull current Wall Street ratings, price targets and revision history.
- Tool: `pnl_analysis` — Compute realised and unrealised P&L over an optional date range, in your preferred currency.

## Tags
longbridge, brokerage, stocks, options, us-stocks, hk-stocks, trading, quotes, fundamentals, analyst-ratings, portfolio, dca, alerts, calendars, market-data, oauth, finance, fintech, hosted, streamable-http

## Documentation URL
https://github.com/longbridge/longbridge-mcp/blob/main/README.md

## Health Check URL
https://openapi.longbridge.com/health
