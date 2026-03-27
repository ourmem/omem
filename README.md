# 🧠 ourmem — Persistent Memory for AI Agents

> Open-source plugins for AI agent persistent memory with Space-based sharing.
> Cloud hosted at [api.ourmem.ai](https://api.ourmem.ai) or self-deploy with Docker.

## Features

- 🔍 **11-stage hybrid retrieval** — vector + BM25 + RRF fusion + cross-encoder reranker + Weibull decay
- 🧹 **7-decision smart dedup** — CREATE / MERGE / SKIP / SUPERSEDE / SUPPORT / CONTEXTUALIZE / CONTRADICT
- 👤 **User Profile** — auto-generated static facts + dynamic context (<100ms)
- 📦 **Space-based sharing** — Personal / Team / Organization memory spaces
- ⏰ **Weibull decay + 3-tier promotion** — Core / Working / Peripheral with automatic lifecycle
- 🛡️ **Noise filter** — regex patterns + embedded prototypes + feedback learning
- 🎯 **Admission control** — 5-dimension scoring (utility, confidence, novelty, recency, type prior)
- 📄 **Multi-modal** — PDF, image OCR, video transcription, code AST chunking
- 🔗 **GitHub connector** — real-time webhook sync for code, issues, PRs
- 🔒 **Privacy protection** — `<private>` tag redaction
- 🚀 **Cross-platform** — OpenCode, Claude Code, OpenClaw, MCP Server

## Quick Start

### 1. Get an API Key

**Hosted (recommended)**:
```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

**Self-deploy**:
```bash
docker run -d -p 8080:8080 -e OMEM_EMBED_PROVIDER=bedrock ourmem:latest
curl -sX POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

Save the returned `api_key` — this is your access key to your memory space.

### 2. Install for Your Platform

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

ourmem organizes memories into **Spaces** — isolated containers with fine-grained access control.

| Space Type | Scope | Use Case |
|-----------|-------|----------|
| **Personal** | One user, multiple agents | Your Coder + Writer + Researcher agents share preferences |
| **Team** | Multiple users | Backend team shares architecture decisions |
| **Organization** | Company-wide | Tech standards, security policies |

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
```

Each agent sees: **own private** + **shared spaces** + **global**. Agents can only modify their own private memories and shared space memories — never another agent's private data.

## API Reference

Full API documentation: [docs/API.md](docs/API.md)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/tenants` | Create workspace & get API key |
| POST | `/v1/memories` | Store memory or ingest conversation |
| GET | `/v1/memories/search` | Hybrid search (vector + BM25) |
| GET | `/v1/memories` | List with filters & pagination |
| GET | `/v1/profile` | Auto-generated user profile |
| POST | `/v1/spaces` | Create shared space |
| POST | `/v1/memories/:id/share` | Share to another space |
| GET | `/v1/stats` | Analytics dashboard data |

See [docs/API.md](docs/API.md) for all 35 endpoints.

## Self-Deploy

```bash
# Minimal (BM25 search only, no embedding API needed)
docker run -d -p 8080:8080 ourmem:latest

# With Bedrock embedding (recommended, needs AWS credentials)
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=bedrock \
  -e AWS_REGION=us-east-1 \
  ourmem:latest

# With OpenAI embedding
docker run -d -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=openai-compatible \
  -e OMEM_EMBED_API_KEY=sk-xxx \
  ourmem:latest
```

See [docs/DEPLOY.md](docs/DEPLOY.md) for full deployment guide.

## License

Apache-2.0 — plugins and documentation.

---

Built with ❤️ by the [ourmem](https://ourmem.ai) team.
