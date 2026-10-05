#!/usr/bin/env python3
"""Fail-closed acceptance gate for Xcode 16 test-results JSON and fixture intent.

The runner retains installed xcresulttool help and unmodified JSON. Unknown JSON
schemas fail this gate; sample JSON checks on Linux are not native test evidence.
"""

import argparse
import json
import re
from pathlib import Path

UI_METHODS = {
    "testBillingDateAndTimezoneRemainIndependentOfDeviceTimezone",
    "testAutomaticRenewalCanBeEnabledThenFrozenThroughControls",
    "testManualDateEditUsesTheSelectedBillingCalendarDate",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def test_cases(tree):
    require(isinstance(tree, dict) and isinstance(tree.get("testNodes"), list),
            "unsupported xcresult tests schema: expected testNodes list")
    cases = []

    def visit(node, ancestors):
        require(isinstance(node, dict) and isinstance(node.get("nodeType"), str)
                and isinstance(node.get("name"), str), "unsupported xcresult test node")
        path = ancestors + [node["name"]]
        if node["nodeType"] == "Test Case":
            require(isinstance(node.get("result"), str), "test case lacks result")
            identifier = node.get("nodeIdentifier")
            require(isinstance(identifier, str) and identifier, "test case lacks nodeIdentifier")
            cases.append({"identifier": identifier, "name": node["name"],
                          "path": "/".join(path), "result": node["result"]})
        children = node.get("children", [])
        require(isinstance(children, list), "unsupported xcresult children schema")
        for child in children:
            visit(child, path)

    for node in tree["testNodes"]:
        visit(node, [])
    require(cases, "zero executed test cases")
    return cases


def validate_results(summary, tree, kind, renewal_source=None):
    keys = ["totalTestCount", "passedTests", "failedTests", "skippedTests"]
    require(isinstance(summary, dict), "unsupported xcresult summary schema")
    for key in keys:
        require(type(summary.get(key)) is int, f"unsupported xcresult summary field {key}")
    cases = test_cases(tree)
    require(summary["totalTestCount"] == len(cases), "summary/tree test counts disagree")
    require(summary["passedTests"] == len(cases) and summary["failedTests"] == 0
            and summary["skippedTests"] == 0, "failed, skipped or unexecuted native tests")
    require(all(case["result"] == "Passed" for case in cases), "non-passing native test case")
    identifiers = [case["identifier"] for case in cases]
    require(len(set(identifiers)) == len(cases), "duplicate/retried test identifiers")
    if kind == "ui":
        require(len(cases) == 3, "UI bundle must execute exactly three acceptance methods")
        for name in UI_METHODS:
            matches = [case for case in cases if "RenewalBillingUITests" in case["path"]
                       and case["name"].removesuffix("()") == name]
            require(len(matches) == 1, f"missing UI acceptance method {name}")
    else:
        require(len(cases) >= 463, "native unit bundle must retain at least 463 passing tests")
        require(all("ServerBeeUITests" not in case["path"] for case in cases),
                "UI success cannot fill a unit bundle count")
        require(renewal_source is not None, "unit gate needs actual renewal test source")
        expected = set(re.findall(r"func\s+(test\w+)\s*\(", renewal_source.read_text()))
        require(len(expected) >= 15, "renewal unit source lost required methods")
        actual = {case["name"].removesuffix("()") for case in cases
                  if "RenewalDateTests" in case["path"]}
        require(expected == actual, "renewal unit methods differ from the checked-out source")
    return {"kind": kind, "counts": {key: summary[key] for key in keys}, "cases": cases}


def validate_fixture(state):
    require(isinstance(state, dict) and state.get("errors") == [], "fixture recorded protocol errors")
    completed = state.get("completed")
    require(isinstance(completed, dict) and set(completed) == {"timezone", "switch", "manual"},
            "fixture did not complete all three scenarios")
    require([len(completed[key]) for key in ["timezone", "switch", "manual"]] == [1, 2, 2],
            "fixture save counts differ")
    expected = {
        "timezone": [{"billing_timezone": "UTC"}],
        "switch": [{"enabled": True}, {"enabled": False}],
        "manual": [{"expiry_date": "2026-02-15"}, None],
    }
    for scenario, renewals in expected.items():
        for body, renewal in zip(completed[scenario], renewals):
            require(isinstance(body, dict) and "expired_at" not in body, "legacy date intent captured")
            if renewal is None:
                require("renewal" not in body and body.get("name") == "Renamed renewal fixture",
                        "name-only save introduced renewal intent")
            else:
                require(body.get("renewal") == renewal, "captured renewal intent differs")
    return {"scenarios": sorted(completed), "saveCount": 5, "errors": []}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=["unit", "ui"], required=True)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--tests", type=Path, required=True)
    parser.add_argument("--renewal-source", type=Path)
    parser.add_argument("--fixture-state", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = validate_results(json.loads(args.summary.read_text()), json.loads(args.tests.read_text()),
                              args.kind, args.renewal_source)
    if args.kind == "ui":
        require(args.fixture_state is not None, "UI gate needs fixture evidence")
        result["fixture"] = validate_fixture(json.loads(args.fixture_state.read_text()))
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"{args.kind}: {result['counts']['passedTests']} passed, zero failed/skipped")


if __name__ == "__main__":
    main()
