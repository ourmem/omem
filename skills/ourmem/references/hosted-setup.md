# ourmem Hosted Setup Guide

API Base: `https://api.ourmem.ai`

The hosted version runs on ourmem's cloud infrastructure. No server to manage, no Docker, no S3 buckets. You get an API key and start using it.

## Step 1: Create API Key

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

Response:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "550e8400-e29b-41d4-a716-446655440000",
  "status": "active"
}
```

`api_key` is your credential for all subsequent requests. Save it somewhere safe.

A personal Space is created automatically with your tenant.

## Step 2: Install Plugin

Pick the platform the user is on.

### OpenCode

Add to `opencode.json`:

```json
{
  "plugins": {
    "omem": {
      "package": "@omem/opencode",
      "config": {
        "serverUrl": "https://api.ourmem.ai",
        "apiKey": "YOUR_API_KEY"
      }
    }
  }
}
```

### Claude Code

```bash
# Set environment variables (add to ~/.bashrc or ~/.zshrc)
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="YOUR_API_KEY"

# Install plugin
mkdir -p ~/.claude/plugins
# Option A: from npm (when published)
# Option B: from source
git clone https://github.com/yhyyz/omem.git /tmp/omem
ln -s /tmp/omem/plugins/claude-code ~/.claude/plugins/omem
```

### OpenClaw

```bash
# Install
openclaw plugins install @omem/openclaw

# For China/Alibaba Cloud networks:
NPM_CONFIG_REGISTRY=https://registry.npmmirror.com openclaw plugins install @omem/openclaw
```

Configure in OpenClaw settings:

```json
{
  "plugins": {
    "@omem/openclaw": {
      "serverUrl": "https://api.ourmem.ai",
      "apiKey": "YOUR_API_KEY"
    }
  }
}
```

## Step 3: Configure

Replace `YOUR_API_KEY` in the config above with the actual key from Step 1.

Double-check the `serverUrl` is `https://api.ourmem.ai` (not localhost).

## Step 4: Restart

- **OpenCode**: restart the `opencode` process
- **Claude Code**: restart `claude`
- **OpenClaw**: restart the gateway

The plugin needs a fresh start to load. Wait about a minute, then send a test message.

## Step 5: Verify

```bash
# Health
curl -sf https://api.ourmem.ai/health && echo "OK"

# Auth
curl -sf -H "X-API-Key: YOUR_API_KEY" \
  "https://api.ourmem.ai/v1/memories?limit=1" && echo "Auth OK"

# Store a test memory
curl -sX POST https://api.ourmem.ai/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"content": "ourmem hosted setup complete", "tags": ["test"]}'

# Search it back
curl -s "https://api.ourmem.ai/v1/memories/search?q=hosted+setup&limit=1" \
  -H "X-API-Key: YOUR_API_KEY" | jq '.results[0].memory.content'
```

If all checks pass, return to the main SKILL.md and continue with Step 6 (handoff).
