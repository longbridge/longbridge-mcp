# Longbridge MCP Server

A [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that exposes Longbridge market data, trading, and financial analysis capabilities as 110 MCP tools. Built with Rust using [rmcp](https://github.com/anthropics/rmcp) and [axum](https://github.com/tokio-rs/axum).

## Features

- **110 MCP tools** across 11 categories: quotes, trading, fundamentals, market data, calendars, portfolio, alerts, content, account statements, DCA, and community sharelists
- **Stateless architecture** -- each request carries a Bearer token forwarded directly to the Longbridge SDK; no server-side sessions or database
- **OAuth 2.1 resource metadata** compliant with RFC 9728, pointing clients to Longbridge OAuth for authorization
- **JSON response transformation** -- field names normalized to snake_case, timestamps converted to RFC 3339, internal counter_id values mapped to human-readable symbols
- **Prometheus metrics** for monitoring tool calls, latency, and errors
- **Configurable** via CLI arguments or a JSON config file (CLI takes precedence)

## Quick Start

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

## Claude Code Integration

Register the server as a remote MCP endpoint:

```bash
claude mcp add --transport http longbridge-mcp http://localhost:8000/mcp
```

Claude Code will handle the OAuth flow automatically when the server requires authentication.

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
| **Fundamental** | 18 | Financial reports, analyst ratings, dividends, EPS forecasts, valuations, company info, shareholders, corporate actions |
| **Market** | 9 | Market status, broker holdings, A/H premium, trade statistics, anomalies, index constituents |
| **Content** | 8 | News, discussion topics, filing details |
| **DCA** | 9 | Dollar-cost averaging plan create/update/pause/resume/stop, execution history, statistics, and support check |
| **Sharelist** | 8 | Community sharelist CRUD, member add/remove/sort, popular lists |
| **Alert** | 5 | Price alert CRUD (add, delete, enable, disable, list) |
| **Portfolio** | 3 | Exchange rates, profit/loss analysis with optional date range |
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
    fundamental.rs     Fundamental data (HTTP API)
    market.rs          Market data extensions (HTTP API)
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
  counter.rs           Symbol <-> counter_id bidirectional conversion
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
