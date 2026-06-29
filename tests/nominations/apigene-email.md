# Apigene MCP Marketplace Submission

- **目标**：把 Longbridge MCP 收录到 https://apigene.ai/mcp
- **触发时机**：可立即发送
- **发送方式**：email
- **收件人**：Apigene 团队（首选 `hello@apigene.ai`，若退回换 `support@apigene.ai`）
- **状态**：未发送

## 前置检查（发信前确认）

- [x] 官方 registry 已上线：`com.longbridge/mcp` 0.1.7 `isLatest`
- [x] `https://mcp.longbridge.com` 外网可达
- [x] README + 400×400 logo 在 `main` 分支
- [x] 名称统一为 `Longbridge MCP`（参 PR / issue 一致）

## 邮件正文

```
To: hello@apigene.ai
Subject: MCP Marketplace listing request — Longbridge MCP (com.longbridge/mcp)

Hi Apigene team,

We'd like to list Longbridge MCP on apigene.ai/mcp.

- Name: Longbridge MCP
- Publisher: Longbridge (https://longbridge.com), licensed brokerage in HK / US / SG / JP / NZ
- Source: https://github.com/longbridge/longbridge-mcp
- Hosted endpoint: https://mcp.longbridge.com
  - Transport: streamable-http
  - Auth: OAuth 2.1 via RFC 9728 protected resource metadata (client auto-discovers Longbridge OAuth; no manual API keys)
- License: MIT
- Official MCP Registry entry:
  https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions
  (com.longbridge/mcp, v0.1.7 latest, isLatest: true)
- Logo (400×400 PNG):
  https://raw.githubusercontent.com/longbridge/longbridge-mcp/main/docs/logo.png

Scope: 110 tools across real-time quotes, options, order routing,
fundamentals, analyst ratings, earnings & dividend calendars, price
alerts, DCA plans, portfolio analytics and community sharelists for
US and HK markets.

Already listed on:
- mcp.so — https://mcp.so/server/longbridge/longbridge
- Cline Marketplace — https://mcpmarket.com/server/longbridge-1 (slug rename pending)
- Glama — https://glama.ai/mcp/connectors/com.longbridge/mcp
- McpMux — pending PR https://github.com/mcpmux/mcp-servers/pull/117
- LobeHub — pending issue https://github.com/lobehub/lobehub/issues/14133
- OpenTools — pending issue https://github.com/opentools/cli/issues/18

Happy to provide anything else (config schemas, tool inventories,
demo videos). Thanks!

— <你的名字> / Longbridge
```

## 发送后

- 记录发送日期：`____-__-__`
- 把 `tests/REGISTRY_CHECKLIST.md` Tier 2 #13 状态改成 `已发送`
- 收到回执后改成 `审核中`
- 若两周内无响应，回信跟进；再没回应放弃
