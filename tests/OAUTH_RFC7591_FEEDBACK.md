# Longbridge OpenAPI OAuth 动态客户端注册(RFC 7591)问题反馈

- **测试日期**: 2026-04-27
- **测试目标**: `https://openapi.longbridge.com`
- **背景**: MCP(Model Context Protocol)客户端依赖 RFC 7591 动态注册接入第三方 server。在 `longbridge-mcp` 接入时,实测发现上游 OAuth 服务器存在若干协议合规与一致性问题,影响 MCP 客户端正常对接。

## 一、测试方法

按 RFC 8414 → RFC 7591 → RFC 7592 标准流程实际跑通完整链路:

1. 拉取 AS metadata(`/.well-known/oauth-authorization-server`)
2. 向 `registration_endpoint` 发起最小注册请求(无 initial access token)
3. 用返回的 `client_id` 调用 `device_authorization_endpoint` 与 `token_endpoint`
4. 用 `registration_access_token` 走 RFC 7592 GET / DELETE

## 二、整体结论

**功能链路是通的**(开放注册可用、token endpoint 接受无 secret 的请求、RFC 7592 管理协议正常)。但 **AS metadata 与 DCR 响应的协议声明跟实际行为不一致**,导致严格遵守标准的 MCP 客户端无法对接。

## 三、关键问题

### 问题 1 — AS metadata 漏声明 `none` (P0,阻塞)

**现状**

```json
"token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post"]
```

**实测行为**

不带 `client_secret` 直接 POST `/oauth2/token`,返回 `authorization_pending`(而非 `invalid_client`),证明 token endpoint **实际支持** `none`。

**影响**

按 RFC 8414 严格实现的 MCP 客户端(包括 Claude.ai、Claude Desktop 的接入逻辑)会:

- 在 DCR 阶段就因为 `none` 不在受支持列表而拒绝注册;或
- 拼装 token 请求时强制带上 secret,然后失败。

**修复**

```diff
- "token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post"]
+ "token_endpoint_auth_methods_supported": ["none", "client_secret_basic", "client_secret_post"]
```

### 问题 2 — DCR 响应字段违反 RFC 7591 §3.2.1 (P0,协议违规)

**现状**

最小注册请求返回:

```json
{
  "client_id": "73547acd-...",
  "client_id_issued_at": 1777255956,
  "client_secret_expires_at": 1777259556,
  "client_name": "...",
  "redirect_uris": ["..."],
  "grant_types": ["authorization_code","refresh_token","urn:ietf:params:oauth:grant-type:device_code"],
  "registration_access_token": "...",
  "registration_client_uri": "..."
}
```

**违反点**

RFC 7591 §3.2.1 原文:

> `client_secret_expires_at` ... is REQUIRED if `client_secret` is issued. **Otherwise it MUST be omitted.**

响应中没有 `client_secret`,却返回了 `client_secret_expires_at`(且值为 `issued_at + 3600`),协议不允许。

**修复(任一即可)**

- **方案 A(推荐)**: 既然 Longbridge 客户端模型实际是 public client,**直接删除** `client_secret_expires_at` 字段;并显式回填 `"token_endpoint_auth_method": "none"`。
- 方案 B: 如果未来要支持机密客户端,真的下发 `client_secret`,且 `client_secret_expires_at` 应为 `0`(永不过期)或合理时长(如 1 年),不能是 1 小时。

### 问题 3 — `scopes_supported` 三处定义不一致 (P1,集成阻塞)

| 位置 | 内容 |
|---|---|
| AS metadata | `["4","5","6","7","10","11"]`(数字 ID) |
| 资源服务器 metadata(`longbridge-mcp`) | `["openapi"]` |
| DCR 注册响应 | 不回显,客户端没传时也不填默认 |

**影响**

MCP 客户端拼装授权请求时无法判定该传什么 scope,体验破碎。

**修复**

- 统一 scope 命名空间(建议用人类可读字符串,如 `quote.read`、`trade.write` 而不是数字)。
- 在 DCR 响应中回显 `scope` 字段(请求未指定时填默认值)。

### 问题 4 — DCR 响应缺少应回填的默认字段 (P2,易用性)

RFC 7591 §3.2.1 要求服务器在响应中回显客户端 metadata,**包括服务器为未指定字段填充的默认值**。当前实测缺失:

| 字段 | 期望值(public client 默认) |
|---|---|
| `token_endpoint_auth_method` | `"none"` |
| `response_types` | `["code"]` |
| `scope` | (默认 scope 字符串) |

### 问题 5 — token endpoint 错误响应混入 RFC 9728 头部 (P2,协议混淆)

**现状**

`POST /oauth2/token` 返回 4xx 时,响应头包含:

```
WWW-Authenticate: Bearer realm="https://openapi.longbridge.com/.well-known/oauth-protected-resource", error="authorization_pending", ...
```

**问题**

`WWW-Authenticate` + `resource_metadata` 是 RFC 9728 给**资源服务器**用的指引头,不属于 OAuth token endpoint(RFC 6749)的标准错误格式。语义错位,虽不破坏现有客户端,但严格的客户端可能误判端点角色。

**修复**

token endpoint 错误响应不要带 `WWW-Authenticate`,只在响应体里返回 RFC 6749 §5.2 的 `{"error": "...", "error_description": "..."}` 即可。

## 四、建议修复优先级

| 优先级 | 项 | 影响面 |
|---|---|---|
| P0 | 问题 1: AS metadata 加 `"none"` | 阻塞 MCP 接入 |
| P0 | 问题 2: DCR 响应删除 `client_secret_expires_at`、回填 `token_endpoint_auth_method: "none"` | 协议违规 |
| P1 | 问题 3: 统一 scope 命名 | 集成体验 |
| P2 | 问题 4: 回填默认字段 | 协议规范性 |
| P2 | 问题 5: 清理 token endpoint 多余头部 | 协议规范性 |

## 五、附: 实测命令(可复现)

```bash
# Step 1 — Discovery
curl -s https://openapi.longbridge.com/.well-known/oauth-authorization-server

# Step 2 — Open registration (no IAT required)
curl -X POST https://openapi.longbridge.com/oauth2/register \
  -H "Content-Type: application/json" \
  -d '{"redirect_uris":["https://example.com/callback"],"client_name":"probe"}'

# Step 3 — Token endpoint accepts the client_id without secret
curl -X POST https://openapi.longbridge.com/oauth2/token \
  -d "grant_type=urn:ietf:params:oauth:grant-type:device_code&device_code=<code>&client_id=<client_id>"
# → returns "authorization_pending", confirming client auth passed without secret

# Step 4 — RFC 7592 manage / cleanup
curl -X DELETE https://openapi.longbridge.com/oauth2/register/<client_id> \
  -H "Authorization: Bearer <registration_access_token>"
# → 204
```
