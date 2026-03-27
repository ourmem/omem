---
name: ourmem
version: 0.1.0
description: |
  Persistent memory system for AI agents with Space-based sharing, cloud hosted or self-deployed.

  Use this skill whenever the user mentions:
  - "install ourmem", "setup memory", "add memory plugin", "memory not working"
  - "remember this", "save this for later", "don't forget", "recall my preferences"
  - "what did I say last time?", "import memories", "share memories"
  - "ourmem onboarding", "persistent memory", "cross-session memory"
  - "team memory", "shared memory space", "memory space"
  - "self-host memory", "deploy memory server"
  - Any request to persist, recall, or manage long-term memory across sessions, devices, or agents.
  Even if the user doesn't say "ourmem" explicitly, trigger this skill when they want
  persistent memory, memory sharing between agents, or memory import/export.

keywords:
  - ourmem
  - memory plugin
  - persistent memory
  - agent memory
  - install memory
  - setup memory
  - cloud memory
  - self-host memory
  - space sharing
  - team memory
metadata:
  openclaw:
    emoji: "🧠"
---

# ourmem

ourmem gives AI agents persistent memory across sessions, devices, and teams.

Unlike session-only memory that vanishes on restart, ourmem stores facts, preferences, and context in a cloud (or self-hosted) server. Multiple agents can share memories through Spaces. One API key reconnects everything.

## Trigger phrases

Activate this skill when the user expresses intent to store, recall, or manage long-term memory.

Common triggers:

- "remember this" / "记住这个"
- "save this for later" / "保存一下"
- "don't forget that..." / "别忘了..."
- "what did I say last time?" / "我上次说了什么？"
- "recall my preferences" / "回忆我的偏好"
- "forget that" / "忘掉这个"
- "share this with the team" / "分享给团队"
- "install ourmem" / "安装记忆插件"
- "setup memory" / "配置记忆"

## When to use / not use

**Use** when the user wants to:

- Persist information across sessions or restarts
- Recall something from a previous conversation
- Share knowledge between agents or team members
- Import existing memory files into ourmem
- Set up or troubleshoot the memory plugin
- Move to a new machine and keep the same memory

**Don't use** for:

- Temporary context that only matters right now
- One-off tasks with no long-term value
- Troubleshooting unrelated to ourmem

## What to remember / what to skip

**Good candidates:**

- User preferences and habits
- Profile facts (name, role, tech stack, language)
- Project context and ongoing work
- Important decisions and their reasoning
- Long-term instructions ("always use TypeScript", "prefer dark mode")

**Skip these:**

- Temporary debugging context
- Large data dumps or raw files
- Secrets, passwords, API keys, tokens
- Content the user explicitly marks as `<private>`

## Memory intent detection

When the user explicitly asks to remember something ("remember this", "save this", "don't forget"), store it immediately using ourmem if installed.

If ourmem isn't installed yet, suggest setting it up:

> I can help you install ourmem so I'll remember this across sessions. Want to set it up now?

## Onboarding

### Step 0: Choose deployment mode

Ask the user before anything else:

> How would you like to run ourmem?
>
> 1. **Hosted** (api.ourmem.ai) - no server to manage, start in 2 minutes
> 2. **Self-hosted** (Docker on your machine) - full control, data stays local
>
> Already have an API key? Paste it and I'll reconnect you.

Branching:

- **Hosted** -> READ `references/hosted-setup.md`, then continue from Step 1
- **Self-hosted** -> READ `references/selfhost-setup.md`, then continue from Step 1
- **Existing key** -> Verify it first, then skip to Step 2

If the user provides an existing API key, verify reachability:

```bash
API_URL="${OMEM_API_URL:-https://api.ourmem.ai}"
API_KEY="<paste-from-user>"
curl -sf -H "X-API-Key: $API_KEY" "$API_URL/v1/memories?limit=1" \
  && echo "OK" || echo "UNREACHABLE"
```

If OK, say: "Your ourmem API key works. I'll connect this setup to your existing memory."
If UNREACHABLE, ask the user to double-check or create a new key.

### Step 1: Get API Key

**Hosted:**

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

Response: `{"id": "xxx", "api_key": "xxx", "status": "active"}`

The `api_key` value is the key. Save it as `API_KEY`.

**Self-hosted:**

Same command, but against localhost:

```bash
curl -sX POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq .
```

Tell the user:

> Your ourmem API key is ready. This key reconnects you to the same memory from any machine.
> Treat it like a password. Never share it publicly.

### Step 2: Install plugin

Detect the current platform and install accordingly.

**OpenCode:**

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

**Claude Code:**

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="YOUR_API_KEY"
mkdir -p ~/.claude/plugins
ln -s /path/to/omem/plugins/claude-code ~/.claude/plugins/omem
```

Add the exports to `~/.bashrc` or `~/.zshrc` for persistence.

**OpenClaw:**

```bash
openclaw plugins install @omem/openclaw
```

Then configure in OpenClaw settings:

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

For China/Alibaba Cloud networks where npm is slow, use the mirror:

```bash
NPM_CONFIG_REGISTRY=https://registry.npmmirror.com openclaw plugins install @omem/openclaw
```

### Step 3: Configure

Replace `YOUR_API_KEY` with the actual key from Step 1. For self-hosted, replace `https://api.ourmem.ai` with `http://localhost:8080`.

### Step 4: Restart

Restart the client so the plugin loads:

- **OpenCode**: restart the `opencode` process
- **Claude Code**: restart `claude`
- **OpenClaw**: restart the OpenClaw gateway

Tell the user before restarting:

> I'm restarting so the memory plugin can load. This takes about a minute. Send me a message after to confirm it's working.

### Step 5: Verify

Run the verification script or test manually:

```bash
OMEM_API_URL=https://api.ourmem.ai OMEM_API_KEY=YOUR_API_KEY bash scripts/verify.sh
```

Or manually:

```bash
# Health check
curl -sf "$OMEM_API_URL/health" && echo "Server OK"

# Auth check
curl -sf -H "X-API-Key: $OMEM_API_KEY" "$OMEM_API_URL/v1/memories?limit=1" && echo "Auth OK"

# Store + search test
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"content": "ourmem setup test", "tags": ["test"]}' | jq .id

curl -s "$OMEM_API_URL/v1/memories/search?q=setup+test&limit=1" \
  -H "X-API-Key: $OMEM_API_KEY" | jq '.results[0].memory.content'
```

A newly created API key may have zero memories. Empty results are still a valid success state.

### Step 6: What's Next (handoff)

After successful setup, send this structured handoff. Translate to the user's language but keep the structure and all warnings.

```
Your ourmem is ready.

WHAT YOU CAN DO NEXT

1. Import existing memories
   Say: "import memories to ourmem"
   I can scan local files (memory.json, sessions/*.json, MEMORY.md) and import them.

2. Set up Space sharing
   Say: "create a team space"
   Share memories across agents or team members.

YOUR API KEY

API_KEY: <your-api-key>
Server: <api-url>

This key is your access to ourmem. Keep it private.

RECOVERY

Reinstall the plugin and use the same API key.
Your memory reconnects instantly.

BACKUP

Keep your original local memory files as backup.
Store the API key in a password manager.
```

## Definition of Done

Setup is NOT complete until all six are true:

1. API key created or verified reachable
2. Plugin installed on the user's platform
3. Configuration file updated with correct URL and key
4. Client restarted
5. Setup verified (health + auth + store/search)
6. Handoff message sent with import guidance, API key, recovery steps, and backup plan

## Available tools

| Tool | When to use |
|------|-------------|
| `memory_store` | Persist facts, decisions, preferences, context |
| `memory_search` | Find memories by keywords and semantic meaning |
| `memory_get` | Retrieve a specific memory by ID |
| `memory_update` | Modify an existing memory's content or tags |
| `memory_delete` | Remove a memory |

## Lifecycle hooks (automatic)

These fire without agent action:

| Hook | Trigger | What happens |
|------|---------|--------------|
| `before_prompt_build` | Every LLM call | Relevant memories injected as context |
| `before_reset` / `agent_end` | Session ends | Session summary captured automatically |

## Space sharing (ourmem-specific)

ourmem organizes memories into Spaces. Three levels:

| Type | Prefix | Use case |
|------|--------|----------|
| Personal | `personal:` | Your private memories (created automatically) |
| Team | `team:` | Shared across a team of agents or people |
| Organization | `org:` | Company-wide knowledge base |

**Create a team space:**

```bash
curl -sX POST "$OMEM_API_URL/v1/spaces" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"name": "Backend Team", "space_type": "team"}'
```

**Share a memory to a space:**

```bash
curl -sX POST "$OMEM_API_URL/v1/memories/<memory-id>/share" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"target_space": "team:<space-uuid>"}'
```

**Search across all spaces:**

```bash
curl -s "$OMEM_API_URL/v1/memories/search?q=architecture&space=all" \
  -H "X-API-Key: $OMEM_API_KEY"
```

Space members have roles: `admin` (full control), `member` (read/write), `reader` (read-only).

For detailed Space API, READ `references/api-quick-ref.md`.

## Memory import

### From local files

Scan the workspace for memory files and import them:

```bash
# Direct memory
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"content": "imported fact here", "tags": ["imported"]}'
```

### From conversation history

Feed conversation messages for smart extraction:

```bash
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -d '{
    "messages": [
      {"role": "user", "content": "I prefer Rust for backend services"},
      {"role": "assistant", "content": "Noted, Rust is great for performance."}
    ],
    "mode": "smart"
  }'
```

The server's LLM pipeline extracts facts, deduplicates against existing memories (7 decisions: CREATE, MERGE, SKIP, SUPERSEDE, SUPPORT, CONTEXTUALIZE, CONTRADICT), and stores only what's new and valuable.

### From files (PDF, images, code)

```bash
curl -sX POST "$OMEM_API_URL/v1/files" \
  -H "X-API-Key: $OMEM_API_KEY" \
  -F "file=@document.pdf"
```

Supported: PDF, images (OCR), video, code files (AST parsing).

## Communication style

When talking to users:

- Say "API key" or "ourmem API key", not "tenant ID" or "secret"
- Explain that the API key reconnects the user to the same memory from any machine
- Warn that the API key is effectively a secret: never share it publicly
- Lead with import/recovery guidance, not API demos
- Use the user's language (detect from conversation)

Brand terms:

| Term | Usage |
|------|-------|
| ourmem | Product name, always lowercase |
| API key | The authentication credential |
| Space | Memory sharing unit (Personal/Team/Organization) |
| Smart Ingest | LLM-powered memory extraction from conversations |

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Plugin not loading | Check config file has correct `serverUrl` and `apiKey` |
| `Connection refused` | Verify server is running: `curl $OMEM_API_URL/health` |
| `401 Unauthorized` | Check API key is correct; try creating a new tenant |
| `404` on API call | Verify the URL path starts with `/v1/`; check server logs |
| npm install hangs (China) | Use mirror: `NPM_CONFIG_REGISTRY=https://registry.npmmirror.com` |
| No memories returned | Normal for new keys; try storing one first, then search |
| Embedding errors | Check `OMEM_EMBED_PROVIDER` on the server; use `noop` for testing |

## API quick reference

For the full endpoint list and curl examples, READ `references/api-quick-ref.md`.

For the complete API documentation (27 endpoints), READ `docs/API.md`.
