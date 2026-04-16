# Longbridge MCP Server

A [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that exposes Longbridge market data, trading, and financial analysis capabilities as 90 MCP tools. Built with Rust using [rmcp](https://github.com/anthropics/rmcp) and [axum](https://github.com/tokio-rs/axum).

## Features

- **90 MCP tools** across 9 categories: quotes, trading, fundamentals, market data, calendars, portfolio, alerts, content, and account statements
- **OAuth 2.1 authentication** compliant with the [MCP authorization spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), with PKCE support
- **Per-user session management** with lazy context creation and configurable idle timeout
- **JSON response transformation** -- field names normalized to snake_case, timestamps converted to RFC 3339, internal counter_id values mapped to human-readable symbols
- **Prometheus metrics** for monitoring tool calls, latency, errors, and session counts
- **Configurable** via CLI arguments or a JSON config file (CLI takes precedence)

## Quick Start

### Prerequisites

- Rust toolchain (edition 2024)

### Build

```bash
cargo build --release
```

### Configure

Create a config file at `~/.longbridge/mcp/config.json` (optional):

```json
{
  "bind": "127.0.0.1:8000",
  "base_url": "https://mcp.example.com",
  "idle_timeout": 300,
  "log_dir": "/var/log/longbridge-mcp"
}
```

### Run

```bash
./target/release/longbridge-mcp
```

## Configuration

| Option | Config Key | CLI Flag | Default | Description |
|--------|-----------|----------|---------|-------------|
| Bind address | `bind` | `--bind` | `127.0.0.1:8000` | HTTP server listen address |
| Base URL | `base_url` | `--base-url` | `http://{bind}` | Public base URL for OAuth callbacks |
| Idle timeout | `idle_timeout` | `--idle-timeout` | `300` | Session idle timeout in seconds |
| Log directory | `log_dir` | `--log-dir` | *(stderr)* | Directory for rolling log files |

CLI arguments override config file values. The config file is read from `~/.longbridge/mcp/config.json`.

## Claude Code Integration

Register the server as a remote MCP endpoint:

```bash
claude mcp add --transport http longbridge-mcp http://localhost:8000/mcp
```

Claude Code will handle the OAuth flow automatically when the server requires authentication.

## OAuth Flow

The server implements a double OAuth flow -- MCP clients authenticate against this server, which in turn authenticates against Longbridge OAuth on behalf of the user.

```
MCP Client                    MCP Server                    Longbridge OAuth
    |                              |                              |
    |-- MCP request (no token) --> |                              |
    |<-- 401 + WWW-Authenticate -- |                              |
    |                              |                              |
    |-- GET /.well-known/* ------> |                              |
    |<-- metadata ----------------- |                              |
    |                              |                              |
    |-- /oauth/authorize --------> |  register client ----------> |
    |                              |<-- client_id ---------------- |
    |                              |-- 302 redirect ------------> |
    |                              |     (Longbridge authorize)   |
    |                              |                              |
    |              [user authorizes on Longbridge page]           |
    |                              |                              |
    |                              |<-- callback?code=xxx -------- |
    |                              |-- exchange code for token -> |
    |                              |<-- Longbridge tokens -------- |
    |                              |                              |
    |<-- 302 redirect_uri?code= -- |  (issue auth code)          |
    |                              |                              |
    |-- POST /oauth/token -------> |                              |
    |   (code + code_verifier)     |  (verify PKCE, issue JWT)   |
    |<-- access_token ------------ |                              |
    |                              |                              |
    |-- MCP request + Bearer ----> |  (validate JWT, find        |
    |                              |   Longbridge session)        |
    |<-- MCP response ------------ |                              |
```

Key points:
- Each user gets a dynamically registered Longbridge OAuth client_id
- Longbridge credentials stay server-side; MCP clients only receive JWTs issued by this server
- Sessions survive idle timeouts -- credentials persist on disk, so reconnection does not require re-authorization
- PKCE (S256) is required for all authorization requests

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/.well-known/oauth-protected-resource` | Protected Resource Metadata (RFC 9728) |
| GET | `/.well-known/oauth-authorization-server` | Authorization Server Metadata (RFC 8414) |
| GET | `/oauth/authorize` | OAuth authorization endpoint |
| GET | `/oauth/callback` | Longbridge OAuth callback |
| POST | `/oauth/token` | Token endpoint (issue / refresh) |
| GET | `/api/users` | List authorized users |
| DELETE | `/api/users/{user_id}` | Revoke a user (delete session, credentials, DB record) |
| GET | `/metrics` | Prometheus metrics |
| POST/GET/DELETE | `/mcp` | MCP Streamable HTTP endpoint (requires Bearer token) |

## Tool Categories

| Category | Count | Description |
|----------|-------|-------------|
| **Quote** | 29 | Real-time and historical quotes, candlesticks, depth, brokers, options, warrants, watchlists, capital flow, market temperature |
| **Trade** | 14 | Order submission/cancellation/replacement, positions, balance, executions, cash flow, margin |
| **Fundamental** | 18 | Financial reports, analyst ratings, dividends, EPS forecasts, valuations, company info, shareholders, corporate actions |
| **Market** | 9 | Market status, broker holdings, A/H premium, trade statistics, anomalies, index constituents |
| **Content** | 8 | News, discussion topics, filing details |
| **Alert** | 5 | Price alert CRUD (add, delete, enable, disable, list) |
| **Portfolio** | 3 | Exchange rates, profit/loss analysis |
| **Statement** | 2 | Account statement listing and export |
| **Calendar** | 1 | Finance calendar events (earnings, dividends, IPOs, macro data, market closures) |
| **Utility** | 1 | Current UTC time |

## Prometheus Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `mcp_active_sessions` | Gauge | Current number of active user sessions |
| `mcp_tool_calls_total` | Counter | Total tool invocations (label: `tool_name`) |
| `mcp_tool_call_duration_seconds` | Histogram | Tool call latency (label: `tool_name`) |
| `mcp_tool_call_errors_total` | Counter | Tool call error count (label: `tool_name`) |

## Project Structure

```
src/
  main.rs              CLI args, config loading, axum server setup
  auth/
    mod.rs             Router composition, MCP service wiring
    server.rs          OAuth endpoints (authorize, callback, token, metadata)
    token.rs           JWT issuance and validation (HS256)
    middleware.rs       Bearer token auth middleware
    longbridge.rs      Longbridge OAuth client registration and token exchange
  registry.rs          Per-user session registry (SQLite-backed, lazy context creation)
  tools/
    mod.rs             MCP tool definitions and ServerHandler impl
    quote.rs           Quote tools (SDK QuoteContext)
    trade.rs           Trade tools (SDK TradeContext)
    fundamental.rs     Fundamental data (HTTP API)
    market.rs          Market data extensions (HTTP API)
    calendar.rs        Finance calendar (HTTP API)
    portfolio.rs       Portfolio analytics (HTTP API)
    alert.rs           Price alerts (HTTP API)
    content.rs         News, topics, filings (SDK ContentContext + HTTP)
    statement.rs       Account statements (SDK)
    http_client.rs     Shared HTTP client with Longbridge auth
    parse.rs           Parameter parsing helpers
  serialize.rs         JSON transformation (snake_case, timestamps, counter_id -> symbol)
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
