#!/usr/bin/env python3
"""Require a reviewed, deterministic encoded-size ceiling for every event fixture."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures" / "events"
POLICY = ROOT / "scripts" / "event-payload-budgets.json"


def padded(length: int) -> int:
    return (length + 3) & ~3


def value_bound(type_name: str) -> int:
    normalized = type_name.replace(" ", "")
    if normalized in {"bool", "u32", "i32"}:
        return 4
    if normalized in {"u64", "i64"}:
        return 8
    if normalized == "Address":
        return 44
    if normalized.startswith("BytesN<"):
        width = int(normalized[7:-1])
        return 4 + padded(width)
    if normalized.startswith("Option<"):
        return 4 + value_bound(normalized[7:-1])
    if all(token not in normalized for token in ("Bytes", "String", "Vec<", "Map<")):
        return 256
    raise ValueError(f"unbounded event field type {type_name!r}")


def payload_bound(payload: dict[str, object]) -> int:
    size = 8
    for field_name, definition in payload.items():
        if not isinstance(definition, dict) or "type" not in definition:
            raise ValueError(f"payload field {field_name!r} has no declared type")
        size += 8 + padded(len(field_name.encode("utf-8")))
        size += value_bound(str(definition["type"]))
    return size


def declared_events() -> dict[str, int]:
    observed: dict[str, int] = {}
    for path in sorted(FIXTURES.rglob("*.json")):
        if path.name == "schema.json":
            continue
        fixture = json.loads(path.read_text(encoding="utf-8"))
        contract = fixture.get("contract")
        if not contract:
            continue
        if "event" in fixture:
            observed[f"{contract}/{fixture['event']}"] = payload_bound(
                fixture.get("payload", {})
            )
        for event in fixture.get("events", []):
            observed.setdefault(f"{contract}/{event}", 8)
    return observed


def main() -> int:
    if "--self-test" in sys.argv:
        try:
            payload_bound({"payload": {"type": "Bytes"}})
        except ValueError:
            print("event payload budget self-test: oversized variable field rejected")
            return 0
        print("event payload budget self-test failed: unbounded field was accepted")
        return 1

    policy = json.loads(POLICY.read_text(encoding="utf-8"))
    budgets = policy["events"]
    observed = declared_events()
    missing = sorted(set(observed) - set(budgets))
    stale = sorted(set(budgets) - set(observed))
    over = sorted(
        (name, observed[name], budgets[name])
        for name in observed.keys() & budgets.keys()
        if observed[name] > budgets[name]
    )
    if missing or stale or over:
        if missing:
            print("events missing reviewed budgets:", *missing, sep="\n  ")
        if stale:
            print("stale event budgets:", *stale, sep="\n  ")
        for name, actual, maximum in over:
            print(f"{name}: encoded upper bound {actual} exceeds budget {maximum}")
        return 1
    print(f"event payload budgets: {len(observed)} fixtures within reviewed limits")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
