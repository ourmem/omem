<p align="center">
  <strong>🧠 ourmem</strong><br/>
  Persistent Memory for AI Agents
</p>

<p align="center">
  <a href="https://github.com/yhyyz/omem/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://ourmem.ai"><img src="https://img.shields.io/badge/hosted-api.ourmem.ai-green.svg" alt="Hosted"></a>
  <a href="https://github.com/yhyyz/omem"><img src="https://img.shields.io/github/stars/yhyyz/omem?style=social" alt="Stars"></a>
</p>

<p align="center">
  <strong>English</strong> | <a href="README_CN.md">简体中文</a>
</p>

---

## The Problem

Your AI agents have amnesia.

- 🧠 **Amnesia** — every session starts from zero. Preferences, decisions, context — all gone.
- 🏝️ **Silos** — your Coder agent can't access what your Writer agent learned.
- 📁 **Local lock-in** — memory tied to one machine. Switch devices, lose everything.
- 🚫 **No sharing** — team members can't benefit from each other's agent knowledge.
- 🔍 **Dumb recall** — keyword match only. No semantic understanding, no relevance ranking.

**ourmem fixes all of this.**

## What is ourmem

ourmem gives AI agents persistent memory that survives sessions, restarts, and machine switches. It stores facts, preferences, and context in a cloud (or self-hosted) server with 11-stage hybrid retrieval, 7-decision smart deduplication, and Space-based sharing across agents and teams.

One API key reconnects everything.

<table>
<tr>
<td width="50%" valign="top">

### 🧑‍💻 I use AI coding tools

Install the plugin for your platform. Memory works automatically — your agent recalls past context on session start and captures key info on session end.

**→ Jump to [Quick Start](#quick-start)**

</td>
<td width="50%" valign="top">

### 🔧 I'm building AI products

REST API with 35 endpoints. Docker one-liner for self-deploy. Embed persistent memory into your own agents and workflows.

**→ Jump to [Self-Deploy](#self-deploy)**

</td>
</tr>
</table>

## How It's Different

| Feature | ourmem | mem9 | Supermemory | mem0 |
|---------|--------|------|-------------|------|
| Open source | ✅ Apache-2.0 | ✅ Apache-2.0 | ❌ Core closed | ✅ Apache-2.0 |
| Self-deploy | ✅ Docker one-liner | ⚠️ Cloud only | ❌ SaaS only | ✅ Local |
| Platforms | 4 (OpenCode, Claude Code, OpenClaw, MCP) | 1 (OpenClaw) | 4 | 3 |
| Space sharing | ✅ Personal / Team / Org | ❌ | ❌ | ❌ |
| Smart dedup | 7 decisions | 4 decisions | Unknown | Basic |
| Retrieval pipeline | 11 stages | Basic RRF | Cloud | Basic vector |
| User Profile | ✅ Static + dynamic | ❌ | ✅ | ❌ |
| Memory decay | Weibull 3-tier | ❌ | Auto-forget | ❌ |
| Multi-modal | ✅ PDF / image / video / code | ❌ | ✅ | ❌ |
| Noise filter | ✅ Regex + vector + feedback | ❌ | ❌ | ❌ |

## How It Works

```
Your AI Agent (OpenCode / Claude Code / OpenClaw / Cursor)
        ↓ auto-recall + auto-capture
   ourmem Plugin (thin HTTP client)
        ↓ REST API (X-API-Key auth)
   ourmem Server
        │
        ├── Smart Ingest ─── LLM extraction → noise filter → admission → 7-decision reconciliation
        ├── Hybrid Search ── vector + BM25 → RRF fusion → reranker → decay boost → MMR (11 stages)
        ├── User Profile ─── static facts + dynamic context, <100ms
        ├── Space Sharing ── Personal / Team / Organization memory isolation
        └── Lifecycle ────── Weibull decay, 3-tier promotion (Core/Working/Peripheral), auto-forgetting
```

## Quick Start

### 1. Get an API Key

**Hosted (no setup needed):**

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
# → {"id": "xxx", "api_key": "xxx", "status": "active"}
```

**Self-deploy:**

```bash
docker run -d -p 8080:8080 -e OMEM_EMBED_PROVIDER=bedrock ourmem:latest
curl -sX POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

Save the returned `api_key` — this reconnects you to the same memory from any machine.

### 2. Install Plugin

#### OpenCode

Add to `opencode.json`:

```json
{
  "plugin": ["@ourmem/opencode"]
}
```

Set environment variables:

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="your-api-key"
```

#### Claude Code

```bash
/plugin marketplace add yhyyz/omem
/plugin install ourmem@yhyyz/omem
```

Set in `~/.claude/settings.json`:

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

Add to `openclaw.json`:

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

#### MCP Server (Cursor, VS Code, Claude Desktop)

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

### 3. Verify

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="your-api-key"

# Store a memory
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"content": "I prefer dark mode in all editors", "tags": ["preference"]}'

# Search it back
curl -s "$OMEM_API_URL/v1/memories/search?q=editor+theme" \
  -H "X-API-Key: $OMEM_API_KEY" | jq '.results[0].memory.content'
```

## Space-Based Memory Sharing

ourmem's unique capability: three-level memory spaces with fine-grained access control.

| Space Type | Scope | Use Case |
|-----------|-------|----------|
| **Personal** | One user, multiple agents | Your Coder + Writer + Researcher share preferences |
| **Team** | Multiple users | Backend team shares architecture decisions |
| **Organization** | Company-wide | Tech standards, security policies, shared knowledge |

```bash
# Create a team space
curl -sX POST "$OMEM_API_URL/v1/spaces" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "Backend Team", "space_type": "team"}'

# Share a memory to the team
curl -sX POST "$OMEM_API_URL/v1/memories/MEMORY_ID/share" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"target_space": "team:SPACE_ID"}'

# Search across all spaces
curl -s "$OMEM_API_URL/v1/memories/search?q=architecture&space=all" \
  -H "X-API-Key: $OMEM_API_KEY"
```

Each agent sees: **own private** + **shared spaces** + **global**. Agents can only modify their own and shared memories — never another agent's private data.

## What Your Agent Gets

| Tool | Purpose |
|------|---------|
| `memory_store` | Save facts, decisions, preferences |
| `memory_search` | Semantic + keyword hybrid search |
| `memory_get` | Retrieve by ID |
| `memory_update` | Modify existing memory |
| `memory_delete` | Remove a memory |

| Hook | Trigger | Effect |
|------|---------|--------|
| SessionStart | New session | Recent memories auto-injected into context |
| SessionEnd | Session ends | Key information auto-captured |

## Self-Deploy

```bash
# Minimal (BM25 search only, no embedding API needed)
docker run -d -p 8080:8080 ourmem:latest

# With Bedrock embedding (recommended, needs AWS credentials)
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=bedrock \
  -e AWS_REGION=us-east-1 \
  ourmem:latest

# With OpenAI-compatible embedding
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=openai-compatible \
  -e OMEM_EMBED_API_KEY=sk-xxx \
  ourmem:latest
```

Full deployment guide: [docs/DEPLOY.md](docs/DEPLOY.md)

## Build from Source

### Two build modes

| Mode | Command | Binary | Bedrock | Runs on |
|------|---------|--------|---------|---------|
| **glibc (full)** | `cargo build --release` | Dynamic linked, ~218MB | ✅ AWS Bedrock | Same glibc version as build host |
| **musl (portable)** | See below | Static linked, ~182MB | ❌ OpenAI-compatible only | **Any Linux x86_64** |

### glibc build (with Bedrock support)

```bash
cargo build --release -p omem-server
# Binary: target/release/omem-server
# Requires: same or newer glibc on target machine
```

### musl static build (portable, zero dependencies)

Single binary that runs on **any Linux x86_64** — no glibc, no libraries, nothing.

```bash
rustup target add x86_64-unknown-linux-musl

RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" \
  cargo build --release --target x86_64-unknown-linux-musl \
  -p omem-server --no-default-features

# Binary: target/x86_64-unknown-linux-musl/release/omem-server
# Statically linked, runs anywhere
```

> **Note:** The musl build disables `--no-default-features` which excludes AWS Bedrock support. Use `OMEM_EMBED_PROVIDER=openai-compatible` (e.g. DashScope, OpenAI) instead. This is because `aws-lc-sys` (AWS crypto library) crashes on musl static linking due to `dlopen(NULL)` incompatibility ([aws-c-cal#213](https://github.com/awslabs/aws-c-cal/issues/213)), and Rust's default `static-pie` output segfaults with musl-gcc ([rust-lang/rust#95926](https://github.com/rust-lang/rust/issues/95926)).

### Transfer to any server

```bash
# Compress
gzip -c target/x86_64-unknown-linux-musl/release/omem-server > omem-server.gz

# Copy to server
scp omem-server.gz user@server:/opt/

# Run (no dependencies needed)
ssh user@server "gunzip /opt/omem-server.gz && chmod +x /opt/omem-server && /opt/omem-server"
```

## API at a Glance

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/tenants` | Create workspace & get API key |
| POST | `/v1/memories` | Store memory or smart-ingest conversation |
| GET | `/v1/memories/search` | 11-stage hybrid search |
| GET | `/v1/memories` | List with filters & pagination |
| GET | `/v1/profile` | Auto-generated user profile |
| POST | `/v1/spaces` | Create shared space |
| POST | `/v1/memories/:id/share` | Share memory to a space |
| POST | `/v1/files` | Upload PDF / image / video / code |
| GET | `/v1/stats` | Analytics & insights |

Full API reference (35 endpoints): [docs/API.md](docs/API.md)

## Documentation

| Document | Description |
|----------|-------------|
| [docs/API.md](docs/API.md) | Complete REST API reference |
| [docs/DEPLOY.md](docs/DEPLOY.md) | Docker & AWS deployment guide |
| [docs/PLUGINS.md](docs/PLUGINS.md) | Plugin installation for all 4 platforms |
| [skills/ourmem/SKILL.md](skills/ourmem/SKILL.md) | AI agent onboarding skill |

## License

Apache-2.0

---

<p align="center">
  <strong>Give your AI a memory. It's about time.</strong><br/>
  <a href="https://ourmem.ai">ourmem.ai</a> · <a href="https://github.com/yhyyz/omem">GitHub</a>
</p>
