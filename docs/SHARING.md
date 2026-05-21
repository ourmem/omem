# ourmem — Memory Sharing Architecture

> How ourmem enables knowledge flow across agents, users, and teams.
> For API reference see [API.md](API.md). For the memory pipeline see [PIPELINE.md](PIPELINE.md).

---

## Table of Contents

- [1. Overview](#1-overview)
- [2. Core Concepts](#2-core-concepts)
- [3. Architecture](#3-architecture)
- [4. Sharing Flows](#4-sharing-flows)
- [5. Cross-Space Search](#5-cross-space-search)
- [6. API Quick Reference](#6-api-quick-reference)
- [7. Tutorials](#7-tutorials)
- [8. Versioning & Staleness](#8-versioning--staleness)
- [9. Auto-Share Rules](#9-auto-share-rules)
- [10. Security & Permissions](#10-security--permissions)
- [11. Limitations](#11-limitations)

---

## 1. Overview

### What is memory sharing?

Most AI memory systems trap knowledge in silos. Each agent, each user, each session starts from scratch. ourmem's sharing system breaks these walls: memories can flow between personal spaces, team spaces, and organization spaces while preserving full provenance.

### The Space bridge model

Sharing in ourmem works through **Spaces**. Two users can't share memories directly. Instead, they share through a bridging Space (Team or Organization) that both have access to.

```
  Alice (personal/alice)          Bob (personal/bob)
         │                              │
         │  share ──►                   │
         │           ▼                  │
         │    ┌──────────────┐          │
         │    │  team/backend │ ◄── search (space=all)
         │    │              │          │
         │    │  [copy of    │          │
         │    │   Alice's    │          │
         │    │   memory]    │          │
         │    └──────────────┘          │
         │                     ◄── pull │
```

### Physical copy + provenance

When Alice shares a memory to a Team Space, ourmem creates a **physical copy** in the target Space's LanceDB store. The copy includes:

- All content fields (content, l0/l1/l2 abstracts)
- The source memory's **vector embedding** (copied, not re-generated)
- A `provenance` object tracking the full lineage

This is not a reference or a pointer. It's an independent snapshot. Why? LanceDB is an embedded database with per-space physical isolation. There's no cross-database vector search. Each Space has its own LanceDB directory, its own vector index, its own FTS index. A shared copy must live in the target Space's store to be searchable there.

### Why not references?

A reference-based model ("memory X in Space A is also visible in Space B") would require either:

1. Cross-database queries at search time (LanceDB doesn't support this)
2. A centralized index that breaks physical isolation

Physical copies are simpler, faster, and maintain the per-space isolation that makes ourmem's security model work. The tradeoff: copies can become stale. ourmem handles this with [version-based staleness detection](#8-versioning--staleness).

---

## 2. Core Concepts

### Space types

| | Personal | Team | Organization |
|---|----------|------|--------------|
| **Scope** | One user, multiple agents | Multiple users | Company-wide |
| **Access** | Owner's agents only | Team members | All org members |
| **Example** | Coder + Writer share preferences | Backend team shares arch decisions | Tech standards, security policies |
| **ID format** | `personal/{uuid}` | `team/{uuid}` | `org/{uuid}` |
| **Default** | Auto-created per tenant | Created via API | Created via API or `org/setup` |

Every tenant gets a Personal Space automatically. Team and Organization Spaces are created explicitly.

### Permission matrix

| Operation | Admin | Member | Reader |
|-----------|:-----:|:------:|:------:|
| Read memories | ✅ | ✅ | ✅ |
| Search memories | ✅ | ✅ | ✅ |
| Share memory TO this space | ✅ | ✅ | ❌ |
| Pull memory FROM this space | ✅ | ✅ | ✅ |
| Unshare memory | ✅ | ✅ (own shares) | ❌ |
| Create auto-share rules | ✅ | ❌ | ❌ |
| Add/remove members | ✅ | ❌ | ❌ |
| Delete space | ✅ | ❌ | ❌ |
| Update space metadata | ✅ | ❌ | ❌ |

### Provenance

Every shared copy carries a `provenance` object:

```json
{
  "shared_from_space": "personal/alice-uuid",
  "shared_from_memory": "original-memory-uuid",
  "shared_by_user": "alice-uuid",
  "shared_by_agent": "coder",
  "shared_at": "2026-03-28T10:00:00Z",
  "original_created_at": "2026-03-25T08:00:00Z",
  "source_version": 3
}
```

This tells you: who shared it, when, from where, and what version of the source it was copied from.

### Version tracking

Every memory has a `version` field (starting at 1, auto-incremented on each update). When a memory is shared, the copy's `provenance.source_version` records the source's version at share time. This enables staleness detection: if the source has been updated since the share, the copy is stale.

### Auto-share rules

Rules that automatically share new memories matching certain criteria. When a memory is created via `POST /v1/memories` (direct mode), the system checks all auto-share rules asynchronously. Matching memories are copied to the target Space without manual intervention.

---

## 3. Architecture

### StoreManager and per-space isolation

```
                    ┌─────────────────────────────────────────┐
                    │              StoreManager                │
                    │         (LRU cache, max 1000)            │
                    ├─────────────────────────────────────────┤
                    │                                         │
                    │  personal/alice  ──► LanceDB instance   │
                    │  personal/bob    ──► LanceDB instance   │
                    │  team/backend    ──► LanceDB instance   │
                    │  org/acme        ──► LanceDB instance   │
                    │                                         │
                    └─────────────────────────────────────────┘
                                      │
                    ┌─────────────────┼─────────────────┐
                    │                 │                 │
              ┌─────▼─────┐   ┌──────▼──────┐   ┌─────▼─────┐
              │ ./omem-data│   │ ./omem-data │   │ ./omem-data│
              │ /personal/ │   │ /team/      │   │ /org/      │
              │ alice/     │   │ backend/    │   │ acme/      │
              │            │   │             │   │            │
              │ memories   │   │ memories    │   │ memories   │
              │ (table)    │   │ (table)     │   │ (table)    │
              │ vector idx │   │ vector idx  │   │ vector idx │
              │ FTS idx    │   │ FTS idx     │   │ FTS idx    │
              └────────────┘   └─────────────┘   └────────────┘
```

Each Space gets its own LanceDB directory. The `StoreManager` maintains an LRU cache of open connections (up to 1000). When a sharing operation needs to write to a target Space, it opens (or retrieves from cache) the target Space's store.

### Data model

```
  Space                    Memory                   Provenance
  ┌──────────────┐         ┌──────────────────┐     ┌─────────────────────┐
  │ id           │         │ id               │     │ shared_from_space   │
  │ space_type   │    1:N  │ content          │     │ shared_from_memory  │
  │ name         │◄────────│ vector           │     │ shared_by_user      │
  │ owner_id     │         │ category         │  ┌──│ shared_by_agent     │
  │ members[]    │         │ version          │  │  │ shared_at           │
  │ auto_share   │         │ space_id ────────┼──┘  │ original_created_at │
  │   _rules[]   │         │ provenance ──────┼────►│ source_version      │
  └──────────────┘         │ ...              │     └─────────────────────┘
                           └──────────────────┘
                                    │
                                    │ recorded per share action
                                    ▼
                           ┌──────────────────┐
                           │  SharingEvent     │
                           │ action (Share/    │
                           │   Pull/Unshare/   │
                           │   Reshare/Batch)  │
                           │ from_space        │
                           │ to_space          │
                           │ user_id           │
                           │ timestamp         │
                           └──────────────────┘
```

---

## 4. Sharing Flows

### Push (Share)

The most common flow. A user shares a memory from their personal space to a team space.

```
  POST /v1/memories/{id}/share
  {"target_space": "team/backend"}

  ┌──────────────┐                    ┌──────────────┐
  │ personal/    │                    │ team/        │
  │ alice        │                    │ backend      │
  │              │                    │              │
  │ Memory M1    │──── copy ────────►│ Copy C1      │
  │ (v=3)        │    + vector       │ (v=1)        │
  │              │    + provenance   │ prov.src=M1  │
  │              │                    │ prov.sv=3    │
  └──────────────┘                    └──────────────┘
```

Steps:
1. Verify caller has write access to target space
2. Read source memory from personal store
3. Read source memory's vector embedding via `get_vector_by_id()`
4. Check for existing copy (idempotent: if copy exists, return it with 200)
5. Create physical copy with `make_shared_copy()` + source vector
6. Record `SharingEvent(action=Share)`
7. Return copy with 201

### Pull

A user pulls a memory from a shared space into their personal space.

```
  POST /v1/memories/{id}/pull
  {"source_space": "team/backend"}

  ┌──────────────┐                    ┌──────────────┐
  │ team/        │                    │ personal/    │
  │ backend      │                    │ bob          │
  │              │                    │              │
  │ Memory M2    │──── copy ────────►│ Copy C2      │
  │              │    + vector       │ prov.src=M2  │
  │              │    + provenance   │              │
  └──────────────┘                    └──────────────┘
```

### Batch Share

Share multiple memories at once. Runs up to 10 shares concurrently via `buffer_unordered(10)`. Hard limit: 500 memories per call.

```
  POST /v1/memories/batch-share
  {"memory_ids": ["m1","m2","m3"], "target_space": "team/backend"}

  Returns: { "succeeded": [...], "failed": [...] }
```

### Share-All

Share all memories from personal space matching optional filters.

```
  POST /v1/memories/share-all
  {"target_space": "team/backend", "filters": {"categories": ["cases"], "min_importance": 0.7}}

  ┌──────────────┐                    ┌──────────────┐
  │ personal/    │  list + filter     │ team/        │
  │ alice        │──────────────────►│ backend      │
  │              │  batch share       │              │
  │ 150 memories │  (matching: 23)   │ +23 copies   │
  └──────────────┘                    └──────────────┘

  Returns: { "total": 150, "shared": 23, "skipped_existing": 2, "failed": 0 }
```

### Share-to-User (Convenience)

One-step cross-user sharing. Creates a bridging Team Space if needed, adds the target user as a member, and shares the memory.

```
  POST /v1/memories/{id}/share-to-user
  {"target_user": "bob-tenant-id"}

  ┌──────────────┐     auto-create      ┌──────────────┐
  │ personal/    │────────────────────►│ team/{uuid}  │
  │ alice        │     if needed        │ (bridge)     │
  │              │                      │              │
  │ Memory M1    │──── share ─────────►│ Copy C1      │
  └──────────────┘                      └──────────────┘
                                               │
                                        auto-add bob
                                        as member
```

Returns: `{ "space_id": "team/xxx", "shared_copy_id": "yyy", "space_created": true }`

### Share-All-to-User (Convenience)

Combines share-to-user with share-all. Creates bridging space if needed, then bulk-shares filtered memories.

```
  POST /v1/memories/share-all-to-user
  {"target_user": "bob-tenant-id", "filters": {"min_importance": 0.8}}

  Returns: { "space_id": "team/xxx", "space_created": true, "total": 50, "shared": 12, ... }
```

### Unshare

Removes a shared copy from the target space. Finds copies by provenance source ID.

```
  POST /v1/memories/{id}/unshare
  {"target_space": "team/backend"}
```

### Re-share

Refreshes a stale shared copy with the latest source content and vector. Old copy is soft-deleted, new copy is created.

```
  POST /v1/memories/{id}/reshare
  {"target_space": "team/backend"}

  ┌──────────────┐                    ┌──────────────┐
  │ personal/    │                    │ team/        │
  │ alice        │                    │ backend      │
  │              │                    │              │
  │ M1 (v=5)    │──── new copy ────►│ C1-new (v=1) │
  │ (updated)    │    + latest vec   │ prov.sv=5    │
  │              │                    │              │
  │              │                    │ C1-old       │
  │              │                    │ (deleted)    │
  └──────────────┘                    └──────────────┘
```

### Organization Publish

Admin creates an Organization Space and publishes memories to it. All org members get read access.

```
  POST /v1/org/setup
  {"name": "Acme Corp", "members": [{"user_id": "bob", "role": "reader"}, ...]}

  POST /v1/org/{id}/publish
  {"memory_ids": ["m1","m2"], "auto_share_rule": {"categories": ["patterns"], "min_importance": 0.8}}
```

---

## 5. Cross-Space Search

When a user searches with `space=all`, ourmem queries all accessible spaces in parallel and merges results.

### Flow

```
  GET /v1/memories/search?q=architecture&space=all

  ┌─────────────────────────────────────────────────────────┐
  │                    search_memories()                      │
  │                                                          │
  │  1. list_spaces_for_user(tenant_id)                      │
  │     → [personal/alice, team/backend, org/acme]           │
  │                                                          │
  │  2. get_accessible_stores()                              │
  │     → Vec<AccessibleStore> with space_type weights       │
  │                                                          │
  │  3. tokio::JoinSet (parallel)                            │
  │     ┌──────────────┐ ┌──────────────┐ ┌──────────────┐  │
  │     │ personal/    │ │ team/        │ │ org/         │  │
  │     │ alice        │ │ backend      │ │ acme         │  │
  │     │              │ │              │ │              │  │
  │     │ 12-stage     │ │ 12-stage     │ │ 12-stage     │  │
  │     │ pipeline     │ │ pipeline     │ │ pipeline     │  │
  │     │              │ │              │ │              │  │
  │     │ results: 8   │ │ results: 5   │ │ results: 3   │  │
  │     └──────┬───────┘ └──────┬───────┘ └──────┬───────┘  │
  │            │                │                │           │
  │  4. Per-space normalization (min-max to [0,1])           │
  │                                                          │
  │  5. Apply space weights:                                 │
  │     Personal = 1.0  │  Team = 0.8  │  Org = 0.6         │
  │                                                          │
  │  6. Merge all results                                    │
  │                                                          │
  │  7. Global sort by weighted score                        │
  │                                                          │
  │  8. Truncate to requested limit                          │
  └─────────────────────────────────────────────────────────┘
```

### Space weights

| Space Type | Weight | Rationale |
|------------|--------|-----------|
| Personal | 1.0 | Your own memories are most relevant |
| Team | 0.8 | Team knowledge is highly relevant |
| Organization | 0.6 | Org-wide knowledge is useful but less specific |

### Error handling

If one space's search fails (e.g., corrupted index), the error is logged and that space is skipped. The search continues with results from the remaining spaces. This ensures a single broken space doesn't take down cross-space search.

### Stale detection (opt-in)

Add `?check_stale=true` to annotate shared copies with staleness info. For each result that has `provenance`, the system looks up the source memory's current version and compares:

- `provenance.source_version < source.version` → `is_stale: true`
- Source memory deleted → `is_stale: true, source_deleted: true`
- Versions match → `is_stale: false`

This is opt-in because it requires extra I/O (reading source memories from other spaces). Without the flag, no extra queries are made.

---

## 6. API Quick Reference

### Space Management

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/spaces` | Create a Space (personal/team/org) |
| GET | `/v1/spaces` | List accessible Spaces (owned + member) |
| GET | `/v1/spaces/{id}` | Get Space details |
| PUT | `/v1/spaces/{id}` | Update Space metadata |
| DELETE | `/v1/spaces/{id}` | Delete Space (admin only) |
| POST | `/v1/spaces/{id}/members` | Add member |
| PUT | `/v1/spaces/{id}/members/{user_id}` | Change member role |
| DELETE | `/v1/spaces/{id}/members/{user_id}` | Remove member |

### Sharing Operations

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/memories/{id}/share` | Share memory to a Space |
| POST | `/v1/memories/{id}/pull` | Pull memory to personal Space |
| POST | `/v1/memories/{id}/unshare` | Remove shared copy from Space |
| POST | `/v1/memories/{id}/reshare` | Refresh stale shared copy |
| POST | `/v1/memories/batch-share` | Share multiple memories (max 500) |
| POST | `/v1/memories/share-all` | Share all matching memories |

### Convenience APIs

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/memories/{id}/share-to-user` | One-step share to another user (auto-creates bridging space) |
| POST | `/v1/memories/share-all-to-user` | Bulk share to another user (auto-creates bridging space) |

### Organization

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/org/setup` | Create org Space + add members in one call |
| POST | `/v1/org/{id}/publish` | Publish memories to org + optional auto-share rule |

### Auto-Share Rules

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/spaces/{id}/auto-share-rules` | Create auto-share rule |
| GET | `/v1/spaces/{id}/auto-share-rules` | List rules |
| DELETE | `/v1/spaces/{id}/auto-share-rules/{rule_id}` | Delete rule |

### Statistics

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/stats/spaces` | Per-space statistics |
| GET | `/v1/stats/sharing` | Sharing flow analysis + graph |

---

## 7. Tutorials

### Tutorial A: Sharing between two API keys (step by step)

This tutorial walks through the full sharing flow using curl.

**Prerequisites:** A running ourmem server (hosted or self-hosted).

```bash
export API="http://localhost:8080"  # or https://api.ourmem.ai
```

**Step 1: Create two tenants**

```bash
# Alice
curl -sX POST $API/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "alice"}' | jq .
# Save: ALICE_KEY=<api_key from response>

# Bob
curl -sX POST $API/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "bob"}' | jq .
# Save: BOB_KEY=<api_key from response>
```

**Step 2: Alice creates a Team Space**

```bash
curl -sX POST $API/v1/spaces \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d '{"name": "Backend Team", "space_type": "team"}' | jq .
# Save: SPACE_ID=<id from response, e.g. "team/xxx">
```

**Step 3: Alice adds Bob as a member**

```bash
curl -sX POST "$API/v1/spaces/$SPACE_ID/members" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d "{\"user_id\": \"$BOB_KEY\", \"role\": \"member\"}" | jq .
```

**Step 4: Alice creates a memory**

```bash
curl -sX POST $API/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d '{"content": "Use hexagonal architecture for all new services", "tags": ["architecture"]}' | jq .
# Save: MEM_ID=<id from response>
```

**Step 5: Alice shares the memory to the Team Space**

```bash
curl -sX POST "$API/v1/memories/$MEM_ID/share" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d "{\"target_space\": \"$SPACE_ID\"}" | jq .
# Returns: the shared copy (201 Created)
```

**Step 6: Bob searches across all spaces**

```bash
curl -s "$API/v1/memories/search?q=hexagonal+architecture&space=all" \
  -H "X-API-Key: $BOB_KEY" | jq .
# Bob finds Alice's shared memory in the team space
```

**Step 7: Alice updates the original memory**

```bash
curl -sX PUT "$API/v1/memories/$MEM_ID" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d '{"content": "Use hexagonal architecture with ports and adapters pattern for all new services"}' | jq .
# Memory version increments to 2
```

**Step 8: Bob detects the stale copy**

```bash
curl -s "$API/v1/memories/search?q=hexagonal&space=all&check_stale=true" \
  -H "X-API-Key: $BOB_KEY" | jq '.results[0].stale_info'
# { "is_stale": true, "source_version": 1, "current_source_version": 2, "source_deleted": false }
```

**Step 9: Bob refreshes the stale copy**

```bash
# Get the copy's ID from the search result
COPY_ID=<id from search result>

curl -sX POST "$API/v1/memories/$COPY_ID/reshare" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $BOB_KEY" \
  -d "{\"target_space\": \"$SPACE_ID\"}" | jq .
# Returns: new copy with updated content and source_version=2
```

**Step 10: Verify the copy is fresh**

```bash
curl -s "$API/v1/memories/search?q=hexagonal&space=all&check_stale=true" \
  -H "X-API-Key: $BOB_KEY" | jq '.results[0].stale_info'
# { "is_stale": false, ... }
```

### Tutorial B: Using share-to-user (3 steps)

The convenience API handles space creation and membership automatically.

**Step 1: Alice creates a memory**

```bash
curl -sX POST $API/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d '{"content": "Our API uses JWT with RS256 signing", "tags": ["security"]}' | jq .
# Save: MEM_ID=<id>
```

**Step 2: Alice shares directly to Bob**

```bash
curl -sX POST "$API/v1/memories/$MEM_ID/share-to-user" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ALICE_KEY" \
  -d "{\"target_user\": \"$BOB_KEY\"}" | jq .
# Returns: { "space_id": "team/xxx", "shared_copy_id": "yyy", "space_created": true }
```

**Step 3: Bob searches and finds it**

```bash
curl -s "$API/v1/memories/search?q=JWT+signing&space=all" \
  -H "X-API-Key: $BOB_KEY" | jq .
# Bob finds the shared memory
```

That's it. No manual space creation, no member management. The system handles everything.

### Tutorial C: Organization Space

An admin creates an org-wide knowledge base that all members can search.

**Step 1: Admin creates the Organization Space**

```bash
curl -sX POST "$API/v1/org/setup" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{
    "name": "Acme Corp Standards",
    "members": [
      {"user_id": "'$BOB_KEY'", "role": "reader"},
      {"user_id": "'$CAROL_KEY'", "role": "reader"}
    ]
  }' | jq .
# Returns: { "space_id": "org/xxx", "name": "Acme Corp Standards", "members_added": 2, ... }
# Save: ORG_ID=<space_id>
```

**Step 2: Admin publishes memories to the org**

```bash
curl -sX POST "$API/v1/org/$ORG_ID/publish" \
  -H "Content-Type: application/json" \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{
    "memory_ids": ["mem-1", "mem-2", "mem-3"],
    "auto_share_rule": {
      "categories": ["patterns"],
      "min_importance": 0.8
    }
  }' | jq .
# Returns: { "published": 3, "skipped_existing": 0, "failed": 0, "auto_share_rule_id": "rule-xxx" }
```

**Step 3: Readers search the org knowledge base**

```bash
curl -s "$API/v1/memories/search?q=coding+standards&space=all" \
  -H "X-API-Key: $BOB_KEY" | jq .
# Bob finds the published org memories alongside his personal ones
```

---

## 8. Versioning & Staleness

### How versions work

```
  Memory M1 created ──► version = 1
       │
  PUT /v1/memories/M1 ──► version = 2
       │
  PUT /v1/memories/M1 ──► version = 3
       │
  Share M1 to team ──► Copy C1 created
                        version = 1 (copy is a new entity)
                        provenance.source_version = 3
       │
  PUT /v1/memories/M1 ──► version = 4
       │
  GET C1?check_stale=true
       │
       ▼
  stale_info: {
    is_stale: true,
    source_version: 3,        ← recorded at share time
    current_source_version: 4, ← source's current version
    source_deleted: false
  }
```

### Version increment rules

- `Memory::new()` sets `version = Some(1)`
- Every `LanceStore::update()` call increments: `version = current.unwrap_or(0) + 1`
- Reconciler operations (MERGE, SUPERSEDE, SUPPORT) go through `update()`, so they increment too
- Shared copies start at `version = Some(1)` (they're new entities in the target space)
- Old memories (pre-versioning) have `version = None`, treated as version 0

### Re-share flow

```
  ┌─────────────────────────────────────────────────────┐
  │                  reshare_memory()                     │
  │                                                      │
  │  1. Find old copy by ID in target space              │
  │  2. Read provenance → source space + source ID       │
  │  3. Open source space store                          │
  │  4. Fetch latest source memory + vector              │
  │  5. Create new copy with updated content + vector    │
  │     provenance.source_version = source.version       │
  │  6. Soft-delete old copy                             │
  │  7. Record SharingEvent(action=Reshare)              │
  │  8. Return new copy                                  │
  └─────────────────────────────────────────────────────┘
```

---

## 9. Auto-Share Rules

### Rule structure

```json
{
  "id": "rule-uuid",
  "source_space": "personal/alice",
  "categories": ["cases", "patterns"],
  "tags": ["architecture"],
  "min_importance": 0.7,
  "require_approval": false
}
```

### Matching logic

A new memory matches a rule if ALL of these conditions are true:

1. Memory's `space_id` matches `source_space`
2. If `categories` is non-empty: memory's `category` is in the list
3. If `tags` is non-empty: memory has at least one matching tag (OR logic)
4. Memory's `importance` >= `min_importance`
5. `require_approval` is `false` (approval queue is not implemented)

### Trigger timing

Auto-share fires **after** direct memory creation (`POST /v1/memories` with `content`). The flow:

```
  POST /v1/memories {"content": "..."}
       │
       ▼
  create_memory handler
       │
       ├── store.create(&memory, vector) ──► 201 Created (returned to caller)
       │
       └── tokio::spawn (fire-and-forget)
              │
              ▼
         check_auto_share()
              │
              ├── list all spaces where user is member
              ├── for each space: check auto_share_rules
              ├── for each matching rule: share memory to space
              └── errors logged, never propagated
```

Key properties:
- **Asynchronous**: the 201 response is returned before auto-share runs
- **Non-blocking**: auto-share failure never fails the memory creation
- **Direct-create only**: smart ingest (messages mode) does not trigger auto-share
- **Idempotent**: if the memory was already shared (e.g., manual share before auto-share fires), the existing copy is returned

---

## 10. Security & Permissions

### Physical isolation

Every Space has its own LanceDB directory. There are no shared tables, no row-level security filters. A query against `personal/alice` physically cannot return data from `team/backend`. The `StoreManager` enforces this by opening separate database connections per space.

### Permission checks

Every sharing operation verifies permissions:

1. **share**: caller must have write access to target space (Admin or Member)
2. **pull**: caller must have read access to source space (any role)
3. **unshare**: caller must be the original sharer OR a Space Admin
4. **auto-share rules**: only Space Admins can create/delete rules
5. **org/setup**: caller becomes the org Admin
6. **org/publish**: caller must be Admin of the org space

### Provenance integrity

Provenance is set at share time and never modified afterward. It records:
- Who shared (user ID + agent ID)
- When (timestamp)
- From where (source space + source memory ID)
- What version (source_version)

This creates an immutable audit trail for every shared memory.

### Space ID normalization

Space IDs use `/` as separator (`personal/uuid`, `team/uuid`, `org/uuid`). If an incoming request uses the legacy `:` separator, it's automatically normalized to `/`. This prevents path traversal issues since Space IDs map directly to filesystem paths and S3 key prefixes.

---

## 11. Limitations

### No automatic sync

Shared copies are snapshots. When the source memory is updated, copies don't auto-update. Users must detect staleness via `?check_stale=true` and manually refresh via `reshare`. There is no background worker propagating updates.

### No backfill

Existing shared copies created before the vector propagation fix (Phase 1) may have zero vectors. These copies won't appear in vector search results. A manual re-share is needed to fix them. There is no automated backfill mechanism.

### `require_approval` not implemented

The `require_approval` field exists on auto-share rules but has no effect. Rules with `require_approval: true` are silently skipped. There is no approval queue or notification system.

### No cross-space vector search

LanceDB doesn't support cross-database vector queries. Each space has its own vector index. Cross-space search works by running independent searches per space and merging results. This means the same query might return slightly different results depending on each space's index state.

### Share-all hard limit

`POST /v1/memories/share-all` processes at most 5000 memories per call. For larger spaces, multiple calls are needed.

### Batch share hard limit

`POST /v1/memories/batch-share` accepts at most 500 memory IDs per call. Requests exceeding this limit return 400 Bad Request.

### Rate limiting on sharing (opt-in)

Per-user rate limiting on sharing operations is available via `OMEM_SHARE_RATE_PER_MIN` (operations per minute per user; **default 0 = disabled**). When enabled, each call to a sharing endpoint (`share`, `pull`, `reshare`, `batch-share`, `share-all`, `share-to-user`, `share-all-to-user`, `org/publish`) consumes one token from the caller's bucket; an empty bucket returns `429 Too Many Requests`. Tokens refill continuously at `OMEM_SHARE_RATE_PER_MIN / 60` per second, so bursts up to the per-minute value are allowed. State is process-local; a multi-replica deployment would need a shared store.

### Organization spaces are read-only for non-admins

Members added to an Organization Space with `reader` role can search and read, but cannot share memories into the org space. Only the Admin can publish to the org.
