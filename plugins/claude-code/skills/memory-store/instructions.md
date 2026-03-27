# Memory Store — omem

When the user asks to remember, save, or store something, use the omem API.

## Usage

Run this command to store a memory:

```bash
curl -sf \
  -X POST \
  -H "X-API-Key: $OMEM_API_KEY" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json" \
  "${OMEM_API_URL:-http://localhost:8080}/v1/memories" \
  -d '{"content": "TEXT_TO_REMEMBER", "tags": ["manual"], "source": "claude-code"}'
```

Replace `TEXT_TO_REMEMBER` with the content to store.

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

## Environment Variables

- `OMEM_API_URL` — Server URL (default: `http://localhost:8080`)
- `OMEM_API_KEY` — API key (required)
