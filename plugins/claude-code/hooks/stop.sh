#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="${CLAUDE_PLUGIN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/..}"
source "${SCRIPT_DIR}/hooks/common.sh"

[[ -z "$OMEM_API_KEY" ]] && echo '{}' && exit 0

INPUT=$(read_stdin)

BODY=$(echo "${INPUT}" | python3 -c '
import sys, json

try:
    data = json.load(sys.stdin)

    transcript_path = data.get("transcript_path", "")
    messages = []

    if transcript_path:
        try:
            with open(transcript_path, "r") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        entry = json.loads(line)
                        role = entry.get("role", "")
                        content = entry.get("content", "")
                        if isinstance(content, list):
                            parts = [p.get("text", "") for p in content if isinstance(p, dict) and p.get("type") == "text"]
                            content = "\n".join(parts)
                        if content and role in ("user", "assistant"):
                            messages.append({"role": role, "content": content[:2000]})
                    except json.JSONDecodeError:
                        continue
        except (FileNotFoundError, PermissionError):
            pass

    # Fallback: read inline transcript/messages array
    if not messages:
        inline = data.get("transcript", data.get("messages", []))
        for msg in inline:
            role = msg.get("role", "")
            content = msg.get("content", "")
            if isinstance(content, list):
                parts = [p.get("text", "") for p in content if isinstance(p, dict) and p.get("type") == "text"]
                content = "\n".join(parts)
            if content and role in ("user", "assistant"):
                messages.append({"role": role, "content": content[:2000]})

    recent = messages[-10:] if messages else []

    if not recent or len(recent) < 2:
        sys.exit(0)

    print(json.dumps({
        "messages": recent,
        "mode": "smart",
        "tags": ["auto-captured", "claude-code"],
        "source": "claude-code"
    }))
except Exception:
    sys.exit(0)
' 2>/dev/null)

if [[ -n "${BODY}" ]]; then
  omem_post "/v1/memories" "${BODY}" > /dev/null 2>&1 || true
fi

echo '{}'
