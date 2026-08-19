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
    # Health is TCP accept; auth may still apply to HTTP.
    if curl -sf -o /dev/null -w "%{http_code}" "$ARP_URL/api/work-sessions" | grep -Eq '200|401|403|503'; then
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
echo "==> Running smoke tests (WorkSession contract, no project-interop fixtures)..."
FAILURES=0

# With ARP_DISABLE_AUTH=1, list should succeed or 503 if Switchboard is absent.
echo -n "  GET /api/work-sessions ... "
CODE=$(curl -s -o /tmp/arp-ws.json -w "%{http_code}" "$ARP_URL/api/work-sessions" || true)
if [ "$CODE" = "200" ] || [ "$CODE" = "503" ]; then
    echo "OK ($CODE)"
else
    echo "FAIL (got $CODE)"
    cat /tmp/arp-ws.json 2>/dev/null || true
    FAILURES=$((FAILURES + 1))
fi

# Old episode routes must be gone.
echo -n "  GET /api/workspaces absent ... "
CODE=$(curl -s -o /dev/null -w "%{http_code}" "$ARP_URL/api/workspaces" || true)
if [ "$CODE" = "404" ] || [ "$CODE" = "405" ]; then
    echo "OK ($CODE)"
else
    echo "FAIL (got $CODE — old route still present?)"
    FAILURES=$((FAILURES + 1))
fi

echo -n "  GET /a2a/agents ... "
CODE=$(curl -s -o /dev/null -w "%{http_code}" "$ARP_URL/a2a/agents" || true)
if [ "$CODE" = "200" ] || [ "$CODE" = "401" ]; then
    echo "OK ($CODE)"
else
    echo "FAIL (got $CODE)"
    FAILURES=$((FAILURES + 1))
fi

echo -n "  OpenAPI has work-sessions not workspaces ... "
SPEC=$(curl -sf "$ARP_URL/api/openapi.json" || true)
if echo "$SPEC" | grep -q '/api/work-sessions' && ! echo "$SPEC" | grep -q '/api/workspaces'; then
    echo "OK"
else
    # openapi may be behind auth
    if [ -z "$SPEC" ]; then
        echo "SKIP (no openapi without auth)"
    else
        echo "FAIL"
        FAILURES=$((FAILURES + 1))
    fi
fi

echo ""
if [ "$FAILURES" -eq 0 ]; then
    echo "==> All smoke tests passed"
    exit 0
else
    echo "==> $FAILURES test(s) failed"
    exit 1
fi
