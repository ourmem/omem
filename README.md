<p align="center">
  <strong>🧠 ourmem</strong><br/>
  Shared Memory That Never Forgets
</p>

<p align="center">
  <a href="https://github.com/ourmem/omem/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
  <a href="https://ourmem.ai"><img src="https://img.shields.io/badge/hosted-api.ourmem.ai-green.svg" alt="Hosted"></a>
  <a href="https://github.com/ourmem/omem"><img src="https://img.shields.io/github/stars/ourmem/omem?style=social" alt="Stars"></a>
</p>

<p align="center">
  <strong>English</strong> | <a href="README_CN.md">简体中文</a>
</p>

---

## The Problem

Your AI agents have amnesia — and they work alone.

- 🧠 **Amnesia** — every session starts from zero. Preferences, decisions, context — all gone.
- 🏝️ **Silos** — your Coder agent can't access what your Writer agent learned.
- 📁 **Local lock-in** — memory tied to one machine. Switch devices, lose everything.
- 🚫 **No sharing** — team agents can't share what they know. Every agent re-discovers the same things.
- 🔍 **Dumb recall** — keyword match only. No semantic understanding, no relevance ranking.
- 🧩 **No collective intelligence** — even when agents work on the same team, there's no shared knowledge layer.

**ourmem fixes all of this.**

## What is ourmem

ourmem gives AI agents shared persistent memory — across sessions, devices, agents, and teams. One API key reconnects everything.

🌐 **Website:** [ourmem.ai](https://ourmem.ai)

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

## Core Capabilities

<table>
<tr>
<td width="25%" align="center">
<h4>🔗 Shared Across Boundaries</h4>
Three-tier Spaces — Personal, Team, Organization — let knowledge flow across agents and teams with full provenance tracking.
</td>
<td width="25%" align="center">
<h4>🧠 Never Forget</h4>
Weibull decay model manages the memory lifecycle — core memories persist, peripheral ones gracefully fade. No manual cleanup.
</td>
<td width="25%" align="center">
<h4>🔍 Deep Understanding</h4>
11-stage hybrid retrieval: vector search, BM25, RRF fusion, cross-encoder reranking, and MMR diversity for precise recall.
</td>
<td width="25%" align="center">
<h4>⚡ Smart Evolution</h4>
7-decision reconciliation — CREATE, MERGE, SUPERSEDE, SUPPORT, CONTEXTUALIZE, CONTRADICT, or SKIP — makes memories smarter over time.
</td>
</tr>
</table>

## Feature Overview

| Category | Feature | Details |
|----------|---------|---------|
| **Platforms** | 4 platforms | OpenCode, Claude Code, OpenClaw, MCP Server |
| **Sharing** | Space-based sharing | Personal / Team / Organization with provenance |
| | Provenance tracking | Every shared memory carries full lineage |
| | Quality-gated auto-sharing | Rules filter by importance, category, tags |
| | Cross-space search | Search across all accessible spaces at once |
| **Ingestion** | Smart dedup | 7 decisions: CREATE, MERGE, SKIP, SUPERSEDE, SUPPORT, CONTEXTUALIZE, CONTRADICT |
| | Noise filter | Regex + vector prototypes + feedback learning |
| | Admission control | 5-dimension scoring gate (utility, confidence, novelty, recency, type prior) |
| | Dual-stream write | Sync fast path (<50ms) + async LLM extraction |
| | Post-import intelligence | Batch import → async LLM re-extraction + relation discovery |
| | Adaptive import strategy | Auto/atomic/section/document — heuristic content type detection |
| | Content fidelity | Original text preserved, dual-path search (vector + BM25 on source text) |
| | Cross-reconcile | Discover relations between memories via vector similarity |
| | Batch self-dedup | LLM deduplicates facts within same import batch |
| | Privacy protection | `<private>` tag redaction before storage |
| **Retrieval** | 11-stage pipeline | Vector + BM25 → RRF → reranker → decay → importance → MMR diversity |
| | User Profile | Static facts + dynamic context, <100ms |
| | Retrieval trace | Per-stage explainability (input/output/score/duration) |
| **Lifecycle** | Weibull decay | Tier-specific β (Core=0.8, Working=1.0, Peripheral=1.3) |
| | Three-tier promotion | Peripheral ↔ Working ↔ Core with access-based promotion |
| | Auto-forgetting | TTL detection for time-sensitive info ("tomorrow", "next week") |
| **Multi-modal** | File processing | PDF, image OCR, video transcription, code AST chunking |
| | GitHub connector | Real-time webhook sync for code, issues, PRs |
| **Deploy** | Open source | Apache-2.0 (plugins + docs) |
| | Self-hostable | Single binary, Docker one-liner, ~$5/month |
| | musl static build | Zero-dependency binary for any Linux x86_64 |
| | Hosted option | api.ourmem.ai — nothing to deploy |

## From Isolated Agents to Collective Intelligence

Most AI memory systems trap knowledge in silos. ourmem's three-tier Space architecture enables knowledge flow across agents and teams — with provenance tracking and quality-gated sharing.

> *Research shows collaborative memory reduces redundant work by up to 61% — agents stop re-discovering what their teammates already know.*
> — Collaborative Memory, ICLR 2026

| | Personal | Team | Organization |
|---|----------|------|--------------|
| **Scope** | One user, multiple agents | Multiple users | Company-wide |
| **Example** | Coder + Writer share preferences | Backend team shares arch decisions | Tech standards, security policies |
| **Access** | Owner's agents only | Team members | All org members (read-only) |

**Provenance-tracked sharing** — every shared memory carries its lineage: who shared it, when, and where it came from.

**Quality-gated auto-sharing** — rules filter by importance, category, and tags. Only high-value insights cross space boundaries.

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
        ├── Space Sharing ── Personal / Team / Organization with provenance tracking
        └── Lifecycle ────── Weibull decay, 3-tier promotion (Core/Working/Peripheral), auto-forgetting
```

## Quick Start

### Agent Install (recommended)

One message to your AI agent. It handles everything — API key, plugin install, config, verification.

**Hosted (api.ourmem.ai — nothing to deploy):**

| Platform | Copy this to your agent |
|----------|------------------------|
| **OpenClaw** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for OpenClaw` |
| **Claude Code** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for Claude Code` |
| **OpenCode** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem for OpenCode` |
| **Cursor / VS Code** | `Read https://ourmem.ai/SKILL.md and follow the instructions to install and configure ourmem as MCP Server` |

**Self-hosted (your own server):**

| Platform | How to install |
|----------|---------------|
| **OpenClaw** | Run `openclaw skills install ourmem`, then tell your agent: `setup ourmem in self-hosted mode` |
| **Claude Code** | `Read https://raw.githubusercontent.com/ourmem/omem/main/skills/ourmem/SKILL.md and install ourmem for Claude Code, self-hosted mode` |
| **OpenCode** | `Read https://raw.githubusercontent.com/ourmem/omem/main/skills/ourmem/SKILL.md and install ourmem for OpenCode, self-hosted mode` |

That's it. Your agent handles the rest.

---

<details>
<summary><b>Manual Install</b> (without agent assistance)</summary>

### 1. Get an API Key

**Hosted:**

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

**OpenCode:** Add `"plugin": ["@ourmem/opencode"]` to `opencode.json` + set `OMEM_API_URL` and `OMEM_API_KEY` env vars.

**Claude Code:** `/plugin marketplace add ourmem/omem` + set env vars in `~/.claude/settings.json`.

**OpenClaw:** `openclaw plugins install @ourmem/openclaw` + configure `openclaw.json` with apiUrl and apiKey.

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

### 3. Verify

```bash
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "X-API-Key: $OMEM_API_KEY" -H "Content-Type: application/json" \
  -d '{"content": "I prefer dark mode", "tags": ["preference"]}'

curl -s "$OMEM_API_URL/v1/memories/search?q=dark+mode" -H "X-API-Key: $OMEM_API_KEY"
```

</details>

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

## Memory Space

Browse, search, and manage your agent's memories visually at **[ourmem.ai/space](https://ourmem.ai/space)** — see how memories connect, evolve, and decay over time.

## Security & Privacy

| | |
|---|---|
| **Rust Memory Safety** | No garbage collector, no data races. Ownership model guarantees safety at compile time. |
| **Tenant Isolation** | X-API-Key auth with query-level tenant filtering. Every operation verifies ownership. |
| **Privacy Protection** | `<private>` tag redaction strips sensitive content before storage. |
| **Encryption** | HTTPS for all API transit. Server-side encryption at rest on S3. |
| **Admission Control** | 5-dimension scoring gate rejects low-quality data before storage. |
| **Open Source Auditable** | Apache-2.0 licensed. Audit every line, fork it, run your own instance. |

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

> **Note:** The musl build uses `--no-default-features` which excludes AWS Bedrock support. Use `OMEM_EMBED_PROVIDER=openai-compatible` (e.g. DashScope, OpenAI) instead. This is because `aws-lc-sys` (AWS crypto library) crashes on musl static linking due to `dlopen(NULL)` incompatibility ([aws-c-cal#213](https://github.com/awslabs/aws-c-cal/issues/213)), and Rust's default `static-pie` output segfaults with musl-gcc ([rust-lang/rust#95926](https://github.com/rust-lang/rust/issues/95926)).

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
  <strong>Shared Memory That Never Forgets.</strong><br/>
  <a href="https://ourmem.ai">ourmem.ai</a> · <a href="https://github.com/ourmem/omem">GitHub</a>
</p>
