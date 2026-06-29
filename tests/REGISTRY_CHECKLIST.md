# MCP Registry 提交清单

`longbridge-mcp` 对外公开后，计划提交到以下 registry / 市场 / 清单。

- **提交状态**：`未开始` / `准备中` / `已提交` / `审核中` / `已上线` / `已驳回`
- **是否提交成功**：`-` / `✅` / `❌`

## 汇总（最后更新：2026-06-05）

| 指标 | 数量 |
|------|------|
| 总提交数 | 40 |
| 已上线 | 17 |
| 等待审核中 | 22 |
| 已驳回 | 1 |
| 未开始 | 7 |
| 不适用/已废弃 | 13 |

## Tier 1 — 官方 & 核心基础设施（必做）

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 1 | Official MCP Registry | https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions | CLI：`mcp-publisher publish`（DNS Ed25519 验证 + `server.json`，当前仅 remotes，packages 待下版本补 Dockerfile label） | 已上线 | ✅ |
| 2 | ~~modelcontextprotocol/servers README~~ | https://github.com/modelcontextprotocol/servers | 官方 CONTRIBUTING 明示：第三方名单已下线，统一用 Tier 1 #1。本项与 #1 重复。 | 不适用 | - |
| 3 | GitHub MCP Registry | https://github.com/mcp/com.longbridge/mcp | 邮件发 `partnerships@github.com`（2026-04-23 发出，GitHub 工单 #143845，2026-04-24 确认受理）；2026-06-24 审核通过上线 | 已上线 | ✅ |

## Tier 2 — 主流第三方市场（大流量）

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 4 | Smithery | https://smithery.ai/servers/longbridge-official/longbridge-mcp | 通过 `/.well-known/mcp/server-card.json` 跳过 scan；2026-04-27 上线，108 tools / 1 connection。`longbridge` namespace 申诉仍在排队 | 已上线 | ✅ |
| 5 | mcp.so | https://mcp.so/server/longbridge/longbridge | 网页 Submit（2026-04-24）+ GitHub issue `chatmcp/mcpso#2151` 备份 | 已上线 | ✅ |
| 6 | Glama | https://glama.ai/api/mcp/v1/servers/longbridge/longbridge-mcp | 双源完整收录：`/connectors/com.longbridge/mcp`（registry 同步）+ `/servers/longbridge/longbridge-mcp`（GitHub 爬虫，含 score badge） | 已上线 | ✅ |
| 7 | PulseMCP | https://www.pulsemcp.com/servers/longbridge | 从 Tier 1 #1 自动同步，2026-04-27 上线（提前于 ~2026-05-01 预期） | 已上线 | ✅ |
| 8 | MCP Market | https://mcpmarket.com/server/longbridge-1 | 由 Cline 运营，与 #18 同入口；上线时 slug 被占为 `longbridge-1`，已在 #1406 评论请求改为 `longbridge-mcp`（2026-04-24） | 已上线（slug 待改名） | ✅ |
| 9 | LobeHub MCP | https://lobehub.com/mcp | 网页 submit 弹窗反复 "Failed to submit MCP plugin"，改走 GitHub issue `lobehub/lobehub#14133`（2026-04-24），2026-05-14 merged | 已上线 | ✅ |
| 10 | McpMux | https://mcpmux.com/servers/com.longbridge-mcp-http | PR #117 (2026-04-24) + PR #121 (2026-05-22 merged，更新至 128 tools) | 已上线 | ✅ |
| 11 | OpenTools | https://opentools.com/ | GitHub issue `opentools/cli#18`（2026-04-24，遵循 `[Server Submission] Add ...` 模板）；2026-06-09 跟进询问所需调整 | 已提交 | - |
| 12 | mcp-marketplace.io | https://mcp-marketplace.io/server/com-longbridge-mcp | 网页 submit；2026-04-27 上线（slug `com-longbridge-mcp`） | 已上线 | ✅ |
| 13 | Apigene MCP Marketplace | https://apigene.ai/mcp | email `hello@apigene.ai`，2026-04-27 已发送；2026-06-09 再次跟进询问所需调整 | 已发送 | - |

## Tier 3 — 平台厂商策展目录（企业级）

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 14 | Docker MCP Catalog | https://github.com/docker/mcp-registry/pull/2992 | PR #2992（2026-04-27，remote 类型，格式与 stripe-remote 一致），2026-05-11 和 2026-05-21 已两次留言跟进；2026-06-09 第三次跟进询问所需调整 | 审核中 | - |
| 15 | Azure API Center MCP Registry | https://mcp.azure.com/ | Azure 账号登录 + Portal 提交（侧重企业内部目录，公域登记可选） | 未开始 | - |
| 16 | Cloudflare MCP Directory | https://developers.cloudflare.com/agents/model-context-protocol/ | 目前以 CF 自家 server 为主；第三方需走合作流程或将 Worker 版本挂上去 | 未开始 | - |
| 17 | MACH Alliance MCP Registry | https://machalliance.org/mach-alliance-mcp-registry | 合作社入会 + 条目提交（面向 composable commerce 企业），2026-05-21 已提交 | 已提交 | - |

## Tier 4 — 客户端内置市场 / IDE 集成目录

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 18 | Cline MCP Marketplace | https://github.com/cline/mcp-marketplace/issues/1544 | GitHub issue（仓库 URL + 400×400 PNG logo + 描述 + 测试过的 `llms-install.md`），issue #1544（2026-05-11）；2026-06-09 跟进询问所需调整 | 已提交 | - |
| 19 | Cursor Directory | https://cursor.directory/plugins/longbridge | 网页 submit，2026-04-27 提交，2026-05-13 审核通过上线 | 已上线 | ✅ |
| 20 | Continue Hub | https://hub.continue.dev | hub.continue.dev 已关闭（308 重定向到 continue.dev，文档 404），产品已转型为 PR 代码审查平台，不再支持 MCP 发布 | 不适用 | - |
| 21 | Windsurf / Codeium Cascade | https://www.windsurf.com | 支持 MCP，有内置 Marketplace，但为内部策展无公开提交入口；需联系 Codeium 合作或通过 Discord 申请 | 待手动 | - |
| 22 | Zed Extensions | https://github.com/zed-industries/extensions | Marketplace 需跨平台预编译二进制（CI 暂未支持）；用户现可直接在 settings 配置 `"context_servers":{"longbridge-mcp":{"url":"https://mcp.longbridge.com"}}`；正式提交待 CI 加跨平台编译 | 待 CI 支持 | - |
| 23 | Claude Desktop Extensions | — | 通过 Tier 1 #1 官方 registry 自动拉取；无单独提交入口 | 未开始 | - |
| 24 | VS Code MCP (GitHub Copilot) | https://github.com/mcp/com.longbridge/mcp | 与 Tier 1 #3 同源 | 未开始 | - |
| 25 | JetBrains AI Assistant | https://www.jetbrains.com/help/ai-assistant/configure-an-mcp-server.html | v2025.2 起内置支持 MCP，无官方注册入口；用户在 Settings → AI Assistant → MCP → Add 直接添加 URL 即可，无需提交 | 不适用（无注册入口） | - |

## Tier 5 — Awesome 清单 & 社区策展

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 26 | punkpeye/awesome-mcp-servers | https://github.com/punkpeye/awesome-mcp-servers | GitHub PR，Finance & Fintech 段替换 longportapp 条目（PR #5488，含 en + fa-ir/ja/th/zh/zh_TW 翻译） | 已上线 | ✅ |
| 27 | appcypher/awesome-mcp-servers | https://github.com/appcypher/awesome-mcp-servers | 上游已关闭 PR（`/pulls` 返回 404）。替代：X `@theappcypher` DM / GitHub @mention | PR 关闭，走 X DM | - |
| 28 | wong2/awesome-mcp-servers | https://mcpservers.org/servers/longbridge/longbridge-mcp | 与 #29 mcpservers.org 同源，通过网页 submit 已上线（不接受直接 PR） | 已上线 | ✅ |
| 29 | mcpservers.org | https://mcpservers.org/servers/longbridge/longbridge-mcp | 网页 submit（2026-04-27），2026-04-29 审核通过上线（slug `longbridge/longbridge-mcp`） | 已上线 | ✅ |
| 30 | modelcontextprotocol.info | https://modelcontextprotocol.info/ | 官方 MCP Registry (#1) 的文档站，同一家，随 #1 自动同步，无需单独提交 | 不适用 | - |
| 31 | k2view awesome 15 | https://www.k2view.com/blog/awesome-mcp-servers | 联系博客作者 / 邮件申请进榜 | 未开始 | - |

## Tier 6 — 包管理器 / 安装工具

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 32 | mcp-get | https://mcp-get.com/ | 已废弃（README 推荐迁移到 Smithery，Nov 2025 后无更新，79个包全为 stdio，不支持 HTTP 远程） | 不适用 | - |
| 33 | Dockerfile on GHCR | https://github.com/longbridge/longbridge-mcp/pkgs/container/longbridge-mcp | 发布 `ghcr.io/longbridge/longbridge-mcp` 镜像以便其他 registry 引用 | 未开始 | - |

## Tier 7 — 批量提交工具（可选，一次性覆盖多站点）

| # | 工具名称 | 地址 | 用法 | 提交状态 | 是否提交成功 |
|---|----------|------|------|----------|--------------|
| 34 | mcp-submit | https://github.com/mcp-submit/mcp-submit | 仓库 404，网站无法访问，项目不存在 | 不适用 | - |

## Tier 8 — 新发现目录（2026年调研补充）

| # | 网站名称 | 地址 | 提交方式 | 提交状态 | 是否提交成功 |
|---|----------|------|----------|----------|--------------|
| 35 | MCP.Directory | https://mcp.directory/skills/longbridge | 网页 form；支持一键安装 Cursor/Claude/VS Code/ChatGPT；自动从 repo 读取工具元数据；2026-06-29 skill 审核通过 | 已上线 | ✅ |
| 36 | MCPFind | https://mcpfind.org/servers/com-longbridge-mcp | 从官方 MCP Registry 自动同步，已收录（v0.3.1，待同步至 v0.3.2） | 已上线 | ✅ |
| 37 | MCPfinder.dev | https://mcpfinder.dev | 从官方 MCP Registry + Glama + Smithery 自动同步，无需单独提交 | 已上线 | ✅ |
| 38 | MCP Hunt | https://mcphunt.com | 域名停放待售，不可用；改提交至 MCPHunter (https://www.mcphunter.com)，HTTP 201 接受 | 已提交 | - |
| 39 | MCPServerFinder | https://www.mcpserverfinder.com | 仅支持邮件提交 `info@mcpserverfinder.com`，邮件已起草待发送 | 已发送 | - |
| 40 | mcp-server-directory.com | https://github.com/ankittyagi140/mcp-server-directory/issues/25 | 网站 TLS 故障，改走 GitHub issue #25（2026-05-20）；2026-06-09 跟进 | 已提交 | - |
| 41 | MCPServerDirectory.org | https://mcpserverdirectory.org | 域名已失效（NXDOMAIN），不可用 | 不适用 | - |
| 42 | AI Agents List | https://aiagentslist.com/mcp-servers | 付费提交（$19/$29），不划算 | 不适用 | - |
| 43 | MCP Index | https://mcpindex.net/en | 网站无法访问（2026-05-22 确认），可能已关闭 | 不适用 | - |
| 44 | tolkonepiu/best-of-mcp-servers | https://github.com/tolkonepiu/best-of-mcp-servers/issues/203 | GitHub issue #203（2026-05-20），finance-and-fintech 分类；2026-06-09 跟进 | 已提交 | - |
| 45 | hireblackout/awesome-mcp-servers | https://github.com/hireblackout/awesome-mcp-servers/pull/18 | PR #18 新增 Finance & Trading 分区（2026-05-20）；2026-06-09 跟进 | 已提交 | - |
| 46 | Apify Store | https://apify.com/store/categories/mcp-servers | 需将 server 包装为 Apify Actor | 未开始 | - |

## Tier 9 — 2026年新发现（第二批）

| # | 平台 | 地址 | 提交方式 | 优先级 | 提交状态 | 是否提交成功 |
|---|------|------|---------|--------|----------|--------------|
| 47 | ToolSDK / MCPSDK.dev | https://github.com/toolsdk-ai/toolsdk-mcp-registry/pull/317 | GitHub PR #317（2026-05-21）；2026-06-09 跟进 | 🔴 高 | 已提交 | - |
| 48 | mcphub.dev | https://mcphub.dev/submit | 提交表单标注 "coming soon"，后端 GitHub repo 404，暂无法提交 | 🔴 高 | 待上线 | - |
| 49 | FindMCPServers | https://findmcpservers.com/submit | Web form，2026-05-21 已提交 | 🔴 高 | 已提交 | - |
| 50 | MCP Server Hub (.net) | https://mcpserverhub.net/submit | Web form，2026-05-21 已提交 | 🔴 高 | 已提交 | - |
| 51 | AIBase MCP | https://mcp.aibase.com/submit | Web form，需登录 AIBase 账号，2026-05-21 已提交 | 🔴 高 | 已提交 | - |
| 52 | MCPHub.ai | https://github.com/lightconetech/mcp-gateway/issues/9 | 无公开提交入口，提交 GitHub issue #9（2026-05-21，联系 @shenli3514）；2026-06-09 跟进| 🟡 中 | 已提交 | - |
| 53 | MCP-Awesome.com | https://mcp-awesome.com | GitHub 仓库不存在（404），可能是私有或纯前端站，暂无法提交 | 🟡 中 | 待查 | - |
| 54 | MCP Server Hub (.com) | https://github.com/mcpserverhub/mcpservers/pull/1 | GitHub PR #1（2026-05-21）；2026-06-09 跟进| 🟡 中 | 已提交 | - |
| 55 | Portkey MCP Servers | https://github.com/Portkey-AI/gateway/issues/1663 | GitHub issue #1663（2026-05-21）；2026-06-09 跟进| 🟡 中 | 已提交 | - |
| 56 | AIxploria MCP | https://www.aixploria.com/en/add-ai/ | 付费提交，不划算 | 🟡 中 | 不适用 | - |
| 57 | mcp-servers-hub.net | https://www.mcp-servers-hub.net | 域名为停放广告站（parklogic.com），不存在；mcpserverhub.net 已提交（#50）| 🟡 中 | 不适用 | - |
| 58 | TensorBlock/awesome-mcp-servers | https://github.com/TensorBlock/awesome-mcp-servers/pull/571 | GitHub PR #571，Finance & Crypto 分区（2026-05-21）| 🟡 中 | 已上线 | ✅ |
| 59 | MCPStar/Awesome-Official-MCP-Servers | https://github.com/MCPStar/Awesome-Official-MCP-Servers/pull/6 | GitHub PR #6，中英双语（2026-05-21）；2026-06-09 跟进| 🟡 中 | 已提交 | - |
| 60 | jaw9c/awesome-remote-mcp-servers | https://github.com/jaw9c/awesome-remote-mcp-servers/pull/334 | GitHub PR #334，Finance/Brokerage 分区（2026-05-21）；2026-06-09 跟进| 🟡 中 | 已提交 | - |
| 61 | appcypher/awesome-mcp-servers | https://github.com/appcypher/awesome-mcp-servers | issues 和 PR 均不可用（has_issues:false，PR 403），无法提交 | 🟡 中 | 不适用 | - |
| 62 | abordage/awesome-mcp | https://github.com/abordage/awesome-mcp/pull/40 | GitHub PR #40，repositories.yaml Finance > Trading（2026-05-21）| 🔵 低 | 已上线 | ✅ |
| 63 | bh-rat/awesome-mcp-enterprise | https://github.com/bh-rat/awesome-mcp-enterprise | CONTRIBUTING 明示不收录 MCP server 本身，仅收录基础设施/平台 | 🔵 低 | 不适用 | - |
| 64 | MCPStar/awesome-dxt-mcp | https://github.com/MCPStar/awesome-dxt-mcp/pull/5 | GitHub PR #5，新增 Finance & Trading 分区（2026-05-21）；2026-06-09 跟进| 🔵 低 | 已提交 | - |
| 65 | yzfly/Awesome-MCP-ZH | https://github.com/yzfly/Awesome-MCP-ZH/pull/239 | GitHub PR #239，💰 金融与加密货币分区，中文描述（2026-05-21）；2026-05-29 被关闭，原因：star < 32，达标后可重新提交 | 🟡 中 | 已驳回 | ❌ |

---

## 提交前的准备清单

### 一次性准备（所有 registry 通用）

- [x] 仓库根 `server.json`：`name` = `com.longbridge/mcp`，填齐 `description` / `version` / `repository` / `remotes` / `packages` / `_meta.brand` / `_meta.authentication` / `_meta.observability`
- [x] `description` ≤ 100 字符（实际 84；与 `_meta.localizedDescriptions` 长版解耦）
- [x] 公开 OAuth 元数据 URL 可达：`https://openapi.longbridge.com/.well-known/oauth-protected-resource` 返回 RFC 9728 JSON
- [x] 仓库 GitHub topics 打齐：`mcp` `model-context-protocol` `mcp-server` `finance` `trading` `longbridge`（Glama / PulseMCP / LobeHub 爬虫依赖）
- [x] Ed25519 密钥对：私钥 `~/.ssh/mcp-publisher.pem`，公钥写入 `longbridge.com` 根域 TXT：`v=MCPv1; k=ed25519; p=<base64>`
- [x] 400×400 PNG logo：`docs/logo.png`（从 `assets.wbrks.com/assets/logo/light/logo.svg` 转得）
- [x] README 顶部：品牌 logo + badges + "Connect from an MCP client"（Claude Desktop / Claude Code / Cursor 等）+ Self-hosting 段
- [ ] `llms-install.md`（Cline 必须，用来指导 agent 安装）
- [ ] 不提交带 `_meta.brand.support` 等多余字段（此前已裁掉）

### 每次发版都要做

- [x] bump `Cargo.toml` 的 `version` 与 `server.json` 的 `version` 保持一致（当前 0.2.1）
- [x] `server.json` 的 `packages[0].identifier` 带 tag：`ghcr.io/longbridge/longbridge-mcp:<version>`
- [x] Dockerfile 最终镜像段带 `LABEL io.modelcontextprotocol.server.name="com.longbridge/mcp"`（registry publish 校验镜像必须有此 label）
- [ ] `docker build + push` 到 GHCR：`ghcr.io/longbridge/longbridge-mcp:<version>`，且镜像公开可拉
- [ ] 本地干跑校验：`sed 's|com.longbridge/|com.example/|' server.json > /tmp/e.json && mcp-publisher validate /tmp/e.json`
- [x] 打 git tag `v<version>`，让 Glama/PulseMCP 的 release 探测器识别（v0.2.1 已推送）

## 发布后自检

```bash
# 官方 registry：注意端点是 `/versions`，直接 GET `/servers/<name>` 已 404
curl -s "https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions" \
  | jq '.servers[] | {version: .server.version, status: ._meta."io.modelcontextprotocol.registry/official".status, isLatest: ._meta."io.modelcontextprotocol.registry/official".isLatest}'

# 或按关键字搜（filter param 必须是 search=）
curl -s "https://registry.modelcontextprotocol.io/v0/servers?search=longbridge" \
  | jq '.servers[] | select(.server.name == "com.longbridge/mcp")'

curl -s "https://glama.ai/api/mcp/servers?search=longbridge" | jq .
curl -s "https://api.pulsemcp.com/v0beta/servers?query=longbridge" | jq .
gh api repos/longbridge/longbridge-mcp --jq .topics
```

## Tier 1 #1 快速发布命令（下一次发版直接复制）

Ed25519 私钥默认放在 `~/.ssh/mcp-publisher.pem`（按实际位置改）。
JWT 约 1 小时过期，`publish` 报 `401 token is expired` 就重跑 `login dns`。

```bash
cd /Users/hogan/work/longbridge/longbridge-mcp

PRIVATE_KEY=$(openssl pkey -in ~/.ssh/mcp-publisher.pem -noout -text \
  | grep -A3 '^priv:' | tail -n +2 | tr -d ' :\n')

mcp-publisher login dns --domain longbridge.com --private-key "$PRIVATE_KEY"
mcp-publisher publish

curl -s "https://registry.modelcontextprotocol.io/v0/servers/com.longbridge%2Fmcp/versions" \
  | jq '.servers[] | {version: .server.version,
                      status: ._meta."io.modelcontextprotocol.registry/official".status,
                      isLatest: ._meta."io.modelcontextprotocol.registry/official".isLatest}'
```

发版前常见需要同步改的字段：

- `server.json` 的 `version`（与 `Cargo.toml` 对齐）
- `packages[0].identifier` 里镜像 tag（例：`ghcr.io/longbridge/longbridge-mcp:0.2.1`）
- 镜像 push 到 GHCR 前，Dockerfile 必须带 `LABEL io.modelcontextprotocol.server.name="com.longbridge/mcp"`
