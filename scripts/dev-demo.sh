#!/usr/bin/env bash
set -euo pipefail

SERVER_URL="http://127.0.0.1:9527"
ADMIN_USER="admin"
ADMIN_PASS="admin123"
SERVER_PID=""

cleanup() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM

echo "Building web assets for the embedded server fallback..."
(cd apps/web && bun install --silent && bun run build)

echo "Starting ServerBee demo server (${ADMIN_USER}/${ADMIN_PASS})..."
bash scripts/server-dev-demo.sh &
SERVER_PID=$!

echo "Waiting for server at ${SERVER_URL}..."
for _ in $(seq 1 180); do
  if curl -fsS "${SERVER_URL}/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -fsS "${SERVER_URL}/healthz" >/dev/null 2>&1; then
  echo "ERROR: Server failed to start within 180s"
  exit 1
fi

echo ""
echo "=========================================="
echo "  Demo data is ready"
echo "  Web:       http://127.0.0.1:5173"
echo "  API:       ${SERVER_URL}"
echo "  Login:     ${ADMIN_USER} / ${ADMIN_PASS}"
echo "  Database:  data/dev-demo.db"
echo "=========================================="
echo ""

echo "Starting web dev server..."
cd apps/web && bun install --silent && bun run dev
