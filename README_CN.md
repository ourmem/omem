<p align="center">
  <strong>🧠 ourmem</strong><br/>
  AI Agent 的持久记忆系统
</p>

<p align="center">
  <a href="https://github.com/yhyyz/omem/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://ourmem.ai"><img src="https://img.shields.io/badge/托管版-api.ourmem.ai-green.svg" alt="Hosted"></a>
  <a href="https://github.com/yhyyz/omem"><img src="https://img.shields.io/github/stars/yhyyz/omem?style=social" alt="Stars"></a>
</p>

<p align="center">
  <a href="README.md">English</a> | <strong>简体中文</strong>
</p>

---

## 痛点

你的 AI Agent 每次对话都在失忆。

- 🧠 **失忆** — 每次新会话从零开始，偏好、决策、上下文全部丢失
- 🏝️ **孤岛** — Coder Agent 学到的东西，Writer Agent 完全不知道
- 📁 **绑定本机** — 记忆存在本地文件里，换台电脑就没了
- 🚫 **无法共享** — 团队成员之间的 Agent 知识无法互通
- 🔍 **检索太蠢** — 只有关键词匹配，没有语义理解，没有相关性排序

**ourmem 解决所有这些问题。**

## 什么是 ourmem

ourmem 让 AI Agent 拥有跨会话、跨设备、跨团队的持久记忆。它把事实、偏好和上下文存储在云端（或自部署）服务器中，配备 11 阶段混合检索、7 种智能去重决策，以及基于 Space 的跨 Agent 记忆共享。

一个 API Key 就能重新连接所有记忆。

<table>
<tr>
<td width="50%" valign="top">

### 🧑‍💻 我是 AI 工具用户

装个插件就行。记忆自动工作 — 开始会话时自动回忆相关上下文，结束时自动保存关键信息。

**→ 跳转到 [快速开始](#快速开始)**

</td>
<td width="50%" valign="top">

### 🔧 我在做 AI 产品

35 个 REST API 端点，Docker 一行命令自部署。把持久记忆嵌入你自己的 Agent 和工作流。

**→ 跳转到 [自部署](#自部署)**

</td>
</tr>
</table>

## 对比

| 特性 | ourmem | mem9 | Supermemory | mem0 |
|------|--------|------|-------------|------|
| 开源 | ✅ Apache-2.0 | ✅ Apache-2.0 | ❌ 核心闭源 | ✅ Apache-2.0 |
| 自部署 | ✅ Docker 一行命令 | ⚠️ 仅云端 | ❌ 仅 SaaS | ✅ 本地 |
| 平台支持 | 4 个（OpenCode、Claude Code、OpenClaw、MCP） | 1 个（OpenClaw） | 4 个 | 3 个 |
| Space 共享 | ✅ 个人 / 团队 / 组织 | ❌ | ❌ | ❌ |
| 智能去重 | 7 种决策 | 4 种决策 | 未知 | 基础 |
| 检索管道 | 11 阶段 | 基础 RRF | 云端 | 基础向量 |
| 用户画像 | ✅ 静态 + 动态 | ❌ | ✅ | ❌ |
| 记忆衰减 | Weibull 三级 | ❌ | 自动遗忘 | ❌ |
| 多模态 | ✅ PDF / 图片 / 视频 / 代码 | ❌ | ✅ | ❌ |
| 噪声过滤 | ✅ 正则 + 向量 + 反馈学习 | ❌ | ❌ | ❌ |

## 工作原理

```
你的 AI Agent（OpenCode / Claude Code / OpenClaw / Cursor）
        ↓ 自动回忆 + 自动捕获
   ourmem 插件（轻量 HTTP 客户端）
        ↓ REST API（X-API-Key 认证）
   ourmem 服务端
        │
        ├── 智能摄入 ─── LLM 提取 → 噪声过滤 → 准入控制 → 7 种去重决策
        ├── 混合检索 ─── 向量 + BM25 → RRF 融合 → 重排序 → 衰减加权 → MMR 多样性（11 阶段）
        ├── 用户画像 ─── 静态事实 + 动态上下文，<100ms
        ├── Space 共享 ── 个人 / 团队 / 组织 三级记忆隔离
        └── 生命周期 ─── Weibull 衰减，三级晋升（核心/工作/边缘），自动遗忘
```

## 快速开始

### 1. 获取 API Key

**托管版（无需部署）：**

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
# → {"id": "xxx", "api_key": "xxx", "status": "active"}
```

**自部署：**

```bash
docker run -d -p 8080:8080 -e OMEM_EMBED_PROVIDER=bedrock ourmem:latest
curl -sX POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

保存返回的 `api_key` — 这个 Key 能从任何设备重新连接你的记忆。

### 2. 安装插件

#### OpenCode

在 `opencode.json` 中添加：

```json
{
  "plugin": ["@ourmem/opencode"]
}
```

设置环境变量：

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="your-api-key"
```

#### Claude Code

```bash
/plugin marketplace add yhyyz/omem
/plugin install ourmem@yhyyz/omem
```

在 `~/.claude/settings.json` 中设置：

```json
{
  "env": {
    "OMEM_API_URL": "https://api.ourmem.ai",
    "OMEM_API_KEY": "your-api-key"
  }
}
```

#### OpenClaw

```bash
openclaw plugins install @ourmem/openclaw
```

在 `openclaw.json` 中添加：

```json
{
  "plugins": {
    "slots": { "memory": "ourmem" },
    "entries": {
      "ourmem": {
        "enabled": true,
        "config": {
          "apiUrl": "https://api.ourmem.ai",
          "apiKey": "your-api-key"
        }
      }
    }
  }
}
```

#### MCP Server（Cursor、VS Code、Claude Desktop）

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
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="your-api-key"

# 存一条记忆
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"content": "我喜欢所有编辑器都用暗色主题", "tags": ["偏好"]}'

# 搜回来
curl -s "$OMEM_API_URL/v1/memories/search?q=编辑器+主题" \
  -H "X-API-Key: $OMEM_API_KEY" | jq '.results[0].memory.content'
```

## Space 记忆共享

ourmem 的独有能力：三级记忆空间，细粒度访问控制。

| 空间类型 | 范围 | 使用场景 |
|---------|------|---------|
| **Personal** | 一个用户，多个 Agent | 你的 Coder + Writer + Researcher 共享偏好 |
| **Team** | 多个用户 | 后端团队共享架构决策 |
| **Organization** | 全公司 | 技术规范、安全策略、共享知识库 |

```bash
# 创建团队空间
curl -sX POST "$OMEM_API_URL/v1/spaces" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "后端团队", "space_type": "team"}'

# 分享记忆到团队
curl -sX POST "$OMEM_API_URL/v1/memories/MEMORY_ID/share" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"target_space": "team:SPACE_ID"}'

# 跨所有空间搜索
curl -s "$OMEM_API_URL/v1/memories/search?q=架构&space=all" \
  -H "X-API-Key: $OMEM_API_KEY"
```

每个 Agent 能看到：**自己的私有记忆** + **共享空间** + **全局记忆**。只能修改自己的和共享空间的记忆，永远无法访问其他 Agent 的私有数据。

## Agent 获得的能力

| 工具 | 用途 |
|------|------|
| `memory_store` | 保存事实、决策、偏好 |
| `memory_search` | 语义 + 关键词混合搜索 |
| `memory_get` | 按 ID 获取 |
| `memory_update` | 修改已有记忆 |
| `memory_delete` | 删除记忆 |

| 钩子 | 触发时机 | 效果 |
|------|---------|------|
| 会话开始 | 新会话 | 自动注入相关记忆到上下文 |
| 会话结束 | 会话结束 | 自动捕获关键信息 |

## 自部署

```bash
# 最小化（仅 BM25 搜索，不需要 Embedding API）
docker run -d -p 8080:8080 ourmem:latest

# 推荐：Bedrock Embedding（需要 AWS 凭证）
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=bedrock \
  -e AWS_REGION=us-east-1 \
  ourmem:latest

# OpenAI 兼容 Embedding
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=openai-compatible \
  -e OMEM_EMBED_API_KEY=sk-xxx \
  ourmem:latest
```

完整部署指南：[docs/DEPLOY.md](docs/DEPLOY.md)

## 从源码编译

### 两种编译模式

| 模式 | 命令 | 产出 | Bedrock | 运行环境 |
|------|------|------|---------|---------|
| **glibc（完整功能）** | `cargo build --release` | 动态链接，~218MB | ✅ 支持 | 需要相同 glibc 版本 |
| **musl（可移植）** | 见下方 | 静态链接，~182MB | ❌ 用 OpenAI 兼容接口 | **任何 Linux x86_64** |

### glibc 编译（含 Bedrock 支持）

```bash
cargo build --release -p omem-server
# 产出: target/release/omem-server
# 限制: 目标机器 glibc 版本需 >= 编译机器
```

### musl 静态编译（可移植，零依赖）

产出的单个二进制可以在**任何 Linux x86_64** 上运行 — 不需要安装任何依赖。

```bash
rustup target add x86_64-unknown-linux-musl

RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" \
  cargo build --release --target x86_64-unknown-linux-musl \
  -p omem-server --no-default-features

# 产出: target/x86_64-unknown-linux-musl/release/omem-server
# 完全静态链接，到处能跑
```

> **注意：** musl 编译通过 `--no-default-features` 禁用了 AWS Bedrock 支持。请使用 `OMEM_EMBED_PROVIDER=openai-compatible` 接入通义千问、OpenAI 等兼容接口。原因是 `aws-lc-sys`（AWS 加密库）在 musl 静态链接下因 `dlopen(NULL)` 不兼容而崩溃（[aws-c-cal#213](https://github.com/awslabs/aws-c-cal/issues/213)），且 Rust 默认的 `static-pie` 输出与 musl-gcc 不兼容（[rust#95926](https://github.com/rust-lang/rust/issues/95926)）。

### 传输到任意服务器

```bash
# 压缩
gzip -c target/x86_64-unknown-linux-musl/release/omem-server > omem-server.gz

# 传到服务器
scp omem-server.gz user@server:/opt/

# 直接运行（不需要任何依赖）
ssh user@server "gunzip /opt/omem-server.gz && chmod +x /opt/omem-server && /opt/omem-server"
```

## API 概览

| 方法 | 端点 | 说明 |
|------|------|------|
| POST | `/v1/tenants` | 创建工作空间，获取 API Key |
| POST | `/v1/memories` | 存储记忆或智能摄入对话 |
| GET | `/v1/memories/search` | 11 阶段混合检索 |
| GET | `/v1/memories` | 列表查询，支持过滤和分页 |
| GET | `/v1/profile` | 自动生成的用户画像 |
| POST | `/v1/spaces` | 创建共享空间 |
| POST | `/v1/memories/:id/share` | 分享记忆到空间 |
| POST | `/v1/files` | 上传 PDF / 图片 / 视频 / 代码 |
| GET | `/v1/stats` | 统计分析 |

完整 API 文档（35 个端点）：[docs/API.md](docs/API.md)

## 文档

| 文档 | 说明 |
|------|------|
| [docs/API.md](docs/API.md) | 完整 REST API 参考 |
| [docs/DEPLOY.md](docs/DEPLOY.md) | Docker 和 AWS 部署指南 |
| [docs/PLUGINS.md](docs/PLUGINS.md) | 四平台插件安装指南 |
| [skills/ourmem/SKILL.md](skills/ourmem/SKILL.md) | AI Agent 引导安装技能 |

## 许可证

Apache-2.0

---

<p align="center">
  <strong>给你的 AI 一份记忆，是时候了。</strong><br/>
  <a href="https://ourmem.ai">ourmem.ai</a> · <a href="https://github.com/yhyyz/omem">GitHub</a>
</p>
