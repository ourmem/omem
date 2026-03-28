<p align="center">
  <strong>🧠 ourmem</strong><br/>
  共享记忆，永不遗忘
</p>

<p align="center">
  <a href="https://github.com/ourmem/omem/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://ourmem.ai"><img src="https://img.shields.io/badge/hosted-api.ourmem.ai-green.svg" alt="Hosted"></a>
  <a href="https://github.com/ourmem/omem"><img src="https://img.shields.io/github/stars/ourmem/omem?style=social" alt="Stars"></a>
</p>

<p align="center">
  <a href="README.md">English</a> | <strong>简体中文</strong>
</p>

---

## 问题

你的 AI 助手每次对话都失忆 — 而且各自为战。

- 🧠 **失忆** — 每次新对话从零开始。偏好、决策、上下文全没了。
- 🏝️ **孤岛** — 你的代码 Agent 不知道写作 Agent 学到了什么。
- 📁 **绑定设备** — 记忆存在一台机器上，换设备全部丢失。
- 🚫 **无法共享** — 团队 Agent 之间知识完全隔绝，每个 Agent 都在重复发现相同的东西。
- 🧩 **没有集体智能** — 即使 Agent 在同一个团队工作，也没有共享的知识层。
- 🔍 **笨搜索** — 只有关键词匹配，没有语义理解。

**ourmem 解决所有这些问题。**

## ourmem 是什么

ourmem 让 AI Agent 拥有共享的持久记忆 — 跨会话、跨设备、跨 Agent、跨团队。一个 API Key 重连一切。

🌐 **官网:** [ourmem.ai](https://ourmem.ai)

<table>
<tr>
<td width="50%" valign="top">

### 🧑‍💻 我用 AI 编程工具

安装你平台的插件就行。记忆自动工作 — 开始对话时自动加载历史记忆，结束时自动保存重要信息。

**→ 跳转到 [快速开始](#快速开始)**

</td>
<td width="50%" valign="top">

### 🔧 我在开发 AI 产品

35 个 REST API 端点。Docker 一行命令自部署。把持久记忆嵌入到你自己的 Agent 和工作流中。

**→ 跳转到 [自部署](#自部署)**

</td>
</tr>
</table>

## 核心能力

<table>
<tr>
<td width="25%" align="center">
<h4>🔗 跨越边界共享</h4>
三级空间 — 个人、团队、组织 — 让知识在 Agent 和团队之间流动，全程溯源追踪。
</td>
<td width="25%" align="center">
<h4>🧠 永不遗忘</h4>
Weibull 衰减模型智能管理记忆生命周期 — 核心记忆持久保留，边缘记忆优雅淡出。
</td>
<td width="25%" align="center">
<h4>🔍 深度理解</h4>
11 阶段混合检索：向量搜索 + BM25 + RRF 融合 + 交叉编码重排 + MMR 多样性。
</td>
<td width="25%" align="center">
<h4>⚡ 智能演化</h4>
7 种去重决策 — 创建、合并、取代、支持、情境化、矛盾、跳过 — 让记忆越来越聪明。
</td>
</tr>
</table>

## 功能一览

| 分类 | 功能 | 详情 |
|------|------|------|
| **平台** | 4 个平台 | OpenCode、Claude Code、OpenClaw、MCP Server |
| **共享** | 空间共享 | 个人 / 团队 / 组织，带溯源追踪 |
| | 溯源追踪 | 每条共享记忆携带完整来源链 |
| | 质量门控自动共享 | 按重要性、类别、标签过滤 |
| | 跨空间搜索 | 一次搜索覆盖所有可访问空间 |
| **摄入** | 智能去重 | 7 种决策：创建、合并、跳过、取代、支持、情境化、矛盾 |
| | 噪声过滤 | 正则 + 向量原型 + 反馈学习 |
| | 准入控制 | 5 维评分门控（效用、置信度、新颖度、时效性、类型先验） |
| | 双流写入 | 同步快速路径（<50ms）+ 异步 LLM 提取 |
| | 隐私保护 | `<private>` 标签脱敏 |
| **检索** | 11 阶段管道 | 向量 + BM25 → RRF → 重排 → 衰减 → 重要性 → MMR 多样性 |
| | 用户画像 | 静态事实 + 动态上下文，<100ms |
| | 检索追踪 | 每阶段可解释性（输入/输出/分数/耗时） |
| **生命周期** | Weibull 衰减 | 按层级 β 值（核心=0.8、工作=1.0、边缘=1.3） |
| | 三层晋升 | 边缘 ↔ 工作 ↔ 核心，基于访问频率晋升 |
| | 自动遗忘 | 时间引用检测（"明天"、"下周"）自动设 TTL |
| **多模态** | 文件处理 | PDF、图片 OCR、视频转录、代码 AST 分块 |
| | GitHub 连接器 | Webhook 实时同步代码、Issue、PR |
| **部署** | 开源 | Apache-2.0（插件 + 文档） |
| | 可自部署 | 单二进制、Docker 一行命令、~$5/月 |
| | musl 静态编译 | 零依赖二进制，任何 Linux x86_64 可运行 |
| | 托管版 | api.ourmem.ai — 无需部署 |

## 从孤立 Agent 到集体智能

大多数 AI 记忆系统把知识困在孤岛里。ourmem 的三级空间架构让知识在 Agent 和团队之间流动 — 带溯源追踪和质量门控的共享机制。

> *研究表明，协作记忆可以减少高达 61% 的重复工作 — Agent 不再重新发现队友已经知道的东西。*
> — Collaborative Memory, ICLR 2026

| | 个人空间 | 团队空间 | 组织空间 |
|---|---------|---------|---------|
| **范围** | 一个用户的多个 Agent | 多个用户 | 全公司 |
| **例子** | 代码 + 写作 Agent 共享偏好 | 后端团队共享架构决策 | 技术标准、安全策略 |
| **权限** | 仅所有者的 Agent | 团队成员 | 全组织（只读） |

## 工作原理

```
你的 AI Agent（OpenCode / Claude Code / OpenClaw / Cursor）
        ↓ 自动召回 + 自动捕获
   ourmem 插件（轻量 HTTP 客户端）
        ↓ REST API（X-API-Key 认证）
   ourmem 服务端
        │
        ├── 智能摄入 ─── LLM 提取 → 噪声过滤 → 准入控制 → 7 种去重决策
        ├── 混合检索 ─── 向量 + BM25 → RRF 融合 → 重排 → 衰减加权 → MMR（11 阶段）
        ├── 用户画像 ─── 静态事实 + 动态上下文，<100ms
        ├── 空间共享 ─── 个人 / 团队 / 组织 记忆隔离 + 溯源
        └── 生命周期 ─── Weibull 衰减、三层晋升（核心/工作/边缘）、自动遗忘
```

## 快速开始

### Agent 安装（推荐）

告诉你的 AI 助手一句话，它会自动完成所有操作 — 创建 API Key、安装插件、配置、验证。

**托管版（api.ourmem.ai — 无需部署）：**

| 平台 | 复制给你的 Agent |
|------|-----------------|
| **OpenClaw** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for OpenClaw` |
| **Claude Code** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for Claude Code` |
| **OpenCode** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for OpenCode` |
| **Cursor / VS Code** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem as MCP Server` |

**自部署版（你自己的服务器）：**

| 平台 | 安装方式 |
|------|---------|
| **OpenClaw** | 运行 `openclaw skills install ourmem`，然后告诉 Agent：`setup ourmem in self-hosted mode` |
| **Claude Code** | `Read https://raw.githubusercontent.com/ourmem/omem/main/skills/ourmem/SKILL.md and install ourmem for Claude Code, self-hosted mode` |
| **OpenCode** | `Read https://raw.githubusercontent.com/ourmem/omem/main/skills/ourmem/SKILL.md and install ourmem for OpenCode, self-hosted mode` |

就这样。Agent 会处理剩下的一切。

---

<details>
<summary><b>手动安装</b>（不通过 Agent）</summary>

### 1. 获取 API Key

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

### 2. 安装插件

**OpenCode:** `"plugin": ["@ourmem/opencode"]` 加到 opencode.json + 设置 `OMEM_API_URL` 和 `OMEM_API_KEY` 环境变量。

**Claude Code:** `/plugin marketplace add ourmem/omem` + 在 `~/.claude/settings.json` 设环境变量。

**OpenClaw:** `openclaw plugins install @ourmem/openclaw` + 配置 openclaw.json。

**MCP (Cursor / VS Code / Claude Desktop):**

```json
{
  "mcpServers": {
    "ourmem": {
      "command": "npx",
      "args": ["@ourmem/mcp"],
      "env": {
        "OMEM_API_URL": "https://api.ourmem.ai",
        "OMEM_API_KEY": "your-api-key"
      }
    }
  }
}
```

### 3. 验证

```bash
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "X-API-Key: $OMEM_API_KEY" -H "Content-Type: application/json" \
  -d '{"content": "I prefer dark mode", "tags": ["preference"]}'
```

</details>

## Memory Space

在 **[ourmem.ai/space](https://ourmem.ai/space)** 可视化浏览和管理记忆。

## 安全与隐私

| | |
|---|---|
| **Rust 内存安全** | 无 GC、无数据竞争。所有权模型编译时保证安全。 |
| **租户隔离** | X-API-Key + 查询级过滤。每个操作验证归属。 |
| **隐私保护** | `<private>` 标签自动脱敏。 |
| **传输加密** | 全部 HTTPS + S3 服务端加密。 |
| **准入控制** | 5 维评分门控，低质量数据拒绝入库。 |
| **开源可审计** | Apache-2.0。审计每一行代码。 |

## API 概览

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/tenants` | 创建空间，获取 Key |
| POST | `/v1/memories` | 存储或智能摄入 |
| GET | `/v1/memories/search` | 混合搜索 |
| GET | `/v1/profile` | 用户画像 |
| POST | `/v1/spaces` | 创建共享空间 |
| POST | `/v1/memories/:id/share` | 分享记忆 |
| GET | `/v1/stats` | 分析洞察 |

完整 API（35 端点）：[docs/API.md](docs/API.md)

## 许可证

Apache-2.0

---

<p align="center">
  <strong>共享记忆，永不遗忘。</strong><br/>
  <a href="https://ourmem.ai">ourmem.ai</a> · <a href="https://github.com/ourmem/omem">GitHub</a>
</p>
