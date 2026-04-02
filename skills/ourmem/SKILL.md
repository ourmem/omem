---
name: ourmem
version: 1.1.0
description: |
  Shared memory that never forgets. Cloud hosted or self-deployed.
  Collective intelligence for AI agents with Space-based sharing across agents and teams.

  Use when users say:
  - "install ourmem" / "install omem"
  - "setup memory" / "setup omem"
  - "add memory plugin"
  - "ourmem onboarding" / "omem onboarding"
  - "memory not working"
  - "remember this"
  - "save this for later"
  - "don't forget"
  - "recall preferences"
  - "what did I say last time"
  - "import memories"
  - "share memories"
  - "share with user"
  - "share memories to someone"
  - "team memory"
  - "shared space"
  - "persistent memory"
  - "cross-session memory"
  - "collective intelligence"
  - "memory analytics"
  - "memory stats"
  - "self-host memory"
  - "deploy memory server"

  Even if the user doesn't say "ourmem" or "omem", trigger when they want persistent memory,
  memory sharing between agents, memory analytics, or memory import/export.
keywords:
  - ourmem
  - omem
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
  - self-host
metadata:
  openclaw:
    emoji: "🧠"
---

# ourmem (omem) — Shared Memory That Never Forgets

ourmem gives AI agents shared persistent memory across sessions, devices, agents, and teams. One API key reconnects everything. Available as hosted (`api.ourmem.ai`) or self-deployed.

---

## Key Concepts

**API Key = Tenant ID = Your Identity.** When you create a tenant (`POST /v1/tenants`), the returned `id` and `api_key` are the same UUID. There is no separate "tenant ID".

**One API Key owns multiple Spaces:**

| Concept | What it is | Example |
|---------|-----------|---------|
| **API Key** | Your identity (`X-API-Key` header) | `a1b2c3d4-...` (1 per user) |
| **Space ID** | A memory storage address | `personal/a1b2c3d4`, `team/xxx`, `org/yyy` |

Personal Space is auto-created. You can create additional Team and Organization Spaces.

**Sharing = passing the other user's API Key as `target_user`.** The system auto-creates a bridging Team Space. No manual space management needed.

---

## When to Use This Skill

Use when the user wants persistent or shared memory across sessions, devices, or agents. Common triggers: "remember this", "save this", "don't forget", "what did I say last time", "share with my team", "import memories", "setup memory", "self-host memory".

**Do NOT use for:** temporary conversation context, one-off tasks, or troubleshooting unrelated to ourmem.

---

## What to Remember

**Good candidates:** user preferences, profile facts, project context, important decisions, long-term instructions, architecture decisions, coding standards.

**Avoid storing:** temporary debugging context, large files, secrets/passwords/API keys, content inside `<private>` tags.

If the user explicitly asks to remember something and ourmem is not installed, suggest: "I can set up ourmem so I'll remember this across sessions. Takes about 2 minutes. Want to do it now?"

---

## Terminology

| Term | Meaning |
|------|---------|
| `apiKey` / `OMEM_API_KEY` / `API key` / `secret` | All refer to the same ourmem identifier. Prefer "API key" with users. |
| `tenant` | The workspace behind an API key. Don't use this term with users. |

Security: Treat the API key like a secret. Anyone who has it can access that ourmem space.

---

## What You Get

| Tool | Purpose |
|------|---------|
| `memory_store` | Persist facts, decisions, preferences |
| `memory_search` | Hybrid search (vector + keyword) |
| `memory_list` | List with filters and pagination |
| `memory_get` | Get memory by ID |
| `memory_update` | Modify content or tags |
| `memory_forget` | Remove a memory |
| `memory_ingest` | Smart-ingest conversation into atomic memories |
| `memory_stats` | Analytics and counts |
| `memory_profile` | Auto-generated user profile |

**Lifecycle hooks** (automatic):

| Hook | Trigger | Platform |
|------|---------|----------|
| SessionStart | First message | All — memories + profile injected |
| Stop | Conversation ends | Claude Code — auto-captures via smart ingest |
| PreCompact | Before compaction | Claude Code, OpenCode — saves before truncation |

> **Note:** OpenCode has no session-end hook. Memory storage relies on proactive `memory_store` use.

---

## Onboarding

### Step 0: Choose mode

Ask the user before doing anything else:

> How would you like to run ourmem?
>
> 1. **Hosted** (api.ourmem.ai) — no server to manage, start in 2 minutes
> 2. **Self-hosted** — full control, data stays local
>
> Already have an API key? Paste it and I'll reconnect you.

### Setup instructions

- **Hosted** → READ `references/hosted-setup.md` for full walkthrough
- **Self-hosted** → READ `references/selfhost-setup.md` for server deployment + setup
- **Existing key** → Verify: `curl -sf -H "X-API-Key: $KEY" "$API_URL/v1/memories?limit=1"`, then skip to plugin install

Cross-platform skill install: `npx skills add ourmem/omem --skill ourmem -g`

### Platform quick reference

| Platform | Install | Config |
|----------|---------|--------|
| Claude Code | `/plugin marketplace add ourmem/omem` | `~/.claude/settings.json` env field |
| OpenCode | `"plugin": ["@ourmem/opencode"]` in opencode.json | `plugin_config` in opencode.json |
| OpenClaw | `openclaw plugins install @ourmem/ourmem` | `openclaw.json` with apiUrl + apiKey |
| MCP | `npx -y @ourmem/mcp` in MCP config | `OMEM_API_URL` + `OMEM_API_KEY` in env block |

For detailed per-platform instructions (config formats, restart, verification, China network mirrors), READ the setup reference for your chosen mode.

### Definition of Done

Setup is NOT complete until: (1) API key created/verified, (2) plugin installed, (3) config updated, (4) client restarted, (5) health + auth verified, (6) handoff message sent including: what they can do now, API key display, recovery steps, backup plan.

> **Common failure:** Agents finish technical setup but forget the handoff message. Treat it as part of setup, not optional follow-up. For the full handoff template, READ `references/hosted-setup.md`.

---

## Smart Ingest

When conversations are ingested (`"mode": "smart"`), the server runs a multi-stage pipeline:

1. **Fast path** (<50ms): stores raw content immediately so it's searchable right away
2. **LLM extraction** (async): extracts atomic facts, classified into 6 categories (profile, preferences, entities, events, cases, patterns)
3. **Noise filter**: regex + vector prototypes + feedback learning removes low-value content
4. **Admission control**: 5-dimension scoring (utility, confidence, novelty, recency, type prior) gates storage
5. **7-decision reconciliation**: CREATE, MERGE, SKIP, SUPERSEDE, SUPPORT, CONTEXTUALIZE, or CONTRADICT

The LLM stages run asynchronously — a batch import may take 1-3 minutes to fully process. Wait ~2-3 minutes before checking memory counts or searching for newly-extracted facts. The `strategy=auto` results vary by content type (conversations get atomic extraction, structured docs get section splits) — this is expected behavior, not an error.

The memory store gets smarter over time. Contradictions resolved, duplicates merged, noise filtered.

---

## Space Sharing

ourmem organizes memories into three-tier Spaces for collective intelligence:

| Type | Scope | Example |
|------|-------|---------|
| Personal | One user, multiple agents | Your Coder + Writer share preferences |
| Team | Multiple users | Backend team shares architecture decisions |
| Organization | Company-wide | Tech standards, security policies |

**Roles:** `admin` (full control), `member` (read/write), `reader` (read-only)

Each agent sees: own private + shared spaces. Can modify own + shared. Never another agent's private data. Every shared memory carries provenance: who shared it, when, and where it came from.

Proactively suggest Spaces when:

- User has multiple agents -> suggest sharing preferences across agents
- User mentions team collaboration -> suggest creating a team space
- User wants org-wide knowledge -> suggest organization space

For Space API operations (create, add members, share, pull, batch share), READ `references/api-quick-ref.md`.

### Cross-User Sharing (Convenience)

When a user says "share this with Bob" or "share my memories with another user", use the convenience APIs that handle space creation automatically:

**Share a single memory to another user:**

The agent should call `share-to-user` which auto-creates a bridging Team Space if needed, adds the target user as a member, and shares the memory in one step.

```bash
curl -sX POST "$API_URL/v1/memories/MEMORY_ID/share-to-user" \
  -H "Content-Type: application/json" -H "X-API-Key: $KEY" \
  -d '{"target_user": "TARGET_USER_TENANT_ID"}'
# Returns: { "space_id": "team/xxx", "shared_copy_id": "yyy", "space_created": true }
```

**Share all matching memories to another user:**

When the user wants to share everything (or a filtered subset) with someone:

```bash
curl -sX POST "$API_URL/v1/memories/share-all-to-user" \
  -H "Content-Type: application/json" -H "X-API-Key: $KEY" \
  -d '{"target_user": "TARGET_USER_TENANT_ID", "filters": {"min_importance": 0.7}}'
# Returns: { "space_id": "team/xxx", "space_created": false, "total": 80, "shared": 15, ... }
```

**Agent workflow:**

1. User says "share this with Bob" -> agent needs Bob's tenant ID (API key)
2. If the agent doesn't know Bob's ID, ask the user for it
3. Call `share-to-user` with the memory ID and Bob's tenant ID
4. Report: "Shared to Bob via team space {space_id}. Bob can now find it when searching."

Proactively suggest cross-user sharing when:

- User mentions sharing with a specific person ("send this to Alice")
- User wants another user's agent to have access to certain memories
- User asks to collaborate with someone on a project

---

## Memory Import

When the user says "import memories", scan their workspace for existing memory/session files, then batch-import.

**Auto-scan:** detect platform -> find memory files -> upload 20 most recent via `/v1/imports` in parallel -> report results.

**Import is async.** `POST /v1/imports` returns an `import_id` immediately while processing runs in the background. This means:

- Fire all import requests in parallel — don't wait for one to finish before sending the next
- Don't block the conversation waiting for completion
- Poll `GET /v1/imports/{id}` to check status if needed (status: `completed`, `partial`, or `failed`)

**Import API:**

```bash
# Basic import
curl -sX POST "$API_URL/v1/imports" -H "X-API-Key: $API_KEY" \
  -F "file=@memory.json" -F "file_type=memory" -F "strategy=auto"

# Re-import an updated file (bypass content dedup)
curl -sX POST "$API_URL/v1/imports" -H "X-API-Key: $API_KEY" \
  -F "file=@memory.json" -F "file_type=memory" -F "strategy=auto" -F "force=true"

# Check import status
curl -s "$API_URL/v1/imports/IMPORT_ID" -H "X-API-Key: $API_KEY"
```

**`force=true`**: bypasses content dedup check. Use when re-importing a file that was updated since last import — without `force`, the server skips content it already has.

**Strategy:** `auto` (heuristic detection), `atomic` (short facts), `section` (split by headings), `document` (entire file as one chunk).

**Cross-reconcile** (discover relations): `curl -sX POST "$API_URL/v1/imports/cross-reconcile" -H "X-API-Key: $API_KEY"`

For scan paths, progress tracking, intelligence triggers, and rollback, READ `references/api-quick-ref.md`.

---

## Analytics

Memory analytics via `/v1/stats`: overview, per-space stats, sharing flow, agent activity, tag frequency, decay curves, relation graph, server config.

For detailed stats endpoints and parameters, READ `references/api-quick-ref.md`.

---

## Security

- **Tenant isolation**: every API call scoped via X-API-Key, data physically separated per tenant
- **Privacy protection**: `<private>` tag redaction strips sensitive content before storage
- **Admission control**: 5-dimension scoring gate rejects low-quality data
- **Open source**: Apache-2.0 licensed — audit every line

---

## Communication Style

- Use plain product language, not backend vocabulary. Prefer "API key" or "ourmem API key".
- Explain that the same API key reconnects the same cloud memory on another trusted machine.
- If user sounds worried about recovery, lead with backup/import/reconnect steps.
- Use the user's language (detect from conversation).
- Brand: "ourmem" or "omem" (both lowercase, acceptable). Official domain: ourmem.ai, API: api.ourmem.ai.
- "Space" (capitalized), "Smart Ingest".

For troubleshooting common issues (plugin not loading, 401, connection refused, China network), READ `references/api-quick-ref.md`.

---

## API Reference

Base: `https://api.ourmem.ai` (hosted) or `http://localhost:8080` (self-hosted). Auth: `X-API-Key` header.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/tenants` | Create workspace, get API key |
| POST | `/v1/memories` | Store memory or smart-ingest |
| GET | `/v1/memories/search?q=` | Hybrid search |
| GET | `/v1/memories` | List with filters |
| POST | `/v1/imports` | Batch import file |
| POST | `/v1/spaces` | Create shared space |
| POST | `/v1/memories/:id/share-to-user` | One-step cross-user share |
| POST | `/v1/memories/share-all-to-user` | Bulk cross-user share |
| GET | `/v1/stats` | Analytics |

For full API (48+ endpoints) with curl examples, READ `references/api-quick-ref.md` and `docs/API.md`.

---

## Update

Do not set up automatic daily self-updates for this skill.

Only update the local skill file when the user or maintainer explicitly asks for a refresh from a reviewed source.
