# ourmem — Plugin Installation Guide

ourmem provides plugins for 4 AI coding platforms. Each plugin is a thin HTTP client that connects to the omem-server REST API.

## Prerequisites (All Platforms)

1. **Running omem-server** — Use the hosted service at `api.ourmem.ai` or self-host (see [DEPLOY.md](DEPLOY.md))
2. **API key** — Create a tenant to get one:
   ```bash
   # Hosted
   curl -sX POST https://api.ourmem.ai/v1/tenants \
     -H "Content-Type: application/json" \
     -d '{"name": "my-workspace"}' | jq .
   # → {"id": "abc-123", "api_key": "abc-123", "status": "active"}

   # Self-hosted
   curl -sX POST http://localhost:8080/v1/tenants \
     -H "Content-Type: application/json" \
     -d '{"name": "my-workspace"}' | jq .
   ```

---

## 1. OpenCode

**Package**: `@ourmem/opencode`
**Version**: 0.3.0
**Runtime**: Bun / Node
**Source**: [`plugins/opencode/`](../plugins/opencode/)

### Features

| Feature | How it works |
|---------|-------------|
| Auto-recall on first message | Semantic search using the first user message, plus profile injection into system prompt |
| Keyword detection | Detects "remember", "save this", "记住", "记下来", etc. and nudges the agent to use `memory_store` (code-block aware) |
| Context preservation on compaction | `session.compacting` hook re-injects top 20 memories so context survives compaction |
| Privacy filtering | `<private>` tag redaction before storage |
| 11 tools | **Memory:** `memory_store`, `memory_search`, `memory_get`, `memory_update`, `memory_delete` · **Sharing:** `space_create`, `space_list`, `space_add_member`, `memory_share`, `memory_pull`, `memory_reshare` |

### Installation

**Step 1**: Add to your `opencode.json`:

```json
{
  "plugin": ["@ourmem/opencode"],
  "plugin_config": {
    "@ourmem/opencode": {
      "apiUrl": "https://api.ourmem.ai",
      "apiKey": "YOUR_API_KEY"
    }
  }
}
```

The `plugin_config` field is the highest priority config source. The plugin also reads `~/.config/ourmem/config.json` (global) or `OMEM_API_URL`/`OMEM_API_KEY` env vars as alternatives.

### Verification

```bash
# Start OpenCode — you should see 11 tools available (5 memory + 6 sharing)
opencode

# In the session, try:
# "search my memories for dark mode"
# "remember that I prefer Rust over Go"
```

On the first message of each session, relevant memories and your user profile are automatically injected into context.

---

## 2. Claude Code

**Package**: Marketplace plugin (bash hooks + skills + bundled MCP)
**Version**: 0.3.0
**Runtime**: Bash 4+, curl, python3
**Source**: [`plugins/claude-code/`](../plugins/claude-code/)

### Features

| Feature | How it works |
|---------|-------------|
| 3 hooks | **SessionStart** (load 20 recent memories), **Stop** (smart-ingest last conversation messages), **PreCompact** (save conversation before context compaction) |
| 2 skills | `memory-recall` (search by query), `memory-store` (manually save a memory) |
| 15 MCP tools | Bundled `@ourmem/mcp` via `.mcp.json`: **Memory:** `memory_store`, `memory_search`, `memory_list`, `memory_ingest`, `memory_get`, `memory_update`, `memory_forget`, `memory_stats`, `memory_profile` · **Sharing:** `space_create`, `space_list`, `space_add_member`, `memory_share`, `memory_pull`, `memory_reshare` |
| Graceful degradation | If `OMEM_API_KEY` is not set, hooks skip silently and print setup instructions |

### Installation

**Step 1**: Configure credentials in `~/.claude/settings.json` (Claude Code's native config):

```json
{
  "env": {
    "OMEM_API_URL": "https://api.ourmem.ai",
    "OMEM_API_KEY": "YOUR_API_KEY"
  }
}
```

Claude Code auto-injects `env` fields into the process environment. This is the recommended approach.

> **Alternative:** You can also `export OMEM_API_KEY=...` in your shell profile as a fallback.

**Step 2**: Install from the marketplace:

```
/plugin marketplace add ourmem/omem
/plugin install ourmem@ourmem
```

For local development instead:

```bash
claude --plugin-dir ./plugins/claude-code
```

### Verification

```bash
# Start Claude Code — hooks fire automatically
claude

# Test manually:
curl -s "${OMEM_API_URL}/v1/memories?limit=5" \
  -H "X-API-Key: ${OMEM_API_KEY}" | python3 -m json.tool
```

On session start, recent memories are injected into context. On session end, the conversation is sent to smart-ingest for automatic memory extraction. Before context compaction, conversation messages are saved so nothing is lost.

---

## 3. OpenClaw

**Package**: `@ourmem/ourmem`
**Version**: 0.3.0
**Runtime**: Node.js
**Source**: [`plugins/openclaw/`](../plugins/openclaw/)

### Features

| Feature | How it works |
|---------|-------------|
| 3 hooks | **before_prompt_build** (semantic search using prompt text), **agent_end** (smart-ingest with Claude content block handling), **before_reset** (save user messages before daily reset) |
| 11 tools | **Memory:** `memory_store`, `memory_search`, `memory_get`, `memory_update`, `memory_delete` · **Sharing:** `space_create`, `space_list`, `space_add_member`, `memory_share`, `memory_pull`, `memory_reshare` |
| ContextEngine | 7 lifecycle hooks for deep integration with OpenClaw's agent loop |
| Claude content blocks | Handles Claude's array-of-blocks content format, not just plain strings |
| Object export | `{id, name, register()}` format for OpenClaw's plugin system |

### Installation

**Step 1**: Install the plugin:

```bash
openclaw plugins install @ourmem/ourmem
```

**Step 2**: Configure in `openclaw.json`:

```json
{
  "plugins": {
    "entries": {
      "ourmem": {
        "apiUrl": "https://api.ourmem.ai",
        "apiKey": "YOUR_API_KEY"
      }
    }
  }
}
```

The plugin also reads `OMEM_API_URL` and `OMEM_API_KEY` from environment variables as fallback.

### Verification

```bash
# Check plugin is installed
openclaw plugins list

# Start OpenClaw — memory tools should appear
openclaw

# The plugin automatically recalls relevant memories before each prompt
# and captures insights after each agent response
```

### Companion Workflow: TweetClaw

OpenClaw agents can pair ourmem with TweetClaw when X/Twitter research or social automation needs durable memory. Use TweetClaw for live X/Twitter work such as search tweets, search tweet replies, follower export, user lookup, monitor tweets, webhooks, media workflows, post tweets, post tweet replies, and giveaway draws. Use ourmem to store concise decisions, selected public source URLs, campaign context, and follow-up notes.

See [TweetClaw OpenClaw Memory Workflow](TWEETCLAW_OPENCLAW.md) for install, verification, credential boundaries, and a memory shape that avoids storing secrets or raw exports.

---

## 4. MCP Server

**Package**: `@ourmem/mcp`
**Version**: 0.3.0
**Runtime**: Node.js (stdio transport)
**Source**: [`plugins/mcp/`](../plugins/mcp/)

### Features

| Feature | Details |
|---------|---------|
| 15 tools | **Memory:** `memory_store`, `memory_search`, `memory_list`, `memory_ingest`, `memory_get`, `memory_update`, `memory_forget`, `memory_stats`, `memory_profile` · **Sharing:** `space_create`, `space_list`, `space_add_member`, `memory_share`, `memory_pull`, `memory_reshare` |
| 1 resource | `omem://profile` (synthesized user profile) |
| Standard MCP | Works with Cursor, VS Code Copilot, Claude Desktop, Windsurf, and any MCP-compatible client |

### Installation

Add to your MCP client's config file:

**Cursor** (`.cursor/mcp.json`), **VS Code** (`.vscode/mcp.json`), **Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "ourmem": {
      "command": "npx",
      "args": ["-y", "@ourmem/mcp"],
      "env": {
        "OMEM_API_URL": "https://api.ourmem.ai",
        "OMEM_API_KEY": "YOUR_API_KEY"
      }
    }
  }
}
```

Or add via CLI (Claude Desktop):

```bash
claude mcp add ourmem -- npx -y @ourmem/mcp
```

### Verification

```bash
# Test the MCP server directly
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | \
  OMEM_API_URL=https://api.ourmem.ai \
  OMEM_API_KEY=YOUR_API_KEY \
  npx -y @ourmem/mcp

# Should return list of 15 tools
```

In your MCP client, you should see the ourmem tools available in the tools panel.

---

## Troubleshooting

### Common Issues

| Problem | Solution |
|---------|----------|
| `Connection refused` | Ensure omem-server is running: `curl https://api.ourmem.ai/health` (or `http://localhost:8080/health` for self-hosted) |
| `401 Unauthorized` | Check API key is correct and tenant exists |
| `Plugin not detected` | Verify plugin path/installation, restart the client |
| `No memories returned` | Check that memories were ingested: `curl /v1/memories?limit=5 -H "X-API-Key: YOUR_KEY"` |
| `Embedding errors` | Check `OMEM_EMBED_PROVIDER` config on the server; use `noop` for testing |
| `npx` not found | MCP server requires Node.js 18+. Install from [nodejs.org](https://nodejs.org/) |

### Debug Logging

Enable debug logs on the server:

```bash
RUST_LOG=debug ./omem-server
```

Or with Docker:

```bash
docker run -e RUST_LOG=debug -p 8080:8080 ghcr.io/ourmem/omem-server:latest
```

### Testing API Connectivity

```bash
# Health check
curl -sf https://api.ourmem.ai/health && echo "OK" || echo "FAIL"

# Test with API key
curl -sf https://api.ourmem.ai/v1/memories?limit=1 \
  -H "X-API-Key: YOUR_API_KEY" && echo "OK" || echo "FAIL"
```
