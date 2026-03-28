# AGENTS.md: ourmem Project Context

> This document gives any AI agent full context to continue development on the ourmem project.
> Last updated: 2026-03-28

---

## A. Project Overview

**Product name:** ourmem
**Brand tagline:** "Shared Memory That Never Forgets"
**License:** Apache-2.0

| Property | Value |
|----------|-------|
| Website | [ourmem.ai](https://ourmem.ai) |
| API endpoint | api.ourmem.ai |
| Public repo | [github.com/ourmem/omem](https://github.com/ourmem/omem) |
| Private repo | github.com/yhyyz/omem |

**What it does:** ourmem gives AI agents persistent shared memory across sessions, devices, agents, and teams. One API key reconnects everything. Agents auto-recall context on session start and auto-capture key information on session end.

**Positioning:** This is not another RAG database. ourmem is a complete memory system with smart ingestion (7-decision reconciliation), hybrid retrieval (11-stage pipeline), lifecycle management (Weibull decay), and Space-based sharing (personal/team/org). Think of it as "collective intelligence infrastructure for AI agents."

**Target users:**
1. Developers using AI coding tools (OpenCode, Claude Code, OpenClaw, Cursor) who want persistent memory
2. AI product builders who need to embed persistent memory into their own agents via REST API

---

## B. Architecture

### High-level

```
AI Agent (OpenCode / Claude Code / OpenClaw / Cursor)
    |  auto-recall + auto-capture
    v
Plugin (thin HTTP client, TypeScript or bash)
    |  REST API, X-API-Key auth
    v
omem-server (Rust, single binary)
    |
    +-- Smart Ingest --- LLM extraction -> noise filter -> admission -> 7-decision reconciliation
    +-- Hybrid Search -- vector + BM25 -> RRF fusion -> reranker -> decay boost -> MMR (11 stages)
    +-- User Profile --- static facts + dynamic context, <100ms
    +-- Space Sharing -- Personal / Team / Organization with provenance tracking
    +-- Lifecycle ------ Weibull decay, 3-tier promotion, auto-forgetting
    |
    v
LanceDB (embedded vector database)
    |
    +-- Local disk: ./omem-data/{space_id}/
    +-- Or S3: s3://{bucket}/omem/{space_id}/
```

### Core components

| Component | Language | Description |
|-----------|----------|-------------|
| `omem-server` | Rust | HTTP server, all business logic, single binary |
| `plugins/opencode` | TypeScript | OpenCode plugin (@ourmem/opencode) |
| `plugins/openclaw` | TypeScript | OpenClaw plugin (@ourmem/openclaw) |
| `plugins/mcp` | TypeScript | MCP Server for Cursor/VS Code/Claude Desktop (@ourmem/mcp) |
| `plugins/claude-code` | Bash + JSON | Claude Code plugin (hooks + skills) |

### Key architectural properties

- **Server-side embedding:** The server calls Bedrock or OpenAI-compatible APIs to generate embeddings. Clients never touch vectors.
- **Per-tenant physical isolation:** Each Space gets its own LanceDB directory. No shared tables, no row-level filtering. The `StoreManager` maintains an LRU cache of up to 1000 open LanceDB connections.
- **Space model:** Space IDs use `/` as separator (not `:`) for filesystem and S3 path safety. Format: `personal/{uuid}`, `team/{uuid}`, `org/{uuid}`.
- **personal_space_id() helper:** All CRUD operations route through `personal_space_id(tenant_id)` which maps `tenant_id` to `personal/{tenant_id}`. This is the default space for every tenant.

---

## C. Directory Structure

```
omem/
+-- Cargo.toml                  # Workspace config (members: ["omem-server"])
+-- Cargo.lock
+-- .cargo/config.toml          # musl target config (no custom linker)
+-- .env.example                # All OMEM_* env vars with docs
+-- .gitignore                  # Excludes target/, omem-data/, node_modules/, .env, .sisyphus/
+-- Dockerfile                  # Multi-stage: rust:1.94-slim-bookworm -> debian:bookworm-slim
+-- docker-compose.yml          # Dev: omem-server + MinIO (S3-compatible)
+-- docker-compose.prod.yml     # Prod: omem-server only, reads .env
+-- Makefile                    # build, test, clippy, fmt, docker, run
+-- README.md                   # English README (full docs)
+-- README_CN.md                # Chinese README
+-- LICENSE                     # Apache-2.0
+-- ourmem-homepage.png         # Homepage screenshot
|
+-- omem-server/                # === RUST SERVER (PRIVATE, never push to public repo) ===
|   +-- Cargo.toml              # Package config, features: default=["bedrock"]
|   +-- src/
|       +-- main.rs             # Entry point: config -> tracing -> stores -> embed -> llm -> axum
|       +-- lib.rs              # Module re-exports
|       +-- config.rs           # OmemConfig: all OMEM_* env vars, store_uri()
|       |
|       +-- api/                # HTTP layer
|       |   +-- router.rs       # 38 routes: authed (36) + public (2: health, create_tenant)
|       |   +-- server.rs       # AppState struct + personal_space_id() helper
|       |   +-- error.rs        # API error types -> HTTP status codes
|       |   +-- middleware/
|       |   |   +-- auth.rs     # X-API-Key extraction, tenant lookup
|       |   |   +-- logging.rs  # Request/response logging middleware
|       |   +-- handlers/
|       |       +-- memory.rs   # CRUD: create, get, update, delete, list, search
|       |       +-- profile.rs  # GET /v1/profile
|       |       +-- stats.rs    # GET /v1/stats, /v1/stats/config, /tags, /decay, /relations, /spaces, /sharing, /agents
|       |       +-- tenant.rs   # POST /v1/tenants
|       |       +-- spaces.rs   # CRUD for spaces + members
|       |       +-- sharing.rs  # share, unshare, pull, batch-share, auto-share-rules
|       |       +-- files.rs    # POST /v1/files (multipart upload)
|       |       +-- imports.rs  # POST/GET /v1/imports
|       |       +-- github.rs   # GitHub connector + webhook
|       |
|       +-- domain/             # Core types (no I/O, pure data)
|       |   +-- memory.rs       # Memory struct (id, content, l0/l1/l2 abstracts, vector, category, etc.)
|       |   +-- category.rs     # Category enum: Profile, Preferences, Entities, Events, Cases, Patterns
|       |   +-- types.rs        # MemoryType (Insight, Fact, Decision, ...), MemoryState, Tier
|       |   +-- relation.rs     # MemoryRelation (supports, contradicts, supersedes, etc.)
|       |   +-- space.rs        # Space, SpaceType (Personal/Team/Org), Member, MemberRole, Provenance
|       |   +-- tenant.rs       # Tenant struct
|       |   +-- profile.rs      # UserProfile struct
|       |   +-- error.rs        # OmemError enum (Storage, NotFound, Validation, etc.)
|       |
|       +-- store/              # Persistence layer
|       |   +-- lancedb.rs      # LanceStore: LanceDB wrapper (1029 lines), schema, CRUD, vector/FTS search
|       |   +-- manager.rs      # StoreManager: LRU cache of LanceStore instances (max 1000)
|       |   +-- tenant.rs       # TenantStore: tenant registration in _system DB
|       |   +-- spaces.rs       # SpaceStore: space + membership persistence in _system DB
|       |
|       +-- embed/              # Embedding providers
|       |   +-- service.rs      # EmbedService trait + create_embed_service() factory
|       |   +-- bedrock.rs      # AWS Bedrock Titan embedding (#[cfg(feature = "bedrock")])
|       |   +-- openai_compat.rs # OpenAI-compatible API (DashScope, OpenAI, etc.)
|       |   +-- noop.rs         # Zero-vector embedder (BM25-only mode)
|       |
|       +-- llm/                # LLM providers (for smart extraction)
|       |   +-- service.rs      # LlmService trait + create_llm_service() factory
|       |   +-- bedrock.rs      # AWS Bedrock LLM (#[cfg(feature = "bedrock")])
|       |   +-- openai_compat.rs # OpenAI-compatible API
|       |   +-- noop.rs         # No-op LLM (skip extraction)
|       |
|       +-- ingest/             # Smart ingestion pipeline
|       |   +-- pipeline.rs     # Main ingest orchestrator
|       |   +-- extractor.rs    # LLM-based fact extraction from conversations
|       |   +-- reconciler.rs   # 7-decision reconciliation: CREATE, MERGE, SUPERSEDE, SUPPORT, CONTEXTUALIZE, CONTRADICT, SKIP
|       |   +-- admission.rs    # 5-dimension admission scoring gate
|       |   +-- noise.rs        # Noise filter (regex + vector prototypes + feedback)
|       |   +-- privacy.rs      # <private> tag redaction
|       |   +-- session.rs      # Session-level ingestion
|       |   +-- preference_slots.rs # Preference slot management
|       |   +-- prompts.rs      # LLM prompt templates
|       |   +-- types.rs        # Ingest-specific types
|       |
|       +-- retrieve/           # Hybrid retrieval pipeline
|       |   +-- pipeline.rs     # 11-stage retrieval orchestrator
|       |   +-- reranker.rs     # Cross-encoder reranking
|       |   +-- trace.rs        # Per-stage explainability (input/output/score/duration)
|       |
|       +-- lifecycle/          # Memory lifecycle management
|       |   +-- decay.rs        # Weibull decay model (Core beta=0.8, Working=1.0, Peripheral=1.3)
|       |   +-- tier.rs         # Three-tier promotion: Peripheral <-> Working <-> Core
|       |   +-- forgetting.rs   # Auto-forgetting for time-sensitive info ("tomorrow", "next week")
|       |
|       +-- profile/            # User profile generation
|       |   +-- service.rs      # Static facts + dynamic context, <100ms
|       |
|       +-- multimodal/         # File processing
|       |   +-- service.rs      # Multimodal dispatch
|       |   +-- pdf.rs          # PDF text extraction (pdf-extract crate)
|       |   +-- image.rs        # Image OCR
|       |   +-- video.rs        # Video transcription
|       |   +-- code.rs         # Code AST chunking (tree-sitter: Rust, Python, JS, TS)
|       |
|       +-- connectors/         # External integrations
|           +-- github.rs       # GitHub webhook sync (code, issues, PRs)
|
+-- plugins/                    # === PLATFORM PLUGINS (PUBLIC) ===
|   +-- opencode/               # @ourmem/opencode (TypeScript)
|   |   +-- package.json
|   |   +-- src/index.ts
|   +-- openclaw/               # @ourmem/openclaw (TypeScript)
|   |   +-- package.json
|   |   +-- src/index.ts
|   +-- mcp/                    # @ourmem/mcp (TypeScript, MCP Server)
|   |   +-- package.json
|   |   +-- src/index.ts
|   +-- claude-code/            # Claude Code plugin (bash hooks + skills)
|       +-- .claude-plugin/plugin.json
|       +-- hooks/              # SessionStart, Stop hooks (bash)
|       +-- skills/             # memory-recall, memory-store skills
|       +-- README.md
|
+-- skills/                     # === SKILL FILES (PUBLIC) ===
|   +-- ourmem/
|       +-- SKILL.md            # Git version: for openclaw skills install + self-hosted
|       +-- SKILL-web.md        # Web version: self-contained, for ourmem.ai/SKILL.md + hosted only
|       +-- references/
|       |   +-- api-quick-ref.md    # API quick reference
|       |   +-- hosted-setup.md     # Hosted setup guide
|       |   +-- selfhost-setup.md   # Self-hosted setup guide
|       +-- scripts/            # Helper scripts for skill operations
|
+-- docs/                       # === DOCUMENTATION (PUBLIC) ===
|   +-- API.md                  # Complete REST API reference
|   +-- DEPLOY.md               # Docker & AWS deployment guide
|   +-- DESIGN.md               # Architecture & design decisions
|   +-- PLUGINS.md              # Plugin installation for all 4 platforms
|   +-- AGENTS.md               # This file
|
+-- eval/                       # === EVALUATION & BENCHMARKS ===
|   +-- README.md               # Eval harness docs
|   +-- run_benchmark.sh        # End-to-end benchmark runner
|   +-- datasets/
|   |   +-- sample_conversations.json  # 10 conversations, 28 search queries
|   +-- provider/
|       +-- omem_provider.py    # Python provider for MemoryBench integration
|
+-- scripts/                    # === BUILD & PUBLISH SCRIPTS ===
    +-- publish.sh              # Sync public files to ourmem/omem (GitHub)
```

---

## D. Key Design Decisions

### LanceDB instead of PostgreSQL
LanceDB is an embedded vector database. No external process, no connection pool, no schema migrations. The server binary is self-contained. Data lives on local disk or S3. This means a single `docker run` command gets you a working server with zero dependencies.

### Space ID uses `/` separator (not `:`)
Space IDs look like `personal/abc-123`, `team/def-456`. The `/` separator was chosen because it maps directly to filesystem paths and S3 key prefixes. A `:` would break S3 key naming. Each space gets its own LanceDB directory, so the space ID literally becomes the directory name.

### personal_space_id() helper
Defined in `omem-server/src/api/server.rs`. Every CRUD operation routes through `personal_space_id(tenant_id)` which returns `personal/{tenant_id}`. This ensures all tenant data lives under a consistent path and makes multi-space search straightforward: just collect all accessible space IDs and query each store.

### Vector search uses prefilter (not postfilter)
Per LanceDB documentation, vector search with postfilter can return fewer results than requested because filtering happens after the ANN search. Prefilter applies the filter first, then runs vector search on the filtered set. This guarantees the requested number of results (if enough data exists).

### FTS search uses postfilter (required)
LanceDB's full-text search (FTS) does not support prefilter. The FTS index is built on the full table, and filtering must happen after the text search. This is a LanceDB limitation, not a design choice.

### FTS index retry on failure
When creating the FTS index, if it fails (e.g., table is empty, concurrent access), the code does NOT mark `fts_indexed = true`. It retries on the next search. An earlier bug marked the flag true even on failure, which silently disabled FTS for the lifetime of that store instance.

### Bedrock as optional feature
In `omem-server/Cargo.toml`, Bedrock support is behind `#[cfg(feature = "bedrock")]`:
```toml
[features]
default = ["bedrock"]
bedrock = ["dep:aws-sdk-bedrockruntime", "dep:aws-config"]
```
This exists because `aws-lc-sys` (pulled in by the AWS SDK) crashes on musl static linking. The musl build uses `--no-default-features` to exclude it entirely.

### aws-lc-sys excluded in musl builds
`aws-lc-sys` calls `dlopen(NULL)` during initialization, which segfaults on statically linked musl binaries. There's no workaround. The fix: don't include it. Use `--no-default-features` for musl builds and configure `OMEM_EMBED_PROVIDER=openai-compatible` instead of `bedrock`. See [aws-c-cal#213](https://github.com/awslabs/aws-c-cal/issues/213).

### `-C relocation-model=static` for musl
Rust's default output for musl targets is `static-pie` (position-independent executable). This causes a segfault on some musl-gcc versions due to a bug in the startup code. Setting `-C relocation-model=static` forces a traditional static binary without PIE, which works reliably. See [rust-lang/rust#95926](https://github.com/rust-lang/rust/issues/95926).

---

## E. Build & Compile

### glibc build (full features, including Bedrock)

```bash
cargo build --release -p omem-server
# Binary: target/release/omem-server (~218MB)
# Requires: same or newer glibc on target machine
```

### musl static build (portable, zero dependencies)

```bash
rustup target add x86_64-unknown-linux-musl

RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" \
  cargo build --release --target x86_64-unknown-linux-musl \
  -p omem-server --no-default-features

# Binary: target/x86_64-unknown-linux-musl/release/omem-server (~182MB)
# Runs on ANY Linux x86_64, zero dependencies
```

**Why `--no-default-features`:** The default feature includes `bedrock`, which pulls in `aws-lc-sys`. That crate calls `dlopen(NULL)` at startup, which segfaults on statically linked musl. Excluding it means no Bedrock support, so you must use `OMEM_EMBED_PROVIDER=openai-compatible`.

**Why `-C relocation-model=static`:** Without this flag, Rust produces a `static-pie` binary for musl targets. The PIE startup code in musl-gcc has a known bug that causes segfaults. Forcing `relocation-model=static` produces a traditional static binary that works reliably.

### Compile time expectations

| Build type | Approximate time |
|------------|-----------------|
| Release (clean) | ~12 minutes |
| Dev (clean) | ~5 minutes |
| Incremental (after code change) | ~30 seconds |

### Verify no aws-lc-sys in musl build

```bash
cargo tree -i aws-lc-sys --target x86_64-unknown-linux-musl
# Should show "aws-lc-sys not found" if --no-default-features is used
# If it shows a dependency tree, the binary WILL segfault on musl
```

### Makefile targets

```bash
make build    # cargo build --release
make test     # cargo test
make clippy   # cargo clippy -- -D warnings
make fmt      # cargo fmt -- --check
make docker   # docker build -t omem-server .
make run      # cargo run --release
```

---

## F. Deployment

### Production server (ECS Singapore)

| Property | Value |
|----------|-------|
| IP | 47.84.31.231 |
| SSH key | `/home/ec2-user/ap-omem.pem` |
| systemd service | `/etc/systemd/system/omem.service` |
| Config file | `/opt/omem.env` (contains DashScope API keys) |
| Data directory | `/opt/omem-data/` |
| Binary path | `/opt/omem-server` (musl static binary) |
| Port | 8080 |

### Deploy flow

```bash
# 1. Build musl static binary
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" \
  cargo build --release --target x86_64-unknown-linux-musl \
  -p omem-server --no-default-features

# 2. Compress
gzip -c target/x86_64-unknown-linux-musl/release/omem-server > omem-server.gz

# 3. Copy to server
scp -i /home/ec2-user/ap-omem.pem omem-server.gz root@47.84.31.231:/opt/

# 4. On server: decompress and restart
ssh -i /home/ec2-user/ap-omem.pem root@47.84.31.231 \
  "gunzip -f /opt/omem-server.gz && chmod +x /opt/omem-server && systemctl restart omem"
```

### Cloudflare DNS

```
ourmem.ai       -> Cloudflare Pages (website)
api.ourmem.ai   -> 47.84.31.231:8080 (proxied through Cloudflare)
```

### Docker (local dev)

```bash
# Dev with MinIO (S3-compatible local storage)
docker-compose up -d

# Prod (reads .env, no MinIO)
docker-compose -f docker-compose.prod.yml up -d
```

The Dockerfile uses a multi-stage build:
1. **Builder stage:** `rust:1.94-slim-bookworm`, installs protoc 29.5 (LanceDB needs it), builds with `cargo build --release`
2. **Runtime stage:** `debian:bookworm-slim`, copies binary, exposes 8080, healthcheck on `/health`

---

## G. Repository Strategy

### Two repos, strict separation

| Repo | Visibility | Contents |
|------|-----------|----------|
| `yhyyz/omem` | PRIVATE | Full codebase: omem-server + plugins + docs + skills + eval |
| `ourmem/omem` | PUBLIC | Plugins + docs + skills + eval only. NO server code. |

**The golden rule: NEVER push `omem-server/` to the public repo.**

### What goes to public

The `scripts/publish.sh` script handles syncing. It copies these directories/files to the public repo:
- `plugins/`
- `skills/`
- `docs/`
- `eval/`
- `.cargo/`
- `README.md`, `README_CN.md`, `LICENSE`
- `.env.example`, `Makefile`, `Dockerfile`, `docker-compose.yml`, `docker-compose.prod.yml`

It generates a separate `.gitignore` for the public repo (without server-specific entries).

### Publish flow

```bash
# Set GH_TOKEN env var first
export GH_TOKEN=your-github-token
./scripts/publish.sh
```

The script:
1. Clones `ourmem/omem` to a temp directory
2. Removes old public files
3. Copies fresh files from the private repo
4. Commits with the same message as the latest private repo commit
5. Pushes to `ourmem/omem` main branch

---

## H. Publishing

### npm packages

| Package | Scope | Directory |
|---------|-------|-----------|
| `@ourmem/opencode` | @ourmem | `plugins/opencode/` |
| `@ourmem/openclaw` | @ourmem | `plugins/openclaw/` |
| `@ourmem/mcp` | @ourmem | `plugins/mcp/` |

npm user: `yhyyz`
npm token: configured via `npm config set //registry.npmjs.org/:_authToken=...`

```bash
# Publish each plugin
cd plugins/opencode && npm publish --access public
cd plugins/openclaw && npm publish --access public
cd plugins/mcp && npm publish --access public
```

### ClawHub (OpenClaw skill registry)

```
Skill: ourmem@0.4.0
Token: stored locally as CLAWHUB_TOKEN env var
```

```bash
clawhub publish
```

### GitHub public repo

```bash
export GH_TOKEN=your-token
./scripts/publish.sh
```

### Full publish sequence

When releasing a new version:
1. Bump version in each `plugins/*/package.json`
2. `npm publish --access public` in each plugin directory
3. Update `skills/ourmem/SKILL.md` and `SKILL-web.md` if needed
4. `clawhub publish` (for OpenClaw)
5. `./scripts/publish.sh` (sync to public GitHub)

---

## I. Known Issues & Gotchas

### LanceDB FTS index must be created AFTER first data write
You can't create an FTS index on an empty table. The code handles this by deferring FTS index creation until the first search, and only if data exists. If the index creation fails, `fts_indexed` stays `false` and retries on the next search.

### Vector search with zero vectors (noop embedder) returns 0 results
When `OMEM_EMBED_PROVIDER=noop`, all vectors are zero-filled. LanceDB's ANN search on zero vectors returns nothing useful. This is expected. In noop mode, only BM25 text search works.

### 11-stage pipeline score normalization
Each stage in the retrieval pipeline produces scores on different scales. The normalization logic needs careful tuning. If you change any stage's scoring, test the full pipeline end-to-end with the eval harness.

### DashScope base URL
Use `https://dashscope.aliyuncs.com/compatible-mode` as the base URL. Do NOT append `/v1/` because the code already adds `/v1/` when constructing the full endpoint URL. Setting the base URL to `.../compatible-mode/v1` results in `.../compatible-mode/v1/v1/...` which 404s.

### ECS glibc version mismatch
The ECS instance runs glibc 2.32. The EC2 dev machine has glibc 2.34. A binary compiled on EC2 with glibc linking won't run on ECS. Solutions:
1. Compile on the ECS instance itself (slow, limited resources)
2. Use musl static build (recommended, what we actually do)

### musl + aws-lc-sys = SEGV
`aws-lc-sys` calls `dlopen(NULL)` during static initialization. On musl static binaries, `dlopen` is a stub that returns NULL, causing a segfault in the AWS crypto library's init code. Fix: `--no-default-features` to exclude the `bedrock` feature entirely. See [aws-c-cal#213](https://github.com/awslabs/aws-c-cal/issues/213).

### musl + static-pie = SEGV
Rust's default musl output is `static-pie`. The PIE startup code has a known bug with musl-gcc that causes segfaults. Fix: `RUSTFLAGS="-C relocation-model=static"` to produce a traditional static binary. See [rust-lang/rust#95926](https://github.com/rust-lang/rust/issues/95926).

### protoc version for LanceDB
LanceDB requires protoc >= 25. Debian bookworm ships an older version. The Dockerfile installs protoc 29.5 manually. If you get protobuf compilation errors, check your protoc version.

---

## J. API Surface

**Total endpoints: 38** (36 authenticated + 2 public)

Auth: `X-API-Key` header. The key value equals the `tenant_id`. Every authenticated request extracts this key in `auth_middleware` and looks up the tenant.

CORS: Enabled for all origins, all methods, all headers.

### Public endpoints (no auth)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/health` | `health()` | Health check, returns `{"status": "ok"}` |
| POST | `/v1/tenants` | `create_tenant` | Create workspace, returns API key |
| POST | `/v1/connectors/github/webhook` | `github_webhook` | GitHub webhook receiver |

### Memory CRUD

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/memories` | `create_memory` | Store memory or smart-ingest conversation |
| GET | `/v1/memories` | `list_memories` | List with filters & pagination |
| GET | `/v1/memories/search` | `search_memories` | 11-stage hybrid search |
| GET | `/v1/memories/{id}` | `get_memory` | Get single memory by ID |
| PUT | `/v1/memories/{id}` | `update_memory` | Update memory content/metadata |
| DELETE | `/v1/memories/{id}` | `delete_memory` | Delete a memory |

### Profile & Stats

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/profile` | `get_profile` | Auto-generated user profile |
| GET | `/v1/stats` | `get_stats` | Analytics & insights |
| GET | `/v1/stats/config` | `get_config` | Server configuration |
| GET | `/v1/stats/tags` | `get_tags` | Tag distribution |
| GET | `/v1/stats/decay` | `get_decay` | Decay statistics |
| GET | `/v1/stats/relations` | `get_relations` | Memory relation graph |
| GET | `/v1/stats/spaces` | `get_spaces_stats` | Space statistics |
| GET | `/v1/stats/sharing` | `get_sharing_stats` | Sharing statistics |
| GET | `/v1/stats/agents` | `get_agents_stats` | Agent activity stats |

### Spaces

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/spaces` | `create_space` | Create shared space |
| GET | `/v1/spaces` | `list_spaces` | List accessible spaces |
| GET | `/v1/spaces/{id}` | `get_space` | Get space details |
| PUT | `/v1/spaces/{id}` | `update_space` | Update space metadata |
| DELETE | `/v1/spaces/{id}` | `delete_space` | Delete a space |
| POST | `/v1/spaces/{id}/members` | `add_member` | Add member to space |
| PUT | `/v1/spaces/{id}/members/{user_id}` | `update_member_role` | Change member role |
| DELETE | `/v1/spaces/{id}/members/{user_id}` | `remove_member` | Remove member |

### Sharing

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/memories/{id}/share` | `share_memory` | Share memory to a space |
| POST | `/v1/memories/{id}/pull` | `pull_memory` | Pull shared memory to personal space |
| POST | `/v1/memories/{id}/unshare` | `unshare_memory` | Remove memory from shared space |
| POST | `/v1/memories/batch-share` | `batch_share` | Share multiple memories at once |
| POST | `/v1/spaces/{id}/auto-share-rules` | `create_auto_share_rule` | Create auto-sharing rule |
| GET | `/v1/spaces/{id}/auto-share-rules` | `list_auto_share_rules` | List auto-sharing rules |
| DELETE | `/v1/spaces/{id}/auto-share-rules/{rule_id}` | `delete_auto_share_rule` | Delete auto-sharing rule |

### Files & Imports

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/files` | `upload_file` | Upload PDF/image/video/code (multipart) |
| POST | `/v1/imports` | `create_import` | Create bulk import job |
| GET | `/v1/imports` | `list_imports` | List import jobs |
| GET | `/v1/imports/{id}` | `get_import` | Get import job status |

### Connectors

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/connectors/github/connect` | `github_connect` | Connect GitHub repo |

---

## K. Feature List

| Category | Feature | Details |
|----------|---------|---------|
| **Platforms** | 4 platforms | OpenCode, Claude Code, OpenClaw, MCP Server |
| **Sharing** | Space-based sharing | Personal / Team / Organization with provenance |
| | Provenance tracking | Every shared memory carries full lineage |
| | Quality-gated auto-sharing | Rules filter by importance, category, tags |
| | Cross-space search | Search across all accessible spaces at once |
| **Ingestion** | Smart dedup | 7 decisions: CREATE, MERGE, SKIP, SUPERSEDE, SUPPORT, CONTEXTUALIZE, CONTRADICT |
| | Noise filter | Regex + vector prototypes + feedback learning |
| | Admission control | 5-dimension scoring gate (utility, confidence, novelty, recency, type prior) |
| | Dual-stream write | Sync fast path (<50ms) + async LLM extraction |
| | Privacy protection | `<private>` tag redaction before storage |
| **Retrieval** | 11-stage pipeline | Vector + BM25 -> RRF -> reranker -> decay -> importance -> MMR diversity |
| | User Profile | Static facts + dynamic context, <100ms |
| | Retrieval trace | Per-stage explainability (input/output/score/duration) |
| **Lifecycle** | Weibull decay | Tier-specific beta (Core=0.8, Working=1.0, Peripheral=1.3) |
| | Three-tier promotion | Peripheral <-> Working <-> Core with access-based promotion |
| | Auto-forgetting | TTL detection for time-sensitive info ("tomorrow", "next week") |
| **Multi-modal** | File processing | PDF, image OCR, video transcription, code AST chunking |
| | GitHub connector | Real-time webhook sync for code, issues, PRs |
| **Deploy** | Open source | Apache-2.0 (plugins + docs) |
| | Self-hostable | Single binary, Docker one-liner, ~$5/month |
| | musl static build | Zero-dependency binary for any Linux x86_64 |
| | Hosted option | api.ourmem.ai, nothing to deploy |

---

## L. Environment Variables

All variables are read in `omem-server/src/config.rs` via `OmemConfig::from_env()`.

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_PORT` | `8080` | HTTP server port |
| `OMEM_LOG_LEVEL` | `info` | Log level (also overridden by `RUST_LOG`) |
| `OMEM_S3_BUCKET` | (empty) | S3 bucket name. Empty = local disk (`./omem-data/`) |
| `OMEM_EMBED_PROVIDER` | `noop` | Embedding provider: `noop`, `bedrock`, `openai-compatible` |
| `OMEM_EMBED_API_KEY` | (empty) | API key for OpenAI-compatible embedding |
| `OMEM_EMBED_BASE_URL` | (empty) | Base URL for OpenAI-compatible embedding |
| `OMEM_EMBED_MODEL` | (empty) | Embedding model name |
| `OMEM_LLM_PROVIDER` | (empty) | LLM provider for smart extraction: (empty), `openai-compatible`, `bedrock` |
| `OMEM_LLM_API_KEY` | (empty) | API key for LLM |
| `OMEM_LLM_BASE_URL` | `https://api.openai.com` | Base URL for LLM |
| `OMEM_LLM_MODEL` | `gpt-4o-mini` | LLM model name |

### Storage behavior

- `OMEM_S3_BUCKET` empty -> data stored at `./omem-data/` (local disk)
- `OMEM_S3_BUCKET` set -> data stored at `s3://{bucket}/omem/` (S3)
- System tables (tenants, spaces) stored at `{base_uri}/_system/`

### AWS-specific variables (not in OmemConfig, read by AWS SDK)

| Variable | Description |
|----------|-------------|
| `AWS_REGION` | AWS region for Bedrock/S3 |
| `AWS_ENDPOINT_URL` | Custom endpoint (e.g., MinIO: `http://minio:9000`) |
| `AWS_ACCESS_KEY_ID` | AWS access key |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key |

---

## M. Testing

### Test counts

- ~336 unit/integration tests total (182 `#[test]` + 154 `#[tokio::test]`)
- All tests use `tempfile::TempDir` for isolated LanceDB instances

### Running tests

```bash
# All Rust tests
cargo test

# Specific module
cargo test --lib store::manager

# With output
cargo test -- --nocapture

# TypeScript plugin tests (if any)
cd plugins/opencode && bun test
```

### End-to-end evaluation

```bash
# Start server
docker-compose up -d

# Run benchmark (10 conversations, 28 search queries)
./eval/run_benchmark.sh

# Or with custom server URL
./eval/run_benchmark.sh http://your-server:8080

# Python provider for programmatic testing
python3 eval/provider/omem_provider.py
```

### What the eval tests

| Step | Description |
|------|-------------|
| Health check | Verify server is running |
| Create tenant | Provision a test tenant |
| Ingest conversations | Load 10 sample conversations (all 6 categories) |
| Search evaluation | Run 28 search queries and verify expected results |
| Profile check | Verify user profile endpoint |
| CRUD operations | Test create, read, update, delete |

### Scoring expectations

| Embedding mode | Expected score |
|----------------|---------------|
| `noop` (BM25 only) | 50-69% (keyword matches only) |
| Real embeddings | 70-89% (semantic + keyword) |
| Real embeddings + LLM extraction | 90%+ (full pipeline) |

---

## N. SKILL Files

### Two versions, different audiences

| File | Purpose | Audience |
|------|---------|----------|
| `skills/ourmem/SKILL.md` | Git version with `references/` directory | `openclaw skills install` + self-hosted users |
| `skills/ourmem/SKILL-web.md` | Self-contained, no external refs | ourmem.ai/SKILL.md + hosted-only users |

### SKILL.md (Git version)
- References external files in `references/` directory: `api-quick-ref.md`, `hosted-setup.md`, `selfhost-setup.md`
- Used when installed via `openclaw skills install ourmem` or cloned from GitHub
- Supports both hosted and self-hosted modes

### SKILL-web.md (Web version)
- Completely self-contained, all instructions inline
- Served at `https://ourmem.ai/SKILL.md`
- Hosted mode only (api.ourmem.ai)
- Agents read this URL directly: `Read https://ourmem.ai/SKILL.md and follow the instructions`

### ClawHub publishing

```
Skill name: ourmem
Current version: 0.4.0
```

### Update flow

1. Edit `skills/ourmem/SKILL.md` and/or `SKILL-web.md`
2. `git add && git commit && git push` (to private repo)
3. `./scripts/publish.sh` (sync to public GitHub)
4. `clawhub publish` (publish to ClawHub registry)
5. Update `ourmem.ai/SKILL.md` if the web version changed

---

## O. Tokens & Credentials

**Location only. Never store actual values in code or docs.**

| Credential | Location |
|------------|----------|
| npm token | `~/.npmrc` (configured via `npm config set`) |
| ClawHub token | env var `CLAWHUB_TOKEN` |
| GitHub token | env var `GH_TOKEN` |
| DashScope API key (embed) | `/opt/omem.env` on ECS server |
| DashScope API key (LLM) | `/opt/omem.env` on ECS server |
| SSH key for ECS | `/home/ec2-user/ap-omem.pem` |
| AWS credentials (dev) | `~/.aws/credentials` or env vars |

---

## Quick Reference: Common Tasks

### "I need to add a new API endpoint"
1. Add handler in `omem-server/src/api/handlers/` (new file or existing)
2. Register in `omem-server/src/api/handlers/mod.rs`
3. Add route in `omem-server/src/api/router.rs`
4. Write tests in the handler file
5. Update `docs/API.md`

### "I need to change the LanceDB schema"
1. Modify `LanceStore::schema()` in `omem-server/src/store/lancedb.rs`
2. Update `memory_to_batch()` and `batch_to_memory()` in the same file
3. Update `Memory` struct in `omem-server/src/domain/memory.rs`
4. Existing data won't auto-migrate. Plan for backward compatibility.

### "I need to deploy a new version"
1. Build musl: `RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static" cargo build --release --target x86_64-unknown-linux-musl -p omem-server --no-default-features`
2. Compress: `gzip -c target/x86_64-unknown-linux-musl/release/omem-server > omem-server.gz`
3. Copy: `scp -i /home/ec2-user/ap-omem.pem omem-server.gz root@47.84.31.231:/opt/`
4. Restart: `ssh -i /home/ec2-user/ap-omem.pem root@47.84.31.231 "gunzip -f /opt/omem-server.gz && chmod +x /opt/omem-server && systemctl restart omem"`

### "I need to publish plugins"
1. Bump version in `plugins/*/package.json`
2. `cd plugins/opencode && npm publish --access public`
3. `cd plugins/openclaw && npm publish --access public`
4. `cd plugins/mcp && npm publish --access public`
5. `./scripts/publish.sh` (sync to public GitHub)

### "I need to run the full test suite"
```bash
cargo test                    # Rust tests (~336 tests)
cargo clippy -- -D warnings   # Lint
cargo fmt -- --check          # Format check
```

---

## LanceDB Schema Reference

The `memories` table in each LanceDB store has these columns:

| Column | Type | Nullable | Description |
|--------|------|----------|-------------|
| `id` | Utf8 | No | UUID v4 |
| `content` | Utf8 | No | Raw memory content |
| `l0_abstract` | Utf8 | No | Level-0 abstract (shortest summary) |
| `l1_overview` | Utf8 | No | Level-1 overview |
| `l2_content` | Utf8 | No | Level-2 detailed content |
| `vector` | FixedSizeList(Float32, 1024) | Yes | Embedding vector (1024 dimensions) |
| `category` | Utf8 | No | Profile, Preferences, Entities, Events, Cases, Patterns |
| `memory_type` | Utf8 | No | Insight, Fact, Decision, etc. |
| `state` | Utf8 | No | Active, Archived, etc. |
| `tier` | Utf8 | No | Core, Working, Peripheral |
| `importance` | Float32 | No | 0.0 to 1.0 |
| `confidence` | Float32 | No | 0.0 to 1.0 |
| `access_count` | Int32 | No | Number of times accessed |
| `tags` | Utf8 | No | JSON array of strings |
| `scope` | Utf8 | No | Visibility scope |
| `agent_id` | Utf8 | Yes | Which agent created this |
| `session_id` | Utf8 | Yes | Which session created this |
| `tenant_id` | Utf8 | No | Owner tenant |
| `source` | Utf8 | Yes | Source identifier |
| `relations` | Utf8 | No | JSON array of MemoryRelation |
| `superseded_by` | Utf8 | Yes | ID of superseding memory |
| `invalidated_at` | Utf8 | Yes | ISO timestamp |
| `created_at` | Utf8 | No | ISO timestamp |
| `updated_at` | Utf8 | No | ISO timestamp |
| `last_accessed_at` | Utf8 | Yes | ISO timestamp |
| `space_id` | Utf8 | No | Which space this memory belongs to |
| `visibility` | Utf8 | No | Visibility level |
| `owner_agent_id` | Utf8 | No | Original creating agent |
| `provenance` | Utf8 | Yes | JSON: sharing lineage (who, when, where from) |

Vector dimension: **1024** (matches Bedrock Titan and most OpenAI-compatible models).

---

## Domain Model Reference

### Memory Categories (6)
`Profile`, `Preferences`, `Entities`, `Events`, `Cases`, `Patterns`

### Memory Types
`Insight`, `Fact`, `Decision`, `Preference`, `Entity`, `Event`, `Case`, `Pattern`

### Memory States
`Active`, `Archived`, `Superseded`, `Invalidated`

### Tiers (3)
- **Core** (beta=0.8): Long-lived, slow decay. Fundamental facts and preferences.
- **Working** (beta=1.0): Medium-lived. Active project context.
- **Peripheral** (beta=1.3): Short-lived, fast decay. Transient observations.

### Space Types (3)
- **Personal**: One user, multiple agents. Default for every tenant.
- **Team**: Multiple users. Shared project knowledge.
- **Organization**: Company-wide. Read-only for most members.

### Member Roles (3)
`Admin` (full control), `Member` (read-write), `Reader` (read-only)

### Reconciliation Decisions (7)
`CREATE` (new memory), `MERGE` (combine with existing), `SUPERSEDE` (replace existing), `SUPPORT` (add evidence), `CONTEXTUALIZE` (add context), `CONTRADICT` (flag conflict), `SKIP` (discard)

### Space Weight for Search
Personal=1.0, Team=0.8, Organization=0.6. Used to rank results from multi-space search.

---

## Dependency Highlights

| Crate | Version | Purpose |
|-------|---------|---------|
| `axum` | 0.8 | HTTP framework |
| `tokio` | 1 | Async runtime |
| `lancedb` | 0.27 | Embedded vector database |
| `arrow` / `arrow-array` / `arrow-schema` | 57 | Arrow data format (LanceDB's storage format) |
| `reqwest` | 0.13 | HTTP client (for embedding/LLM API calls) |
| `rustls` | 0.23 | TLS (using `ring` backend, NOT `aws-lc-rs`) |
| `tree-sitter` | 0.24 | Code AST parsing |
| `pdf-extract` | 0.10 | PDF text extraction |
| `aws-sdk-bedrockruntime` | 1 | AWS Bedrock (optional, glibc only) |

**Critical note on TLS:** The project uses `rustls` with the `ring` crypto backend. NOT `aws-lc-rs`. This is deliberate. `aws-lc-sys` (the C library behind `aws-lc-rs`) segfaults on musl static builds. The workspace `Cargo.toml` has a metadata note about this. If `aws-lc-sys` appears in `Cargo.lock` for a musl build, the binary will crash.
