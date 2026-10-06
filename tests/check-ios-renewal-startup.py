#!/usr/bin/env python3
"""Bounded real loopback readiness with preserved fixture process diagnostics."""

import argparse
import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=10)
    args = parser.parse_args()
    if args.pid <= 0 or args.timeout_seconds <= 0:
        parser.error("pid and timeout must be positive")
    started = time.monotonic()
    report = {"pid": args.pid, "timeoutSeconds": args.timeout_seconds,
              "urlPublished": False, "attempts": 0, "lastProbeError": None}
    outcome = "timeout"
    while time.monotonic() - started < args.timeout_seconds:
        # ps exposes an exited/zombie process on both hosted macOS and Linux;
        # do not record command lines, environment variables or credentials.
        process = subprocess.run(["ps", "-p", str(args.pid), "-o", "pid=",
                                  "-o", "stat=", "-o", "etime="],
                                 capture_output=True, text=True, timeout=1)
        report["processStatus"] = process.stdout.strip()
        fields = report["processStatus"].split()
        if not fields or fields[1].startswith("Z"):
            outcome = "process-exited"
            break
        report["attempts"] += 1
        try:
            url = args.evidence_dir.joinpath("fixture-url.txt").read_text().strip()
            report["urlPublished"] = True
            # Readiness uses the same literal loopback transport as the UI.
            # Never let runner proxy configuration redirect this local probe.
            remaining = args.timeout_seconds - (time.monotonic() - started)
            if remaining <= 0:
                break
            local_http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with local_http.open(url + "/__test/state", timeout=min(1, remaining)) as response:
                if response.status == 200:
                    outcome = "ready"
                    break
        except (OSError, ValueError) as error:
            report["lastProbeError"] = type(error).__name__
        time.sleep(min(0.1, max(0, args.timeout_seconds - (time.monotonic() - started))))
    report.update(outcome=outcome, elapsedSeconds=round(time.monotonic() - started, 3))
    args.evidence_dir.joinpath("fixture-startup-check.json").write_text(
        json.dumps(report, indent=2) + "\n")
    if outcome != "ready":
        message = ("Loopback renewal fixture process exited before readiness" if outcome == "process-exited"
                   else f"Loopback renewal fixture did not become ready within {args.timeout_seconds:g} seconds")
        print(message + "; see fixture-startup-check.json and fixture.log", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
