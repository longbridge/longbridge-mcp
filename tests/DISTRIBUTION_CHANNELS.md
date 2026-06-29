# 发布渠道整理

> 更新：2026-05-22  
> 覆盖：longbridge-skills（Skill）、longbridge-terminal（CLI）

**提交规范：所有 Skill 平台统一使用以下 skill 提交：**

> 🔗 `/Users/hogan/work/longbridge/longbridge.zip`（包含 SKILL.md + references/ 完整目录）

---

## 一、Skill 可提交平台（完整版）

### 1.1 Hermes 生态

| # | 平台 | URL | 提交方式 | 审核 | 状态 |
|---|---|---|---|---|---|
| 1 | **HermesHub**（官方） | https://hermeshub.xyz | GitHub PR（`skills/<slug>/SKILL.md`） | 65+ 威胁规则自动扫描 | 🔄 审核中（PR #95）|
| 2 | **NousResearch/hermes-agent** | https://github.com/nousresearch/hermes-agent | PR 到官方源码库 | 人工审核 | 🔄 审核中（PR #30212）|
| 3 | **awesome-hermes-agent** | https://github.com/0xNyk/awesome-hermes-agent | GitHub issue（维护者添加）| 维护者审核 | 🔄 审核中（Issue #103）|
| 3b | **awesome-hermes-agent (SamurAI)** | https://github.com/SamurAIGPT/awesome-hermes-agent | GitHub PR | 维护者审核 | ✅ 已合并（PR #49）|

### 1.2 官方 / Anthropic 渠道

| # | 平台 | URL | 提交方式 | 审核 | 状态 |
|---|---|---|---|---|---|
| 4 | **Anthropic**（anthropics/skills + 插件市场） | https://github.com/anthropics/skills | GitHub PR（合并后自动进入官方目录、插件市场、平台） | Anthropic 人工审核 | 🔄 审核中（PR #1179）|

### 1.3 通用 Skill 标准注册表

| # | 平台 | URL | 提交方式 | 规模 | 状态 |
|---|---|---|---|---|---|
| 5 | **skills.sh**（Vercel） | https://skills.sh | `npx skills publish` / 公开 GitHub repo | 57,000+ | ✅ 已上线（GitHub topics 自动收录）|
| 6 | **officialskills.sh** | https://officialskills.sh | VoltAgent/awesome-agent-skills 前端（同 PR #607）| 581 skills, 48 teams | 🔄 审核中（同 VoltAgent PR #607）|
| 7 | **askill.sh** | https://askill.sh | `askill publish` CLI | 44,000+ | ✅ 已上线（https://askill.sh/skills/gh/longbridge/skills）|
| 8 | **agentskills.to** | https://www.agentskills.to | Discord 社区分享 | 开放 | ❌ 暂不接受 skill 提交（早期阶段）|
| 9 | **agentskills.so** | https://agentskills.so/skills | repo 根目录添加 WKI 索引文件 | 目录/指南 | 🔄 索引文件已就绪（PR #33 已合并），等待 agentskills.so 爬取 |

### 1.4 主流第三方市场

| # | 平台 | URL | 提交方式 | 特点 | 状态 |
|---|---|---|---|---|---|
| 10 | **SkillsMP** | https://skillsmp.com | GitHub repo ≥2 star 自动收录 | 大目录 | ✅ 已上线（GitHub topics 自动收录）|
| 11 | **LobeHub** | https://lobehub.com/skills | `@lobehub/market-cli` CLI | 多平台 | ❌ LobeHub 无独立 Skills 市场（仅支持 Agents/Plugins）|
| 12 | **Agensi** | https://agensi.io | 网页表单 | 8项安全扫描；创作者分成80% | ✅ 已上线（https://www.agensi.io/skills/longbridge）|
| 13 | **claudemarketplaces.com** | https://claudemarketplaces.com | GitHub repo + `marketplace.json` + ≥500次安装 | 6,700+ skills | 🔄 已收录但显示旧数据（https://claudemarketplaces.com/skills/longbridge/developers/longbridge），等待每日爬虫重新索引 |
| 14 | **claudeskills.info** | https://claudeskills.info | GitHub repo，网页提交 | 658+ 免费 skills | 🔄 已提交（待审核）|
| 15 | **ClaudePluginHub** | https://claudepluginhub.com/tools/submit-plugin | 提交 URL | 自动发现+快速通道 | 🔄 已提交（待审核）|
| 16 | **MDSkills.ai** | https://mdskills.ai | GitHub repo + 网页提交 | 支持 27+ AI 工具 | 🔄 已提交（待审核）|
| 17 | **SkillHQ** | https://skillhq.dev | `skillhq publish` CLI | 付费市场；创作者85%分成 | ✅ 已上线（https://skillhq.dev/skills/longbridge/longbridge）|
| 18 | **Skly** | https://skly.ai | 上传+审核 | 买卖市场 | ❌ 卖家上传入口未开放（仅有需求请求页面）|
| 19 | **SkillHub.club** | https://skillhub.club | 自动爬取 GitHub SKILL.md | 7,000+ AI 评估 | ✅ 已收录（longbridge v1.0.1 已被自动索引）|
| 20 | **SkillDock.io** | https://skilldock.io | `skilldock publish` CLI | 版本化注册 | ✅ 已上线（longbridge/longbridge@1.0.1）|
| 21 | **Smithery（skill 标签页）** | https://smithery.ai/skills | `smithery skill publish` | 100,000+ | ✅ 已上线（https://smithery.ai/skills/longbridge-official/longbridge）|
| 22 | **SkillsLLM** | https://skillsllm.com | 网页提交 https://skillsllm.com/submit | 社区目录 | 🔄 已提交（待审核）|
| 23 | **llmskills.org** | https://llmskills.org | GitHub | 精选 Claude/ChatGPT | ❌ 静态网站，无提交机制 |
| 24 | **ClawHub** | https://clawhub.ai | `clawhub publish` CLI（需适配 claw.json） | VirusTotal 扫描；13,000+ | ✅ 已上线 |

### 1.5 大厂官方 Skill 仓库（GitHub PR）

| # | 平台 | URL | 适用场景 | 状态 |
|---|---|---|---|---|
| 25 | **github/awesome-copilot** | https://github.com/github/awesome-copilot | GitHub Copilot CLI；PR 到 staged 分支 | ❌ 被拒绝（PR #1836："This does not fit the repos purpose."）|
| 26 | **block/agent-skills**（Square） | https://github.com/block/agent-skills | Goose agent 生态 | 🔄 审核中（PR #38）|
| 27 | **google/agents-cli** | https://github.com/google/agents-cli | Google Cloud skills | ❌ 不接受 PR（issues only）|
| 28 | **microsoft/skills** | https://microsoft.github.io/skills/ | Azure/Copilot 生态 | ❌ 仅限 Microsoft 内部 skills |
| 29 | **MicrosoftDocs/Agent-Skills** | https://github.com/MicrosoftDocs/Agent-Skills | Azure 文档 skills | ❌ 仅接受自动爬取的 Microsoft 内容 |

### 1.6 IDE 专属渠道

| # | 平台 | URL | 特点 | 状态 |
|---|---|---|---|---|
| 30 | **Cursor Directory** | https://cursor.directory | 网页提交 | 🔄 审核中（https://cursor.directory/plugins/longbridge-skills）|
| 31 | **Kiro（AWS IDE）** | https://kiro.dev/docs/skills/ | Kiro marketplace + Bedrock Registry 集成 | ⬜ |
| 32 | **VS Code 扩展市场** | https://marketplace.visualstudio.com | 标准 VS Code 扩展提交 | ⬜ |

### 1.7 安全审计渠道

| # | 平台 | URL | 审核特点 | 状态 |
|---|---|---|---|---|
| 33 | **trailofbits/skills-curated** | https://github.com/trailofbits/skills-curated | Trail of Bits 人工代码审查 | 🔄 审核中（Issue #31）|
| 34 | **Agensi**（同上） | https://agensi.io | 8点安全扫描（最严格公开市场） | ⬜ |
| 35 | **JFrog Agent Skills Registry** | https://jfrog.com/ai-catalog/skills-registry/ | 企业级静态分析+NVIDIA AI-Q | ⬜ |
| 36 | **AWS Agent Registry** | https://aws.amazon.com/bedrock/agentcore/ | 企业审批流 | ⬜ |

### 1.8 社区精选 GitHub 列表（PR 提交）

| # | 列表 | URL | 状态 |
|---|---|---|---|
| 37 | VoltAgent/awesome-agent-skills | https://github.com/VoltAgent/awesome-agent-skills | 🔄 审核中（PR #607）|
| 38 | travisvn/awesome-claude-skills | https://github.com/travisvn/awesome-claude-skills | 🔄 审核中（PR #761）|
| 39 | skillmatic-ai/awesome-agent-skills | https://github.com/skillmatic-ai/awesome-agent-skills | 🔄 审核中（PR #92）|
| 40 | addyosmani/agent-skills | https://github.com/addyosmani/agent-skills | ❌ 被拒绝（PR #204："Classic SEO/backlink contribution"）|
| 41 | claude-market/marketplace | https://github.com/claude-market/marketplace | 🔄 审核中（PR #12）|
| 42 | netresearch/claude-code-marketplace | https://github.com/netresearch/claude-code-marketplace | ❌ 被拒绝（PR #61："We do not accept external skills on this marketplace."）|
| 43 | daymade/claude-code-skills | https://github.com/daymade/claude-code-skills | 🔄 审核中（PR #69）|
| 44 | gmh5225/awesome-skills | https://github.com/gmh5225/awesome-skills | ✅ 已合并（PR #20）|
| 45 | GetBindu/awesome-claude-code-and-skills | https://github.com/GetBindu/awesome-claude-code-and-skills | ✅ 已合并（PR #43）|

### 1.9 中文平台

| # | 平台 | URL | 特点 | 状态 |
|---|---|---|---|---|
| 46 | **觅游 Miyou**（美团） | 美团 App 内 | 中国首个 AI agent skills 社区平台；2026年5月公测 | ⬜ |
| 47 | **GitHub 话题（中文开发者）** | `claude-skills` `agent-skills` topic | 被 AgentSkills.to 中文社区收录 | ✅ 已添加（13 个 topics：claude-skills, agent-skills, claude-code, finance, trading 等）|

### 1.10 新发现平台（2026-05-29）

| # | 平台 | URL | 提交方式 | 特点 | 状态 |
|---|---|---|---|---|---|
| 48 | **AwesomeSkills.dev** | https://www.awesomeskills.dev/en/submit | 填 GitHub URL，即时收录 | 1,000+ skills，无审核队列 | ✅ 已收录（auto-crawled，search "longbridge"）|
| 49 | **AwesomeSkill.ai** | https://awesomeskill.ai | 网站提交 | 1,375+ skills，15 分类 | 🔄 已提交（待审核）|
| 50 | **Awesome Agent Skills** | https://awesomeagentskills.dev | GitHub Issue 模板 | 26,385 条目，每日自动更新 | 🔄 审核中（Issue #14）|
| 51 | **agentskill.sh** | https://agentskill.sh/submit | `/submit` UI | 185,000+ skills，20+ 工具支持 | ✅ 已上线（https://agentskill.sh/@longbridge）|
| 52 | **Skills Directory** | https://www.skillsdirectory.com | 网页表单 + 安全审核 | 73,665 skills，50+ 安全规则扫描 | ✅ 已上线（https://www.skillsdirectory.com/skills/longbridge-longbridge）|
| 53 | **Skillstore** | https://skillstore.io | `/submit` + 安全审核 | Anthropic Verified 徽章 | ✅ 已上线（https://skillstore.io/zh-hans/skills/longbridge-longbridge，PR #1919 已合并）|
| 54 | **Agentpedia** | https://agentpedia.codes | 社区提交 | 548+ skills，35 分类 | 🔄 已提交（https://github.com/devanshug2307/antigravity-discussions/discussions/110）|
| 55 | **MCP.Directory** | https://mcp.directory/skills | `/submit-skill` | 4,400+ skills，1,653+ 发布者 | 🔄 已提交（待审核）|
| 56 | **MCP Servers Skills** | https://mcpservers.org/agent-skills | `/submit` | Anthropic/MS/GitHub 官方收录 | ✅ 已上线（https://mcpservers.org/zh-CN/servers/longbridge/skills）|
| 57 | **AI Agents Directory** | https://aiagentsdirectory.com/skills | `/submit-agent` | 3,001 skills | 🔄 已提交（待审核）|
| 58 | **Claude Marketplace** | https://claude-marketplace.com | "Submit a skill" | 122 skills + 2,798 MCP | 🔄 已提交（待审核）|
| 59 | **AgentConn** | https://agentconn.com | "Submit Agent" | 综合目录 | ⬜ 表单未上线，可发邮件 hello@agentconn.com |
| 60 | **Antigravity Skills** | https://antigravityskills.org | "+ Submit Skill" | 261+ skills，安全审核徽章 | 🔄 已提交（待审核）|
| 61 | **SkillReg** | https://skillreg.dev | `skillreg push` CLI | 语义版本化，跨代理兼容 | ❌ 用于管理自己的 skill 注册表，不是公开目录 |
| 62 | **Skilldex** | https://skilldex-web.vercel.app | `skillpm install` | 3,000+ skills，MCP server | 🔄 npm v2.0.2 已发布（skillpm install longbridge-skill ✓），等待 Skilldex 网站索引（24-72h）|
| 63 | **Playbooks.com** | https://playbooks.com/skills | `clawhub` CLI | 12,214+ skills | ❌ 无提交入口，通过 GitHub repo 自动爬取收录 |
| 64 | **MCP Market** | https://mcpmarket.com/tools/skills | 网页表单 | 与 MCP 生态结合 | 🔄 已提交（待审核）|
| 65 | **Composio Awesome-Agent-CLIs** | https://github.com/ComposioHQ/awesome-agent-clis | GitHub PR | CLI + SKILL.md 列表 | 🔄 审核中（PR #14）|
| 66 | **Tech Leads Club Skills** | https://agent-skills.techleads.club | GitHub PR + CI | 安全强化，100% 开源 | 🔄 分支就绪（hogan-yuan/agent-skills-2），需手动开 PR |
| 67 | **AgentBase Registry** | https://agentbase-registry.vercel.app | GitHub PR / `/submit/` | 早期阶段，机器可读 API | 🔄 审核中（AgentBase1/registry PR #1）|
| 68 | **Antigravity-Awesome-Skills** | https://github.com/sickn33/antigravity-awesome-skills | GitHub PR | 1,400+ skills，含测试要求 | ✅ 已合并（PR #630）|
| 69 | **anthropics/claude-plugins-community** | https://github.com/anthropics/claude-plugins-community | clau.de/plugin-directory-submission | Anthropic 官方社区插件市场 | 🔄 已提交（claude.ai/settings/plugins/submit，待审核）|
| 70 | **Agent37** | https://www.agent37.com | Web 上传 | 分成 80%，Stripe | ⬜ |

---

## 二、longbridge-terminal CLI 发布渠道

### 2.1 包管理器

| # | 平台 | 系统 | 提交方式 | 难度 | 状态 |
|---|---|---|---|---|---|
| 1 | **crates.io** | Rust 生态 | `cargo publish`，完善 Cargo.toml 元数据 | ⭐ 极低 | ⬜ |
| 2 | **Homebrew tap** | macOS/Linux | 建 `homebrew-longbridge-terminal` repo + Formula | ⭐ 低 | 🔄 审核中（longbridge/homebrew-tap PR #11，新增 Formula 支持 Linux）|
| 3 | **Homebrew core** | macOS/Linux | PR 到 homebrew-core（严格审核） | ⭐⭐⭐ 高 | ❌ git 依赖无法满足 core 要求，改提 homebrew-cask |
| 3b | **homebrew/homebrew-cask** | macOS | PR 提交预编译 Cask | ⭐⭐ 中 | ❌ 需 Apple 签名+公证（Notarization），等 longbridge-terminal 添加签名 CI 流程后再提 |
| 4 | **Scoop** | Windows | 建 scoop bucket repo + JSON manifest | ⭐ 低 | ✅ 已支持 |
| 5 | **winget** | Windows | PR 到 microsoft/winget-pkgs | ⭐⭐ 中 | ✅ 已合并（PR #380089）|
| 6 | **Chocolatey** | Windows | `.nupkg` + choco push | ⭐⭐ 中 | 🔄 包已就绪（https://github.com/hogan-yuan/chocolatey-longbridge-terminal），待 choco push |
| 7 | **AUR** | Arch Linux | 写 PKGBUILD，push 到 AUR | ⭐⭐ 中 | ✅ 已上线（https://aur.archlinux.org/packages/longbridge-terminal）|
| 8 | **nixpkgs** | NixOS | PR + Nix derivation | ⭐⭐⭐ 高 | 🔄 分支就绪（https://github.com/hogan-yuan/nixpkgs/tree/longbridge-terminal），需填真实 hash 再提 PR |
| 9 | **Snap Store** | Ubuntu/Linux | snapcraft.yaml + `snapcraft upload` | ⭐⭐ 中 | 🔄 snapcraft.yaml PR #222 已提交（longbridge-terminal），待合并后发布 |
| 10 | **pkgx** | 跨平台 | PR 到 pkgxdev/pantry | ⭐ 低 | ✅ 已合并（PR #13014）|
| 11 | **mise** | 跨平台 | PR 到 mise registry | ⭐ 低 | ✅ 已合并（PR #10073）|

### 2.2 CLI 展示目录

| # | 平台 | URL | 特点 | 提交方式 | 状态 |
|---|---|---|---|---|---|
| 12 | **Terminal Trove** | https://terminaltrove.com | 专注 CLI/TUI；Rust + Finance 分类 | 网页 "Post a Tool" | 🔄 已提交（待审核）|
| 13 | **tldr-pages** | https://github.com/tldr-pages/tldr | 替代 man page；数百万覆盖 | GitHub PR（手写使用示例） | 🔄 审核中（PR #22593）|
| 14 | **awesome-cli-apps** | https://github.com/agarrharr/awesome-cli-apps | 36k stars；需 ≥20 star | GitHub PR | ❌ 被关闭（PR #1103，无说明）|
| 15 | **awesome-rust** | https://github.com/rust-unofficial/awesome-rust | 57k stars | GitHub PR | ✅ 已合并（PR #2530）|
| 16 | **awesome-tuis** | https://github.com/rothgar/awesome-tuis | TUI 专项 | GitHub PR | ✅ 已合并（PR #692）|
| 17 | **clihub.ai** | https://clihub.ai | AI agent App Store | YAML + GitHub PR | 🔄 审核中（PR #3）|
| 18 | **AlternativeTo** | https://alternativeto.net | SEO 流量（Bloomberg Terminal 替代品） | 网页表单 | ⏳ 账号需满 7 天才能提交，可在 2026-06-04 后提交 |
| 24 | **lib.rs** | https://lib.rs/command-line-utilities | Rust crate 目录（crates.io 镜像） | 发布到 crates.io 自动收录 | ⬜ 需 crates.io 发布 |
| 25 | **libs.tech** | https://libs.tech/rust/cli-libraries | Rust CLI 库列表 | GitHub 登录贡献 | ⬜ |
| 26 | **Composio Awesome-Agent-CLIs** | https://github.com/ComposioHQ/awesome-agent-clis | Agent-ready CLI 列表 | GitHub PR | ⬜ |

### 2.3 开发者社区

| # | 平台 | 建议操作 | 状态 |
|---|---|---|---|
| 19 | **Hacker News Show HN** | 发布 "Show HN: longbridge-terminal — ..." | ⬜ |
| 20 | **Product Hunt** | 准备截图/GIF 正式发布 | ⬜ |
| 21 | **DevHunt** | GitHub 验证提交 | ❌ 需付费 |
| 22 | **Dev.to** | 写技术文章介绍架构和使用场景 | ⬜ |
| 23 | **Reddit** | r/rust + r/algotrading + r/commandline | ⬜ |

---

## 三、优先顺序建议

### 近期（低成本高回报）
1. `anthropics/skills` — 最权威 Skill 入口
2. `skills.sh` + GitHub Topics（加 `claude-skills` 话题）— 自动被多平台收录
3. `HermesHub` — 新兴但增长最快的 Skill 市场 ✅ 已提交
4. `crates.io` — Rust 生态必做
5. `Homebrew tap` — macOS 用户最常用
6. `Terminal Trove` + `tldr-pages` — CLI 社区曝光

### 中期（需要一定准备）
- `Agensi` — 安全审核完成后权威性更高
- `SkillHQ` / `Skly` — 付费变现渠道
- `github/awesome-copilot` + `block/agent-skills` — 跨生态曝光
- `winget` + `Chocolatey` — Windows 用户
- `Claude Connectors` — 见 MCP 发布文档（PUBLISHING.md）
- `Product Hunt` / `Hacker News Show HN` — 发布节点集中曝光

### 注意事项
- **ClawHub** 是 OpenClaw 格式（`claw.json`），SKILL.md 需适配
- **Claude Connectors** 30% 拒绝原因是工具缺少 `readOnlyHint`/`destructiveHint` 注解
- **tldr-pages** 不接受 AI 生成内容，需手写真实使用示例
- **claudemarketplaces.com** 要求 ≥500 次安装才能提交
- **Homebrew core** 要求 macOS 三版本 + Linux x86_64 全平台通过 CI
