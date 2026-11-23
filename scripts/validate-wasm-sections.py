#!/usr/bin/env python3
"""Validate release WASM sections without modifying the artifact bytes."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


class ValidationError(ValueError):
    pass


def read_u32(data: bytes, offset: int) -> tuple[int, int]:
    value = 0
    for index in range(5):
        if offset >= len(data):
            raise ValidationError("truncated unsigned LEB128")
        byte = data[offset]
        offset += 1
        if index == 4 and byte > 0x0F:
            raise ValidationError("u32 LEB128 overflow")
        value |= (byte & 0x7F) << (index * 7)
        if byte & 0x80 == 0:
            return value, offset
    raise ValidationError("u32 LEB128 exceeds five bytes")


def inspect_wasm(path: Path, policy: dict) -> dict:
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if len(data) > policy["maxArtifactBytes"]:
        raise ValidationError(f"artifact is {len(data)} bytes; limit is {policy['maxArtifactBytes']}")
    if data[:8] != b"\x00asm\x01\x00\x00\x00":
        raise ValidationError("invalid WASM magic or version")

    allowed = policy["customSections"]
    forbidden = set(policy.get("forbiddenCustomSections", []))
    seen = set()
    sections = []
    custom_total = 0
    offset = 8
    while offset < len(data):
        section_offset = offset
        section_id = data[offset]
        offset += 1
        if section_id > 12:
            raise ValidationError(f"unknown section id {section_id} at byte {section_offset}")
        size, payload_offset = read_u32(data, offset)
        end = payload_offset + size
        if end > len(data):
            raise ValidationError(f"section at byte {section_offset} exceeds artifact bounds")
        item = {"id": section_id, "offset": section_offset, "size": size}
        if section_id == 0:
            name_size, name_offset = read_u32(data, payload_offset)
            name_end = name_offset + name_size
            if name_end > end:
                raise ValidationError(f"custom section name at byte {section_offset} exceeds section bounds")
            try:
                name = data[name_offset:name_end].decode("utf-8")
            except UnicodeDecodeError as exc:
                raise ValidationError(f"custom section name at byte {section_offset} is not UTF-8") from exc
            item["name"] = name
            custom_total += size
            if name in forbidden:
                raise ValidationError(f"custom section {name!r} is forbidden in release artifacts")
            if name not in allowed:
                raise ValidationError(f"custom section {name!r} is not governed by the release policy")
            if size > allowed[name]["maxBytes"]:
                raise ValidationError(
                    f"custom section {name!r} is {size} bytes; limit is {allowed[name]['maxBytes']}"
                )
            seen.add(name)
        sections.append(item)
        if len(sections) > policy["maxSections"]:
            raise ValidationError(f"section count exceeds {policy['maxSections']}")
        offset = end

    if custom_total > policy["maxTotalCustomSectionBytes"]:
        raise ValidationError(
            f"custom sections total {custom_total} bytes; limit is {policy['maxTotalCustomSectionBytes']}"
        )
    missing = sorted(name for name, rule in allowed.items() if rule.get("required") and name not in seen)
    if missing:
        raise ValidationError(f"missing required custom sections: {', '.join(missing)}")
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise ValidationError("artifact changed during validation")
    return {"artifact": path.name, "bytes": len(data), "sha256": digest, "sections": sections}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--wasm-dir", type=Path, required=True)
    parser.add_argument("--policy", type=Path, default=Path("artifacts/wasm-section-policy.json"))
    parser.add_argument("--inventory", type=Path, default=Path("artifacts/wasm-section-inventory.json"))
    args = parser.parse_args()
    policy = json.loads(args.policy.read_text(encoding="utf-8"))
    paths = sorted(args.wasm_dir.glob("*.wasm"), key=lambda path: path.name)
    if not paths:
        raise ValidationError(f"no .wasm artifacts found in {args.wasm_dir}")
    inventory = {"schema": "earnproof-wasm-section-inventory/v1", "artifacts": []}
    for path in paths:
        try:
            inventory["artifacts"].append(inspect_wasm(path, policy))
        except ValidationError as exc:
            raise ValidationError(f"{path}: {exc}") from exc
    args.inventory.parent.mkdir(parents=True, exist_ok=True)
    args.inventory.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"validated {len(paths)} artifact(s); inventory: {args.inventory}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, KeyError, json.JSONDecodeError, ValidationError) as exc:
        print(f"WASM section validation failed: {exc}", file=sys.stderr)
        sys.exit(1)
