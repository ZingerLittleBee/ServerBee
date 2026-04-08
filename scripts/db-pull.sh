#!/usr/bin/env bash
set -euo pipefail

# Pull the production SQLite database via the backup API.
#
# Required env vars (or pass as arguments):
#   SERVERBEE_PROD_URL     — Production server URL (e.g. https://xxx.up.railway.app)
#   SERVERBEE_PROD_API_KEY — API key with admin access
#
# Usage:
#   ./scripts/db-pull.sh
#   SERVERBEE_PROD_URL=https://xxx.up.railway.app SERVERBEE_PROD_API_KEY=serverbee_xxx ./scripts/db-pull.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

# Auto-load .env from project root (won't override existing env vars)
if [ -f "$ROOT_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.env"
  set +a
fi

PROD_URL="${SERVERBEE_PROD_URL:-}"
API_KEY="${SERVERBEE_PROD_API_KEY:-}"

if [ -z "$PROD_URL" ] || [ -z "$API_KEY" ]; then
  echo "Error: SERVERBEE_PROD_URL and SERVERBEE_PROD_API_KEY must be set."
  echo ""
  echo "Example:"
  echo "  export SERVERBEE_PROD_URL=https://your-app.up.railway.app"
  echo "  export SERVERBEE_PROD_API_KEY=serverbee_xxxxxxxx"
  echo "  make db-pull"
  exit 1
fi

# Strip trailing slash
PROD_URL="${PROD_URL%/}"

DATA_DIR="$ROOT_DIR/data"
OUTPUT="$DATA_DIR/prod.db"

mkdir -p "$DATA_DIR"

echo "Downloading production database from $PROD_URL ..."

HTTP_CODE=$(curl -s -w "%{http_code}" -o "$OUTPUT.tmp" \
  -X POST \
  -H "X-API-Key: $API_KEY" \
  "$PROD_URL/api/settings/backup")

if [ "$HTTP_CODE" != "200" ]; then
  echo "Error: Backup request failed with HTTP $HTTP_CODE"
  BODY=$(cat "$OUTPUT.tmp" 2>/dev/null || echo "(empty)")
  rm -f "$OUTPUT.tmp"
  echo "Response: $BODY"
  exit 1
fi

# Validate it's a SQLite file
HEADER=$(head -c 16 "$OUTPUT.tmp" | cat -v 2>/dev/null || echo "")
if [[ "$HEADER" != *"SQLite format 3"* ]]; then
  echo "Error: Downloaded file is not a valid SQLite database"
  rm -f "$OUTPUT.tmp"
  exit 1
fi

mv "$OUTPUT.tmp" "$OUTPUT"
SIZE=$(wc -c < "$OUTPUT" | tr -d ' ')

echo "Done! Saved to data/prod.db ($SIZE bytes)"
echo ""
echo "Start the server with production data:"
echo "  make server-dev-prod"
