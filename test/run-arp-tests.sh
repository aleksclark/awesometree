#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPOSE_FILE="$PROJECT_DIR/docker-compose.test.yml"
ARP_URL="http://localhost:19099"

cleanup() {
    echo "==> Stopping containers..."
    docker compose -f "$COMPOSE_FILE" down --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT

echo "==> Building and starting ARP test server..."
docker compose -f "$COMPOSE_FILE" up --build -d

echo "==> Waiting for ARP server to be healthy..."
MAX_WAIT=60
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -sf "$ARP_URL/api/workspaces" >/dev/null 2>&1; then
        echo "    Server is ready (${WAITED}s)"
        break
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    echo "ERROR: Server did not become healthy within ${MAX_WAIT}s"
    echo "==> Container logs:"
    docker compose -f "$COMPOSE_FILE" logs
    exit 1
fi

echo ""
echo "==> Running smoke tests..."
FAILURES=0

# Test 1: GET /api/workspaces
echo -n "  GET /api/workspaces ... "
RESP=$(curl -sf "$ARP_URL/api/workspaces")
if echo "$RESP" | grep -q "arp-test"; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $RESP"
    FAILURES=$((FAILURES + 1))
fi

# Test 2: GET /api/workspaces/arp-test
echo -n "  GET /api/workspaces/arp-test ... "
RESP=$(curl -sf "$ARP_URL/api/workspaces/arp-test")
if echo "$RESP" | grep -q '"active":true'; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $RESP"
    FAILURES=$((FAILURES + 1))
fi

# Test 3: GET /api/workspaces/nonexistent -> 404
echo -n "  GET /api/workspaces/nonexistent (expect 404) ... "
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "$ARP_URL/api/workspaces/nonexistent" || true)
if [ "$STATUS" = "404" ]; then
    echo "OK"
else
    echo "FAIL (got $STATUS)"
    FAILURES=$((FAILURES + 1))
fi

# Test 4: GET /api/projects
echo -n "  GET /api/projects ... "
RESP=$(curl -sf "$ARP_URL/api/projects")
if echo "$RESP" | grep -q "test-project"; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $RESP"
    FAILURES=$((FAILURES + 1))
fi

# Test 5: GET /api/projects/test-project
echo -n "  GET /api/projects/test-project ... "
RESP=$(curl -sf "$ARP_URL/api/projects/test-project")
if echo "$RESP" | grep -q '"name":"test-project"'; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $RESP"
    FAILURES=$((FAILURES + 1))
fi

# Test 6: GET /api/openapi.json
echo -n "  GET /api/openapi.json ... "
RESP=$(curl -sf "$ARP_URL/api/openapi.json")
if echo "$RESP" | grep -q '"openapi"'; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $(echo "$RESP" | head -c 200)"
    FAILURES=$((FAILURES + 1))
fi

# Test 7: GET /a2a/agents
echo -n "  GET /a2a/agents ... "
RESP=$(curl -sf "$ARP_URL/a2a/agents")
if echo "$RESP" | grep -q "echo-agent"; then
    echo "OK"
else
    echo "FAIL"
    echo "    Response: $RESP"
    FAILURES=$((FAILURES + 1))
fi

# Test 8: GET /a2a/discover
echo -n "  GET /a2a/discover ... "
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "$ARP_URL/a2a/discover")
if [ "$STATUS" = "200" ]; then
    echo "OK"
else
    echo "FAIL (got $STATUS)"
    FAILURES=$((FAILURES + 1))
fi

echo ""
if [ $FAILURES -eq 0 ]; then
    echo "All smoke tests passed."
else
    echo "FAILED: $FAILURES test(s) failed."
    echo ""
    echo "==> Container logs:"
    docker compose -f "$COMPOSE_FILE" logs --tail=30
    exit 1
fi

# If spec-torture is available, run it
if command -v spec-torture >/dev/null 2>&1; then
    echo ""
    echo "==> Running spec-torture..."
    if [ -f "$SCRIPT_DIR/spec-http.yaml" ]; then
        spec-torture run "$SCRIPT_DIR/spec-http.yaml" --url "$ARP_URL"
    else
        echo "    No spec-http.yaml found, skipping"
    fi
fi
