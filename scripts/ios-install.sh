#!/usr/bin/env bash
set -euo pipefail

# Build the iOS app (Debug), install it on a connected iPhone, and launch it.
#
# Required env var (read from the project-root .env when present):
#   IOS_DEVELOPMENT_TEAM — Apple Developer Team ID used for automatic signing
#
# Optional:
#   IOS_DEVICE — device identifier; defaults to the first paired physical iPhone
#
# Usage:
#   make ios-install
#   IOS_DEVICE=<udid> ./scripts/ios-install.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
IOS_DIR="$ROOT_DIR/apps/ios"
DERIVED_DATA="$IOS_DIR/build/DerivedData-device"
BUNDLE_ID="com.serverbee.mobile"

# Auto-load .env from project root (won't override existing env vars)
if [ -f "$ROOT_DIR/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  source "$ROOT_DIR/.env"
  set +a
fi

TEAM="${IOS_DEVELOPMENT_TEAM:-}"
if [ -z "$TEAM" ]; then
  echo "Error: IOS_DEVELOPMENT_TEAM is not set (add it to .env; see .env.example)." >&2
  exit 1
fi

# First physical iPhone reported by devicectl. A JSON file also works with
# Xcode versions that cannot emit JSON to stdout.
find_iphone() {
  local json
  json="$(mktemp -t serverbee-devices)"
  xcrun devicectl list devices --quiet --json-output "$json" >/dev/null
  python3 - "$json" <<'PY'
import json
import sys

devices = json.load(open(sys.argv[1]))["result"]["devices"]
for device in devices:
    hardware = device.get("properties", {}).get("hardware") or device.get("hardwareProperties", {})
    if hardware.get("deviceType") == "iPhone" and hardware.get("reality") == "physical" and hardware.get("udid"):
        print(hardware["udid"])
        break
PY
  rm -f "$json"
}

DEVICE="${IOS_DEVICE:-$(find_iphone)}"
if [ -z "$DEVICE" ]; then
  echo "Error: no physical iPhone found. Connect and pair one, or set IOS_DEVICE=<udid>." >&2
  exit 1
fi

cd "$IOS_DIR"
xcodegen generate

# Building against the concrete device lets automatic signing register it in
# the development profile.
xcodebuild -project ServerBee.xcodeproj -scheme ServerBee -configuration Debug \
  -destination "platform=iOS,id=$DEVICE" -derivedDataPath "$DERIVED_DATA" \
  -skipPackagePluginValidation -allowProvisioningUpdates -allowProvisioningDeviceRegistration \
  DEVELOPMENT_TEAM="$TEAM" CODE_SIGN_STYLE=Automatic build

xcrun devicectl device install app --device "$DEVICE" \
  "$DERIVED_DATA/Build/Products/Debug-iphoneos/ServerBee.app"
xcrun devicectl device process launch --terminate-existing --device "$DEVICE" "$BUNDLE_ID" \
  || echo "Installed, but the launch was refused (device locked?). Unlock it and open ServerBee."
