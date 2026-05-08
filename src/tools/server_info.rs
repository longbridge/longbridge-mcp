/// Returns server endpoints, capabilities overview, and quick-setup instructions.
pub fn server_info() -> String {
    r#"Longbridge MCP Server

Endpoint:      https://openapi.longbridge.com/mcp
Transport:     streamable-http
Auth:          OAuth 2.1 — auto-discovered via RFC 9728, no API key required

Capabilities: 110 tools across 11 categories
  Quote       (32): real-time/historical quotes, candlesticks, depth, options, warrants, watchlists, capital flow
  Trade       (14): order routing (submit/cancel/replace), positions, balance, executions, cash flow, margin
  Fundamental (18): financial reports, analyst ratings, dividends, EPS forecasts, valuations, corporate actions
  Market       (9): market status, broker holdings, A/H premium, index constituents, anomalies
  Content      (8): news, discussion topics, filing details
  DCA          (9): dollar-cost averaging plan lifecycle (create/update/pause/resume/stop)
  Sharelist    (8): community sharelist CRUD, member management, popular lists
  Alert        (5): price alert add/delete/enable/disable/list
  Portfolio    (3): exchange rates, P&L analysis
  Statement    (2): account statement listing and export
  Calendar     (1): earnings, dividends, IPOs, macro, market closures

Quick Setup

Claude Code
  claude mcp add --transport http longbridge https://openapi.longbridge.com/mcp

Cursor
  Add to .cursor/mcp.json (or Settings → MCP Servers):
  {
    "mcpServers": {
      "longbridge": {
        "url": "https://openapi.longbridge.com/mcp"
      }
    }
  }

Codex
  Add to codex config (codex.toml or via --mcp-server flag):
  [[mcp_servers]]
  name = "longbridge"
  url  = "https://openapi.longbridge.com/mcp"

Zed
  Add to ~/.config/zed/settings.json:
  {
    "context_servers": {
      "longbridge": {
        "url": "https://openapi.longbridge.com/mcp",
        "transport": "streamable-http"
      }
    }
  }

Cherry Studio
  In Settings → MCP Servers, click Add and enter:
    Name:      Longbridge
    URL:       https://openapi.longbridge.com/mcp
    Transport: streamable-http

On first tool use, your MCP client will open a browser for the Longbridge OAuth flow.
No API keys or manual token setup required."#
        .to_string()
}
