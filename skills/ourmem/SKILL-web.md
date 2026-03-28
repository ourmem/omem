---
name: ourmem
version: 0.1.0
description: |
  Shared memory that never forgets — hosted at api.ourmem.ai.
  Collective intelligence for AI agents with Space-based sharing across agents and teams.
  Use when users mention: "install ourmem", "setup memory", "add memory plugin",
  "memory not working", "remember this", "save this", "don't forget",
  "recall preferences", "what did I say last time", "import memories",
  "share memories", "team memory", "shared space", "persistent memory",
  "cross-session memory", "ourmem onboarding", "collective intelligence",
  "agent collaboration", "knowledge sharing", "memory decay", "memory space",
  "memory analytics", "memory stats".
  Even if the user doesn't say "ourmem", trigger when they want persistent memory,
  memory sharing between agents, memory analytics, or memory import/export.
keywords:
  - ourmem
  - memory
  - persistent memory
  - agent memory
  - remember
  - recall
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

Most AI memory systems trap knowledge in silos. ourmem's three-tier Space architecture enables collective intelligence: knowledge flows across agents and teams with provenance tracking and quality-gated sharing. One API key reconnects everything. Hosted at `api.ourmem.ai` — nothing to deploy.

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

---

## Setup (hosted — api.ourmem.ai)

### Step 1: Get API Key

```bash
curl -sX POST https://api.ourmem.ai/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}'
```

Response: `{"id": "...", "api_key": "xxx-xxx-xxx", "status": "active"}`

Save the `api_key` value. Tell the user:

> Your ourmem API key is ready. This key reconnects you to the same memory from any machine. Treat it like a password — never share it publicly.

If the user already has a key, verify it:

```bash
curl -sf -H "X-API-Key: THE_KEY" "https://api.ourmem.ai/v1/memories?limit=1" && echo "OK"
```

### Step 2: Install plugin

Detect the current platform and install:

**OpenClaw:**

```bash
openclaw plugins install @ourmem/openclaw
```

Configure in OpenClaw settings:

```json
{
  "plugins": {
    "slots": { "memory": "ourmem" },
    "entries": {
      "ourmem": {
        "enabled": true,
        "config": {
          "apiUrl": "https://api.ourmem.ai",
          "apiKey": "YOUR_API_KEY"
        }
      }
    },
    "allow": ["ourmem"]
  }
}
```

For slow npm networks, use a mirror:

```bash
NPM_CONFIG_REGISTRY=https://registry.npmmirror.com openclaw plugins install @ourmem/openclaw
```

**Claude Code:**

Set environment variables (add to `~/.bashrc` or `~/.zshrc` for persistence):

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="YOUR_API_KEY"
```

Install the plugin:

```bash
/plugin marketplace add ourmem/omem
/plugin install ourmem@ourmem/omem
```

**OpenCode:**

Add to `opencode.json`:

```json
{
  "plugin": ["@ourmem/opencode"]
}
```

Set environment variables:

```bash
export OMEM_API_URL="https://api.ourmem.ai"
export OMEM_API_KEY="YOUR_API_KEY"
```

**MCP (Cursor / VS Code / Claude Desktop):**

```json
{
  "mcpServers": {
    "ourmem": {
      "command": "npx",
      "args": ["@ourmem/mcp"],
      "env": {
        "OMEM_API_URL": "https://api.ourmem.ai",
        "OMEM_API_KEY": "YOUR_API_KEY"
      }
    }
  }
}
```

### Step 3: Replace YOUR_API_KEY

In every config above, replace `YOUR_API_KEY` with the actual key from Step 1.

### Step 4: Restart

Restart the client so the plugin loads:

- **OpenClaw**: restart the gateway
- **Claude Code**: restart `claude`
- **OpenCode**: restart the `opencode` process
- **MCP clients**: restart the app

Tell the user before restarting:

> I'm restarting so the memory plugin loads. Send me a message after to confirm it's working.

### Step 5: Verify

```bash
# Health
curl -sf https://api.ourmem.ai/health && echo "Server OK"

# Auth
curl -sf -H "X-API-Key: YOUR_API_KEY" "https://api.ourmem.ai/v1/memories?limit=1" && echo "Auth OK"

# Store + search
curl -sX POST https://api.ourmem.ai/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"content": "ourmem setup test", "tags": ["test"]}'

curl -s "https://api.ourmem.ai/v1/memories/search?q=setup+test" \
  -H "X-API-Key: YOUR_API_KEY"
```

Empty search results are normal for new keys — the search index builds after the first write.

### Step 6: Handoff

After successful setup, send this (translate to the user's language):

```
ourmem is ready.

WHAT YOU CAN DO NOW

1. Import your existing memories (recommended — say "import memories to ourmem")
   I'll scan local files and import your history automatically.
2. Just talk normally — I'll automatically remember important things
3. Say "remember: [something]" to explicitly save a fact
4. Say "create a team space" to share memories between agents
5. Say "import memories" to bring in existing memory files
6. Visit https://ourmem.ai/space to browse and manage your memories visually

YOUR API KEY

  API Key: <key>
  Server:  https://api.ourmem.ai

Keep this key private. Use it to reconnect from any machine or new install.

RECOVERY

Reinstall the plugin with the same API key — your memory reconnects instantly.
```

## Definition of Done

Setup is NOT complete until all six are true:

1. API key created or verified reachable
2. Plugin installed for the user's platform
3. Config updated with correct URL and key
4. Client restarted
5. Verified: health + auth + store/search all pass
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

1. **LLM extraction** — extracts atomic facts from conversation, classified into 6 categories (profile, preferences, entities, events, cases, patterns)
2. **Noise filter** — regex patterns + vector prototype matching + feedback learning removes low-value content
3. **Admission control** — 5-dimension scoring (utility, confidence, novelty, recency, type prior) gates what gets stored
4. **7-decision reconciliation** — each fact is compared against existing memories: CREATE (new), MERGE (refine), SKIP (duplicate), SUPERSEDE (outdated), SUPPORT (reinforce), CONTEXTUALIZE (add nuance), or CONTRADICT (conflicting)

This means the memory store gets smarter over time — contradictions are resolved, duplicates are merged, noise is filtered.

## Space sharing

ourmem organizes memories into three-tier Spaces for collective intelligence:

| Type | Scope | Example |
|------|-------|---------|
| Personal | One user, multiple agents | Your Coder + Writer share preferences |
| Team | Multiple users | Backend team shares architecture decisions |
| Organization | Company-wide | Tech standards, security policies |

**Roles:** `admin` (full control), `member` (read/write), `reader` (read-only)

Each agent sees: own private + shared spaces. Can modify own + shared. Never another agent's private data. Every shared memory carries provenance — who shared it, when, and where it came from.

### When to suggest Spaces

Proactively suggest Spaces when:
- User has multiple agents ("I also use Claude Code for writing") → suggest sharing preferences across agents
- User mentions team collaboration ("share this with my team") → suggest creating a team space
- User wants org-wide knowledge ("company coding standards") → suggest organization space

How to explain it:

> Right now your memories are private to this agent. If you want your other agents to also access these memories, I can set up sharing. Want me to create a shared space?

### Space operations

**List my spaces:**

```bash
curl -s https://api.ourmem.ai/v1/spaces -H "X-API-Key: YOUR_API_KEY"
```

**Create a team space:**

```bash
curl -sX POST https://api.ourmem.ai/v1/spaces \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"name": "Backend Team", "space_type": "team"}'
```

**Add a member to a space:**

```bash
curl -sX POST "https://api.ourmem.ai/v1/spaces/SPACE_ID/members" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"user_id": "colleague-tenant-id", "role": "member"}'
```

**Share a memory to a space:**

```bash
curl -sX POST "https://api.ourmem.ai/v1/memories/MEMORY_ID/share" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"target_space": "team/SPACE_ID"}'
```

**Batch share multiple memories:**

```bash
curl -sX POST https://api.ourmem.ai/v1/memories/batch-share \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"memory_ids": ["id1", "id2", "id3"], "target_space": "team/SPACE_ID"}'
```

**Pull a memory from another space to personal:**

```bash
curl -sX POST "https://api.ourmem.ai/v1/memories/MEMORY_ID/pull" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"source_space": "team/SPACE_ID"}'
```

**Set up auto-sharing rules** (high-importance decisions auto-flow to team):

```bash
curl -sX POST "https://api.ourmem.ai/v1/spaces/SPACE_ID/auto-share-rules" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"source_space": "personal/USER_ID", "categories": ["cases", "patterns"], "min_importance": 0.7}'
```

After sharing, search automatically spans all spaces the user has access to — no extra configuration needed.

## Memory Space (visual interface)

Users can browse, search, and manage memories visually at **https://ourmem.ai/space** — see how memories connect, evolve, and decay over time.

## Analytics

ourmem provides memory analytics through the stats API:

- **Overview** (`/v1/stats`) — totals by type, category, tier, space, agent + timeline
- **Space overview** (`/v1/stats/spaces`) — per-space stats, member contributions, sharing activity
- **Sharing flow** (`/v1/stats/sharing`) — who shared what, where, when + flow graph
- **Agent activity** (`/v1/stats/agents`) — per-agent memory creation, search counts, top categories
- **Tag frequency** (`/v1/stats/tags`) — tag usage across spaces
- **Decay curves** (`/v1/stats/decay?memory_id=X`) — Weibull decay visualization for any memory
- **Relation graph** (`/v1/stats/relations`) — memory relationship network with cross-space edges
- **Server config** (`/v1/stats/config`) — decay parameters, promotion thresholds, retrieval settings

## Memory import

When the user says "import memories to ourmem", scan their workspace for existing memory and session files, then batch-import them via the Import API.

### Auto-scan and import flow

1. Detect agent_id from platform config (if available)
2. Scan workspace for memory/session files
3. Upload the 20 most recent files (by modification time) via `/v1/imports`
4. Report results (imported/skipped/errors)

**Paths to scan:**

| Path pattern | file_type | Platform |
|-------------|-----------|----------|
| `./memory.json` | memory | OpenClaw |
| `./memories.json` | memory | OpenClaw |
| `./memories/*.json` | memory | OpenClaw |
| `./sessions/*.json` | session | OpenClaw |
| `./session/*.json` | session | OpenClaw |
| `./.claude/memory/*.json` | memory | Claude Code |
| `./MEMORY.md` | markdown | Common |
| `./memory/*.md` | markdown | Common |

### Import API

**Batch import a file (with auto intelligence):**

```bash
curl -sX POST https://api.ourmem.ai/v1/imports \
  -H "X-API-Key: YOUR_API_KEY" \
  -F "file=@memory.json" \
  -F "file_type=memory" \
  -F "agent_id=coder" \
  -F "strategy=auto"
```

**Strategy parameter** controls how content is chunked before LLM extraction:

| Strategy | When auto-detected | Behavior |
|----------|-------------------|----------|
| `auto` (default) | — | Heuristic detection based on content type |
| `atomic` | Short JSON facts, key-value pairs | Each item = one memory, minimal LLM processing |
| `section` | Markdown with headings, structured docs | Split by sections (headings/paragraphs), LLM extracts per section |
| `document` | Long prose, conversations, session logs | Entire file as one chunk, LLM extracts all facts holistically |

By default, `post_process=true`: after storage completes, a background task runs LLM re-extraction (same as Smart Ingest) + reconciliation to discover relations. The imported fragments are merged into large chunks, LLM extracts atomic facts with proper categories and L0/L1/L2 summaries, then Reconciler discovers SUPPORT/CONTEXTUALIZE/CONTRADICT/SUPERSEDE relations.

The `content` field preserves original source text — vector embeddings and BM25 index are built from the original text, ensuring content fidelity and language preservation (e.g., Chinese input stays Chinese).

When the database is empty, batch self-dedup kicks in: the LLM deduplicates facts within the same import batch to avoid redundancy. Reconciliation runs serially (semaphore=1) to prevent race conditions, while extraction runs in parallel (semaphore=3) for throughput.

To skip intelligence processing: add `-F "post_process=false"`.

Supported `file_type`: `memory` (JSON array), `session` (JSON/JSONL messages), `markdown` (split by paragraphs), `jsonl` (one JSON per line).

Optional fields: `agent_id`, `session_id`, `space_id` (defaults to personal space), `post_process` (default true), `strategy` (default auto: auto/atomic/section/document).

**Cross-reconcile (discover relations via vector similarity):**

```bash
curl -sX POST "https://api.ourmem.ai/v1/imports/cross-reconcile" -H "X-API-Key: YOUR_API_KEY"
```

Scans all memories and discovers SUPPORT/CONTEXTUALIZE/CONTRADICT/SUPERSEDE relations between them using vector similarity. Useful after multiple imports to connect related memories across batches.

**Check import progress:**

```bash
curl -s "https://api.ourmem.ai/v1/imports/IMPORT_ID" -H "X-API-Key: YOUR_API_KEY"
```

Returns phase-by-phase progress: storage → extraction → reconciliation.

**Manually trigger intelligence on a past import:**

```bash
curl -sX POST "https://api.ourmem.ai/v1/imports/IMPORT_ID/intelligence" -H "X-API-Key: YOUR_API_KEY"
```

Response:

```json
{
  "id": "task-uuid",
  "status": "completed",
  "imported": 15,
  "skipped": 2,
  "errors": []
}
```

### Smart ingest from conversation history

```bash
curl -sX POST https://api.ourmem.ai/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"messages": [{"role":"user","content":"I prefer Rust"}], "mode": "smart"}'
```

### Direct fact

```bash
curl -sX POST https://api.ourmem.ai/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"content": "User prefers dark mode", "tags": ["preference"]}'
```

## Security

- **Tenant isolation** — every API call is scoped to your tenant via X-API-Key. Data is physically separated per tenant.
- **Privacy protection** — `<private>` tag redaction strips sensitive content before storage.
- **Admission control** — 5-dimension scoring gate rejects low-quality or noisy data before it enters the memory store.
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
| Plugin not loading | Check config has correct `apiUrl` and `apiKey` |
| Connection refused | Server may be down — try again in a minute |
| 401 Unauthorized | API key is wrong — verify or create a new tenant |
| 404 on API call | URL path should start with `/v1/` |
| npm install hangs | Use mirror: `NPM_CONFIG_REGISTRY=https://registry.npmmirror.com` |
| No memories returned | Normal for new keys — store one first, then search |
| Search returns empty | Index builds after first write — wait a moment and retry |

## API quick reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| **Core** | | |
| POST | `/v1/tenants` | Create workspace, get API key |
| POST | `/v1/memories` | Store memory or smart-ingest conversation |
| GET | `/v1/memories/search?q=` | Hybrid search (vector + keyword) |
| GET | `/v1/memories?limit=20` | List with filters + pagination |
| GET | `/v1/memories/:id` | Get single memory |
| PUT | `/v1/memories/:id` | Update memory |
| DELETE | `/v1/memories/:id` | Soft delete |
| POST | `/v1/memories/batch-delete` | Batch delete by IDs or filter |
| DELETE | `/v1/memories/all` | Clear all memories (requires X-Confirm header) |
| POST | `/v1/imports/{id}/rollback` | Rollback an import (delete memories + sessions) |
| POST | `/v1/imports` | Batch import file (strategy=auto\|atomic\|section\|document) |
| GET | `/v1/imports/{id}` | Check import progress |
| POST | `/v1/imports/{id}/intelligence` | Trigger intelligence on past import |
| POST | `/v1/imports/cross-reconcile` | Discover relations via vector similarity |
| GET | `/v1/profile` | User profile (static + dynamic) |
| **Spaces** | | |
| POST | `/v1/spaces` | Create shared space |
| GET | `/v1/spaces` | List accessible spaces |
| POST | `/v1/spaces/:id/members` | Add member to space |
| **Sharing** | | |
| POST | `/v1/memories/:id/share` | Share to a space |
| POST | `/v1/memories/:id/pull` | Pull from another space |
| POST | `/v1/memories/batch-share` | Batch share multiple memories |
| **Files** | | |
| POST | `/v1/files` | Upload file (PDF/image/video/code) |
| **Analytics** | | |
| GET | `/v1/stats` | Global stats (by type/category/tier/space/agent) |
| GET | `/v1/stats/spaces` | Per-space overview |
| GET | `/v1/stats/sharing` | Sharing flow analysis |
| GET | `/v1/stats/agents` | Agent activity |
| GET | `/v1/stats/tags` | Tag frequency |
| GET | `/v1/stats/decay?memory_id=X` | Decay curve for a memory |
| GET | `/v1/stats/relations` | Memory relationship graph |
| GET | `/v1/stats/config` | Server config parameters |
| **System** | | |
| GET | `/health` | Health check (no auth) |

Full API (35 endpoints): https://github.com/ourmem/omem/blob/main/docs/API.md
