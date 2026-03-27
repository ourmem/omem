# Memory Recall — omem

When the user asks to recall, search, or find memories, use the omem API to search.

## Usage

Run this command to search memories:

```bash
curl -sf \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Accept: application/json" \
  "${OMEM_API_URL:-http://localhost:8080}/v1/memories/search?q=QUERY&limit=10"
```

Replace `QUERY` with the URL-encoded search term.

## Response Format

The API returns:
```json
{
  "results": [
    {
      "memory": {
        "id": "...",
        "content": "...",
        "tags": ["..."],
        "created_at": "..."
      },
      "score": 0.95
    }
  ]
}
```

## Environment Variables

- `OMEM_API_URL` — Server URL (default: `http://localhost:8080`)
- `OMEM_API_KEY` — API key (required)
