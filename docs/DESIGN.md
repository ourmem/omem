# omem — Architecture & Design

## Table of Contents

1. [System Architecture](#1-system-architecture)
2. [Storage Layer](#2-storage-layer)
3. [Ingestion Pipeline](#3-ingestion-pipeline)
4. [Retrieval Pipeline](#4-retrieval-pipeline)
5. [Memory Lifecycle](#5-memory-lifecycle)
6. [User Profile](#6-user-profile)
7. [Multi-Tenant Isolation](#7-multi-tenant-isolation)
8. [Cross-Platform Plugin Strategy](#8-cross-platform-plugin-strategy)

---

## 1. System Architecture

omem follows a **stateless server + stateless plugins** pattern. The Rust server is the single source of truth; all plugins are thin HTTP clients.

```
                    ┌─────────────────────────────┐
                    │       Plugin Layer           │
                    │  (stateless HTTP clients)    │
                    │                              │
                    │  OpenCode  Claude  OpenClaw  │
                    │  MCP       Code              │
                    └──────────────┬───────────────┘
                                   │ REST API
                    ┌──────────────┴───────────────┐
                    │      omem-server (Rust)       │
                    │                               │
                    │  ┌─────────┐  ┌───────────┐  │
                    │  │ Ingest  │  │ Retrieve  │  │
                    │  │Pipeline │  │ Pipeline  │  │
                    │  └────┬────┘  └─────┬─────┘  │
                    │       │             │        │
                    │  ┌────┴─────────────┴─────┐  │
                    │  │     Domain Layer       │  │
                    │  │  Memory · Profile ·    │  │
                    │  │  Tenant · Lifecycle    │  │
                    │  └───────────┬────────────┘  │
                    │              │                │
                    │  ┌───────────┴────────────┐  │
                    │  │    LanceDB Store       │  │
                    │  │  (vector + FTS + CRUD) │  │
                    │  └───────────┬────────────┘  │
                    └──────────────┬───────────────┘
                                   │
                    ┌──────────────┴───────────────┐
                    │     S3 / MinIO Storage        │
                    │   s3://bucket/omem/           │
                    │   ├── memories.lance/         │
                    │   ├── sessions.lance/         │
                    │   └── tenants.lance/          │
                    └──────────────────────────────┘
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **Rust** | Memory safety, zero-cost abstractions, single binary deployment |
| **Axum 0.8** | Async-first, tower middleware ecosystem, excellent ergonomics |
| **LanceDB** | Embedded vector DB, no separate process, native S3 support |
| **Stateless plugins** | No state in plugins → easy to update, no sync issues |
| **S3 storage** | Durable, cheap ($0.023/GB/month), serverless-compatible |

### Module Structure

```
omem-server/src/
├── api/          # HTTP layer (axum router, handlers, middleware)
├── domain/       # Core types (Memory, Tenant, Category, Tier)
├── ingest/       # Two-phase ingestion pipeline
├── retrieve/     # 11-stage retrieval pipeline
├── lifecycle/    # Decay, tier promotion, auto-forgetting
├── store/        # LanceDB persistence
├── embed/        # Embedding providers (Bedrock, OpenAI-compat, noop)
├── llm/          # LLM providers (Bedrock, OpenAI-compat, noop)
├── multimodal/   # PDF, image, video, code processing
├── connectors/   # GitHub webhook integration
└── profile/      # User profile aggregation
```

---

## 2. Storage Layer

### LanceDB on S3

omem uses [LanceDB](https://lancedb.com/) as an embedded vector database. LanceDB stores data in the Lance columnar format, which supports:

- **Vector search** — IVF-PQ index on 1024-dimensional embeddings
- **Full-text search** — BM25 inverted index (Tantivy-based)
- **CRUD operations** — Append, update, delete with ACID transactions
- **S3-native** — Direct read/write to S3 without local disk

### Table Schemas

**memories** — Primary memory storage

| Column | Type | Description |
|--------|------|-------------|
| `id` | String | UUID v4 primary key |
| `content` | String | Full memory content |
| `l0_abstract` | String | One-line abstract (LLM-generated) |
| `l1_overview` | String | Paragraph summary (LLM-generated) |
| `l2_content` | String | Full detail content |
| `category` | String | profile / preferences / entities / events / cases / patterns |
| `memory_type` | String | pinned / insight / session |
| `state` | String | active / archived / deleted |
| `tier` | String | core / working / peripheral |
| `importance` | Float32 | 0.0–1.0 importance score |
| `confidence` | Float32 | 0.0–1.0 confidence score |
| `access_count` | UInt32 | Number of times retrieved |
| `tags` | List[String] | User/system tags |
| `scope` | String | Scope filter (default: "global") |
| `tenant_id` | String | Tenant isolation key |
| `agent_id` | String? | Optional agent isolation |
| `session_id` | String? | Session grouping |
| `source` | String? | Origin (e.g., "github:owner/repo") |
| `vector` | FixedSizeList[Float32, 1024] | Embedding vector |
| `created_at` | String | ISO 8601 timestamp |
| `updated_at` | String | ISO 8601 timestamp |

**sessions** — Raw conversation messages (fast path)

| Column | Type | Description |
|--------|------|-------------|
| `id` | String | UUID v4 |
| `content` | String | Message content |
| `role` | String | user / assistant |
| `content_hash` | String | SHA-256 for deduplication |
| `tenant_id` | String | Tenant isolation |
| `session_id` | String | Session grouping |
| `created_at` | String | ISO 8601 timestamp |

**tenants** — Tenant registry

| Column | Type | Description |
|--------|------|-------------|
| `id` | String | UUID v4 (also used as API key) |
| `name` | String | Human-readable name |
| `status` | String | active / suspended |
| `config` | String | JSON-encoded TenantConfig |
| `created_at` | String | ISO 8601 timestamp |

### Indexing Strategy

- **Vector index**: IVF-PQ with nprobes=20 for recall/speed balance
- **FTS index**: BM25 on `content` field, Tantivy tokenizer
- **No secondary indexes**: LanceDB scans are fast enough for tenant-filtered queries at current scale

---

## 3. Ingestion Pipeline

The ingestion pipeline has two paths:

### Fast Path (All Modes)

Every message is immediately stored in the `sessions` table with SHA-256 content hash deduplication. This ensures no data loss even if the slow path fails.

```
Messages → SHA-256 hash → Deduplicate → Store in sessions table
```

### Slow Path (Smart Mode Only)

Runs asynchronously via `tokio::spawn`. Extracts structured facts from conversations using LLM.

```
┌──────────────────────────────────────────────────────────────┐
│                    Smart Ingestion Pipeline                   │
│                                                              │
│  1. Message Selection (last 20 msgs, 200KB budget)           │
│                          ↓                                   │
│  2. Privacy Filter (<private> tag stripping)                 │
│                          ↓                                   │
│  3. Fact Extraction (LLM → ExtractedFact[])                  │
│     - content, l0_abstract, l1_overview, l2_content          │
│     - category, tags, importance, confidence                 │
│                          ↓                                   │
│  4. Noise Filter (regex patterns + vector prototype)         │
│                          ↓                                   │
│  5. Admission Control (5-dimension scoring)                  │
│     score = w_u·utility + w_c·confidence + w_n·novelty       │
│           + w_r·recency + w_t·type_prior                     │
│     threshold = 0.4 (configurable)                           │
│                          ↓                                   │
│  6. Reconciliation (compare with existing memories)          │
│     7 decisions: CREATE | MERGE | SKIP | SUPERSEDE           │
│                  | SUPPORT | CONTEXTUALIZE | CONTRADICT       │
│                          ↓                                   │
│  7. Preference Slot Guard                                    │
│     Same brand, different product → CREATE (not MERGE)       │
│                          ↓                                   │
│  8. Pinned Memory Protection                                 │
│     MERGE/SUPERSEDE on Pinned → downgrade to CREATE          │
│                          ↓                                   │
│  9. Embed + Store                                            │
└──────────────────────────────────────────────────────────────┘
```

### 7 Reconciliation Decisions

| Decision | When | Action |
|----------|------|--------|
| **CREATE** | New fact, no existing match | Insert new memory |
| **MERGE** | Same topic, complementary info | Update existing memory content |
| **SKIP** | Duplicate or near-duplicate | Discard |
| **SUPERSEDE** | New info replaces old | Archive old, create new with relation |
| **SUPPORT** | Confirms existing fact | Boost confidence of existing memory |
| **CONTEXTUALIZE** | Adds context to existing | Create new with `Contextualizes` relation |
| **CONTRADICT** | Conflicts with existing | Create new with `Contradicts` relation |

### Three-Layer Storage (L0/L1/L2)

Each memory stores content at three granularity levels:

- **L0 (Abstract)**: One-line summary (~20 tokens) — used for quick scanning
- **L1 (Overview)**: Paragraph summary (~100 tokens) — used for search result display
- **L2 (Content)**: Full detail — used when memory is selected for context

---

## 4. Retrieval Pipeline

The retrieval pipeline has 11 stages, each transformable and traceable:

```
Query → [1] Parallel Search → [2] RRF Fusion → [3] Min Score Filter
      → [4] Top-K Cap → [5] Cross-Encoder Rerank → [6] BM25 Floor
      → [7] Decay Boost → [8] Importance Weight → [9] Length Norm
      → [10] Hard Cutoff → [11] MMR Diversity → Results
```

### Stage Details

| # | Stage | Description | Key Parameters |
|---|-------|-------------|----------------|
| 1 | **parallel_search** | Vector search + BM25 full-text search run in parallel | `tokio::join!` |
| 2 | **rrf_fusion** | Reciprocal Rank Fusion combines both result sets | `vector_weight=0.7`, `bm25_weight=0.3`, `k=60` |
| 3 | **min_score_filter** | Remove results below minimum relevance | `min_score=0.3` (default) |
| 4 | **topk_cap** | Truncate to limit×2 for reranking budget | |
| 5 | **cross_encoder_rerank** | Optional neural reranker (Jina/Voyage/Pinecone) | `60% rerank + 40% original` blend |
| 6 | **bm25_floor** | Protect high BM25 matches from reranker demotion | `≥0.75 BM25 → floor at 95% pre-rerank score` |
| 7 | **decay_boost** | Apply Weibull time decay to scores | See [Memory Lifecycle](#5-memory-lifecycle) |
| 8 | **importance_weight** | Weight by memory importance | `score × (0.7 + 0.3 × importance)` |
| 9 | **length_normalization** | Penalize very long memories | `score / log2(word_count + 1)` normalization |
| 10 | **hard_cutoff** | Remove results below absolute threshold | `cutoff=0.35` |
| 11 | **mmr_diversity** | Maximal Marginal Relevance deduplication | `Jaccard > 0.85 → 50% score penalty` |

### RRF Fusion Formula

```
score(d) = Σ [ weight_i / (k + rank_i(d)) ]

where:
  weight_vector = 0.7
  weight_bm25   = 0.3
  k             = 60

Pinned memories get 1.5× score boost after fusion.
```

### Retrieval Trace

When `include_trace=true`, each stage reports:

```json
{
  "stages": [
    {
      "name": "parallel_search",
      "input_count": 0,
      "output_count": 42,
      "duration_ms": 12.5,
      "score_range": [0.31, 0.92]
    }
  ],
  "total_duration_ms": 45.2,
  "final_count": 10
}
```

---

## 5. Memory Lifecycle

### Weibull Decay Model

Memory strength decays over time using the Weibull distribution:

```
S(t) = exp(-(t/λ)^β)

where:
  t = time since last access (hours)
  λ = scale parameter (derived from half-life)
  β = shape parameter (varies by tier)
```

**Tier-specific decay rates:**

| Tier | β (shape) | Behavior | Half-life |
|------|-----------|----------|-----------|
| Core | 0.8 | Sub-exponential (slow decay) | Long |
| Working | 1.0 | Exponential (standard decay) | Medium |
| Peripheral | 1.3 | Super-exponential (fast decay) | Short |

**Importance modulation:**

```
hl_effective = hl_base × exp(μ × importance)
```

Higher importance → longer effective half-life.

**Composite score:**

```
composite = w_r · recency + w_f · frequency + w_i · intrinsic

where:
  recency   = Weibull survival function S(t)
  frequency = log(1 + access_count) / log(1 + max_access)
  intrinsic = importance × confidence
```

### Three-Tier Promotion

```
Peripheral ──→ Working ──→ Core
     ↑              ↑
     └──────────────┘ (demotion on decay)
```

**Promotion criteria:**

| Transition | Requirements |
|------------|-------------|
| Peripheral → Working | `access_count ≥ 3` AND `composite ≥ 0.5` |
| Working → Core | `access_count ≥ 10` AND `composite ≥ 0.7` AND `importance ≥ 0.6` |
| Core → Working | `composite < 0.4` (demotion) |
| Working → Peripheral | `composite < 0.2` (demotion) |

### Auto-Forgetting

The `AutoForgetter` handles:

1. **TTL detection** — Parses temporal markers (today, tomorrow, next week, this month) and sets expiry
2. **Expired cleanup** — Archives memories past their TTL
3. **Superseded archival** — Archives memories that have been superseded by newer ones

---

## 6. User Profile

The User Profile provides a fast (<100ms) summary of what's known about the user.

### Structure

```json
{
  "static_facts": [
    "Senior backend engineer at Stripe",
    "3 years of Rust experience",
    "Based in San Francisco, PST timezone"
  ],
  "dynamic_context": [
    "Currently working on Nexus API gateway project",
    "Debugging Redis split-brain issue this week"
  ]
}
```

- **Static facts**: Stable information (name, job, location, skills) — sourced from `Profile` category memories
- **Dynamic context**: Recent/changing information — sourced from recent `Events` and `Cases` category memories

### Aggregation

The `ProfileService` aggregates the profile by:

1. Querying `Profile` category memories → static facts
2. Querying recent `Events` + `Cases` memories (last 7 days) → dynamic context
3. Deduplicating and ranking by importance

---

## 7. Multi-Tenant Isolation

### Authentication Flow

```
Request → X-API-Key header → TenantStore lookup → AuthInfo injection
```

1. Every authenticated request must include `X-API-Key` header
2. The middleware looks up the key in the `tenants` table
3. On success, `AuthInfo { tenant_id, agent_id }` is injected into the request
4. All subsequent operations filter by `tenant_id`

### Isolation Guarantees

| Layer | Mechanism |
|-------|-----------|
| **API** | Auth middleware rejects invalid keys |
| **Query** | All LanceDB queries include `tenant_id` filter |
| **Mutation** | Create operations stamp `tenant_id` on every record |
| **Read** | Get/list operations verify `tenant_id` matches |
| **Delete** | Soft-delete verifies ownership before marking |

### Agent Isolation

Optional `X-Agent-Id` header enables multi-agent isolation within a tenant. When set, memories are tagged with `agent_id` for per-agent filtering.

---

## 8. Cross-Platform Plugin Strategy

### Design Principles

1. **Server is the brain** — All intelligence (extraction, reconciliation, retrieval) lives in the Rust server
2. **Plugins are thin clients** — Each plugin is a ~200-line HTTP wrapper
3. **Platform-native integration** — Each plugin uses the platform's native extension mechanism
4. **Automatic hooks** — Memory capture and recall happen without user intervention

### Plugin Comparison

| Feature | OpenCode | Claude Code | OpenClaw | MCP |
|---------|----------|-------------|----------|-----|
| **Language** | TypeScript | Bash | TypeScript | TypeScript |
| **Runtime** | Bun | Shell | Node | Bun (stdio) |
| **Auto-recall** | `system.transform` hook | `SessionStart` hook | `before_prompt_build` | On-demand |
| **Auto-capture** | `session.idle` event | `Stop` hook | `agent_end` event | On-demand |
| **Tools** | 5 (store/search/get/update/delete) | 2 skills (store/recall) | 5 (store/search/get/update/delete) | 4 tools + 1 resource |
| **Privacy filter** | Yes (`<private>` tags) | No | No | No |
| **Keyword detection** | Yes (CN + EN) | No | No | No |
| **Context engine** | No | No | Yes (7 lifecycle methods) | No |

### Hook Lifecycle

```
Session Start
  │
  ├─ [Auto-Recall] Load recent memories → inject into system prompt
  │
  ├─ User sends message
  │    │
  │    ├─ [Keyword Detection] Check for memory-related keywords
  │    │
  │    └─ Agent processes and responds
  │
  ├─ ... (conversation continues) ...
  │
  └─ Session End / Idle
       │
       └─ [Auto-Capture] Send conversation to ingest pipeline
```

### Shared Client Interface

All plugins share the same HTTP client pattern:

```typescript
class OmemClient {
  constructor(baseUrl: string, apiKey: string)

  // Core operations
  async ingestMessages(messages, mode, sessionId): Promise<IngestResponse>
  async searchMemories(query, limit): Promise<SearchResult[]>
  async storeMemory(content, tags): Promise<Memory>
  async getMemory(id): Promise<Memory>
  async updateMemory(id, updates): Promise<Memory>
  async deleteMemory(id): Promise<void>
  async getProfile(): Promise<UserProfile>
}
```
