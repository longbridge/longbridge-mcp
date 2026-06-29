# Publishing to MCP registries

本文档描述如何把 `longbridge-mcp`（已部署在
`https://mcp.longbridge.com`）注册到两个主流 MCP registry。

## 目标 registry

- **官方**：<https://registry.modelcontextprotocol.io/> — 由
  `modelcontextprotocol/registry` 维护，是社区事实标准。
- **第三方**：<https://mcpmarket.com/> — 目录站点，通常会自动同步官方
  registry；如需独立登记，可通过其页面的 "Submit" 入口或 GitHub 仓库
  issue 提交。

## 前置条件

| 项目 | 当前值 |
|------|--------|
| 公网 endpoint | `https://mcp.longbridge.com` |
| Transport | `streamable-http`（rmcp）|
| 鉴权 | Bearer token（Longbridge OAuth）|
| 源码仓库 | `https://github.com/longbridge/longbridge-mcp` |
| 企业域名 | `longbridge.com`（用于 DNS 验证时需要）|

---

## 一、提交到官方 registry

### 1. 安装 CLI

```bash
curl -L "https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_$(uname -s | tr '[:upper:]' '[:lower:]')_$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/').tar.gz" \
  | tar xz mcp-publisher && sudo mv mcp-publisher /usr/local/bin/
mcp-publisher --version
```

### 2. 创建 `server.json`

放在仓库根目录。我们是 remote HTTP 服务，用 `remotes[]` 而不是
`packages[]`：

```json
{
  "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
  "name": "com.longbridge/longbridge-mcp",
  "description": "Longbridge OpenAPI MCP — market data, trading, fundamentals across HK/US/CN/SG.",
  "version": "0.1.6",
  "repository": {
    "url": "https://github.com/longbridge/longbridge-mcp",
    "source": "github"
  },
  "remotes": [
    {
      "type": "streamable-http",
      "url": "https://mcp.longbridge.com",
      "headers": [
        {
          "name": "Authorization",
          "description": "Bearer <Longbridge OpenAPI access token>. See https://open.longportapp.com/en/docs/getting-started",
          "isRequired": true,
          "isSecret": true
        }
      ]
    }
  ]
}
```

字段要点：
- `name`：官方 registry 约定为**反向 DNS**。用 `longbridge.com` DNS 验证对应的 name 必须是 `com.longbridge/<server-name>`；GitHub 验证则用 `io.github.<user-or-org>/<server-name>`。见下一节。
- `version`：每次发布必须单调递增，与 `Cargo.toml` 同步（当前 `0.1.6`）。
- `description`：**≤ 100 字符**（registry 实测会在 422 校验失败），一句话展示用。
- `headers[]`：如实声明 Bearer token，客户端接入时会提示用户填入。

> 不放心格式的话，先跑 `mcp-publisher init` 生成模板再手工替换 `packages[]` 为 `remotes[]`，能看到官方默认的字段形状。
>
> **本地试跑要用 placeholder name**：`mcp-publisher validate` / `publish` 会直接打到
> `registry.modelcontextprotocol.io`，别用真实的 `com.longbridge/*` 做调试，改成
> `com.example/longbridge-mcp-test` 之类，确认无误再换回真实名字发布。

### 3. 选 namespace + 认证

`name` 字段的前缀决定认证方式，任选其一：

| 策略 | `name` 示例 | 认证方式 | 谁能做 |
|------|-------------|---------|--------|
| **DNS（推荐）** | `com.longbridge/longbridge-mcp` | 在 `longbridge.com` 发布 ed25519 公钥 TXT，用私钥签发 token | 能改公司 DNS 的人 |
| **GitHub org** | `io.github.longbridge/longbridge-mcp` | GitHub OAuth，登录者需是 `longbridge` org 成员 | Org maintainer |
| **GitHub user（过渡）** | `io.github.<your-user>/longbridge-mcp` | 个人 GitHub OAuth | 谁都行，但名字不权威 |

DNS 方式最稳（name 直接对应公司域名，不依赖 GitHub org 成员身份），但流程比 GitHub 多两步。

#### 3a. DNS 验证流程

MCP registry 的 DNS 验证不是简单的 TXT 挑战应答，而是**密码学签名**：你要自己生成 ed25519 密钥对，把公钥挂到 DNS TXT，本地用私钥给时间戳签名换取 token。

1. **生成 ed25519 密钥对**（任选一种工具）：

   ```bash
   # openssl 方式
   openssl genpkey -algorithm Ed25519 -out /tmp/mcp-ed25519.pem
   # 私钥 hex（mcp-publisher 需要这个）
   openssl pkey -in /tmp/mcp-ed25519.pem -outform DER | tail -c 32 | xxd -p | tr -d '\n'
   # 公钥 base64（DNS TXT 里用）
   openssl pkey -in /tmp/mcp-ed25519.pem -pubout -outform DER | tail -c 32 | base64
   ```

2. **在 `longbridge.com` 域的 DNS 管理后台加一条 TXT 记录**（host 指向根域，即 `@`）：

   ```
   v=MCPv1; k=ed25519; p=<上一步得到的 base64 公钥>
   ```

   > 源码实测：server 端用 `net.LookupTXT(domain)` 读取，支持 apex；支持子域名（`allowSubdomains=true`）。

3. **等 DNS 生效**（几分钟到几十分钟，视 TTL）。可用 `dig longbridge.com TXT +short` 自查。

4. **登录**：

   ```bash
   mcp-publisher login dns \
     --domain longbridge.com \
     --private-key <步骤 1 的私钥 hex>
   # 可选 --algorithm ecdsap384（默认 ed25519）
   ```

5. 保管好私钥：只要 TXT 还挂着对应公钥，任何人拿到这把私钥就能以 `com.longbridge/*` 的身份发包。建议放到 secrets manager。

#### 3b. GitHub 验证流程

```bash
mcp-publisher login github
# 浏览器跳转 GitHub 做 device code 授权；允许发布的 name 前缀是
# io.github.<登录者 username> 或 io.github.<用户所属 org>。
```

### 4. 先用 placeholder 本地校验

在 `server.json` 同目录临时把 `name` 改成 `com.example/longbridge-mcp-test`，跑：

```bash
mcp-publisher validate        # 打到 registry 做 schema 校验，但不写库
```

常见失败：`description > 100` 字符、字段缺失、`$schema` URL 错等。修完再把 `name` 改回 `com.longbridge/longbridge-mcp`。

### 5. 发布

```bash
mcp-publisher publish       # 读取当前目录的 server.json
```

失败会在 stderr 上打印原因（常见：name 与认证不匹配、version 已存在、
schema 校验错）。

### 6. 验证

```bash
curl -s "https://registry.modelcontextprotocol.io/v0/servers?search=longbridge" \
  | jq '.servers[] | select(.server.name == "com.longbridge/mcp")'
```

返回应包含刚发布的条目。也可以去
<https://registry.modelcontextprotocol.io/> 搜索 UI 确认。

### 7. 版本迭代

后续改了 `server.json` 或源码：
1. bump `version`（语义化版本）。
2. 再跑 `mcp-publisher publish`。

旧版本会被保留，客户端按 `name` 取最新。

### 8. 快速命令（下一次发版直接复制）

Ed25519 私钥默认放在 `~/.ssh/mcp-publisher.pem`（按实际位置改）。
JWT 一般 1 小时就过期，`publish` 报 `401 token is expired` 就重跑一次
`login dns` 即可。

```bash
cd /Users/hogan/work/longbridge/longbridge-mcp

PRIVATE_KEY=$(openssl pkey -in ~/.ssh/mcp-publisher.pem -noout -text \
  | grep -A3 '^priv:' | tail -n +2 | tr -d ' :\n')

mcp-publisher login dns --domain longbridge.com --private-key "$PRIVATE_KEY"
mcp-publisher publish

# 验证上线
curl -s "https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions" \
  | jq '.servers[] | {version: .server.version,
                      status: ._meta."io.modelcontextprotocol.registry/official".status,
                      isLatest: ._meta."io.modelcontextprotocol.registry/official".isLatest}'
```

> 发版前常见需要同步改的字段：
> - `server.json` 的 `version`（与 `Cargo.toml` 对齐）
> - `packages[0].identifier` 里镜像 tag（例如 `ghcr.io/longbridge/longbridge-mcp:0.1.7`）
> - 对应镜像要在 GHCR 上 push 好，并带 `LABEL io.modelcontextprotocol.server.name="com.longbridge/mcp"`

---

## 二、提交到 MCPMarket

提交入口：<https://mcpmarket.com/submit>

建议两条路径都走：

1. **先完成官方 registry** — 很多目录站（包括 MCPMarket）会自动同步
   官方 registry，官方注册成功后 1–3 天内通常会被动收录，无需额外
   操作。
2. **再走 MCPMarket 自己的 submit 表单**，以保证露出时间 + 可以自定义
   logo / 分类 / 介绍。打开 <https://mcpmarket.com/submit>，按表单填写
   以下信息（字段名以页面为准，下列为一份标准答案模板）：

   | 表单字段 | 建议值 |
   |---------|--------|
   | Name | `longbridge-mcp` |
   | Display name | `Longbridge MCP` |
   | Description | 同 `server.json.description`（一句话，英文优先）|
   | Long description / README | 贴仓库 README 或提交时 import URL |
   | Homepage | `https://github.com/longbridge/longbridge-mcp` |
   | Endpoint / Server URL | `https://mcp.longbridge.com` |
   | Transport | `streamable-http` |
   | Auth | Bearer token（附 Longbridge OpenAPI 文档链接）|
   | Category / Tags | `Finance` / `Trading` / `Market Data` / `Stock` |
   | Logo | Longbridge 品牌 logo（建议 ≥ 256×256 PNG）|
   | Contact email | 用品牌官方邮箱 |

   若页面额外要求上传 `server.json`，直接用上面那份即可。

---

## 参考

- 官方 registry 仓库：<https://github.com/modelcontextprotocol/registry>
- 发布 quickstart：
  <https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx>
- `server.json` schema：
  <https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json>
- Remote server 字段说明：
  <https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md>
