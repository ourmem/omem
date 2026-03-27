#!/usr/bin/env bash
# =============================================================================
# omem Benchmark Runner
# =============================================================================
# Runs a basic end-to-end evaluation of omem's memory ingestion and retrieval.
#
# Usage:
#   ./eval/run_benchmark.sh                    # Use default localhost:8080
#   ./eval/run_benchmark.sh http://myhost:8080 # Custom server URL
#
# Prerequisites:
#   - omem-server running (or use docker-compose up)
#   - curl, jq installed
# =============================================================================

set -euo pipefail

OMEM_URL="${1:-http://localhost:8080}"
DATASET="$(dirname "$0")/datasets/sample_conversations.json"
PASS=0
FAIL=0
TOTAL=0
ERRORS=""

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log()  { echo -e "${CYAN}[omem-eval]${NC} $*"; }
pass() { echo -e "  ${GREEN}✓${NC} $*"; PASS=$((PASS + 1)); TOTAL=$((TOTAL + 1)); }
fail() { echo -e "  ${RED}✗${NC} $*"; FAIL=$((FAIL + 1)); TOTAL=$((TOTAL + 1)); ERRORS="${ERRORS}\n  - $*"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $*"; }

# ── Step 0: Check prerequisites ──────────────────────────────────────────────
log "Checking prerequisites..."

for cmd in curl jq; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "ERROR: '$cmd' is required but not installed."
        exit 1
    fi
done

if [ ! -f "$DATASET" ]; then
    echo "ERROR: Dataset not found at $DATASET"
    exit 1
fi

# ── Step 1: Health check ─────────────────────────────────────────────────────
log "Checking server health at ${OMEM_URL}..."

HEALTH=$(curl -sf "${OMEM_URL}/health" 2>/dev/null || echo "FAIL")
if echo "$HEALTH" | jq -e '.status == "ok"' &>/dev/null; then
    pass "Server is healthy"
else
    echo "ERROR: Server at ${OMEM_URL} is not responding. Start it first:"
    echo "  docker-compose up -d"
    exit 1
fi

# ── Step 2: Create tenant ────────────────────────────────────────────────────
log "Creating test tenant..."

TENANT_RESP=$(curl -sf -X POST "${OMEM_URL}/v1/tenants" \
    -H "Content-Type: application/json" \
    -d '{"name":"eval-benchmark"}')

API_KEY=$(echo "$TENANT_RESP" | jq -r '.api_key // empty')

if [ -z "$API_KEY" ]; then
    echo "ERROR: Failed to create tenant. Response: $TENANT_RESP"
    exit 1
fi

pass "Created tenant (api_key: ${API_KEY:0:8}...)"

# ── Step 3: Ingest all conversations ─────────────────────────────────────────
log "Ingesting 10 sample conversations..."

CONV_COUNT=$(jq 'length' "$DATASET")
INGEST_OK=0

for i in $(seq 0 $((CONV_COUNT - 1))); do
    CONV_ID=$(jq -r ".[$i].id" "$DATASET")
    MESSAGES=$(jq -c ".[$i].messages" "$DATASET")

    BODY=$(jq -n --argjson msgs "$MESSAGES" '{
        messages: $msgs,
        mode: "raw",
        session_id: "eval-session"
    }')

    RESP=$(curl -sf -X POST "${OMEM_URL}/v1/memories" \
        -H "Content-Type: application/json" \
        -H "X-API-Key: ${API_KEY}" \
        -d "$BODY" 2>/dev/null || echo "FAIL")

    if echo "$RESP" | jq -e '.stored_count' &>/dev/null; then
        STORED=$(echo "$RESP" | jq '.stored_count')
        INGEST_OK=$((INGEST_OK + 1))
    else
        warn "Failed to ingest conversation $CONV_ID: $RESP"
    fi
done

if [ "$INGEST_OK" -eq "$CONV_COUNT" ]; then
    pass "Ingested all $CONV_COUNT conversations"
else
    fail "Only ingested $INGEST_OK / $CONV_COUNT conversations"
fi

# ── Step 4: Wait for async processing ────────────────────────────────────────
log "Waiting 2s for async processing..."
sleep 2

# ── Step 5: Verify memories were stored ──────────────────────────────────────
log "Verifying stored memories..."

LIST_RESP=$(curl -sf "${OMEM_URL}/v1/memories?limit=100" \
    -H "X-API-Key: ${API_KEY}" 2>/dev/null || echo "FAIL")

MEM_COUNT=$(echo "$LIST_RESP" | jq '.memories | length' 2>/dev/null || echo "0")

if [ "$MEM_COUNT" -gt 0 ]; then
    pass "Found $MEM_COUNT memories in store"
else
    fail "No memories found after ingestion"
fi

# ── Step 6: Search evaluation ────────────────────────────────────────────────
log "Running search queries..."

# Iterate over each conversation's search queries
for i in $(seq 0 $((CONV_COUNT - 1))); do
    CONV_ID=$(jq -r ".[$i].id" "$DATASET")
    QUERY_COUNT=$(jq ".[$i].search_queries | length" "$DATASET")

    for q in $(seq 0 $((QUERY_COUNT - 1))); do
        QUERY=$(jq -r ".[$i].search_queries[$q].query" "$DATASET")
        SHOULD_CONTAIN=$(jq -r ".[$i].search_queries[$q].should_contain // empty" "$DATASET")
        SHOULD_NOT_CONTAIN=$(jq -r ".[$i].search_queries[$q].should_not_contain // empty" "$DATASET")

        ENCODED_QUERY=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$QUERY'))" 2>/dev/null || echo "$QUERY")

        SEARCH_RESP=$(curl -sf "${OMEM_URL}/v1/memories/search?q=${ENCODED_QUERY}&limit=5" \
            -H "X-API-Key: ${API_KEY}" 2>/dev/null || echo '{"results":[]}')

        RESULT_COUNT=$(echo "$SEARCH_RESP" | jq '.results | length' 2>/dev/null || echo "0")

        if [ "$RESULT_COUNT" -eq 0 ]; then
            fail "[${CONV_ID}] Query '${QUERY}' returned 0 results"
            continue
        fi

        # Check should_contain
        if [ -n "$SHOULD_CONTAIN" ]; then
            ALL_CONTENT=$(echo "$SEARCH_RESP" | jq -r '.results[].memory.content' 2>/dev/null | tr '[:upper:]' '[:lower:]')
            NEEDLE=$(echo "$SHOULD_CONTAIN" | tr '[:upper:]' '[:lower:]')

            if echo "$ALL_CONTENT" | grep -qi "$NEEDLE"; then
                pass "[${CONV_ID}] '${QUERY}' → found '${SHOULD_CONTAIN}'"
            else
                fail "[${CONV_ID}] '${QUERY}' → expected '${SHOULD_CONTAIN}' not found in results"
            fi
        fi

        # Check should_not_contain
        if [ -n "$SHOULD_NOT_CONTAIN" ]; then
            ALL_CONTENT=$(echo "$SEARCH_RESP" | jq -r '.results[].memory.content' 2>/dev/null | tr '[:upper:]' '[:lower:]')
            NEEDLE=$(echo "$SHOULD_NOT_CONTAIN" | tr '[:upper:]' '[:lower:]')

            if echo "$ALL_CONTENT" | grep -qi "$NEEDLE"; then
                fail "[${CONV_ID}] '${QUERY}' → should NOT contain '${SHOULD_NOT_CONTAIN}'"
            else
                pass "[${CONV_ID}] '${QUERY}' → correctly excludes '${SHOULD_NOT_CONTAIN}'"
            fi
        fi
    done
done

# ── Step 7: Profile check ───────────────────────────────────────────────────
log "Checking user profile endpoint..."

PROFILE_RESP=$(curl -sf "${OMEM_URL}/v1/profile" \
    -H "X-API-Key: ${API_KEY}" 2>/dev/null || echo "FAIL")

if echo "$PROFILE_RESP" | jq -e '.static_facts' &>/dev/null; then
    pass "Profile endpoint returns valid response"
else
    fail "Profile endpoint returned invalid response: $PROFILE_RESP"
fi

# ── Step 8: CRUD operations ──────────────────────────────────────────────────
log "Testing CRUD operations..."

# Create
CREATE_RESP=$(curl -sf -X POST "${OMEM_URL}/v1/memories" \
    -H "Content-Type: application/json" \
    -H "X-API-Key: ${API_KEY}" \
    -d '{"content":"benchmark test memory","tags":["eval","test"]}' 2>/dev/null || echo "FAIL")

MEM_ID=$(echo "$CREATE_RESP" | jq -r '.id // empty')

if [ -n "$MEM_ID" ]; then
    pass "Created memory (id: ${MEM_ID:0:8}...)"
else
    fail "Failed to create memory: $CREATE_RESP"
fi

# Read
if [ -n "$MEM_ID" ]; then
    GET_RESP=$(curl -sf "${OMEM_URL}/v1/memories/${MEM_ID}" \
        -H "X-API-Key: ${API_KEY}" 2>/dev/null || echo "FAIL")

    if echo "$GET_RESP" | jq -e '.id' &>/dev/null; then
        pass "Read memory by ID"
    else
        fail "Failed to read memory: $GET_RESP"
    fi
fi

# Update
if [ -n "$MEM_ID" ]; then
    UPDATE_RESP=$(curl -sf -X PUT "${OMEM_URL}/v1/memories/${MEM_ID}" \
        -H "Content-Type: application/json" \
        -H "X-API-Key: ${API_KEY}" \
        -d '{"content":"updated benchmark memory","tags":["eval","test","updated"]}' 2>/dev/null || echo "FAIL")

    UPDATED_CONTENT=$(echo "$UPDATE_RESP" | jq -r '.content // empty')
    if [ "$UPDATED_CONTENT" = "updated benchmark memory" ]; then
        pass "Updated memory content"
    else
        fail "Failed to update memory: $UPDATE_RESP"
    fi
fi

# Delete
if [ -n "$MEM_ID" ]; then
    DELETE_RESP=$(curl -sf -X DELETE "${OMEM_URL}/v1/memories/${MEM_ID}" \
        -H "X-API-Key: ${API_KEY}" 2>/dev/null || echo "FAIL")

    if echo "$DELETE_RESP" | jq -e '.status == "deleted"' &>/dev/null; then
        pass "Deleted memory"
    else
        fail "Failed to delete memory: $DELETE_RESP"
    fi
fi

# ── Report ───────────────────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e "  ${CYAN}omem Benchmark Results${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo -e "  Total:  ${TOTAL}"
echo -e "  ${GREEN}Passed: ${PASS}${NC}"
echo -e "  ${RED}Failed: ${FAIL}${NC}"
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo -e "  ${RED}Failures:${NC}${ERRORS}"
    echo ""
fi

SCORE=0
if [ "$TOTAL" -gt 0 ]; then
    SCORE=$((PASS * 100 / TOTAL))
fi

echo -e "  Score:  ${SCORE}%"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
