#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

INPUT=$(read_stdin)

LAST_MSG=$(echo "${INPUT}" | python3 -c '
import sys, json

try:
    data = json.load(sys.stdin)
    messages = data.get("transcript", data.get("messages", []))
    for msg in reversed(messages):
        role = msg.get("role", "")
        if role == "assistant":
            content = msg.get("content", "")
            if isinstance(content, list):
                parts = [p.get("text", "") for p in content if isinstance(p, dict) and p.get("type") == "text"]
                content = "\n".join(parts)
            print(content[:1000])
            sys.exit(0)
    print("")
except Exception:
    print("")
' 2>/dev/null)

if [[ ${#LAST_MSG} -lt 50 ]]; then
  echo '{}'
  exit 0
fi

BODY=$(python3 -c '
import sys, json
content = sys.argv[1]
print(json.dumps({
    "content": content,
    "tags": ["auto-captured"],
    "source": "claude-code"
}))
' "${LAST_MSG}" 2>/dev/null)

omem_post "/v1/memories" "${BODY}" > /dev/null 2>&1 || true

echo '{}'
