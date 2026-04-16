# Architecture

## Overview

Longbridge MCP Server is a **stateless** Rust service that exposes Longbridge financial data and trading capabilities through the [Model Context Protocol](https://modelcontextprotocol.io/) (MCP). It translates MCP tool calls into Longbridge SDK and HTTP API calls, handling authentication, JSON response transformation, and metrics collection.

```
┌─────────────┐         ┌──────────────────────┐         ┌──────────────────┐
│  MCP Client │  HTTP   │  Longbridge MCP      │  SDK /  │  Longbridge      │
│ (Claude,    │ Bearer  │  Server              │  HTTP   │  OpenAPI         │
│  etc.)      │────────▶│                      │────────▶│                  │
│             │◀────────│  (stateless)         │◀────────│  (quote, trade,  │
│             │  JSON   │                      │  JSON   │   content, etc.) │
└─────────────┘         └──────────────────────┘         └──────────────────┘
                               │
                               ▼
                        ┌──────────────┐
                        │  Longbridge  │
                        │  OAuth       │
                        │  Server      │
                        └──────────────┘
```

## Design Principles

1. **Stateless** — No sessions, no database. Each request carries a Bearer token; the server creates SDK contexts on the fly and discards them after use.

2. **Direct OAuth** — The server does not proxy OAuth. It publishes [RFC 9728](https://datatracker.ietf.org/doc/html/rfc9728) Protected Resource Metadata pointing MCP clients directly to Longbridge's OAuth authorization server.

3. **Streaming JSON transformation** — Responses are transformed (snake_case, timestamp conversion, counter_id mapping) during serialization via a custom `serde::Serializer` wrapper, avoiding intermediate allocations.

## Request Lifecycle

```
1. MCP Client sends POST /mcp with Authorization: Bearer <longbridge_token>

2. Auth middleware extracts token, stores as BearerToken in request extensions

3. rmcp StreamableHttpService routes to the appropriate tool handler

4. Tool handler:
   a. Extracts McpContext (token + Accept-Language) from request
   b. Creates Config via OAuth::from_token(token)
   c. Creates QuoteContext / TradeContext / HttpClient as needed
   d. Calls the Longbridge SDK or HTTP API
   e. Serializes the response through TransformSerializer
   f. Returns CallToolResult with transformed JSON
   g. Config and contexts are dropped (connections closed)

5. Response flows back through rmcp → axum → MCP Client
```

## Module Structure

```
src/
├── main.rs                 Entry point, CLI/config, server startup
├── error.rs                Unified error type (thiserror)
├── counter.rs              Symbol ↔ counter_id bidirectional conversion
├── metrics.rs              Prometheus metrics and /metrics handler
│
├── auth/
│   ├── mod.rs              Router composition, AppState, MCP service wiring
│   ├── metadata.rs         /.well-known/oauth-protected-resource (RFC 9728)
│   └── middleware.rs       Bearer token extraction middleware
│
├── serialize/
│   ├── mod.rs              Public API: to_tool_json(), transform_json()
│   ├── transform.rs        TransformSerializer + compound type wrappers
│   ├── timestamp.rs        TimestampSerializer (_at fields → RFC 3339)
│   └── counter_id.rs       CounterIdSerializer (counter_id → symbol)
│
└── tools/
    ├── mod.rs              McpContext, #[tool_router], forwarding layer
    ├── parse.rs            Parameter parsing helpers
    ├── http_client.rs      Shared HTTP request helpers
    ├── quote.rs            SDK QuoteContext tools (29)
    ├── trade.rs            SDK TradeContext tools (14)
    ├── fundamental.rs      HTTP fundamental data tools (18)
    ├── market.rs           HTTP market data tools (9)
    ├── content.rs          SDK ContentContext + HTTP content tools (8)
    ├── alert.rs            HTTP price alert tools (5)
    ├── portfolio.rs        HTTP portfolio tools (3)
    ├── statement.rs        HTTP account statement tools (2)
    └── calendar.rs         HTTP finance calendar tool (1)
```

## Authentication

The server implements the **resource server** role from the MCP OAuth 2.1 spec.

```
MCP Client                        MCP Server                    Longbridge OAuth
    │                                  │                              │
    ├─ POST /mcp (no token) ──────────▶│                              │
    │◀── 401 + WWW-Authenticate ───────┤                              │
    │    (resource_metadata URL)       │                              │
    │                                  │                              │
    ├─ GET /.well-known/               │                              │
    │   oauth-protected-resource ─────▶│                              │
    │◀── { authorization_servers:      │                              │
    │      ["https://openapi..."] } ───┤                              │
    │                                  │                              │
    ├─ OAuth flow directly with ───────┼─────────────────────────────▶│
    │  Longbridge (PKCE, etc.)         │                              │
    │◀── access_token ─────────────────┼──────────────────────────────┤
    │                                  │                              │
    ├─ POST /mcp + Bearer token ──────▶│                              │
    │                                  ├─ SDK/HTTP calls ────────────▶│
    │◀── MCP response ─────────────────┤◀─────────────────────────────┤
```

The server never sees or stores user credentials. Each Bearer token is used to construct a throwaway `Config` via `OAuth::from_token()`.

## McpContext

Every tool call receives an `McpContext` struct extracted from the HTTP request:

```rust
pub struct McpContext {
    pub token: String,             // Longbridge access token
    pub language: Option<String>,  // Accept-Language header
}
```

`McpContext` provides factory methods that encapsulate SDK configuration:

- `create_config()` → `Arc<Config>` with language, overnight trading enabled
- `create_http_client()` → authenticated `HttpClient` for `/v1/*` API calls

This struct is the single point of extension for future per-request context (e.g., region, feature flags).

## JSON Response Transformation

All tool responses pass through a custom `serde::Serializer` wrapper that performs three transformations in a single serialization pass:

| Transformation | Example |
|---------------|---------|
| Field names → snake_case | `lastDone` → `last_done` |
| `*_at` fields (i64) → RFC 3339 | `1700000000` → `2023-11-14T22:13:20Z` |
| `counter_id` → `symbol` | `ST/US/TSLA` → `TSLA.US` |
| `counter_ids` → `symbols` | `["ST/US/TSLA"]` → `["TSLA.US"]` |

Two entry points:

- **`to_tool_json(value)`** — For SDK types that implement `Serialize`. Zero intermediate allocation.
- **`transform_json(bytes)`** — For raw HTTP JSON responses. Uses `serde_transcode` for streaming token-by-token transformation without parsing into `serde_json::Value`.

## Tool Categories

| Module | Count | Data Source | Description |
|--------|-------|-------------|-------------|
| `quote` | 29 | SDK `QuoteContext` | Quotes, candlesticks, depth, options, warrants, watchlists, capital flow |
| `trade` | 14 | SDK `TradeContext` | Orders, positions, balance, executions, margin |
| `fundamental` | 18 | HTTP `/v1/quote/*` | Financial reports, ratings, valuations, company info |
| `market` | 9 | HTTP `/v1/quote/*` | Broker holdings, A/H premium, anomalies, index constituents |
| `content` | 8 | SDK + HTTP | News, topics, filings, community posts |
| `alert` | 5 | HTTP `/v1/notify/*` | Price alert CRUD |
| `portfolio` | 3 | HTTP `/v1/portfolio/*` | Exchange rates, P&L analysis |
| `statement` | 2 | HTTP `/v1/asset/*` | Account statement listing and export |
| `calendar` | 1 | HTTP `/v1/quote/*` | Finance calendar events |

SDK tools create `QuoteContext`/`TradeContext`/`ContentContext` per request (WebSocket-based). HTTP tools use the authenticated `HttpClient` for REST calls. Both paths produce JSON that flows through the same `TransformSerializer`.

## Symbol Mapping

Longbridge HTTP APIs use an internal `counter_id` format (`ST/US/TSLA`, `ETF/US/SPY`, `IX/HK/HSI`). The MCP server converts between this and the user-facing symbol format (`TSLA.US`, `SPY.US`, `HSI.HK`):

- **Request path**: `symbol_to_counter_id()` converts tool input parameters before HTTP calls
- **Response path**: `TransformSerializer` automatically renames `counter_id` → `symbol` and converts values

ETF detection uses an embedded list of ~4,500 US ETF symbols compiled into the binary at build time.

## Metrics

Prometheus metrics are exposed at `GET /metrics`:

| Metric | Type | Labels |
|--------|------|--------|
| `mcp_tool_calls_total` | Counter | `tool_name` |
| `mcp_tool_call_duration_seconds` | Histogram | `tool_name` |
| `mcp_tool_call_errors_total` | Counter | `tool_name` |

Every tool call is wrapped with `measured_tool_call()` which records timing and error status.

## Configuration

The server reads configuration from CLI arguments (highest priority), a JSON config file (`~/.longbridge/mcp/config.json`), and environment variables. Key settings:

| Setting | Purpose |
|---------|---------|
| `bind` | Listen address (default: `127.0.0.1:8000`) |
| `base_url` | Public URL for OAuth metadata (**required for public deployments**) |
| `tls_cert` / `tls_key` | Enable HTTPS with PEM certificate and key |
| `LONGBRIDGE_HTTP_URL` | Override Longbridge API endpoint (env var) |

## Deployment

The server is designed for containerized deployment:

- Single static binary (no runtime dependencies beyond CA certificates)
- No persistent state (no volumes needed for data)
- Horizontal scaling: any number of instances behind a load balancer
- Health check: `GET /metrics` returns 200
