# ourmem — Claude Code Plugin

Persistent memory for Claude Code — memories survive across sessions, projects, and machines.

## Installation

### Marketplace (recommended)

```bash
/plugin marketplace add ourmem/omem
```

### Local development

```bash
claude --plugin-dir ./plugins/claude-code
```

## Setup

Set your environment variables:

```bash
export OMEM_API_KEY="your-api-key"
export OMEM_API_URL="https://api.ourmem.ai"  # optional, this is the default
```

Get an API key at [ourmem.ai](https://ourmem.ai) or self-host:

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .api_key
```

## What It Does

### Automatic Hooks

| Hook | Trigger | Effect |
|------|---------|--------|
| **SessionStart** | New session begins | Loads 20 most recent memories and injects them as context |
| **Stop** | Session ends | Sends recent conversation to smart-ingest for automatic memory extraction |
| **PreCompact** | Before context compaction | Saves conversation messages before they're compacted away |

### MCP Tools (on-demand)

The plugin bundles the `@ourmem/mcp` server, giving Claude these tools:

| Tool | Purpose |
|------|---------|
| `memory_store` | Save facts, decisions, preferences |
| `memory_search` | Semantic + keyword hybrid search |
| `memory_get` | Retrieve memory by ID |
| `memory_update` | Modify existing memory |
| `memory_delete` | Remove a memory |

### Skills

| Skill | Trigger |
|-------|---------|
| `/ourmem:memory-recall` | Search memories by query |
| `/ourmem:memory-store` | Manually save a memory |

## API Endpoints Used

| Endpoint | Method | Used By |
|----------|--------|---------|
| `/v1/memories?limit=20` | GET | SessionStart hook |
| `/v1/memories` | POST | Stop + PreCompact hooks (smart-ingest) |
| `/v1/memories/search?q=...` | GET | memory-recall skill |
| `/v1/memories` | POST | memory-store skill |

## Requirements

- `bash` 4+
- `curl`
- `python3` (for JSON processing in hooks)
- `OMEM_API_KEY` environment variable set

## Plugin Structure

```
plugins/claude-code/
├── .claude-plugin/
│   └── plugin.json          # Plugin manifest
├── .mcp.json                # MCP server config
├── hooks/
│   ├── hooks.json           # Hook event definitions
│   ├── common.sh            # Shared HTTP utilities
│   ├── session-start.sh     # SessionStart hook
│   ├── stop.sh              # Stop hook (smart-ingest)
│   └── pre-compact.sh       # PreCompact hook
├── skills/
│   ├── memory-recall/
│   │   └── SKILL.md
│   └── memory-store/
│       └── SKILL.md
└── README.md
```
