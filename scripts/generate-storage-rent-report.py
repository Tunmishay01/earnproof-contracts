#!/usr/bin/env python3
"""Generate the deterministic storage-rent projection report."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
INPUT = ROOT / "scripts" / "storage-rent-scenarios.json"
OUTPUT = ROOT / "docs" / "storage-rent-projections.md"


def render() -> str:
    raw = INPUT.read_bytes()
    model = json.loads(raw)
    ttl = int(model["ttl_ledgers"])
    entry_types = model["entry_types"]
    scenarios = model["scenarios"]

    lines = [
        "# Storage Rent Projections",
        "",
        "This report is generated from synthetic fixtures. It estimates storage exposure in",
        "encoded bytes and byte-ledgers; it does not replace transaction resource limits or",
        "claim a network fee, because ledger rent pricing is network-configurable.",
        "",
        "## Reproducibility",
        "",
        f"- Model version: `{model['model_version']}`",
        f"- Rust: `{model['toolchain']['rust']}`",
        f"- Soroban SDK: `{model['toolchain']['soroban_sdk']}`",
        f"- Renewal horizon: `{ttl:,}` ledgers",
        f"- Scenario SHA-256: `{hashlib.sha256(raw).hexdigest()}`",
        "- Generate: `python scripts/generate-storage-rent-report.py`",
        "- Verify: `python scripts/generate-storage-rent-report.py --check`",
        "",
        "Encoded sizes are deterministic version-1 synthetic fixture lengths. Update them only",
        "with a reviewed encoding measurement when a stored record shape changes.",
        "",
        "## Entry Assumptions",
        "",
        "| Entry type | Durability | Encoded bytes | TTL assumption |",
        "|---|---:|---:|---:|",
    ]
    for name, item in entry_types.items():
        lines.append(
            f"| `{name}` | {item['durability']} | {int(item['encoded_bytes']):,} | {ttl:,} ledgers |"
        )

    lines.extend(
        [
            "",
            "## Scenario Totals",
            "",
            "| Scenario | Entries | Encoded bytes | Byte-ledgers | Delta vs baseline |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    totals: list[tuple[int, int, int]] = []
    for scenario in scenarios:
        entries = sum(int(value) for value in scenario["counts"].values())
        encoded = sum(
            int(count) * int(entry_types[name]["encoded_bytes"])
            for name, count in scenario["counts"].items()
        )
        byte_ledgers = encoded * ttl
        totals.append((entries, encoded, byte_ledgers))
    baseline = totals[0][2]
    for scenario, (entries, encoded, byte_ledgers) in zip(scenarios, totals):
        delta = byte_ledgers - baseline
        lines.append(
            f"| `{scenario['name']}` | {entries:,} | {encoded:,} | {byte_ledgers:,} | {delta:+,} |"
        )

    lines.extend(["", "## Scenario Detail", ""])
    for scenario, (entries, encoded, byte_ledgers) in zip(scenarios, totals):
        lines.extend(
            [
                f"### {scenario['name']}",
                "",
                scenario["description"] + ".",
                "",
                "| Entry type | Count | Encoded bytes | Total bytes |",
                "|---|---:|---:|---:|",
            ]
        )
        for name, count in scenario["counts"].items():
            size = int(entry_types[name]["encoded_bytes"])
            lines.append(f"| `{name}` | {int(count):,} | {size:,} | {int(count) * size:,} |")
        lines.extend(
            [
                "",
                f"Total: {entries:,} entries, {encoded:,} encoded bytes, and {byte_ledgers:,} byte-ledgers.",
                "",
            ]
        )

    lines.extend(
        [
            "## Interpretation",
            "",
            "Convert byte-ledgers to a network fee only with the target network's current rent",
            "configuration. These projections intentionally exclude archived cold state, code",
            "entries, transaction envelopes, and temporary execution memory. The stress scenario",
            "is a planning comparison and does not raise or replace on-chain capacity limits.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = render()
    if args.check:
        actual = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if actual != expected:
            print(f"outdated generated report: {OUTPUT.relative_to(ROOT)}", file=sys.stderr)
            return 1
        print(f"verified {OUTPUT.relative_to(ROOT)}")
        return 0
    OUTPUT.write_text(expected, encoding="utf-8", newline="\n")
    print(f"generated {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
