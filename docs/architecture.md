# Architecture

## Overview

Longbridge MCP Server is a Rust service with no durable session state that exposes Longbridge financial data and trading capabilities through the [Model Context Protocol](https://modelcontextprotocol.io/) (MCP). It translates MCP tool calls into Longbridge SDK and HTTP API calls, handling authentication, JSON response transformation, metrics collection, and a bounded process-local quote WebSocket context cache.

```
┌─────────────┐         ┌──────────────────────┐         ┌──────────────────┐
│  MCP Client │  HTTP   │  Longbridge MCP      │  SDK /  │  Longbridge      │
│ (Claude,    │ Bearer  │  Server              │  HTTP   │  OpenAPI         │
│  etc.)      │────────▶│                      │────────▶│                  │
│             │◀────────│  (no durable state)  │◀────────│  (quote, trade,  │
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

1. **No durable server state** — No sessions and no database. Each request carries a Bearer token. HTTP and trade contexts are created on demand; quote WebSocket contexts are cached per authenticated identity inside each process with an idle TTL and maximum size so concurrent quote tools share one upstream connection.

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
   c. Reuses or creates a cached QuoteContext, or creates TradeContext / HttpClient as needed
   d. Calls the Longbridge SDK or HTTP API
   e. Serializes the response through TransformSerializer
   f. Returns CallToolResult with transformed JSON
   g. HTTP/trade contexts are dropped; cached quote contexts stay until idle TTL, token rotation, explicit eviction, or capacity pressure

5. Response flows back through rmcp → axum → MCP Client
```

## Module Structure

```
src/
├── main.rs                 Entry point, CLI/config, server startup
├── error.rs                Unified error type (thiserror)
├── counter.rs              Symbol ↔ counter_id bidirectional conversion
├── metrics.rs              Prometheus metrics and /metrics handler
├── ws_pool.rs              Cached QuoteContext pool (idle TTL, capacity eviction)
│
├── auth/
│   ├── mod.rs              Router composition, AppState, MCP service wiring
│   ├── metadata.rs         /.well-known/oauth-protected-resource (RFC 9728)
│   ├── middleware.rs       Bearer token extraction middleware
│   └── landing.html        Landing page served at `/` for browser visitors
│
├── serialize/
│   ├── mod.rs              Public API: to_tool_json(), transform_json()
│   ├── transform.rs        TransformSerializer + compound type wrappers
│   ├── timestamp.rs        TimestampSerializer (_at fields → RFC 3339)
│   └── counter_id.rs       CounterIdSerializer (counter_id → symbol)
│
└── tools/
    ├── mod.rs              McpContext, #[tool_router], forwarding layer, TOOL_ENDPOINTS
    ├── quote.rs            Quote tools (32)
    ├── fundamental.rs      Fundamental data tools (31)
    ├── trade.rs            Trade tools (15)
    ├── market.rs           Market data tools (14)
    ├── dca.rs              Dollar-cost averaging tools (9)
    ├── sharelist.rs        Community sharelist tools (8)
    ├── ipo.rs              IPO tools (7)
    ├── content.rs          News/discussion tools (6)
    ├── alert.rs            Price alert tools (5)
    ├── screener.rs         Stock screener tools (5)
    ├── portfolio.rs        Portfolio tools (3)
    ├── atm.rs              ATM/bank card tools (3)
    ├── macrodata.rs        Macroeconomic indicator tools (2)
    ├── search.rs           Search tools (2)
    ├── statement.rs        Account statement tools (2)
    ├── authenticate.rs     Self-service OAuth code exchange tool (1)
    ├── calendar.rs         Finance calendar tool (1)
    ├── quant.rs            Quant indicator script tool (1)
    │
    ├── output/             Typed output schemas for tools with a known post-transform shape
    │   ├── mod.rs
    │   ├── account.rs
    │   ├── discovery.rs
    │   ├── fundamental.rs
    │   ├── market.rs
    │   ├── quote.rs
    │   └── social.rs
    │
    └── support/            Shared plumbing, not MCP tools themselves
        ├── mod.rs
        ├── http_client.rs  Shared HTTP request helpers
        ├── parse.rs        Parameter parsing helpers
        └── tolerant.rs     Lenient deserializers for loosely-typed client input
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

## MCP Schema Resources

Tool descriptors keep typed `outputSchema` values where available so OpenAI Apps
and other validating MCP clients can reason about structured results. To reduce
`tools/list` payload size, the descriptor schemas are compacted by stripping
documentation-only JSON Schema keys (`$schema`, `title`, and `description`).
For tools that already declare an `outputSchema`, top-level tool descriptions
are also compacted by removing duplicated return field lists and keeping the
exposed description under 240 characters.

The full verbose schemas are exposed as MCP resources using the Longbridge
resource scheme:

```
lb://tools/{tool_name}/output-schema
```

For example, `lb://tools/depth/output-schema` returns the complete JSON Schema
for the `depth` tool, including field descriptions.

## Tool Categories

| Module | Count | Data Source | Description |
|--------|-------|-------------|-------------|
| `quote` | 32 | SDK `QuoteContext` + HTTP `/v1/quote/*` | Quotes, candlesticks, depth, brokers, options, warrants, watchlists, capital flow |
| `fundamental` | 31 | HTTP `/v1/quote/*` | Financial reports/statements, ratings, valuations, company info, shareholders, corporate actions |
| `trade` | 15 | SDK `TradeContext` | Orders, positions, balance, executions, margin |
| `market` | 14 | HTTP `/v1/quote/*` | Broker holdings, A/H premium, anomalies, top movers, short trades, index constituents |
| `dca` | 9 | HTTP `/v1/dailycoins/*` | Dollar-cost averaging plan CRUD, execution history, statistics |
| `sharelist` | 8 | HTTP `/v1/sharelists/*` | Community sharelist CRUD, member management, popular lists |
| `ipo` | 7 | HTTP `/v1/ipo/*` | IPO subscriptions, calendar, listed stocks, profit/loss analysis |
| `content` | 6 | SDK `ContentContext` | News, discussion topic CRUD and replies |
| `alert` | 5 | HTTP `/v1/notify/*` | Price alert CRUD |
| `screener` | 5 | HTTP `/v1/quote/*` | Stock screener search, indicators, strategy recommendation/management |
| `portfolio` | 3 | HTTP `/v1/portfolio/*` + `/v1/asset/*` | Exchange rates, P&L analysis |
| `atm` | 3 | HTTP `/v1/account/*` | Bank cards, withdrawal/deposit records |
| `macrodata` | 2 | SDK `FundamentalContext` | Macroeconomic indicator list and detail data |
| `search` | 2 | HTTP `/v1/search/*` | News search, community topic search |
| `statement` | 2 | SDK `AssetContext` | Account statement listing and export |
| `authenticate` | 1 | OAuth authorization-code exchange | Self-service auth for MCP-native clients that can't complete a browser redirect |
| `calendar` | 1 | HTTP `/v1/quote/*` | Finance calendar events |
| `quant` | 1 | HTTP `/v1/quant/*` | Run a quant indicator script against historical K-line data server-side |
| `utility` | 1 | none | Current UTC time |

SDK tools create `QuoteContext`/`TradeContext`/`ContentContext`/`AssetContext`/`FundamentalContext` per request (`QuoteContext` is WebSocket-based and cached; the rest are per-request). HTTP tools use the authenticated `HttpClient` for REST calls. Both paths produce JSON that flows through the same `TransformSerializer`.

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
| `LONGBRIDGE_MCP_QUOTE_WS_IDLE_TTL_SECS` | Idle TTL for cached quote WebSocket contexts (default: 600) |
| `LONGBRIDGE_MCP_QUOTE_WS_MAX_CONTEXTS` | Maximum cached quote WebSocket contexts per process (default: 1024) |

## Deployment

The server is designed for containerized deployment:

- Single static binary (no runtime dependencies beyond CA certificates)
- No persistent state; the process-local quote WebSocket cache is bounded and disposable
- Horizontal scaling: any number of instances behind a load balancer
- Health check: `GET /metrics` returns 200

### CN public-domain network path

The CN public domains share the Shenzhen public ingress path. At the time this
topology was verified, both `mcp.longbridge.cn` and `openapi.longbridge.cn`
resolved through `traefik-sz-public.lbkrs.com`. The MCP IngressRoute sends
`mcp.longbridge.cn` traffic through Traefik to the MCP ClusterIP service on
port 80, which targets MCP containers on port 8000.

```text
MCP client
    |
    | HTTPS mcp.longbridge.cn
    v
CN public edge / Shenzhen ALB
    |
    v
Traefik IngressRoute (longbridge-mcp-sg)
    |
    v
MCP ClusterIP service:80 -> MCP container:8000
    |
    | HTTPS openapi.longbridge.cn
    v
The same CN public edge / Shenzhen ALB
    |
    v
OpenAPI ingress and application
```

This second traversal is intentional, but ALB routing metadata from the first
traversal must not be replayed. In particular, Alibaba Cloud ALB adds
`ALICLOUD-ALB-TRACE` for loop detection. Forwarding that header from the client
request into the MCP server's upstream OpenAPI request can repeat the same rule
trace or exceed the trace-chain limit. ALB then returns HTTP 463 before the
request reaches the OpenAPI application. `collect_headers` therefore treats
`ALICLOUD-ALB-TRACE` as hop-specific and removes it at the MCP-to-OpenAPI
boundary.

The failure mode was verified against
`openapi.longbridge.cn/v1/quote/market-status`: changing only an
`ALICLOUD-ALB-TRACE` comma-separated chain from 16 values to 17 changed the
response from the OpenAPI application's HTTP 401 JSON response to an empty HTTP
463 response. Equally long control and `X-Forwarded-For` headers still reached
OpenAPI and returned 401. OpenAPI application logs also contained no entry for
the affected 463 requests, locating the rejection at the public edge rather
than in the application.

If the CN DNS, CDN, ALB, or ingress layout changes, re-check this path before
removing the filter. The invariant is that load-balancer-generated tracing and
loop-detection headers belong to one proxy hop and must not be forwarded as
end-user headers.
