#!/usr/bin/env python3
"""Validate deterministic Cargo advisory, source, and license policy inputs."""

from __future__ import annotations

import datetime as dt
import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DENY = ROOT / "deny.toml"
PIN = ROOT / ".cargo" / "advisory-db-revision"
EXCEPTIONS = ROOT / ".cargo" / "policy-exceptions.json"
LOCK = ROOT / "Cargo.lock"


def validate(
    policy: dict[str, object],
    exceptions: dict[str, object],
    revision: str,
    lock_text: str,
    today: dt.date,
) -> list[str]:
    errors: list[str] = []
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        errors.append("advisory database revision must be a full lowercase commit hash")

    advisories = policy.get("advisories", {})
    if advisories.get("yanked") != "deny":
        errors.append("advisories.yanked must be deny")
    if advisories.get("unused-ignored-advisory") != "deny":
        errors.append("unused advisory exceptions must fail closed")
    for ignored in advisories.get("ignore", []):
        if not isinstance(ignored, dict):
            errors.append("advisory exceptions must include reason and expiry")
            continue
        if not ignored.get("reason") or not ignored.get("expiry"):
            errors.append(f"advisory exception {ignored!r} lacks reason or expiry")
            continue
        try:
            expiry = dt.date.fromisoformat(str(ignored["expiry"]))
            if expiry < today:
                errors.append(f"advisory exception {ignored.get('id')} expired {expiry}")
        except ValueError:
            errors.append(f"advisory exception {ignored.get('id')} has invalid expiry")

    licenses = policy.get("licenses", {})
    allowed = licenses.get("allow", [])
    if not allowed or len(allowed) != len(set(allowed)):
        errors.append("license allowlist must be nonempty and duplicate-free")
    configured = {
        (item.get("crate"), tuple(item.get("allow", [])))
        for item in licenses.get("exceptions", [])
        if isinstance(item, dict)
    }
    tracked = set()
    if exceptions.get("schema_version") != 1:
        errors.append("unsupported policy exception schema version")
    for item in exceptions.get("license_exceptions", []):
        if not all(item.get(field) for field in ("crate", "allow", "reason", "expires")):
            errors.append(f"license exception {item!r} is incomplete")
            continue
        try:
            expiry = dt.date.fromisoformat(str(item["expires"]))
            if expiry < today:
                errors.append(f"license exception {item['crate']} expired {expiry}")
        except ValueError:
            errors.append(f"license exception {item['crate']} has invalid expiry")
        tracked.add((item["crate"], tuple(item["allow"])))
    if configured != tracked:
        errors.append("deny.toml license exceptions and expiry registry differ")

    sources = policy.get("sources", {})
    if sources.get("unknown-git") != "deny" or sources.get("unknown-registry") != "deny":
        errors.append("unknown dependency sources must be denied")
    if sources.get("required-git-spec") != "rev":
        errors.append("git dependencies must use immutable revisions")
    allowed_git = set(sources.get("allow-git", []))
    for source in re.findall(r'^source = "git\+([^?#]+)', lock_text, re.MULTILINE):
        normalized = source.removesuffix(".git")
        if normalized not in allowed_git:
            errors.append(f"Cargo.lock contains unapproved git source {normalized}")
    return errors


def main() -> int:
    policy = tomllib.loads(DENY.read_text(encoding="utf-8"))
    exceptions = json.loads(EXCEPTIONS.read_text(encoding="utf-8"))
    revision = PIN.read_text(encoding="utf-8").strip()
    lock_text = LOCK.read_text(encoding="utf-8")
    errors = validate(policy, exceptions, revision, lock_text, dt.date.today())

    if "--self-test" in sys.argv:
        bad = json.loads(json.dumps(policy))
        bad["sources"]["unknown-git"] = "allow"
        bad["advisories"]["ignore"] = [
            {"id": "RUSTSEC-0000-0000", "reason": "test", "expiry": "2000-01-01"}
        ]
        seeded = validate(bad, exceptions, "main", lock_text, dt.date.today())
        expected = ("unknown dependency sources", "expired", "full lowercase commit")
        if not all(any(fragment in error for error in seeded) for fragment in expected):
            print("Cargo policy self-test failed to reject seeded violations")
            return 1
        print("Cargo policy self-test: unknown source, expired exception, and moving pin rejected")
        return 0

    if errors:
        print("Cargo policy validation failed:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("Cargo policy inputs: pinned advisory DB, bounded exceptions, and denied unknown sources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
