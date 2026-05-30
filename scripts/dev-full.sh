#!/usr/bin/env bash
set -euo pipefail

# Start server + web dev, and print agent startup command with a freshly minted one-time enrollment code.

ADMIN_PASS="admin123"
SERVER_URL="http://localhost:9527"

echo "Building web assets (required by rust-embed)..."
(cd apps/web && bun install --silent && bun run build)

echo "Starting ServerBee server (admin / $ADMIN_PASS)..."
SERVERBEE_ADMIN__PASSWORD="$ADMIN_PASS" SERVERBEE_AUTH__SECURE_COOKIE=false \
  cargo run -p serverbee-server &
SERVER_PID=$!

# Wait for server to be ready
echo "Waiting for server..."
for i in $(seq 1 30); do
  if curl -s "$SERVER_URL/healthz" > /dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -s "$SERVER_URL/healthz" > /dev/null 2>&1; then
  echo "ERROR: Server failed to start within 30s"
  kill "$SERVER_PID" 2>/dev/null
  exit 1
fi

echo "Server is ready at $SERVER_URL"

# Login and mint a one-time enrollment code
COOKIE_JAR=$(mktemp)
curl -s -c "$COOKIE_JAR" -X POST "$SERVER_URL/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASS\"}" > /dev/null

CODE=$(curl -s -b "$COOKIE_JAR" -X POST "$SERVER_URL/api/agent/enrollments" \
  -H 'Content-Type: application/json' -d '{}' \
  | grep -o '"code":"[^"]*"' | cut -d'"' -f4)
rm -f "$COOKIE_JAR"

echo ""
echo "=========================================="
echo "  To start the agent, run in another terminal"
echo "  (this enrollment code is single-use and freshly minted each run):"
echo ""
echo "  SERVERBEE_ENROLLMENT_CODE=\"$CODE\" make agent-dev"
echo ""
echo "=========================================="
echo ""

# Start web dev server in foreground
echo "Starting web dev server..."
cd apps/web && bun install --silent && bun run dev
