#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

INPUT=$(read_stdin)

RESPONSE=$(omem_get "/v1/memories?limit=20&offset=0")

if echo "${RESPONSE}" | grep -q '"error"'; then
  echo '{"hookSpecificOutput": {"SessionStart": {"additionalContext": "[omem] Failed to load memories."}}}'
  exit 0
fi

MEMORIES=$(echo "${RESPONSE}" | python3 -c '
import sys, json
from datetime import datetime, timezone

try:
    data = json.load(sys.stdin)
    memories = data.get("memories", [])
except Exception:
    print("[omem] No memories available.")
    sys.exit(0)

if not memories:
    print("[omem] No memories stored yet.")
    sys.exit(0)

now = datetime.now(timezone.utc)
lines = ["## omem — Recent Memories", ""]

for m in memories:
    content = m.get("content", "")[:500]
    tags = ", ".join(m.get("tags", []))
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
    if tags:
        line += f" ({tags})"
    line += f" {content}"
    lines.append(line)

print("\n".join(lines))
' 2>/dev/null || echo "[omem] No memories available.")

CONTEXT=$(echo "${MEMORIES}" | python3 -c '
import sys, json
text = sys.stdin.read()
print(json.dumps(text))
' 2>/dev/null)

echo "{\"hookSpecificOutput\": {\"SessionStart\": {\"additionalContext\": ${CONTEXT}}}}"
