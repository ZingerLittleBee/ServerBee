#!/usr/bin/env bash
set -euo pipefail

ADMIN_USER="${SERVERBEE_DEV_ADMIN_USER:-admin}"
ADMIN_PASS="${SERVERBEE_DEV_ADMIN_PASSWORD:-admin123}"
SERVER_URL="${SERVERBEE_DEV_SERVER_URL:-http://localhost:9527}"

SERVER_LOG=$(mktemp)
COOKIE_JAR=$(mktemp)
SERVER_PID=""
TAIL_PID=""

cleanup() {
  if [ -n "$TAIL_PID" ]; then
    kill "$TAIL_PID" 2>/dev/null || true
  fi
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$SERVER_LOG" "$COOKIE_JAR"
}
trap cleanup EXIT INT TERM

echo "Building web assets (required by rust-embed)..."
(cd apps/web && bun install --silent && bun run build)

echo "Building ServerBee server..."
cargo build -p serverbee-server

echo "Starting ServerBee server..."
SERVERBEE_AUTH__SECURE_COOKIE=false cargo run -p serverbee-server >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
tail -n +1 -f "$SERVER_LOG" &
TAIL_PID=$!

echo "Waiting for server..."
for _ in $(seq 1 30); do
  if curl -s "$SERVER_URL/healthz" > /dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -s "$SERVER_URL/healthz" > /dev/null 2>&1; then
  echo "ERROR: Server failed to start within 30s"
  exit 1
fi

echo "Server is ready at $SERVER_URL"

GENERATED_PASS=$(sed -nE 's/.*Password:[[:space:]]+([^[:space:]]+).*/\1/p' "$SERVER_LOG" | head -n 1 | tr -d '\r' || true)
AUTHENTICATED=false

if [ -n "$GENERATED_PASS" ]; then
  echo "Completing first-run admin onboarding ($ADMIN_USER / $ADMIN_PASS)..."
  curl -fsS -c "$COOKIE_JAR" -X POST "$SERVER_URL/api/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$GENERATED_PASS\"}" > /dev/null
  curl -fsS -b "$COOKIE_JAR" -c "$COOKIE_JAR" -X POST "$SERVER_URL/api/auth/onboarding" \
    -H 'Content-Type: application/json' \
    -d "{\"new_password\":\"$ADMIN_PASS\"}" > /dev/null
  AUTHENTICATED=true
else
  if curl -fsS -c "$COOKIE_JAR" -X POST "$SERVER_URL/api/auth/login" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PASS\"}" > /dev/null 2>&1; then
    AUTHENTICATED=true
  else
    echo "WARN: Could not log in as '$ADMIN_USER' with configured dev password."
    echo "      Set SERVERBEE_DEV_ADMIN_PASSWORD (and SERVERBEE_DEV_ADMIN_USER if renamed) or create an enrollment code in the web UI."
  fi
fi

if [ "$AUTHENTICATED" = "true" ]; then
  ONBOARDING_REQUEST_ID=$(uuidgen)
  CODE=$(curl -fsS -b "$COOKIE_JAR" -X POST "$SERVER_URL/api/servers" \
    -H 'Content-Type: application/json' \
    -d "{\"onboarding_request_id\":\"$ONBOARDING_REQUEST_ID\",\"name\":\"Dev Agent\"}" \
    | grep -o '"code":"[^"]*"' | cut -d'"' -f4 || true)

  if [ -n "$CODE" ]; then
    echo ""
    echo "=========================================="
    echo "  To start the agent, run in another terminal"
    echo "  (this Server-bound enrollment offer is single-use and freshly created):"
    echo ""
    echo "  SERVERBEE_ENROLLMENT_CODE=\"$CODE\" make agent-dev"
    echo ""
    echo "=========================================="
    echo ""
  fi
fi

echo "Starting web dev server..."
cd apps/web && bun install --silent && bun run dev
