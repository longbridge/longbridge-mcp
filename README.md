<p align="center">
  <img src="https://raw.githubusercontent.com/longbridge/longbridge-mcp/main/docs/logo.png" alt="Longbridge" width="120" height="120">
</p>

<h1 align="center">Longbridge MCP Server</h1>

<p align="center">
  <a href="https://chatgpt.com"><img alt="ChatGPT App" src="https://img.shields.io/badge/ChatGPT-App-10a37f?logo=openai&logoColor=white"></a>
  <a href="https://claude.ai/settings/connectors"><img alt="Claude Connector" src="https://img.shields.io/badge/Claude-Connector-d97757?logo=claude&logoColor=white"></a>
  <a href="https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions"><img alt="Official MCP Registry" src="https://img.shields.io/badge/MCP%20Registry-com.longbridge%2Fmcp-0a66c2"></a>
  <a href="https://smithery.ai/servers/longbridge-official/longbridge-mcp"><img alt="Smithery" src="https://smithery.ai/badge/longbridge-official/longbridge-mcp"></a>
  <a href="https://lobehub.com/mcp/longbridge-longbridge-mcp"><img alt="LobeHub" src="https://lobehub.com/badge/mcp/longbridge-longbridge-mcp"></a>
  <a href="https://glama.ai/mcp/servers/longbridge/longbridge-mcp"><img alt="longbridge-mcp MCP server" src="https://glama.ai/mcp/servers/longbridge/longbridge-mcp/badges/score.svg"></a>
  <a href="https://github.com/longbridge/longbridge-mcp/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-blue"></a>
  <a href="https://longbridge.com"><img alt="Longbridge" src="https://img.shields.io/badge/brokerage-Longbridge-ffe000?labelColor=000"></a>
</p>

Official MCP server for the [Longbridge](https://longbridge.com) brokerage. **163 tools** across real-time quotes, options, order routing, fundamentals, analyst ratings, calendars, IPO, price alerts, DCA plans, grid trading, portfolio analytics and community sharelists — covering **US and HK markets**. Built with Rust using [rmcp](https://github.com/anthropics/rmcp) and [axum](https://github.com/tokio-rs/axum).

---

<h2 align="center">Now live in ChatGPT and Claude</h2>

<p align="center">
  <b>Longbridge is officially listed in the ChatGPT Apps directory and the Claude Connectors directory.</b><br>
  Talk to the markets in plain language — quotes, options, fundamentals, and your own portfolio —<br>
  with no config files to edit and no tokens to paste.
</p>

| | Add it in one place | Then just ask |
|---|---|---|
| **ChatGPT** | Settings → **Apps & Connectors** → add **Longbridge** | *"How's NVDA trading today?"* · *"Show my HK positions"* |
| **Claude** | Settings → **Connectors** → add **Longbridge** (web · desktop · mobile) | *"Compare AAPL and MSFT valuations"* · *"Any IPOs this week?"* |

Sign in once with your Longbridge account. Every request runs over the same hosted, OAuth 2.1–secured endpoint documented below — read-only market data plus full account, portfolio, and trading tools, all gated by your own credentials.

---

## Highlights

- **163 tools, one endpoint** — quotes, options, order routing, fundamentals, analyst research, screeners, IPO, alerts, DCA, grid trading and portfolio analytics across **US and HK markets**.
- **Stateless by design** — every request forwards its Bearer token straight to the Longbridge SDK. No sessions, no database, nothing stored server-side.
- **OAuth 2.1, auto-discovered** — RFC 9728 protected-resource and RFC 8414 authorization-server metadata; clients complete the flow with no token to paste.
- **Clean, typed responses** — snake_case fields, RFC 3339 timestamps, human-readable symbols, and typed response schemas available as MCP resources.

Built in Rust with [rmcp](https://github.com/anthropics/rmcp) and [axum](https://github.com/tokio-rs/axum).

## Filter tool responses with jq

Every tool accepts an optional `_jq` string in its arguments. The expression runs
on the complete returned JSON, after the normal response serialization. The `_jq`
name is reserved for response filtering to avoid conflicts with business parameters.
Usage guidance is sent once in the MCP `initialize` response's `instructions`;
each tool schema declares only the optional parameter name and type.
For example:

```json
{
  "name": "quote",
  "arguments": {
    "symbols": ["AAPL.US", "MSFT.US"],
    "_jq": "map({symbol, last_done})"
  }
}
```

Use `.data[:5]` to take the first five entries of a `data` array,
`.data | map(select(.price > 10))` to select rows, or `{total: .total}` to
project fields. Expressions use the embedded [jaq](https://github.com/01mf02/jaq)
engine's jq-compatible syntax; no separate `jq` executable is needed.

- Omit `_jq` (or pass `null`) to preserve the original response.
- One output value is returned directly, multiple values as an array, and no
  values as `[]`. Scalars and arrays are JSON text; objects also appear in
  `structuredContent`, containing only the filtered fields.
- Plain text responses are available as JSON strings. Multiple content blocks
  without structured content are available as an array.
- Tool errors and permission/no-data explanations remain unfiltered.
- Empty, invalid, or non-string expressions are rejected before the tool runs.
  If filtering fails at runtime, the response explicitly says the tool already
  executed. Do not automatically retry writes such as placing an order.
- Environment access, filesystem imports, and logging filters are unavailable.
  Output is limited to 10,000 values and 8 MiB; exceeding a limit returns an
  error rather than a partial result.

Because filters can change the response shape, tools do not advertise a fixed
`outputSchema`. Original typed schemas remain available through `resources/list`
and `resources/read` at `lb://tools/{tool-name}/output-schema` for schema-backed tools.

## Connect your own client

Longbridge runs a hosted endpoint at **`https://mcp.longbridge.com`** — point any MCP client at it and complete OAuth when prompted. Authorization is auto-discovered via RFC 9728; there is no token to paste.

**Claude Code**

```bash
claude mcp add --transport http longbridge https://mcp.longbridge.com
```

**Claude Desktop** — add to `claude_desktop_config.json`, then restart:

```json
{ "mcpServers": { "longbridge": { "url": "https://mcp.longbridge.com" } } }
```

**Cursor · Cline · Windsurf · Zed · other clients** — point them at `https://mcp.longbridge.com` with transport `streamable-http`.

<details>
<summary>More Claude Code commands</summary>

```bash
# Local self-hosted instance (see Self-hosting below)
claude mcp add --transport http longbridge-local http://localhost:8000/mcp

claude mcp list                  # registered servers
claude mcp get longbridge        # config + auth status
claude mcp remove longbridge     # unregister
claude mcp logout longbridge     # re-trigger OAuth after revocation
```

On first use, the client reads the `WWW-Authenticate` challenge, fetches `/.well-known/oauth-protected-resource` (RFC 9728), and opens your browser for the Longbridge OAuth flow. Tokens are cached per session and refreshed automatically.

</details>

## The 163 tools

Twenty categories spanning market data, trading, research and account management.

| Category | Count | Coverage |
|----------|-------|----------|
| **Quote** | 32 | Real-time and historical quotes, candlesticks, depth, brokers, options, warrants, watchlists, capital flow, market temperature, short positions, option volume |
| **Fundamental** | 33 | Financial statements/reports, business segments, institutional views, industry peers/valuation, dividends, EPS forecasts, valuations & valuation comparison, company info/executives, shareholders, corporate actions, operating metrics |
| **Trade** | 14 | Order submission/cancellation/replacement, positions, balance, executions, cash flow, margin |
| **Market** | 15 | Market status, industry/top-mover rank, broker holdings, A/H premium, trade statistics, anomalies, short trades/margin, index constituents |
| **DCA** | 9 | Dollar-cost averaging plan create/update/pause/resume/stop, execution history, statistics, support check |
| **Grid** | 11 | Grid trading order submit/replace/cancel/suspend/restart, list/detail/trigger-history reads, per-symbol setup info, one-time strategy consent |
| **Sharelist** | 8 | Community sharelist CRUD, member add/remove/sort, popular lists |
| **IPO** | 7 | IPO subscriptions, calendar, listed stocks, order detail, profit/loss analysis |
| **Content** | 7 | News list/detail, discussion topic CRUD and replies |
| **Alert** | 5 | Price alert CRUD (add, delete, enable, disable, list) |
| **Screener** | 5 | Stock screener search, indicators, strategy recommendation/management |
| **Portfolio** | 4 | Exchange rates, profit/loss analysis (summary, detail, realized) |
| **ATM** | 3 | Bank cards, withdrawal records, deposit records |
| **Macrodata** | 2 | Macroeconomic indicator list and detail |
| **Search** | 2 | News search, community topic search |
| **Statement** | 2 | Account statement listing and export |
| **Calendar** | 1 | Finance calendar (earnings, dividends, IPOs, macro data, closures) |
| **Quant** | 1 | Run a quant indicator script against historical K-line data |
| **Authenticate** | 1 | OAuth code exchange for clients that can't complete a browser redirect |
| **Utility** | 1 | Current UTC time |

## Self-hosting

Prefer your own instance? Run the published image:

```bash
docker run -p 8443:8443 \
  -v /path/to/certs:/certs:ro \
  ghcr.io/longbridge/longbridge-mcp \
  --bind 0.0.0.0:8443 \
  --base-url https://mcp.example.com \
  --tls-cert /certs/cert.pem \
  --tls-key /certs/key.pem
```

> **Set `--base-url`** to your externally reachable URL on any public deployment — it is published in the OAuth metadata clients use to discover the authorization server. It defaults to `http://localhost:{port}`, which remote clients cannot use.

Or build from source: `cargo build --release && ./target/release/longbridge-mcp`.

<details>
<summary>Configuration &amp; environment variables</summary>

Config lives at `~/.longbridge/mcp/config.json` (override the directory with `LONGBRIDGE_MCP_CONFIG_DIR`). CLI flags take precedence. When `tls_cert` and `tls_key` are both set the server runs HTTPS, otherwise HTTP; `base_url` defaults to `https://localhost:{port}` with TLS or `http://localhost:{port}` without.

| Option | Config Key | CLI Flag | Default | Description |
|--------|-----------|----------|---------|-------------|
| Bind address | `bind` | `--bind` | `127.0.0.1:8000` | HTTP server listen address |
| Base URL | `base_url` | `--base-url` | auto | Public base URL for resource metadata |
| Log directory | `log_dir` | `--log-dir` | *(stderr)* | Directory for rolling log files |
| TLS certificate | `tls_cert` | `--tls-cert` | *(none)* | PEM certificate file for HTTPS |
| TLS private key | `tls_key` | `--tls-key` | *(none)* | PEM private key file for HTTPS |
| Canary upstream | `canary` | `--canary` | `false` | Talk to the Longbridge canary environment (`*.longbridge.xyz`) instead of production. `--canary=false` forces production even when the config file enables it |

**Upstream endpoints** are fixed by the mode, not by the environment:

| | Production (default) | Canary (`--canary`) |
|---|---|---|
| OpenAPI | `https://openapi.longbridge.com` | `https://openapi-global.longbridge.xyz` |
| Quote WebSocket | `wss://openapi-quote.longbridge.com/v2` | `wss://openapi-global-quote.longbridge.xyz/v2` |
| Trade WebSocket | `wss://openapi-trade.longbridge.com/v2` | `wss://openapi-global-trade.longbridge.xyz/v2` |
| OAuth / connect page | `openapi.longbridge.com` / `open.longbridge.com` | `openapi-global.longbridge.xyz` / `open.longbridge.xyz` |

Canary uses the `-global` gateway, not `openapi.longbridge.xyz`: only the former is CloudFront-fronted and performs `x-dc-region` data-center routing, which this server depends on to serve `us_`- and `ap_`-prefixed credentials from one process.

All three are set explicitly on the SDK, so `LONGBRIDGE_HTTP_URL`, `LONGBRIDGE_QUOTE_WS_URL`, `LONGBRIDGE_TRADE_WS_URL`, their `LONGPORT_*` aliases, `LONGBRIDGE_REGION`, and a `.env` file are all inert — as is the SDK's geolocation probe, which means the `openapi.longbridge.cn` access point is never selected. Which data center serves a request is unaffected: that is decided by the `x-dc-region` header the SDK derives from the credential's `us_` / `ap_` prefix.

Advanced environment variables — most deployments never touch these; they exist for SDK debugging and edge/global-entry deployments.

| Variable | Default | Description |
|----------|---------|-------------|
| `LONGBRIDGE_MCP_CONFIG_DIR` | `~/.longbridge/mcp` | Config file directory |
| `LONGBRIDGE_PUBLIC_HOSTS` | *(none)* | Comma-separated hostnames accepted from the edge-injected `X-Host` header; matching requests echo that host in the 401 challenge / RFC 9728 metadata. Unset = `X-Host` ignored |
| `LONGBRIDGE_GLOBAL_OAUTH_URL` | *(none)* | Authorization-server URL advertised to requests arriving via an allowlisted `X-Host` (global single-domain entry). Unset = fall back to the mode's OpenAPI base URL |
| `LONGBRIDGE_MCP_QUOTE_WS_IDLE_TTL_SECS` | `600` | Idle seconds before a cached quote WebSocket context is evicted |
| `LONGBRIDGE_MCP_QUOTE_WS_MAX_CONTEXTS` | `1024` | Maximum cached quote WebSocket contexts per server process |
| `LONGBRIDGE_MCP_LOG_PAYLOADS` | *(unset)* | `1` lifts the payload log caps (see below). Never set this in production |
| `LONGBRIDGE_LOG_PATH` | *(none)* | SDK internal log path. **Leave unset in production** — the SDK writes unfiltered request/response bodies there |

</details>

<details>
<summary>Logging &amp; customer data</summary>

MCP requests and responses carry customer data — cash balances, positions, order history — and upstream SDK frames carry access tokens. None of it belongs in a log file, so the server caps the log targets that would print it, independent of `RUST_LOG`:

| Target | Cap | What it would otherwise print |
|--------|-----|-------------------------------|
| `longbridge_httpcli` | `warn` | OpenAPI request and full response bodies (INFO) |
| `longbridge_wscli` | `warn` | Every WebSocket frame, auth token included (INFO) |
| `longbridge::trade` | `warn` | Order push events (INFO) |
| `rmcp` | `info` | Decoded MCP requests and full tool results (DEBUG), raw JSON-RPC frames (TRACE) |

So raising verbosity is safe: `RUST_LOG=debug` (or `trace`) gives you the server's own logs without leaking customer data. Two switches defeat this, both off by default — `LONGBRIDGE_MCP_LOG_PAYLOADS=1` (removes the caps; use only against a test account locally) and `LONGBRIDGE_LOG_PATH` (makes the SDK write unfiltered bodies to that directory; the server warns at startup when set).

</details>

<details>
<summary>HTTP endpoints, authentication &amp; metrics</summary>

The server expects a Longbridge OAuth access token in `Authorization: Bearer <token>`. On missing or invalid auth it returns `401` with a `WWW-Authenticate` header pointing to the protected-resource metadata, which directs clients to the Longbridge OAuth authorization server.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/.well-known/oauth-protected-resource` | Protected Resource Metadata (RFC 9728) |
| GET | `/.well-known/oauth-authorization-server` | Authorization Server Metadata (RFC 8414); advertises direct Longbridge authorize/register and proxied token/revoke endpoints |
| POST | `/oauth2/token` | OAuth token proxy; derives `x-dc-region` from the code/refresh token, defaulting to AP |
| POST | `/oauth2/revoke` | OAuth revocation proxy; derives `x-dc-region` from the token, defaulting to AP |
| GET | `/metrics` | Prometheus metrics |
| POST/GET/DELETE | `/mcp` | MCP Streamable HTTP endpoint (requires Bearer token) |

Prometheus metrics: `mcp_tool_calls_total` (counter), `mcp_tool_call_duration_seconds` (histogram), and `mcp_tool_call_errors_total` (counter) — each labelled by `tool_name`.

</details>

## Development

```bash
cargo +nightly fmt      # format
cargo clippy            # lint
cargo test              # test
```

## License

Released under the [MIT License](LICENSE).
