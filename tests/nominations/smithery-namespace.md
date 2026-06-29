# Smithery Namespace Reclaim — `longbridge`

- **目标**：把 Smithery 上空壳的 `longbridge` namespace 转给 Longbridge 团队
- **触发时机**：现在（不等 Tier 1）
- **发送方式**：email
- **收件人**：`support@smithery.ai`
- **状态**：未发送
- **临时占位**：`longbridge-official/longbridge-mcp`（已/待发布）

## 验证依据（发信前再跑一次确认没变化）

```bash
curl -sS -o /dev/null -w '%{http_code}\n' https://smithery.ai/@longbridge
# 期望：404

curl -sS 'https://registry.smithery.ai/servers?q=longbridge' \
  | jq '.servers[] | select(.qualifiedName | startswith("longbridge/"))'
# 期望：空
```

## 邮件正文

```
To: support@smithery.ai
Subject: Namespace transfer — "longbridge" (Longbridge Group)

Hi Smithery team,

We're the Longbridge brokerage (https://longbridge.com), publishing our
official MCP server. The `longbridge` namespace on Smithery exists but
has zero published servers (verified: /servers?q=longbridge returns no
`longbridge/*` qualified names; /@longbridge returns 404). It appears
to have been reserved without being used.

Could you transfer `longbridge` to our team? We're happy to verify
ownership via DNS TXT on longbridge.com or any other mechanism.

- GitHub org: https://github.com/longbridge
- My GitHub login (public member): <你的主账号用户名>
- Source: https://github.com/longbridge/longbridge-mcp
- Published meanwhile under: https://smithery.ai/@longbridge-official/longbridge-mcp
- Upstream registry: com.longbridge/mcp (DNS-verified)

Thanks,
<你的名字> / Longbridge
```

## 发送后

- 记录发送日期：`____-__-__`
- 收到回复后：
  - 批准 → 按 Smithery 指示完成 transfer，把 `longbridge-official/longbridge-mcp` 挪到 `longbridge/longbridge-mcp`
  - 驳回 → 保留 `longbridge-official` 作为最终 namespace，更新清单
