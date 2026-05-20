#!/usr/bin/env bash
# apps/ios/scripts/format-check.sh
#
# Runs `swift-format lint` recursively over the iOS sources.
# Intended for CI and pre-commit-style local verification.
#
# Requirements:
#   - Xcode 16+ (ships swift-format as `xcrun swift-format`)
#
# Usage:
#   ./apps/ios/scripts/format-check.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${IOS_DIR}"

echo "Running swift-format lint on ServerBee/ and ServerBeeTests/..."

xcrun swift-format lint \
    --recursive \
    ServerBee \
    ServerBeeTests

echo "swift-format lint: OK"
