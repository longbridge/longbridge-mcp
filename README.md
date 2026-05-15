<p align="center">
  <img src="https://raw.githubusercontent.com/longbridge/longbridge-mcp/main/docs/logo.png" alt="Longbridge" width="120" height="120">
</p>

<h1 align="center">Longbridge MCP Server</h1>

<p align="center">
  <a href="https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions"><img alt="Official MCP Registry" src="https://img.shields.io/badge/MCP%20Registry-com.longbridge%2Fmcp-0a66c2"></a>
  <a href="https://smithery.ai/servers/longbridge-official/longbridge-mcp"><img alt="Smithery" src="https://smithery.ai/badge/longbridge-official/longbridge-mcp"></a>
  <a href="https://lobehub.com/mcp/longbridge-longbridge-mcp"><img alt="LobeHub" src="https://lobehub.com/badge/mcp/longbridge-longbridge-mcp"></a>
  <a href="https://github.com/longbridge/longbridge-mcp/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <a href="https://longbridge.com"><img alt="Longbridge" src="https://img.shields.io/badge/brokerage-Longbridge-ffe000?labelColor=000"></a>
</p>

Official MCP server for the [Longbridge](https://longbridge.com) brokerage. **133 tools** across real-time quotes, options, order routing, fundamentals, analyst ratings, calendars, IPO, price alerts, DCA plans, portfolio analytics and community sharelists — covering **US and HK markets**. Built with Rust using [rmcp](https://github.com/anthropics/rmcp) and [axum](https://github.com/tokio-rs/axum).

## Features

- **133 MCP tools** across 12 categories: quotes, trading, fundamentals, market data, calendars, IPO, portfolio, alerts, content, account statements, DCA, and community sharelists
- **Stateless architecture** -- each request carries a Bearer token forwarded directly to the Longbridge SDK; no server-side sessions or database
- **OAuth 2.1 resource metadata** compliant with RFC 9728, pointing clients to Longbridge OAuth for authorization
- **JSON response transformation** -- field names normalized to snake_case, timestamps converted to RFC 3339, internal counter_id values mapped to human-readable symbols
- **Prometheus metrics** for monitoring tool calls, latency, and errors
- **Configurable** via CLI arguments or a JSON config file (CLI takes precedence)

## Connect from an MCP client

Longbridge operates a hosted endpoint at `https://openapi.longbridge.com/mcp`, so most users don't need to run their own server — just point your MCP client at it and complete OAuth when prompted. Authorization is auto-discovered via RFC 9728.

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or the equivalent on your OS:

```json
{
  "mcpServers": {
    "longbridge": {
      "url": "https://openapi.longbridge.com/mcp"
    }
  }
}
```

Restart Claude Desktop. On first tool invocation it will open a browser to complete the Longbridge OAuth flow.

### Claude Code

```bash
claude mcp add --transport http longbridge https://openapi.longbridge.com/mcp
```

### Cursor / Cline / Windsurf / other MCP clients

Point the client at `https://openapi.longbridge.com/mcp` using transport `streamable-http`. OAuth is auto-discovered via RFC 9728; no manual token required.

---

## Self-hosting

Prefer running your own instance? Use Docker or build from source.

### Docker (recommended)

```bash
docker run -p 8443:8443 \
  -v /path/to/certs:/certs:ro \
  ghcr.io/longbridge/longbridge-mcp \
  --bind 0.0.0.0:8443 \
  --base-url https://mcp.example.com \
  --tls-cert /certs/cert.pem \
  --tls-key /certs/key.pem
```

> **Important:** When deploying to a public network, you **must** set `--base-url` to the externally reachable URL of your server (e.g. `https://mcp.example.com`). This URL is returned in the OAuth protected resource metadata and used by MCP clients to discover the authorization server. If not set, it defaults to `http://localhost:{port}` which will not work for remote clients.

### Build from source

```bash
cargo build --release
./target/release/longbridge-mcp
```

### Configure

Create a config file at `~/.longbridge/mcp/config.json` (optional):

```json
{
  "bind": "127.0.0.1:8000",
  "base_url": "https://mcp.example.com",
  "log_dir": "/var/log/longbridge-mcp"
}
```

## Configuration

| Option | Config Key | CLI Flag | Default | Description |
|--------|-----------|----------|---------|-------------|
| Bind address | `bind` | `--bind` | `127.0.0.1:8000` | HTTP server listen address |
| Base URL | `base_url` | `--base-url` | auto | Public base URL for resource metadata |
| Log directory | `log_dir` | `--log-dir` | *(stderr)* | Directory for rolling log files |
| TLS certificate | `tls_cert` | `--tls-cert` | *(none)* | PEM certificate file for HTTPS |
| TLS private key | `tls_key` | `--tls-key` | *(none)* | PEM private key file for HTTPS |

CLI arguments override config file values. The config file is read from `~/.longbridge/mcp/config.json` (override with `LONGBRIDGE_MCP_CONFIG_DIR`).

When `tls_cert` and `tls_key` are both set, the server runs HTTPS. Otherwise it falls back to HTTP. The `base_url` defaults to `https://localhost:{port}` with TLS or `http://localhost:{port}` without.

### Environment Variables

These are **advanced settings** — most users do not need to change them. They are primarily useful for connecting to non-production Longbridge environments or debugging SDK internals.

| Variable | Default | Description |
|----------|---------|-------------|
| `LONGBRIDGE_MCP_CONFIG_DIR` | `~/.longbridge/mcp` | Config file directory |
| `LONGBRIDGE_HTTP_URL` | `https://openapi.longbridge.com` | Longbridge API base URL (also used for OAuth metadata) |
| `LONGBRIDGE_QUOTE_WS_URL` | `wss://openapi-quote.longbridge.com/v2` | Quote WebSocket endpoint |
| `LONGBRIDGE_TRADE_WS_URL` | `wss://openapi-trade.longbridge.com/v2` | Trade WebSocket endpoint |
| `LONGBRIDGE_LOG_PATH` | *(none)* | SDK internal log path |

## Authentication

The server expects a Longbridge OAuth access token in the `Authorization: Bearer <token>` header. On missing or invalid auth, it returns 401 with a `WWW-Authenticate` header pointing to the protected resource metadata endpoint, which in turn directs MCP clients to the Longbridge OAuth authorization server.

## Claude Code integration

The one-liner in [Connect → Claude Code](#claude-code) gets you connected. Below are the extra commands you'll reach for while developing against this server.

```bash
# Hosted — use this unless you have a reason not to
claude mcp add --transport http longbridge https://openapi.longbridge.com/mcp

# Local self-hosted instance (see Self-hosting above)
claude mcp add --transport http longbridge-local http://localhost:8000/mcp

# Inspect
claude mcp list                         # registered servers
claude mcp get longbridge               # config + auth status of one server
claude mcp remove longbridge            # unregister

# Re-trigger OAuth (e.g. after token revocation on the Longbridge side)
claude mcp logout longbridge
```

On the first tool invocation, Claude Code reads the `WWW-Authenticate` challenge from the server, fetches `/.well-known/oauth-protected-resource` (RFC 9728), and opens your browser for the Longbridge OAuth flow. Access tokens are cached per-session and refreshed automatically.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/.well-known/oauth-protected-resource` | Protected Resource Metadata (RFC 9728) |
| GET | `/metrics` | Prometheus metrics |
| POST/GET/DELETE | `/mcp` | MCP Streamable HTTP endpoint (requires Bearer token) |

## Tool Categories

| Category | Count | Description |
|----------|-------|-------------|
| **Quote** | 32 | Real-time and historical quotes, candlesticks, depth, brokers, options, warrants, watchlists, capital flow, market temperature, short positions, option volume |
| **Trade** | 14 | Order submission/cancellation/replacement, positions, balance, executions, cash flow, margin |
| **Fundamental** | 19 | Financial statements, business segments, institutional views, industry peers, earnings snapshot, dividends, EPS forecasts, valuations, company info, shareholders, corporate actions |
| **Market** | 10 | Market status, industry rank, broker holdings, A/H premium, trade statistics, anomalies, index constituents |
| **IPO** | 8 | IPO subscriptions, calendar, listed stocks, order detail, profit/loss analysis |
| **Content** | 8 | News, discussion topics, filing details |
| **DCA** | 9 | Dollar-cost averaging plan create/update/pause/resume/stop, execution history, statistics, and support check |
| **Sharelist** | 8 | Community sharelist CRUD, member add/remove/sort, popular lists |
| **Alert** | 5 | Price alert CRUD (add, delete, enable, disable, list) |
| **ATM** | 3 | Bank cards, withdrawal records, deposit records |
| **Portfolio** | 3 | Exchange rates, profit/loss analysis with optional date range |
| **Search** | 2 | News search, community topic search |
| **Statement** | 2 | Account statement listing and export |
| **Calendar** | 1 | Finance calendar events (earnings, dividends, IPOs, macro data, market closures) |
| **Utility** | 1 | Current UTC time |

## Prometheus Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `mcp_tool_calls_total` | Counter | Total tool invocations (label: `tool_name`) |
| `mcp_tool_call_duration_seconds` | Histogram | Tool call latency (label: `tool_name`) |
| `mcp_tool_call_errors_total` | Counter | Tool call error count (label: `tool_name`) |

## Project Structure

```
src/
  main.rs              CLI args, config loading, axum server setup
  auth/
    mod.rs             Router composition, MCP service wiring
    metadata.rs        Protected Resource Metadata (RFC 9728)
    middleware.rs       Bearer token extraction middleware
  tools/
    mod.rs             MCP tool definitions and ServerHandler impl
    quote.rs           Quote tools (SDK QuoteContext)
    trade.rs           Trade tools (SDK TradeContext)
    fundamental.rs     Fundamental data, business segments, industry peers (HTTP API)
    market.rs          Market data, industry rank, broker holdings, anomalies (HTTP API)
    ipo.rs             IPO subscriptions, calendar, orders, profit/loss (HTTP API)
    search.rs          News and topic search (HTTP API)
    atm.rs             Bank cards, withdrawals, deposits (HTTP API)
    calendar.rs        Finance calendar (HTTP API)
    portfolio.rs       Portfolio analytics (HTTP API)
    dca.rs             Dollar-cost averaging / recurring investment (HTTP API)
    sharelist.rs       Community sharelist management (HTTP API)
    alert.rs           Price alerts (HTTP API)
    content.rs         News, topics, filings (SDK ContentContext + HTTP)
    statement.rs       Account statements (HTTP API)
    http_client.rs     Shared HTTP client helpers
    parse.rs           Parameter parsing helpers
  serialize/           JSON transformation (snake_case, timestamps, counter_id -> symbol)
  counter.rs           Symbol <-> counter_id bidirectional conversion (ST/ETF/IX/BK)
  metrics.rs           Prometheus metric definitions and /metrics handler
  error.rs             Unified error type (thiserror)
```

## Development

```bash
# Format
cargo +nightly fmt

# Lint
cargo clippy

# Test
cargo test
```

## License

See [LICENSE](LICENSE) for details.
