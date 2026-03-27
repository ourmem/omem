# omem Evaluation & Benchmarks

End-to-end evaluation harness for testing omem's memory ingestion and retrieval quality.

## Quick Start

```bash
# 1. Start omem server
docker-compose up -d

# 2. Run the benchmark
./eval/run_benchmark.sh

# 3. Or with a custom server URL
./eval/run_benchmark.sh http://your-server:8080
```

## What It Tests

The benchmark runner performs a full end-to-end evaluation:

| Step | Description |
|------|-------------|
| Health check | Verify server is running |
| Create tenant | Provision a test tenant |
| Ingest conversations | Load 10 sample conversations (all 6 categories) |
| Search evaluation | Run 28 search queries and verify expected results |
| Profile check | Verify user profile endpoint |
| CRUD operations | Test create, read, update, delete |

## Dataset

`datasets/sample_conversations.json` contains 10 realistic multi-turn conversations covering all 6 memory categories:

| # | ID | Category | Description |
|---|-----|----------|-------------|
| 1 | conv-01-profile | Profile | Personal background (name, job, location) |
| 2 | conv-02-preferences | Preferences | Coding style, tools, food preferences |
| 3 | conv-03-entities | Entities | Projects, technologies, team members |
| 4 | conv-04-events | Events | Production incidents, meetings |
| 5 | conv-05-cases | Cases | Debugging sessions, solutions |
| 6 | conv-06-patterns | Patterns | Work habits, daily routines |
| 7 | conv-07-preference-update | Preferences | Updating a previously stated preference |
| 8 | conv-08-entity-relation | Entities | Relationships between entities |
| 9 | conv-09-private-data | Profile | Privacy redaction (`<private>` tags) |
| 10 | conv-10-complex-event | Events | Multi-turn technical decision |

Each conversation includes:
- `messages` — The actual conversation turns
- `expected_facts` — Facts that should be extracted
- `search_queries` — Queries with expected results (`should_contain` / `should_not_contain`)

## Python Provider

For programmatic use or integration with MemoryBench frameworks:

```python
from eval.provider.omem_provider import OmemProvider

provider = OmemProvider(base_url="http://localhost:8080")
provider.setup()

# Ingest a conversation
provider.ingest(messages=[
    {"role": "user", "content": "I prefer dark mode in all editors"},
    {"role": "assistant", "content": "Noted! Dark mode preference saved."},
])

# Search
results = provider.search("editor theme preference")
for r in results:
    print(f"  [{r.score:.2f}] {r.memory.content}")

# Run full benchmark
report = provider.run_benchmark("eval/datasets/sample_conversations.json")
print(f"Score: {report['score']}%")
```

Or run directly from CLI:

```bash
python3 eval/provider/omem_provider.py
# Or with a custom dataset:
python3 eval/provider/omem_provider.py path/to/dataset.json
```

## Interpreting Results

- **With `noop` embedding** (default): Only BM25 text search is active. Expect keyword-match queries to pass but semantic queries to fail.
- **With real embeddings** (Bedrock/OpenAI): Both vector and BM25 search are active. Expect higher scores.
- **With LLM extraction** (`mode: "smart"`): Facts are extracted and reconciled. Expect the highest scores.

### Scoring

```
Score = (passed queries / total queries) × 100%
```

| Score | Rating |
|-------|--------|
| 90%+ | Excellent — full semantic retrieval working |
| 70-89% | Good — most queries returning relevant results |
| 50-69% | Fair — basic retrieval working, semantic gaps |
| <50% | Needs work — check embedding/LLM configuration |

## Adding Custom Datasets

Create a JSON file following the same schema:

```json
[
  {
    "id": "my-test-01",
    "category": "preferences",
    "description": "Test description",
    "messages": [
      {"role": "user", "content": "..."},
      {"role": "assistant", "content": "..."}
    ],
    "expected_facts": ["fact 1", "fact 2"],
    "search_queries": [
      {"query": "search text", "should_contain": "expected keyword"}
    ]
  }
]
```

Then run:

```bash
python3 eval/provider/omem_provider.py path/to/your/dataset.json
```
