#!/usr/bin/env bash
set -euo pipefail

mkdir -p data
rm -f data/dev-demo.db data/dev-demo.db-shm data/dev-demo.db-wal

SERVERBEE_DATABASE__PATH=dev-demo.db \
SERVERBEE_DEV__DEMO_DATA=true \
SERVERBEE_AUTH__SECURE_COOKIE=false \
exec cargo run -p serverbee-server
