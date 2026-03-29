---
name: memory-store
description: Store a memory in ourmem. Use when user says remember, save, store, or don't forget something.
---

# Memory Store

Save information to ourmem for persistent memory across sessions.

## How to store

```bash
curl -sf \
  -X POST \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  "${OMEM_API_URL:-https://api.ourmem.ai}/v1/memories" \
  -d '{"content": "$ARGUMENTS", "tags": ["manual"], "source": "claude-code"}'
```

Replace `$ARGUMENTS` with the content to store.

## Response Format

The API returns the created memory:
```json
{
  "id": "...",
  "content": "...",
  "tags": ["manual"],
  "source": "claude-code",
  "created_at": "..."
}
```
