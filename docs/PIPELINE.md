# ourmem — Memory Pipeline Architecture

> Technical deep-dive into how ourmem stores, retrieves, and evolves memories.
> For API reference see [API.md](API.md). For deployment see [DEPLOY.md](DEPLOY.md).

---

## Table of Contents

- [1. Memory Storage Pipeline](#1-memory-storage-pipeline)
  - [1.1 Conversation Ingest](#11-conversation-ingest-post-v1memories-with-messages)
  - [1.2 File Import](#12-file-import-post-v1imports)
  - [1.3 Direct Memory Creation](#13-direct-memory-creation-post-v1memories-with-content)
  - [1.4 Reconciler — 7 Decision Types](#14-reconciler--7-decision-types)
  - [1.5 Memory Field Model](#15-memory-field-model)
- [2. Memory Retrieval Pipeline](#2-memory-retrieval-pipeline)
  - [2.1 Overview](#21-overview)
  - [2.2 Stage-by-Stage Detail](#22-stage-by-stage-detail)
  - [2.3 Dual-Path Search](#23-dual-path-search)
- [3. Plugin Integration](#3-plugin-integration)
  - [3.1 OpenCode Plugin](#31-opencode-plugin-ourmemopencode)
  - [3.2 OpenClaw Plugin](#32-openclaw-plugin-ourmemopenclaw)
  - [3.3 Claude Code Plugin](#33-claude-code-plugin-hooks)
  - [3.4 MCP Server](#34-mcp-server-ourmemmcp)
  - [3.5 Cross-Plugin Comparison](#35-cross-plugin-comparison)

---

## 1. Memory Storage Pipeline

ourmem provides three ingestion paths, each optimized for different use cases:

```
                    ┌─────────────────────────────────────────────────────────┐
                    │                   Storage Paths                         │
                    ├──────────────────┬──────────────────┬───────────────────┤
                    │  Conversation    │  File Import     │  Direct Memory    │
                    │  Ingest          │                  │  Creation         │
                    │  POST /memories  │  POST /imports   │  POST /memories   │
                    │  {messages}      │  multipart/form  │  {content}        │
                    └────────┬─────────┴────────┬─────────┴─────────┬─────────┘
                             │                  │                   │
                    ┌────────▼─────────┐ ┌──────▼──────────┐ ┌─────▼──────────┐
                    │ Dual-Stream      │ │ Intelligence    │ │ Direct Store   │
                    │ (sync + async)   │ │ Task (async)    │ │ (sync)         │
                    │                  │ │                 │ │                │
                    │ Fast: session    │ │ Strategy detect │ │ Memory::new()  │
                    │ Slow: LLM path  │ │ → extract       │ │ → embed        │
                    │                  │ │ → reconcile     │ │ → LanceDB      │
                    └────────┬─────────┘ └──────┬──────────┘ └─────┬──────────┘
                             │                  │                   │
                             └──────────────────┼───────────────────┘
                                                │
                                       ┌────────▼────────┐
                                       │    LanceDB      │
                                       │  (per-space)    │
                                       └─────────────────┘
```

### 1.1 Conversation Ingest (`POST /v1/memories` with `messages`)

The primary path for plugin-driven memory capture. Uses a **dual-stream architecture**: a synchronous fast path stores raw messages immediately, while an asynchronous slow path extracts and reconciles facts via LLM.

```
Messages ──▶ Session Store (sync, <50ms)
    │
    └──▶ Background Task (async)
         │
         ├── 1. select_messages()     ── Budget: 20 messages / 200KB
         ├── 2. PrivacyFilter         ── Strip <private> tags → [REDACTED]
         ├── 3. FactExtractor         ── LLM extracts atomic facts (max 50)
         ├── 4. NoiseFilter           ── Regex + vector prototype matching
         ├── 5. AdmissionControl      ── 5-dimension scoring gate
         └── 6. Reconciler            ── 7-decision reconciliation
                  │
                  ▼
              LanceDB Store
```

**Stage Details:**

| Stage | Component | What It Does |
|-------|-----------|-------------|
| **Message Selection** | `select_messages()` | Takes the last N messages within budget (20 messages, 200KB). Selects from the end of the conversation to capture the most recent context. |
| **Privacy Filter** | `strip_private_content()` | Replaces `<private>...</private>` blocks with `[REDACTED]`. Messages that are fully private (nothing left after stripping) are dropped entirely. |
| **Fact Extraction** | `FactExtractor` | Sends sanitized conversation to LLM with structured prompt. Extracts atomic facts with 3-layer detail (l0/l1/l2), category, and tags. Max 50 facts per extraction. Strips platform envelope metadata (channel info, sender metadata) before sending. |
| **Noise Filter** | `NoiseFilter` | Three-layer filtering: (1) Regex patterns catch greetings, thanks, meta-questions, agent refusals in EN/CN; (2) Vector prototype matching (cosine similarity ≥ 0.82) catches semantically similar noise; (3) Feedback learning — confirmed noise vectors are remembered (up to 200) for future filtering. |
| **Admission Control** | `AdmissionControl` | 5-dimension weighted scoring: `composite = 0.1·utility + 0.1·confidence + 0.1·novelty + 0.1·recency + 0.6·type_prior`. Balanced preset: reject < 0.45, admit ≥ 0.60. Category priors: Profile=0.95, Preferences=0.90, Patterns=0.85, Cases=0.80, Entities=0.75, Events=0.45. |
| **Reconciliation** | `Reconciler` | Compares extracted facts against existing memories using dual search (vector + FTS). Makes one of 7 decisions per fact. See [Section 1.4](#14-reconciler--7-decision-types). |

**Ingest Modes:**
- `smart` (default) — Full pipeline: session store + async LLM extraction
- `raw` — Session store only, no LLM processing

**Graceful Degradation:** If the LLM fails, raw messages are still preserved in the session store. The slow path logs the error and exits without crashing.

### 1.2 File Import (`POST /v1/imports`)

Handles bulk document import with intelligent content-type detection and strategy routing.

```
┌──────────┐    ┌─────────────────┐    ┌───────────────┐    ┌───────────┐
│ File      │───▶│ Intelligence    │───▶│ Reconciler    │───▶│ LanceDB   │
│ Upload    │    │ Task            │    │ (7 decisions)  │    │ Store     │
└──────────┘    └──────┬──────────┘    └───────────────┘    └───────────┘
                       │
              ┌────────▼────────┐
              │ Strategy Router │
              │ auto / atomic / │
              │ section / doc   │
              └────────┬────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐
    │ Atomic   │ │ Section  │ │ Document │
    │ Extract  │ │ Extract  │ │ Extract  │
    └──────────┘ └──────────┘ └──────────┘
```

**Import Flow:**

1. **Upload** — Multipart form with file, strategy, file_type, space_id
2. **Dedup Check** — SHA-256 content hash prevents duplicate imports
3. **Session Storage** — Raw content stored in session table with `import-{task_id}` session ID
4. **Background Processing** — Acquires `import_semaphore` (capacity=3), then:

**Strategy Detection (`auto` mode):**

| Content Hint | Detection Rule | Extraction Method |
|-------------|----------------|-------------------|
| `Conversation` | ≥ 3 role-pattern lines (`\nuser:`, `\nassistant:`, etc.) | Atomic — chunk & extract per chunk |
| `LargeDoc` | Content > 80,000 characters | Atomic — smart_split with 2,000 char overlap |
| `StructuredDoc` | Has markdown headings (`# ` or `## `) AND ≥ 500 words | Section — split by headings, one memory per section |
| `ShortNote` | < 500 words, no headings | Document — single comprehensive memory |

**Extraction Paths:**

- **Atomic** (`extract_atomic`): Splits text into chunks (max 80K chars, 2K overlap) using `smart_split()`. Boundary detection prefers: heading (`## `) > paragraph (`\n\n`) > newline (`\n`) > hard cut. Each chunk is sent to LLM for fact extraction.
- **Section** (`extract_sections`): Splits at `# ` and `## ` headings. Each section gets a dedicated LLM prompt producing exactly one memory per section. Retries up to 2 times with exponential backoff (1s, 2s).
- **Document** (`extract_document`): Entire text sent as one prompt, producing a single comprehensive memory. Also retries up to 2 times.

**Concurrency Control:**
- `import_semaphore` (capacity=3) — Limits concurrent extraction tasks
- `reconcile_semaphore` (capacity=1) — Serializes reconciliation to prevent race conditions

**Source Text Preservation:** Each extracted fact retains `source_text` — the original chunk/section/document text. This becomes the `content` field in the stored memory, ensuring the original text is searchable via both vector and BM25.

**Batch Self-Dedup:** When the database is empty (no existing memories to reconcile against), facts within the same import batch are deduplicated via LLM. The LLM identifies duplicate/overlapping facts and returns indices to keep.

### 1.3 Direct Memory Creation (`POST /v1/memories` with `content`)

The simplest path — creates a single pinned memory with no LLM processing.

```
API Body ──▶ Memory::new() ──▶ Embed content ──▶ LanceDB create
```

- Memory type: `Pinned` (protected from MERGE/SUPERSEDE by reconciler)
- Category: `Preferences` (default)
- Embedding: Generated immediately from `content` field
- No noise filter, no admission control, no reconciliation

### 1.4 Reconciler — 7 Decision Types

The reconciler is the intelligence layer that prevents duplicate memories and maintains knowledge consistency. For each extracted fact, it:

1. **Gathers existing memories** via dual search (vector + FTS) — up to 60 existing memories, 5 per fact
2. **Preference slot guard** — Detects same-brand-different-item preferences (e.g., "likes Starbucks latte" vs "likes Starbucks americano") and auto-creates without LLM
3. **LLM decision** — Sends facts + existing memories to LLM, receives one decision per fact

```
Extracted Facts ──▶ gather_existing() ──▶ LLM Reconciliation ──▶ Execute Decisions
                    │                                              │
                    ├── vector_search (per fact, top 5, min 0.3)   ├── CREATE
                    └── fts_search (per fact, top 5)               ├── MERGE
                                                                   ├── SKIP
                                                                   ├── SUPERSEDE
                                                                   ├── SUPPORT
                                                                   ├── CONTEXTUALIZE
                                                                   └── CONTRADICT
```

**Decision Types:**

| Decision | Effect | When Used |
|----------|--------|-----------|
| **CREATE** | New memory created with embedding | Genuinely new information |
| **MERGE** | Existing memory updated with combined content + re-embedded | Fact adds detail to existing memory. Profile category always merges. |
| **SKIP** | No action | Duplicate or less informative than existing |
| **SUPERSEDE** | New memory created, old memory archived (`invalidated_at` set, `superseded_by` linked) | Fact updates/replaces outdated information |
| **SUPPORT** | Existing memory's `confidence` boosted by +0.1 (max 1.0), `Supports` relation added | Fact reinforces existing memory |
| **CONTEXTUALIZE** | New memory created with `Contextualizes` relation to existing | Fact adds situational nuance (e.g., "prefers tea in the evening") |
| **CONTRADICT** | For temporal categories → routes to SUPERSEDE. Otherwise: new memory created, bidirectional `Contradicts` relations added | Fact directly contradicts existing memory |

**Category-Aware Rules:**
- `profile` — Always MERGE when match exists (never SUPERSEDE/CONTRADICT)
- `events`, `cases` — Only CREATE or SKIP (append-only, never modify)
- `preferences`, `entities` — All 7 operations supported
- `patterns` — Supports MERGE

**Pinned Memory Protection:** Memories with type `Pinned` cannot be MERGED or SUPERSEDED. These decisions are automatically downgraded to CREATE.

**ID Mapping:** The reconciler maps internal UUIDs to sequential integer IDs (`[0]`, `[1]`, ...) in the LLM prompt to prevent UUID leakage and reduce token usage.

### 1.5 Memory Field Model

Each memory is stored with a multi-layer content structure:

| Field | Source | Purpose |
|-------|--------|---------|
| `content` | Original source text (chunk/section/document) | BM25 FTS index + vector embedding. The ground truth. |
| `l0_abstract` | LLM-generated | One-line index entry. Used for scan/browse, FTS indexed. |
| `l1_overview` | LLM-generated | Structured markdown summary (2-5 lines). Key attributes at a glance. |
| `l2_content` | LLM-generated | Full narrative with all details, context, and nuance. |

**Why dual content?** The `content` field preserves the original text for faithful keyword search and embedding. The `l0`/`l1`/`l2` layers provide progressively detailed LLM-generated summaries optimized for agent consumption.

**Full Schema (29 columns in LanceDB):**

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Unique identifier |
| `content` | String | Original source text |
| `l0_abstract` | String | One-line summary |
| `l1_overview` | String | Structured overview |
| `l2_content` | String | Detailed narrative |
| `vector` | Float32[1024] | Embedding vector (nullable) |
| `category` | Enum | profile, preferences, entities, events, cases, patterns |
| `memory_type` | Enum | Insight, Session, Pinned |
| `state` | Enum | Active, Archived, Deleted |
| `tier` | Enum | Core, Working, Peripheral |
| `importance` | Float32 | 0.0–1.0, affects retrieval scoring |
| `confidence` | Float32 | 0.0–1.0, boosted by SUPPORT decisions |
| `access_count` | Int32 | Retrieval frequency counter |
| `tags` | JSON | User-defined labels |
| `relations` | JSON | Array of `{relation_type, target_id, context_label}` |
| `superseded_by` | UUID | Link to replacement memory |
| `invalidated_at` | Timestamp | When memory was superseded |
| `tenant_id` | String | Tenant isolation |
| `space_id` | String | Space-based isolation |
| `visibility` | String | `global`, `private`, or `shared:<space-id>` |
| `provenance` | JSON | Sharing lineage tracking |

---

## 2. Memory Retrieval Pipeline

### 2.1 Overview

The retrieval pipeline processes search queries through 12 stages, combining vector similarity and keyword matching with progressive refinement:

```
SearchRequest
  │
  ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 1:  parallel_search     Vector + BM25 in parallel            │
│ Stage 2:  rrf_fusion          Reciprocal Rank Fusion (K=60)        │
│ Stage 3:  rrf_normalize       Min-max normalize to [0, 1]          │
│ Stage 4:  min_score_filter    Drop if score < 0.3                  │
│ Stage 5:  topk_cap            Truncate to limit × 2                │
│ Stage 6:  cross_encoder       Rerank blend: 0.6·rerank + 0.4·orig │
│ Stage 7:  bm25_floor          Protect exact keyword matches        │
│ Stage 8:  decay_boost         Weibull time decay                   │
│ Stage 9:  importance_weight   Score × (0.7 + 0.3·importance)       │
│ Stage 10: length_norm         Penalize overly long content         │
│ Stage 11: hard_cutoff         Drop if score < 0.005                │
│ Stage 12: mmr_diversity       Jaccard dedup + truncate to limit    │
└─────────────────────────────────────────────────────────────────────┘
  │
  ▼
Vec<SearchResult> { memory, score }
```

### 2.2 Stage-by-Stage Detail

#### Stage 1: Parallel Search

Executes vector search and BM25 full-text search concurrently via `tokio::join!`.

- **Vector search**: Embeds query → ANN search on `vector` column (cosine similarity). Fetches `limit × 3` candidates with min_score=0.0 (no pre-filtering).
- **BM25 search**: Full-text search on `content` and `l0_abstract` columns. Same fetch limit.
- **Fault tolerance**: If either search fails, the pipeline continues with results from the other. Both failing → empty result.

#### Stage 2: RRF Fusion

Combines results from both search legs using [Reciprocal Rank Fusion](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf):

```
vector_rrf = vector_weight / (rrf_k + rank)    →  0.7 / (60 + rank)
bm25_rrf   = bm25_weight   / (rrf_k + rank)    →  0.3 / (60 + rank)
```

- Memories appearing in both legs have their RRF scores **summed**
- **Pinned boost**: Pinned memories get `score × 1.5`

| Parameter | Default | Description |
|-----------|---------|-------------|
| `vector_weight` | 0.7 | Weight for vector search leg |
| `bm25_weight` | 0.3 | Weight for BM25 search leg |
| `rrf_k` | 60.0 | RRF smoothing constant |
| `pinned_boost` | 1.5 | Multiplier for pinned memories |

#### Stage 3: RRF Normalize

Normalizes raw RRF scores (typically ~0.01–0.03) to the [0, 1] range:

- **Multiple results**: Min-max normalization → highest=1.0, lowest=0.0
- **Single result**: `score = min(score × 40.0, 1.0)`
- **All equal scores**: All set to 1.0

#### Stage 4: Min Score Filter

Drops candidates below the minimum score threshold.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `min_score` | 0.3 | Minimum normalized score to keep |

#### Stage 5: Top-K Cap

Sorts by score descending and truncates to `limit × 2` candidates. This provides enough candidates for reranking without excessive computation.

#### Stage 6: Cross-Encoder Rerank

If a reranker is configured (Jina, Voyage, or Pinecone), blends reranker scores with original scores:

```
final_score = rerank_score × 0.6 + original_score × 0.4
```

| Provider | Default Endpoint |
|----------|-----------------|
| `jina` | `https://api.jina.ai/v1/rerank` |
| `voyage` | `https://api.voyageai.com/v1/rerank` |
| `pinecone` | `https://api.pinecone.io/rerank` |

Configure via `OMEM_RERANK_PROVIDER` and `OMEM_RERANK_API_KEY` environment variables. Timeout: 5 seconds. If reranker fails, original scores are preserved.

#### Stage 7: BM25 Floor

Protects high-quality keyword matches from being over-penalized by the reranker:

```
if bm25_score ≥ 0.75:
    floor = pre_rerank_score × 0.95
    score = max(score, floor)
```

This ensures that exact keyword matches retain at least 95% of their pre-rerank score.

#### Stage 8: Decay Boost

Applies Weibull time-decay to favor recent, frequently-accessed, and important memories:

```
composite = 0.4·recency + 0.3·frequency + 0.3·intrinsic

recency   = exp(-λ · t^β)
    λ = ln(2) / (half_life × exp(1.5 × importance))
    β = 0.8 (Core) | 1.0 (Working) | 1.3 (Peripheral)

frequency = (1 - exp(-count/5)) × gap_factor
intrinsic = importance × confidence

boosted_score = score × (0.3 + 0.7 × composite)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `half_life_days` | 30.0 | Base half-life for recency decay |
| `beta_core` | 0.8 | Sub-exponential — Core memories decay slowly |
| `beta_working` | 1.0 | Exponential — standard decay |
| `beta_peripheral` | 1.3 | Super-exponential — Peripheral memories decay fast |
| `search_boost_min` | 0.3 | Minimum boost factor (floor) |
| `floor_core` | 0.9 | Minimum composite for Core tier |
| `floor_working` | 0.7 | Minimum composite for Working tier |
| `floor_peripheral` | 0.5 | Minimum composite for Peripheral tier |

#### Stage 9: Importance Weight

Applies a mild importance-based multiplier:

```
score × = 0.7 + 0.3 × importance
```

- `importance=0` → score × 0.7
- `importance=1` → score × 1.0 (unchanged)

#### Stage 10: Length Normalization

Penalizes excessively long content to prevent verbose memories from dominating:

```
len_ratio = content.len() / 500.0
denominator = max(1.0, 1.0 + log₂(len_ratio))
score /= denominator
```

| Content Length | Penalty |
|---------------|---------|
| ≤ 500 chars | None (÷1.0) |
| 1,000 chars | ÷2.0 |
| 2,000 chars | ÷3.0 |
| 4,000 chars | ÷4.0 |

#### Stage 11: Hard Cutoff

Final safety net — drops any candidate with score below the hard cutoff threshold.

| Parameter | Default | Description |
|-----------|---------|-------------|
| `hard_cutoff` | 0.005 | Absolute minimum score after all adjustments |

#### Stage 12: MMR Diversity

Maximal Marginal Relevance removes near-duplicate results:

1. Sort by score descending
2. For each candidate, compute word-level Jaccard similarity against all higher-ranked results
3. If `jaccard > 0.85` with any prior result → `score × 0.5` (50% penalty)
4. Re-sort and truncate to final `limit`

### 2.3 Dual-Path Search

The retrieval pipeline combines two complementary search strategies:

```
                    ┌──────────────────────┐
                    │    Search Query       │
                    └──────────┬───────────┘
                               │
                    ┌──────────▼───────────┐
                    │   Embed Query         │
                    │   (1024-dim vector)   │
                    └──────────┬───────────┘
                               │
              ┌────────────────┼────────────────┐
              ▼                                  ▼
    ┌─────────────────┐                ┌─────────────────┐
    │  Vector Search   │                │  BM25 Search     │
    │                  │                │                  │
    │  ANN on vector   │                │  FTS on content  │
    │  column          │                │  + l0_abstract   │
    │                  │                │                  │
    │  Cosine distance │                │  Keyword match   │
    │  → similarity    │                │  with ranking    │
    │                  │                │                  │
    │  Weight: 0.7     │                │  Weight: 0.3     │
    └────────┬────────┘                └────────┬────────┘
             │                                   │
             └──────────────┬────────────────────┘
                            ▼
                  ┌─────────────────┐
                  │  RRF Fusion     │
                  │  K=60           │
                  └─────────────────┘
```

**Vector Search:**
- Index type: IVF-HNSW-SQ (IVF + HNSW + Scalar Quantization)
- Distance metric: Cosine
- Score conversion: `similarity = 1.0 - cosine_distance`
- Dimension: 1024 (configurable via embedding provider)
- Filter: `state != 'deleted'` + optional scope/visibility filters

**BM25 Full-Text Search:**
- Two FTS indexes: `content` column and `l0_abstract` column
- Post-filtering (search first, then apply scope/visibility filters)
- Auto-created on first write (LanceDB requires data before index creation)

**Cross-Space Search:**
When a user has access to multiple spaces, the pipeline runs independently on each space's store, normalizes scores per-space, applies space weights, then merges and re-ranks globally.

---

## 3. Plugin Integration

ourmem integrates with AI coding platforms through four plugins, each adapted to the platform's extension model:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        AI Agent Platforms                            │
├──────────────┬──────────────┬──────────────┬────────────────────────┤
│  OpenCode    │  OpenClaw    │  Claude Code │  MCP (Cursor/VS Code) │
│  Plugin      │  Plugin      │  Hooks       │  Server               │
│              │              │              │                        │
│  4 hooks     │  7 lifecycle │  2 shell     │  6 tools              │
│  5 tools     │  5 tools     │  hooks       │  1 resource           │
│              │  MemoryBackend│             │                        │
│              │  ContextEngine│             │                        │
└──────┬───────┴──────┬───────┴──────┬───────┴────────────┬──────────┘
       │              │              │                    │
       └──────────────┴──────────────┴────────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │  ourmem Server     │
                    │  REST API          │
                    │  X-API-Key auth    │
                    └────────────────────┘
```

### 3.1 OpenCode Plugin (`@ourmem/opencode`)

**Architecture:** TypeScript plugin implementing `@opencode-ai/plugin` interface. Registers hooks, tools, and event handlers.

**Configuration:**

| Env Variable | Default | Purpose |
|-------------|---------|---------|
| `OMEM_API_URL` | `http://localhost:8080` | Server URL |
| `OMEM_API_KEY` | `""` | API authentication |

**Container Tags:** Each session generates two SHA-256 hash-based tags for isolation:
- `omem_user_{hash(email)[0:16]}` — User dimension
- `omem_project_{hash(cwd)[0:16]}` — Project dimension

These tags are attached to all store and search operations.

**Automatic Memory Retrieval:**

| Hook | Trigger | Behavior |
|------|---------|----------|
| `experimental.chat.system.transform` | Every system prompt build | Searches `q=*&limit=10` with container tags. Groups results by category. Injects `<omem-context>` XML block into system prompt. |
| `chat.message` | Every user message | Scans for memory keywords ("remember", "save this", "记住", "记一下", etc.). If detected, flags session for nudge prompt on next system build. |

**Automatic Memory Storage:**

| Hook | Trigger | Behavior |
|------|---------|----------|
| `event` (session.idle) | Session becomes idle | Fetches recent 20 memories. Strips `<private>` tags. Sends cleaned messages via `POST /v1/memories` with `mode: "smart"` for LLM extraction. |

**On-Demand Tools (5):**

| Tool | API Call |
|------|---------|
| `memory_store` | `POST /v1/memories` with content + container tags |
| `memory_search` | `GET /v1/memories/search` with container tags |
| `memory_get` | `GET /v1/memories/{id}` |
| `memory_update` | `PUT /v1/memories/{id}` |
| `memory_delete` | `DELETE /v1/memories/{id}` |

### 3.2 OpenClaw Plugin (`@ourmem/openclaw`)

**Architecture:** The richest integration — registers MemoryBackend, ContextEngine, Hooks, and Tools. Four integration layers.

**Layer 1: MemoryBackend** — Registers as OpenClaw's memory storage backend, providing `store()`, `search()`, `get()`, `update()`, `delete()`, `list()` methods that proxy directly to the ourmem API.

**Layer 2: ContextEngine** — Implements 7 lifecycle methods:

```
bootstrap()              ──▶ Health check (GET /health)
    │
ingest(message)          ──▶ Smart ingest single message
    │
assemble(budget)         ──▶ Parallel: GET /v1/profile + search memories
    │                        Format within token budget (text.length / 4)
    │                        Inject <user-profile> + <memories> blocks
    │
afterTurn(turn)          ──▶ Smart ingest user + assistant messages
    │
prepareSubagentSpawn()   ──▶ Search memories relevant to sub-task (limit=5)
    │
onSubagentEnded(result)  ──▶ Smart ingest sub-agent summary
    │
compact()                ──▶ No-op (server-side not implemented)
```

**Layer 3: Hooks (2):**

| Hook | Trigger | Behavior |
|------|---------|----------|
| `before_prompt_build` | Before each prompt | Clears old `<omem-context>`, searches `q=*&limit=10`, injects fresh context |
| `agent_end` (success only) | Agent execution ends | Takes last 20 messages (200KB budget), strips self-references, sends `mode: "smart"` |

**Layer 4: Tools** — Same 5 tools as OpenCode (without container tags).

### 3.3 Claude Code Plugin (Hooks)

**Architecture:** Pure Bash scripts registered via `hooks.json`. The simplest integration.

**Configuration:**

| Env Variable | Required | Purpose |
|-------------|----------|---------|
| `OMEM_API_URL` | Yes | Server URL |
| `OMEM_API_KEY` | Yes | API key (exits if empty) |

**Hooks:**

| Event | Script | Behavior |
|-------|--------|----------|
| `SessionStart` | `session-start.sh` | Calls `GET /v1/memories?limit=20`. Formats as markdown list with relative timestamps ("3d ago"). Injects via `hookSpecificOutput.SessionStart.additionalContext`. |
| `Stop` (timeout: 120s) | `stop.sh` | Reads transcript from stdin. Extracts last assistant message (truncated to 1000 chars). Skips if < 50 chars. Stores with `tags: ["auto-captured"], source: "claude-code"`. |

**Key Differences:**
- No smart ingest — stores raw last assistant message (not the full conversation)
- No on-demand tools — agents cannot explicitly store/search memories
- No privacy filtering at the plugin level
- Uses `curl` with 8-second timeout

### 3.4 MCP Server (`@ourmem/mcp`)

**Architecture:** Standalone MCP (Model Context Protocol) server process communicating via stdio transport. Pure on-demand mode with no automatic hooks.

**Configuration:**

| Env Variable | Required | Purpose |
|-------------|----------|---------|
| `OMEM_API_URL` | Yes | Server URL |
| `OMEM_API_KEY` | Yes (throws on missing) | API key |

**Tools (6 — one more than other plugins):**

| Tool | Parameters | API Call |
|------|-----------|---------|
| `memory_store` | `content`, `tags?`, `source?` | `POST /v1/memories` (source defaults to "mcp") |
| `memory_search` | `query`, `limit?` (1-50), `scope?` | `GET /v1/memories/search` |
| `memory_get` | `id` | `GET /v1/memories/{id}` |
| `memory_update` | `id`, `content`, `tags?` | `PUT /v1/memories/{id}` |
| `memory_forget` | `id` | `DELETE /v1/memories/{id}` |
| `memory_profile` | (none) | `GET /v1/profile` |

**Resource (1):**

| Resource | URI | Description |
|----------|-----|-------------|
| User Profile | `omem://profile` | Returns synthesized user profile as JSON |

**Key Differences:**
- No automatic hooks — fully agent-driven
- Unique `memory_profile` tool and `omem://profile` resource
- Uses `memory_forget` instead of `memory_delete`
- Longer timeout: 8000ms (vs 5000ms for other plugins)
- Errors are thrown (not silently swallowed)

### 3.5 Cross-Plugin Comparison

**When Memories Are Stored:**

| Trigger | OpenCode | OpenClaw | Claude Code | MCP |
|---------|----------|----------|-------------|-----|
| Session end/idle | ✅ Smart ingest last 20 msgs | ✅ Smart ingest last 20 msgs (200KB) | ✅ Store last assistant msg (>50 chars) | ❌ |
| After each turn | ❌ | ✅ ContextEngine.afterTurn() | ❌ | ❌ |
| Per message | ❌ | ✅ ContextEngine.ingest() | ❌ | ❌ |
| Sub-agent end | ❌ | ✅ ContextEngine.onSubagentEnded() | ❌ | ❌ |
| Agent explicit call | ✅ memory_store tool | ✅ memory_store tool | ❌ | ✅ memory_store tool |
| Keyword detection | ✅ Nudges agent to use tool | ❌ | ❌ | ❌ |

**When Memories Are Retrieved:**

| Trigger | OpenCode | OpenClaw | Claude Code | MCP |
|---------|----------|----------|-------------|-----|
| Session start | ✅ Search `*` (10 results) | ✅ Search `*` (10 results) | ✅ List recent 20 | ❌ |
| Context assembly | ❌ | ✅ Profile + search (token budget) | ❌ | ❌ |
| Sub-agent spawn | ❌ | ✅ Task-relevant search (5 results) | ❌ | ❌ |
| Agent explicit call | ✅ memory_search tool | ✅ memory_search tool | ❌ | ✅ memory_search tool |
| User profile | ❌ | ✅ ContextEngine.assemble() | ❌ | ✅ memory_profile tool |

**Privacy Protection:**

| Feature | OpenCode | OpenClaw | Claude Code | MCP |
|---------|----------|----------|-------------|-----|
| `<private>` tag stripping | ✅ Client-side | ❌ | ❌ | ❌ |
| Server-side privacy filter | ✅ (ingest pipeline) | ✅ (ingest pipeline) | N/A (direct store) | N/A (direct store) |

---

## Appendix: Key Constants Reference

| Constant | Value | Location |
|----------|-------|----------|
| `SMART_SPLIT_MAX_CHARS` | 80,000 | intelligence.rs |
| `SMART_SPLIT_OVERLAP` | 2,000 | intelligence.rs |
| `DEFAULT_MAX_FACTS` | 50 | extractor.rs |
| `DEFAULT_MAX_INPUT_CHARS` | 8,000 | extractor.rs |
| `BYTE_BUDGET` | 200,000 | pipeline.rs (ingest) |
| `MESSAGE_BUDGET` | 20 | pipeline.rs (ingest) |
| `NOISE_THRESHOLD` | 0.82 | noise.rs |
| `MAX_LEARNED_NOISE` | 200 | noise.rs |
| `W_TYPE_PRIOR` | 0.6 | admission.rs |
| `MAX_EXISTING` | 60 | reconciler.rs |
| `MAX_PER_FACT` | 5 | reconciler.rs |
| `MIN_SIMILARITY` | 0.3 | reconciler.rs |
| `VECTOR_DIM` | 1024 | lancedb.rs |
| `RRF_K` | 60.0 | pipeline.rs (retrieve) |
| `VECTOR_WEIGHT` | 0.7 | pipeline.rs (retrieve) |
| `BM25_WEIGHT` | 0.3 | pipeline.rs (retrieve) |
| `MIN_SCORE` | 0.3 | pipeline.rs (retrieve) |
| `HARD_CUTOFF` | 0.005 | pipeline.rs (retrieve) |
| `PINNED_BOOST` | 1.5 | pipeline.rs (retrieve) |
| `RRF_SCALE` | 40.0 | pipeline.rs (retrieve) |
| `HALF_LIFE_DAYS` | 30.0 | decay.rs |
| `BETA_CORE` | 0.8 | decay.rs |
| `BETA_WORKING` | 1.0 | decay.rs |
| `BETA_PERIPHERAL` | 1.3 | decay.rs |
| `SEARCH_BOOST_MIN` | 0.3 | decay.rs |
