# ourmem Self-Hosted Setup Guide

API Base: `http://localhost:8080` (or your custom host)

Self-hosting gives you full control over your data. Everything stays on your infrastructure. You need Docker (or the binary) and an S3-compatible store.

## Prerequisites

- Docker Engine 20+ with Docker Compose v2
- OR: the `omem-server` binary (Rust, single file)

## Option A: Docker (recommended)

### Start the server

```bash
git clone https://github.com/yhyyz/omem.git
cd omem
cp .env.example .env
docker-compose up -d
```

This starts two services:

| Service | Port | Description |
|---------|------|-------------|
| omem-server | 8080 | REST API |
| MinIO | 9000/9001 | S3-compatible storage (dev mode) |

### Quick one-liner (no git clone)

If you just want the server running without cloning the repo:

```bash
docker run -d --name omem \
  -p 8080:8080 \
  -e OMEM_EMBED_PROVIDER=noop \
  -e OMEM_S3_BUCKET=omem-data \
  -e AWS_ENDPOINT_URL=http://minio:9000 \
  -e AWS_ACCESS_KEY_ID=minioadmin \
  -e AWS_SECRET_ACCESS_KEY=minioadmin \
  -e AWS_REGION=us-east-1 \
  omem-server:latest
```

Note: this needs a running MinIO or S3-compatible store. For production, point to real S3.

### Verify the server

```bash
curl http://localhost:8080/health
# -> {"status":"ok"}
```

## Option B: Binary

Download the pre-built binary for your platform from the GitHub releases page:

```bash
# Download (replace with actual release URL)
curl -LO https://github.com/ourmem/omem/releases/latest/download/omem-server-linux-amd64
chmod +x omem-server-linux-amd64

# Run
OMEM_PORT=8080 \
OMEM_EMBED_PROVIDER=noop \
./omem-server-linux-amd64
```

## Option C: Build from source (musl static binary)

Build a single static binary that runs on **any Linux x86_64** with zero dependencies:

```bash
rustup target add x86_64-unknown-linux-musl

RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" \
  cargo build --release --target x86_64-unknown-linux-musl \
  -p omem-server --no-default-features

# Binary: target/x86_64-unknown-linux-musl/release/omem-server (182MB, statically linked)
```

Note: `--no-default-features` disables AWS Bedrock support. Use `OMEM_EMBED_PROVIDER=openai-compatible` instead (e.g. DashScope, OpenAI).

Transfer to any server:

```bash
gzip -c target/x86_64-unknown-linux-musl/release/omem-server > omem-server.gz
scp omem-server.gz user@server:/opt/
ssh user@server "gunzip /opt/omem-server.gz && chmod +x /opt/omem-server && /opt/omem-server"
```

## Step 1: Create API Key

```bash
curl -sX POST http://localhost:8080/v1/tenants \
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

Save the `api_key`. A personal Space is created automatically.

## Step 2: Install Plugin

Pick the user's platform. All configs below use `http://localhost:8080` as the server URL. Adjust if your server runs elsewhere.

### OpenCode

Add to `opencode.json`:

```json
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

### Claude Code

```bash
# Set environment variables (add to ~/.bashrc or ~/.zshrc)
export OMEM_API_URL="http://localhost:8080"
export OMEM_API_KEY="YOUR_API_KEY"

# Install plugin
mkdir -p ~/.claude/plugins
ln -s /path/to/omem/plugins/claude-code ~/.claude/plugins/omem
```

### OpenClaw

```bash
openclaw plugins install @omem/openclaw
```

Configure:

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

## Step 3: Configure

Replace `YOUR_API_KEY` with the actual key from Step 1.

If the server runs on a different host or port, update `serverUrl` / `OMEM_API_URL` accordingly.

## Step 4: Restart

- **OpenCode**: restart the `opencode` process
- **Claude Code**: restart `claude`
- **OpenClaw**: restart the gateway

## Step 5: Verify

```bash
# Health
curl -sf http://localhost:8080/health && echo "OK"

# Auth
curl -sf -H "X-API-Key: YOUR_API_KEY" \
  "http://localhost:8080/v1/memories?limit=1" && echo "Auth OK"

# Store a test memory
curl -sX POST http://localhost:8080/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"content": "ourmem self-hosted setup complete", "tags": ["test"]}'

# Search it back
curl -s "http://localhost:8080/v1/memories/search?q=self-hosted+setup&limit=1" \
  -H "X-API-Key: YOUR_API_KEY" | jq '.results[0].memory.content'
```

If all checks pass, return to the main SKILL.md and continue with Step 6 (handoff).

## Environment Variables

Key server-side variables (set in `.env` or Docker environment):

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_PORT` | `8080` | Server port |
| `OMEM_LOG_LEVEL` | `info` | Log level |
| `OMEM_S3_BUCKET` | `omem-data` | S3 bucket for LanceDB |
| `OMEM_EMBED_PROVIDER` | `noop` | Embedding: `noop`, `bedrock`, `openai-compatible` |
| `OMEM_LLM_PROVIDER` | _(empty)_ | LLM: `openai-compatible`, `bedrock` |

For production with real embeddings, set `OMEM_EMBED_PROVIDER=bedrock` (or `openai-compatible` with your endpoint).

### Example: DashScope (Alibaba Cloud)

```bash
OMEM_EMBED_PROVIDER=openai-compatible
OMEM_EMBED_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode
OMEM_EMBED_MODEL=text-embedding-v3
OMEM_EMBED_API_KEY=sk-your-dashscope-key
OMEM_LLM_PROVIDER=openai-compatible
OMEM_LLM_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode
OMEM_LLM_MODEL=qwen-turbo
OMEM_LLM_API_KEY=sk-your-dashscope-key
```

### Example: OpenAI

```bash
OMEM_EMBED_PROVIDER=openai-compatible
OMEM_EMBED_BASE_URL=https://api.openai.com
OMEM_EMBED_MODEL=text-embedding-3-small
OMEM_EMBED_API_KEY=sk-your-openai-key
OMEM_LLM_PROVIDER=openai-compatible
OMEM_LLM_BASE_URL=https://api.openai.com
OMEM_LLM_MODEL=gpt-4o-mini
OMEM_LLM_API_KEY=sk-your-openai-key
```

### Example: AWS Bedrock (glibc build only)

```bash
OMEM_EMBED_PROVIDER=bedrock
OMEM_LLM_PROVIDER=bedrock
OMEM_LLM_MODEL=anthropic.claude-3-haiku-20240307-v1:0
AWS_REGION=us-east-1
```

See `docs/DEPLOY.md` for the full environment variable reference and deployment guide.
