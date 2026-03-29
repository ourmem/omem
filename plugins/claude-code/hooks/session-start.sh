#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/..}"
source "${SCRIPT_DIR}/hooks/common.sh"

if [[ -z "$OMEM_API_KEY" ]]; then
  CONTEXT=$(python3 -c '
import json
msg = """[ourmem] OMEM_API_KEY not set — memory is disabled.

To enable persistent memory, set your API key:
  export OMEM_API_KEY="your-key"

Get a free key:
  curl -X POST https://api.ourmem.ai/v1/tenants -H "Content-Type: application/json" -d "{}"

Then restart Claude Code.
"""
print(json.dumps({"hookSpecificOutput": {"SessionStart": {"additionalContext": msg.strip()}}}))')
  echo "$CONTEXT"
  exit 0
fi

INPUT=$(read_stdin)

RESPONSE=$(omem_get "/v1/memories?limit=20&offset=0")

if echo "${RESPONSE}" | grep -q '"error"'; then
  echo '{"hookSpecificOutput": {"SessionStart": {"additionalContext": "[ourmem] Could not load memories."}}}'
  exit 0
fi

MEMORIES=$(echo "${RESPONSE}" | python3 -c '
import sys, json
from datetime import datetime, timezone

try:
    data = json.load(sys.stdin)
    memories = data.get("memories", [])
except Exception:
    print("[ourmem] No memories available.")
    sys.exit(0)

if not memories:
    print("[ourmem] No memories stored yet.")
    sys.exit(0)

now = datetime.now(timezone.utc)
lines = ["## ourmem — Your Persistent Memories", ""]

for m in memories:
    content = m.get("l0_abstract") or m.get("content", "")
    content = content[:200]
    tags = ", ".join(m.get("tags", []))
    category = m.get("category", "")
    created = m.get("created_at", "")
    age = ""
    if created:
        try:
            dt = datetime.fromisoformat(created.replace("Z", "+00:00"))
            delta = now - dt
            if delta.days > 0:
                age = f"{delta.days}d ago"
            elif delta.seconds >= 3600:
                age = f"{delta.seconds // 3600}h ago"
            else:
                age = f"{delta.seconds // 60}m ago"
        except Exception:
            age = created[:10]

    line = f"- [{age}]"
    if category:
        line += f" ({category})"
    if tags:
        line += f" [{tags}]"
    line += f" {content}"
    lines.append(line)

print("\n".join(lines))
' 2>/dev/null || echo "[ourmem] No memories available.")

CONTEXT=$(echo "${MEMORIES}" | python3 -c '
import sys, json
text = sys.stdin.read()
print(json.dumps(text))
' 2>/dev/null)

echo "{\"hookSpecificOutput\": {\"SessionStart\": {\"additionalContext\": ${CONTEXT}}}}"
