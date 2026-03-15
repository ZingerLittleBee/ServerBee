#!/usr/bin/env bash
set -euo pipefail

# Start server + web dev, and print agent startup command with auto-discovery key.

ADMIN_PASS="admin123"
SERVER_URL="http://127.0.0.1:9527"

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

# Login and fetch auto-discovery key
COOKIE_JAR=$(mktemp)
curl -s -c "$COOKIE_JAR" -X POST "$SERVER_URL/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$ADMIN_PASS\"}" > /dev/null

ADK=$(curl -s -b "$COOKIE_JAR" "$SERVER_URL/api/settings/auto-discovery-key" \
  | grep -o '"key":"[^"]*"' | cut -d'"' -f4)
rm -f "$COOKIE_JAR"

echo ""
echo "=========================================="
echo "  To start the agent, run in another terminal:"
echo ""
echo "  SERVERBEE_AUTO_DISCOVERY_KEY=\"$ADK\" make agent-dev"
echo ""
echo "=========================================="
echo ""

# Start web dev server in foreground
echo "Starting web dev server..."
cd apps/web && bun install --silent && bun run dev
