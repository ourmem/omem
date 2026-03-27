# omem — Claude Code Plugin

Persistent memory for Claude Code, powered by omem.

## Setup

1. Set environment variables:

```bash
export OMEM_API_URL="http://localhost:8080"
export OMEM_API_KEY="your-api-key"
```

2. Install the plugin by symlinking into your Claude Code plugins directory:

```bash
ln -s /path/to/plugins/claude-code ~/.claude/plugins/omem
```

Or copy the plugin directory:

```bash
cp -r plugins/claude-code ~/.claude/plugins/omem
```

## How It Works

### Automatic Hooks

- **SessionStart** — Loads the 20 most recent memories and injects them as context
- **Stop** — Captures the last assistant message (if >50 chars) and stores it as a memory

### On-Demand Skills

- **memory-recall** — Search memories by query
- **memory-store** — Manually save a memory

## API Endpoints Used

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/v1/memories?limit=20` | GET | Load recent memories |
| `/v1/memories` | POST | Store a new memory |
| `/v1/memories/search?q=...` | GET | Search memories |

## Requirements

- `bash` 4+
- `curl`
- `python3` (for JSON processing)
- Running omem-server instance
