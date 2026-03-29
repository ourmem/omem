#!/usr/bin/env bash
# ourmem Claude Code plugin — shared HTTP utilities
set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────
OMEM_API_URL="${OMEM_API_URL:-https://api.ourmem.ai}"
OMEM_API_KEY="${OMEM_API_KEY:-}"

# Strip trailing slash
OMEM_API_URL="${OMEM_API_URL%/}"

# Each hook script checks OMEM_API_KEY and exits early if unset

# ─── HTTP Functions ──────────────────────────────────────────────────────────

# GET request to ourmem API
# Usage: omem_get "/v1/memories?limit=20"
omem_get() {
  local path="$1"
  curl -sf --max-time 8 \
    -H "X-API-Key: ${OMEM_API_KEY}" \
    -H "Accept: application/json" \
    "${OMEM_API_URL}${path}" 2>/dev/null || echo '{"error": "request failed"}'
}

# POST request to ourmem API
# Usage: omem_post "/v1/memories" '{"content": "..."}'
omem_post() {
  local path="$1"
  local body="$2"
  curl -sf --max-time 8 \
    -X POST \
    -H "X-API-Key: ${OMEM_API_KEY}" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d "${body}" \
    "${OMEM_API_URL}${path}" 2>/dev/null || echo '{"error": "request failed"}'
}

# ─── Input Functions ─────────────────────────────────────────────────────────

# Read hook input JSON from stdin
# Claude Code pipes hook context as JSON to stdin
read_stdin() {
  local input=""
  if [[ ! -t 0 ]]; then
    input=$(cat)
  fi
  echo "${input:-"{}"}"
}
