---
name: ourmem
version: 0.1.0
description: |
  Shared memory that never forgets — cloud hosted or self-deployed.
  Collective intelligence for AI agents with Space-based sharing across agents and teams.

  Use this skill whenever the user mentions:
  - "install ourmem", "setup memory", "add memory plugin", "memory not working"
  - "remember this", "save this for later", "don't forget", "recall my preferences"
  - "what did I say last time?", "import memories", "share memories"
  - "ourmem onboarding", "persistent memory", "cross-session memory"
  - "team memory", "shared memory space", "memory space", "memory analytics"
  - "self-host memory", "deploy memory server", "collective intelligence"
  - "memory decay", "memory stats", "agent collaboration", "knowledge sharing"
  - Any request to persist, recall, or manage long-term memory across sessions, devices, or agents.
  Even if the user doesn't say "ourmem" explicitly, trigger this skill when they want
  persistent memory, memory sharing between agents, memory analytics, or memory import/export.

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
  - collective intelligence
  - memory analytics
  - memory decay
metadata:
  openclaw:
    emoji: "🧠"
---

# ourmem — Shared Memory That Never Forgets

ourmem gives AI agents shared persistent memory — across sessions, devices, agents, and teams.

Most AI memory systems trap knowledge in silos. ourmem's three-tier Space architecture enables collective intelligence: knowledge flows across agents and teams with provenance tracking and quality-gated sharing. One API key reconnects everything. Available as hosted (api.ourmem.ai) or self-deployed.

## When to activate

Trigger this skill when the user wants to:

- Persist information across sessions ("remember this", "save this for later")
- Recall something from before ("what did I say last time?")
- Share knowledge between agents or team members
- Set up or fix the memory plugin
- Import existing memory files
- View memory analytics or stats
- Manage memory spaces or sharing

Don't use for temporary context, one-off tasks, or unrelated troubleshooting.

## What to remember / skip

**Remember:** preferences, profile facts, project context, decisions, long-term instructions
**Skip:** temp debugging, raw data dumps, passwords/tokens/secrets, content inside `<private>` tags

## Memory intent detection

When the user says "remember this" / "save this" / "don't forget" — store it immediately via `memory_store` if ourmem is installed.

If not installed yet:

> I can set up ourmem so I'll remember this across sessions. Takes about 2 minutes. Want to do it now?

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
  -d '{"name": "my-workspace"}'
```

Response: `{"id": "xxx", "api_key": "xxx", "status": "active"}`

**Self-hosted:**

Same command, but against localhost:

```bash
curl -sX POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}'
```

Tell the user:

> Your ourmem API key is ready. This key reconnects you to the same memory from any machine. Treat it like a password — never share it publicly.

### Step 2: Install plugin

Detect the current platform and install. For detailed per-platform instructions, READ `references/hosted-setup.md` or `references/selfhost-setup.md`.

**OpenCode:** `"plugin": ["@ourmem/opencode"]` in opencode.json + env vars
**Claude Code:** `/plugin marketplace add ourmem/omem` + env vars
**OpenClaw:** `openclaw plugins install @ourmem/openclaw` + config
**MCP (Cursor/VS Code/Claude Desktop):** `npx @ourmem/mcp` in MCP config

### Step 3: Configure

Replace `YOUR_API_KEY` with the actual key from Step 1. For self-hosted, replace `https://api.ourmem.ai` with `http://localhost:8080`.

### Step 4: Restart

Restart the client so the plugin loads:

- **OpenCode**: restart the `opencode` process
- **Claude Code**: restart `claude`
- **OpenClaw**: restart the OpenClaw gateway
- **MCP clients**: restart the app

Tell the user before restarting:

> I'm restarting so the memory plugin loads. Send me a message after to confirm it's working.

### Step 5: Verify

```bash
curl -sf "$OMEM_API_URL/health" && echo "Server OK"
curl -sf -H "X-API-Key: $OMEM_API_KEY" "$OMEM_API_URL/v1/memories?limit=1" && echo "Auth OK"

curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"content": "ourmem setup test", "tags": ["test"]}'

curl -s "$OMEM_API_URL/v1/memories/search?q=setup+test" -H "X-API-Key: $OMEM_API_KEY"
```

Empty search results are normal for new keys — the search index builds after the first write.

### Step 6: Handoff

After successful setup, send this (translate to the user's language):

```
ourmem is ready.

WHAT YOU CAN DO NOW

1. Just talk normally — I'll automatically remember important things
2. Say "remember: [something]" to explicitly save a fact
3. Say "create a team space" to share memories between agents
4. Say "import memories" to bring in existing memory files
5. Visit https://ourmem.ai/space to browse and manage your memories visually

YOUR API KEY

  API Key: <key>
  Server:  <api-url>

Keep this key private. Use it to reconnect from any machine or new install.

RECOVERY

Reinstall the plugin with the same API key — your memory reconnects instantly.
```

## Definition of Done

Setup is NOT complete until all six are true:

1. API key created or verified reachable
2. Plugin installed on the user's platform
3. Configuration file updated with correct URL and key
4. Client restarted
5. Setup verified (health + auth + store/search)
6. Handoff message sent with key, recovery steps, and next actions

---

## Tools

| Tool | Purpose |
|------|---------|
| `memory_store` | Save facts, decisions, preferences |
| `memory_search` | Find memories by meaning or keywords |
| `memory_get` | Get a specific memory by ID |
| `memory_update` | Modify content or tags |
| `memory_delete` | Remove a memory |

## Automatic hooks

These fire without user action:

| Hook | When | What happens |
|------|------|--------------|
| Session start | New conversation begins | Recent relevant memories injected into context |
| Session end | Conversation ends | Key information auto-captured and stored |

## Smart Ingest

When conversations are ingested (`"mode": "smart"`), the server runs a multi-stage pipeline:

1. **LLM extraction** — extracts atomic facts classified into 6 categories (profile, preferences, entities, events, cases, patterns)
2. **Noise filter** — regex patterns + vector prototype matching + feedback learning removes low-value content
3. **Admission control** — 5-dimension scoring (utility, confidence, novelty, recency, type prior) gates what gets stored
4. **7-decision reconciliation** — each fact is compared against existing memories: CREATE, MERGE, SKIP, SUPERSEDE, SUPPORT, CONTEXTUALIZE, or CONTRADICT

The memory store gets smarter over time — contradictions are resolved, duplicates are merged, noise is filtered.

## Space sharing

ourmem organizes memories into three-tier Spaces for collective intelligence:

| Type | Prefix | Scope |
|------|--------|-------|
| Personal | `personal/` | One user, multiple agents |
| Team | `team:` | Multiple users |
| Organization | `org/` | Company-wide |

**Roles:** `admin` (full control), `member` (read/write), `reader` (read-only)

Each agent sees: own private + shared spaces. Can modify own + shared. Never another agent's private data. Every shared memory carries provenance — who shared it, when, and where it came from.

For detailed Space API, READ `references/api-quick-ref.md`.

## Memory Space (visual interface)

Users can browse, search, and manage memories visually at **https://ourmem.ai/space** — see how memories connect, evolve, and decay over time.

## Analytics

Memory analytics through the stats API:

- **Overview** (`/v1/stats`) — totals by type, category, tier, space, agent + timeline
- **Space overview** (`/v1/stats/spaces`) — per-space stats, member contributions
- **Sharing flow** (`/v1/stats/sharing`) — who shared what, where, when
- **Agent activity** (`/v1/stats/agents`) — per-agent memory creation, search counts
- **Tag frequency** (`/v1/stats/tags`) — tag usage across spaces
- **Decay curves** (`/v1/stats/decay?memory_id=X`) — Weibull decay visualization
- **Relation graph** (`/v1/stats/relations`) — memory relationship network
- **Server config** (`/v1/stats/config`) — decay parameters, retrieval settings

## Memory import

**From conversation history** (LLM extracts facts automatically):

```bash
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"messages": [{"role":"user","content":"I prefer Rust"}], "mode": "smart"}'
```

**From files** (PDF, images, code):

```bash
curl -sX POST "$OMEM_API_URL/v1/files" -H "X-API-Key: $OMEM_API_KEY" -F "file=@doc.pdf"
```

**Direct fact:**

```bash
curl -sX POST "$OMEM_API_URL/v1/memories" \
  -H "Content-Type: application/json" -H "X-API-Key: $OMEM_API_KEY" \
  -d '{"content": "User prefers dark mode", "tags": ["preference"]}'
```

## Security

- **Tenant isolation** — every API call scoped via X-API-Key. Data physically separated per tenant.
- **Privacy protection** — `<private>` tag redaction strips sensitive content before storage.
- **Admission control** — 5-dimension scoring gate rejects low-quality data before storage.
- **Open source** — Apache-2.0 licensed plugins. Audit every line of code.

## Communication style

- Say "API key", not "tenant ID" or "secret"
- Explain that the API key reconnects memory from any machine
- Warn that the key is secret — never share publicly
- Use the user's language (detect from conversation)
- Brand: "ourmem" (lowercase), "Space" (capitalized), "Smart Ingest"

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Plugin not loading | Check config has correct `apiUrl`/`serverUrl` and `apiKey` |
| Connection refused | Verify server is running: `curl $OMEM_API_URL/health` |
| 401 Unauthorized | Check API key; try creating a new tenant |
| 404 on API call | URL path should start with `/v1/` |
| npm install hangs | Use mirror: `NPM_CONFIG_REGISTRY=https://registry.npmmirror.com` |
| No memories returned | Normal for new keys — store one first, then search |
| Search returns empty | Index builds after first write — wait a moment and retry |
| Embedding errors | Check `OMEM_EMBED_PROVIDER` on the server; use `noop` for testing |

## API quick reference

For the full endpoint list and curl examples, READ `references/api-quick-ref.md`.

For the complete API documentation (35 endpoints), READ `docs/API.md`.
