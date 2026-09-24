#!/usr/bin/env bash
set -euo pipefail

# Memory soak for a Linux release build of serverbee-server.
#
# Runs the binary in an alpine container with 200 TCP service monitors that
# point at a closed local port, so every scheduler tick commits ~200 write
# transactions. Samples RssAnon of the server process after a warmup and
# fails when it keeps growing. A build linked against a leaky malloc (for
# example zig 0.16's SmpAllocator) grows ~2 MB/min here; a healthy build stays
# flat within a few hundred KB.
#
# Usage:
#   scripts/memory-soak.sh <path-to-linux-serverbee-server> [duration_secs] [max_growth_kb]
#
# Requires docker and sqlite3. The binary must be a static Linux build for the
# docker host architecture (or one docker can emulate).

if [ $# -lt 1 ]; then
  echo "Usage: $0 <path-to-linux-serverbee-server> [duration_secs] [max_growth_kb]" >&2
  exit 2
fi

BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
DURATION="${2:-300}"
MAX_GROWTH_KB="${3:-4096}"
NAME="sb-memory-soak-$$"
WORK="$(mktemp -d)"

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

start_server() {
  # Docker Desktop bind mounts can fail the first open of a file the host just
  # rewrote ("unable to open database file"), so retry a few times.
  for _ in 1 2 3 4 5; do
    docker run -d --name "$NAME" \
      -v "$BIN":/serverbee-server:ro \
      -v "$WORK":/data \
      -e SERVERBEE_SERVER__DATA_DIR=/data \
      -e RUST_LOG=warn \
      alpine:3.21 /serverbee-server >/dev/null
    sleep 5
    if [ "$(docker inspect -f '{{.State.Running}}' "$NAME")" = "true" ]; then
      return 0
    fi
    docker rm -f "$NAME" >/dev/null
    sleep 3
  done
  echo "server failed to start" >&2
  exit 1
}

rss_anon_kb() {
  docker exec "$NAME" awk '/RssAnon/{print $2}' /proc/1/status
}

# First boot runs the migrations and creates the schema.
start_server
sleep 10
docker rm -f "$NAME" >/dev/null

sqlite3 "$WORK/serverbee.db" <<'SQL'
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200)
INSERT INTO service_monitor (id, name, monitor_type, target, interval, config_json,
  notification_group_id, retry_count, server_ids_json, enabled, last_status,
  consecutive_failures, last_checked_at, created_at, updated_at)
SELECT printf('soak-%04d', i), 'soak', 'tcp', '127.0.0.1:1', 1, '{}', NULL, 1, NULL, 1, NULL, 0,
  NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
FROM n;
SQL
# sqlite3 exited as the last connection, so the WAL is checkpointed and empty.
# Its leftover file is not openable from the container through the bind mount.
rm -f "$WORK/serverbee.db-wal" "$WORK/serverbee.db-shm"

start_server
echo "Warming up for 60s..."
sleep 60

FIRST="$(rss_anon_kb)"
START="$(date +%s)"
LAST="$FIRST"
echo "t=0s rss_anon=${FIRST}KB"
while [ $(( $(date +%s) - START )) -lt "$DURATION" ]; do
  sleep 30
  LAST="$(rss_anon_kb)"
  echo "t=$(( $(date +%s) - START ))s rss_anon=${LAST}KB"
done

CHECKS="$(sqlite3 "$WORK/serverbee.db" "SELECT count(*) FROM service_monitor_record" 2>/dev/null || echo '?')"
GROWTH=$(( LAST - FIRST ))
echo "growth=${GROWTH}KB over ${DURATION}s (${CHECKS} check records, limit ${MAX_GROWTH_KB}KB)"

if [ "$GROWTH" -gt "$MAX_GROWTH_KB" ]; then
  echo "FAIL: server memory keeps growing under write load" >&2
  exit 1
fi
echo "PASS"
