#!/usr/bin/env bash
# Serial hosted Simulator acceptance after the existing ServerBee unit scheme.
set -euo pipefail

RENEWAL_ROOT="$(git rev-parse --show-toplevel)"
RENEWAL_EVIDENCE_DIR="${SERVERBEE_RENEWAL_EVIDENCE_DIR:-${RUNNER_TEMP:-/tmp}/serverbee-renewal-ui}"
mkdir -p "$RENEWAL_EVIDENCE_DIR"
RENEWAL_FIXTURE_PID=""
cleanup() {
  if [[ -n "$RENEWAL_FIXTURE_PID" ]]; then
    kill "$RENEWAL_FIXTURE_PID" 2>/dev/null || true
    wait "$RENEWAL_FIXTURE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

git -C "$RENEWAL_ROOT" rev-parse HEAD > "$RENEWAL_EVIDENCE_DIR/source-sha.txt"
git -C "$RENEWAL_ROOT" status --short > "$RENEWAL_EVIDENCE_DIR/source-status.txt"
python3 - "$RENEWAL_EVIDENCE_DIR" <<'PY'
import json, os, pathlib, platform
directory = pathlib.Path(__import__('sys').argv[1])
keys = ['GITHUB_SHA', 'GITHUB_REF', 'GITHUB_EVENT_NAME', 'GITHUB_RUN_ID',
        'GITHUB_RUN_ATTEMPT', 'RENEWAL_PR_HEAD_SHA', 'RENEWAL_PR_MERGE_SHA']
directory.joinpath('checkout-metadata.json').write_text(json.dumps({
    'runner': platform.platform(), 'github': {key: os.environ.get(key) for key in keys},
    'checkoutNote': 'push executes candidate head; pull_request executes the recorded merge ref',
}, indent=2) + '\n')
PY

[[ "$(uname -s)" == Darwin ]] || { echo "Native Simulator acceptance requires macOS" >&2; exit 1; }
xcodebuild -version > "$RENEWAL_EVIDENCE_DIR/xcode-version.txt"
xcodegen --version > "$RENEWAL_EVIDENCE_DIR/xcodegen-version.txt"
xcodebuild -help > "$RENEWAL_EVIDENCE_DIR/xcodebuild-help.txt" 2>&1
grep -q -- '-parallel-testing-enabled' "$RENEWAL_EVIDENCE_DIR/xcodebuild-help.txt"
grep -q -- '-resultBundlePath' "$RENEWAL_EVIDENCE_DIR/xcodebuild-help.txt"
xcrun xcresulttool help get test-results > "$RENEWAL_EVIDENCE_DIR/xcresulttool-help.txt"
xcrun xcresulttool help get test-results summary > "$RENEWAL_EVIDENCE_DIR/xcresulttool-summary-help.txt"
xcrun xcresulttool help get test-results tests > "$RENEWAL_EVIDENCE_DIR/xcresulttool-tests-help.txt"
xcrun xcresulttool help export attachments > "$RENEWAL_EVIDENCE_DIR/xcresulttool-attachments-help.txt"

# Keep the original native unit suite independently nonzero, including all
# renewal model/request/view-model methods from this exact checkout.
RENEWAL_UNIT_RESULT="$RENEWAL_ROOT/apps/ios/test.xcresult"
test -d "$RENEWAL_UNIT_RESULT"
xcrun xcresulttool get test-results summary --path "$RENEWAL_UNIT_RESULT" > "$RENEWAL_EVIDENCE_DIR/unit-summary.json"
xcrun xcresulttool get test-results tests --path "$RENEWAL_UNIT_RESULT" > "$RENEWAL_EVIDENCE_DIR/unit-tests.json"
python3 "$RENEWAL_ROOT/tests/check-ios-renewal-results.py" --kind unit \
  --summary "$RENEWAL_EVIDENCE_DIR/unit-summary.json" --tests "$RENEWAL_EVIDENCE_DIR/unit-tests.json" \
  --renewal-source "$RENEWAL_ROOT/apps/ios/ServerBeeTests/RenewalDateTests.swift" \
  --output "$RENEWAL_EVIDENCE_DIR/unit-validated.json"

xcrun simctl list devices available -j > "$RENEWAL_EVIDENCE_DIR/simulators.json"
xcrun simctl list runtimes -j > "$RENEWAL_EVIDENCE_DIR/runtimes.json"
python3 - "$RENEWAL_EVIDENCE_DIR" <<'PY'
import json, pathlib, sys
directory = pathlib.Path(sys.argv[1])
devices = json.loads(directory.joinpath('simulators.json').read_text())['devices']
runtimes = json.loads(directory.joinpath('runtimes.json').read_text())['runtimes']
options = []
for runtime in runtimes:
    if not runtime.get('isAvailable') or not runtime['identifier'].startswith('com.apple.CoreSimulator.SimRuntime.iOS-'):
        continue
    version = tuple(int(part) for part in runtime['version'].split('.'))
    if version < (17,):
        continue
    for device in devices.get(runtime['identifier'], []):
        if device.get('isAvailable') and device['name'].startswith('iPhone'):
            preferred = device['name'] == 'iPhone 16' and version == (18, 5)
            options.append((preferred, version, device['name'], device, runtime))
if not options:
    raise SystemExit('No installed available iPhone Simulator with iOS >=17')
_, _, _, device, runtime = sorted(options, key=lambda entry: entry[:3], reverse=True)[0]
directory.joinpath('destination.json').write_text(json.dumps({'device': device, 'runtime': runtime}, indent=2) + '\n')
directory.joinpath('simulator-udid.txt').write_text(device['udid'] + '\n')
directory.joinpath('simulator-state.txt').write_text(device['state'] + '\n')
PY
RENEWAL_SIM_UDID="$(cat "$RENEWAL_EVIDENCE_DIR/simulator-udid.txt")"
if [[ "$(cat "$RENEWAL_EVIDENCE_DIR/simulator-state.txt")" != Booted ]]; then
  xcrun simctl boot "$RENEWAL_SIM_UDID"
fi
python3 - "$RENEWAL_SIM_UDID" "$RENEWAL_EVIDENCE_DIR/simulator-boot.log" <<'PY'
import subprocess, sys
with open(sys.argv[2], 'w') as log:
    subprocess.run(['xcrun', 'simctl', 'bootstatus', sys.argv[1], '-b'], stdout=log,
                   stderr=subprocess.STDOUT, timeout=180, check=True)
PY

python3 "$RENEWAL_ROOT/tests/fixtures/ios-renewal-http.py" --evidence-dir "$RENEWAL_EVIDENCE_DIR" \
  > "$RENEWAL_EVIDENCE_DIR/fixture.log" 2>&1 &
RENEWAL_FIXTURE_PID=$!
python3 - "$RENEWAL_EVIDENCE_DIR" <<'PY'
import pathlib, sys, time, urllib.request
directory = pathlib.Path(sys.argv[1])
for attempt in range(100):
    try:
        url = directory.joinpath('fixture-url.txt').read_text().strip()
        with urllib.request.urlopen(url + '/__test/state', timeout=1) as response:
            if response.status == 200:
                break
    except (OSError, ValueError):
        time.sleep(0.1)
else:
    raise SystemExit('Loopback renewal fixture did not become ready within 10 seconds')
PY
export TEST_RUNNER_SERVERBEE_RENEWAL_FIXTURE_URL="$(cat "$RENEWAL_EVIDENCE_DIR/fixture-url.txt")"
cd "$RENEWAL_ROOT/apps/ios"
# Regenerate with the installed XcodeGen after adding the new source bundle.
xcodegen generate > "$RENEWAL_EVIDENCE_DIR/xcodegen.log" 2>&1
set +e
xcodebuild -project ServerBee.xcodeproj -scheme ServerBeeUI -configuration Debug \
  -destination "platform=iOS Simulator,id=$RENEWAL_SIM_UDID" \
  -parallel-testing-enabled NO -maximum-concurrent-test-simulator-destinations 1 \
  -resultBundlePath "$RENEWAL_EVIDENCE_DIR/RenewalUI.xcresult" \
  -skipPackagePluginValidation -only-testing:ServerBeeUITests/RenewalBillingUITests test \
  2>&1 | tee "$RENEWAL_EVIDENCE_DIR/xcodebuild-ui.log"
RENEWAL_BUILD_STATUS=${PIPESTATUS[0]}
set -e
printf '%s\n' "$RENEWAL_BUILD_STATUS" > "$RENEWAL_EVIDENCE_DIR/xcodebuild-exit-status.txt"
if [[ -d "$RENEWAL_EVIDENCE_DIR/RenewalUI.xcresult" ]]; then
  xcrun xcresulttool get test-results summary --path "$RENEWAL_EVIDENCE_DIR/RenewalUI.xcresult" > "$RENEWAL_EVIDENCE_DIR/ui-summary.json"
  xcrun xcresulttool get test-results tests --path "$RENEWAL_EVIDENCE_DIR/RenewalUI.xcresult" > "$RENEWAL_EVIDENCE_DIR/ui-tests.json"
  xcrun xcresulttool export attachments --path "$RENEWAL_EVIDENCE_DIR/RenewalUI.xcresult" \
    --output-path "$RENEWAL_EVIDENCE_DIR/screenshots" > "$RENEWAL_EVIDENCE_DIR/attachment-export.log" 2>&1
fi
[[ "$RENEWAL_BUILD_STATUS" == 0 ]] || exit "$RENEWAL_BUILD_STATUS"
python3 "$RENEWAL_ROOT/tests/check-ios-renewal-results.py" --kind ui \
  --summary "$RENEWAL_EVIDENCE_DIR/ui-summary.json" --tests "$RENEWAL_EVIDENCE_DIR/ui-tests.json" \
  --fixture-state "$RENEWAL_EVIDENCE_DIR/fixture-state.json" \
  --output "$RENEWAL_EVIDENCE_DIR/ui-validated.json"
python3 - "$RENEWAL_EVIDENCE_DIR/screenshots" <<'PY'
import pathlib, sys
screenshots = list(pathlib.Path(sys.argv[1]).rglob('*.png'))
if len(screenshots) < 9:
    raise SystemExit('Missing successful interaction screenshots (expected at least nine PNG attachments)')
print(f'Preserved {len(screenshots)} actual screenshot attachments')
PY
