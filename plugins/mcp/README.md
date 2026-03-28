# @ourmem/mcp

MCP (Model Context Protocol) server for [ourmem](https://ourmem.ai) — persistent memory for AI agents.

Works with **Cursor**, **VS Code Copilot**, **Claude Desktop**, **Windsurf**, and any MCP-compatible client.

## Setup

### 1. Get an API Key

Sign up at [ourmem.ai](https://ourmem.ai) or self-host the server.

### 2. Configure your MCP client

**Claude Desktop** (`~/.claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "ourmem": {
      "command": "npx",
      "args": ["-y", "@ourmem/mcp"],
      "env": {
        "OMEM_API_KEY": "your-api-key",
        "OMEM_API_URL": "https://api.ourmem.ai"
      }
    }
  }
}
```

**Cursor** (`.cursor/mcp.json`):
```json
{
  "mcpServers": {
    "ourmem": {
      "command": "npx",
      "args": ["-y", "@ourmem/mcp"],
      "env": {
        "OMEM_API_KEY": "your-api-key",
        "OMEM_API_URL": "https://api.ourmem.ai"
      }
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `memory_store` | Save a new memory |
| `memory_search` | Semantic search with optional tag filtering |
| `memory_list` | Browse recent memories |
| `memory_ingest` | Ingest conversation messages for smart extraction |
| `memory_get` | Retrieve a memory by ID |
| `memory_update` | Update memory content or tags |
| `memory_forget` | Delete a memory |
| `memory_stats` | Get memory statistics |
| `memory_profile` | View synthesized user profile |

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OMEM_API_KEY` | Yes | — | Your ourmem API key |
| `OMEM_API_URL` | No | `http://localhost:8080` | API server URL |

## License

Apache-2.0
