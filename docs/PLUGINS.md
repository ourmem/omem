# omem — Plugin Installation Guide

omem provides plugins for 4 AI coding platforms. Each plugin is a thin HTTP client that connects to the omem-server REST API.

## Prerequisites (All Platforms)

1. **Running omem-server** — See [DEPLOY.md](DEPLOY.md) for setup
2. **API key** — Create a tenant to get one:
   ```bash
   curl -X POST http://localhost:8080/v1/tenants \
     -H "Content-Type: application/json" \
     -d '{"name": "my-workspace"}'
   # → {"id": "abc-123", "api_key": "abc-123", "status": "active"}
   ```

---

## 1. OpenCode

**Package**: `@omem/opencode`  
**Runtime**: Bun  
**Source**: [`plugins/opencode/`](../plugins/opencode/)

### Features

- Auto-recall on session start (injects recent memories into system prompt)
- Auto-capture on session idle (sends conversation to ingest pipeline)
- Keyword detection (Chinese + English memory-related phrases)
- Privacy filtering (`<private>` tag redaction)
- 5 tools: store, search, get, update, delete

### Installation

**Step 1**: Add to your `opencode.json`:

```jsonc
{
  "plugins": {
    "omem": {
      "package": "@omem/opencode",
      "config": {
        "serverUrl": "http://localhost:8080",
        "apiKey": "YOUR_API_KEY"
      }
    }
  }
}
```

**Step 2**: Or install from local path (for development):

```jsonc
{
  "plugins": {
    "omem": {
      "path": "./plugins/opencode",
      "config": {
        "serverUrl": "http://localhost:8080",
        "apiKey": "YOUR_API_KEY"
      }
    }
  }
}
```

### Configuration

See [`plugins/opencode/omem.example.jsonc`](../plugins/opencode/omem.example.jsonc) for all options.

| Option | Default | Description |
|--------|---------|-------------|
| `serverUrl` | `http://localhost:8080` | omem server URL |
| `apiKey` | _(required)_ | Tenant API key |

### Verification

```bash
# Start OpenCode — you should see memory tools available
opencode

# In the session, try:
# /memory-search "your query"
# /memory-store "fact to remember"
```

Check that on session start, recent memories are loaded into context.

---

## 2. Claude Code

**Package**: Shell hooks + Markdown skills  
**Runtime**: Bash 4+, curl, python3  
**Source**: [`plugins/claude-code/`](../plugins/claude-code/)

### Features

- SessionStart hook: loads 20 most recent memories
- Stop hook: captures last assistant message (>50 chars)
- memory-recall skill: search memories by query
- memory-store skill: manually save a memory

### Installation

**Step 1**: Set environment variables:

```bash
export OMEM_API_URL="http://localhost:8080"
export OMEM_API_KEY="YOUR_API_KEY"
```

Add these to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) for persistence.

**Step 2**: Install the plugin by symlinking:

```bash
# Create plugins directory if it doesn't exist
mkdir -p ~/.claude/plugins

# Symlink the plugin
ln -s /path/to/omem/plugins/claude-code ~/.claude/plugins/omem
```

Or copy:

```bash
cp -r /path/to/omem/plugins/claude-code ~/.claude/plugins/omem
```

### Configuration

| Environment Variable | Required | Description |
|---------------------|----------|-------------|
| `OMEM_API_URL` | Yes | omem server URL |
| `OMEM_API_KEY` | Yes | Tenant API key |

### Verification

```bash
# Check plugin is detected
ls ~/.claude/plugins/omem/.claude-plugin/plugin.json

# Start Claude Code — hooks should fire automatically
claude

# Test manually:
curl -s "${OMEM_API_URL}/v1/memories?limit=5" \
  -H "X-API-Key: ${OMEM_API_KEY}" | python3 -m json.tool
```

On session start, you should see recent memories injected into the context. On session end, the last assistant message is automatically captured.

---

## 3. OpenClaw

**Package**: `@omem/openclaw`  
**Runtime**: Node.js  
**Source**: [`plugins/openclaw/`](../plugins/openclaw/)

### Features

- Context engine with 7 lifecycle methods
- Auto-recall via `before_prompt_build` hook
- Auto-capture via `agent_end` event
- Server backend integration (OmemMemoryBackend)
- 5 tools: store, search, get, update, delete

### Installation

**Step 1**: Install the plugin:

```bash
openclaw plugins install @omem/openclaw
```

Or for local development:

```bash
openclaw plugins install ./plugins/openclaw
```

**Step 2**: Configure in OpenClaw settings:

```json
{
  "plugins": {
    "@omem/openclaw": {
      "serverUrl": "http://localhost:8080",
      "apiKey": "YOUR_API_KEY"
    }
  }
}
```

### Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `serverUrl` | `http://localhost:8080` | omem server URL |
| `apiKey` | _(required)_ | Tenant API key |

### Verification

```bash
# Check plugin is installed
openclaw plugins list

# Start OpenClaw — memory tools should appear
openclaw

# Test the context engine
# The plugin should automatically recall relevant memories
# before each prompt and capture insights after each response
```

---

## 4. MCP Server

**Package**: `@omem/mcp`  
**Runtime**: Bun (stdio transport)  
**Source**: [`plugins/mcp/`](../plugins/mcp/)

### Features

- Standard MCP protocol (stdio transport)
- 4 tools: `memory_store`, `memory_search`, `memory_get`, `memory_delete`
- 1 resource: `memory://profile` (user profile)
- Works with any MCP-compatible client (Claude Desktop, etc.)

### Installation

**Option A**: Using `claude mcp add` (Claude Desktop):

```bash
claude mcp add omem -- bunx @omem/mcp
```

**Option B**: Manual MCP configuration:

Add to your MCP client's config (e.g., `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "omem": {
      "command": "bunx",
      "args": ["@omem/mcp"],
      "env": {
        "OMEM_API_URL": "http://localhost:8080",
        "OMEM_API_KEY": "YOUR_API_KEY"
      }
    }
  }
}
```

**Option C**: From local source:

```json
{
  "mcpServers": {
    "omem": {
      "command": "bun",
      "args": ["run", "/path/to/omem/plugins/mcp/src/index.ts"],
      "env": {
        "OMEM_API_URL": "http://localhost:8080",
        "OMEM_API_KEY": "YOUR_API_KEY"
      }
    }
  }
}
```

### Configuration

| Environment Variable | Required | Description |
|---------------------|----------|-------------|
| `OMEM_API_URL` | Yes | omem server URL |
| `OMEM_API_KEY` | Yes | Tenant API key |

### Verification

```bash
# Test the MCP server directly
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | \
  OMEM_API_URL=http://localhost:8080 \
  OMEM_API_KEY=YOUR_API_KEY \
  bunx @omem/mcp

# Should return list of 4 tools
```

In Claude Desktop, you should see the omem tools available in the tools panel.

---

## Troubleshooting

### Common Issues

| Problem | Solution |
|---------|----------|
| `Connection refused` | Ensure omem-server is running: `curl http://localhost:8080/health` |
| `401 Unauthorized` | Check API key is correct and tenant exists |
| `Plugin not detected` | Verify plugin path/installation, restart the client |
| `No memories returned` | Check that memories were ingested: `curl /v1/memories?limit=5` |
| `Embedding errors` | Check `OMEM_EMBED_PROVIDER` config; use `noop` for testing |

### Debug Logging

Enable debug logs on the server:

```bash
OMEM_LOG_LEVEL=debug docker-compose up
```

### Testing API Connectivity

```bash
# From the plugin's environment, test connectivity:
curl -sf http://localhost:8080/health && echo "OK" || echo "FAIL"

# Test with API key:
curl -sf http://localhost:8080/v1/memories?limit=1 \
  -H "X-API-Key: YOUR_API_KEY" && echo "OK" || echo "FAIL"
```
