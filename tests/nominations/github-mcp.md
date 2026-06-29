# GitHub MCP Registry Nomination

- **目标**：让 `com.longbridge/mcp` 出现在 https://github.com/mcp 的策展列表
- **触发时机**：Tier 1 #1（Official MCP Registry）上线之后
- **发送方式**：email
- **收件人**：`partnerships@github.com`
- **状态**：审核中（2026-04-23 发出，GitHub 工单 #143845，2026-04-24 回执确认受理）

## 前置检查（发信前逐条确认）

- [x] 官方 registry 已上线（2026-04-23 起 0.1.7 `isLatest: true`）：
      ```
      curl -s "https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions" \
        | jq '.servers[] | {version: .server.version, status: ._meta."io.modelcontextprotocol.registry/official".status, isLatest: ._meta."io.modelcontextprotocol.registry/official".isLatest}'
      ```
- [x] `https://mcp.longbridge.com` 外网可达，OAuth 元数据 200
- [x] `https://github.com/longbridge/longbridge-mcp` 是 public 仓库
- [x] README 顶部有 "Connect from an MCP client" 段（Claude Desktop / Claude Code / Cursor / Cline）+ 独立 Self-hosting 段
- [x] `docs/logo.png` 400×400，README 顶部引用

## 邮件正文

```
To: partnerships@github.com
Subject: MCP Registry nomination — Longbridge (com.longbridge/mcp)

Hi GitHub MCP Registry team,

We'd like to nominate the Longbridge MCP server for inclusion in the
GitHub MCP Registry.

- Name (official registry): com.longbridge/mcp
- Publisher: Longbridge (https://longbridge.com)
- Source: https://github.com/longbridge/longbridge-mcp
- Remote endpoint: https://mcp.longbridge.com
- Auth: OAuth 2.1 (RFC 9728 protected resource metadata)
- Scope: 110 tools across quotes, options, trading, fundamentals, calendars,
  price alerts, DCA plans, portfolio analytics, community sharelists for
  US & HK markets.
- Official registry entry:
  https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions

Happy to provide anything else you need.

Thanks,
<你的名字> / Longbridge
```

## 发送后

- 发送日期：`2026-04-23`
- 回执确认：`2026-04-24`，GitHub 工单号 `#143845`
- 清单 Tier 1 #3 状态：`审核中`
- 收录后再改成 `已上线` + `✅`
- 若两周（~2026-05-07）内无进一步回复，回信跟进同一工单
- 若仍无响应，考虑通过 Anthropic / Claude Code 渠道间接引荐
