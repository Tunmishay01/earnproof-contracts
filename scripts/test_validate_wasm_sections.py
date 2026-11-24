import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("validate-wasm-sections.py")
SPEC = importlib.util.spec_from_file_location("wasm_sections", SCRIPT)
wasm_sections = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(wasm_sections)


def leb(value):
    output = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        output.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(output)


def custom(name, payload=b""):
    encoded = name.encode()
    body = leb(len(encoded)) + encoded + payload
    return b"\x00" + leb(len(body)) + body


class WasmSectionValidationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "contract.wasm"
        self.policy = {
            "maxArtifactBytes": 256,
            "maxSections": 8,
            "maxTotalCustomSectionBytes": 128,
            "customSections": {
                "contractenvmetav0": {"required": True, "maxBytes": 64},
                "contractspecv0": {"required": True, "maxBytes": 64},
                "producers": {"required": False, "maxBytes": 32},
            },
            "forbiddenCustomSections": ["name"],
        }

    def tearDown(self):
        self.temp.cleanup()

    def write(self, suffix=b""):
        self.path.write_bytes(
            b"\x00asm\x01\x00\x00\x00"
            + custom("contractenvmetav0")
            + custom("contractspecv0")
            + suffix
        )

    def test_inventory_is_deterministic_and_artifact_is_unchanged(self):
        self.write(custom("producers", b"rustc"))
        before = self.path.read_bytes()
        first = wasm_sections.inspect_wasm(self.path, self.policy)
        second = wasm_sections.inspect_wasm(self.path, self.policy)
        self.assertEqual(first, second)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(first["sha256"], __import__("hashlib").sha256(before).hexdigest())

    def test_rejects_debug_unknown_and_oversized_custom_sections(self):
        for section in [custom("name"), custom("mystery"), custom("producers", b"x" * 40)]:
            with self.subTest(section=section[:20]):
                self.write(section)
                with self.assertRaises(wasm_sections.ValidationError):
                    wasm_sections.inspect_wasm(self.path, self.policy)

    def test_rejects_truncated_and_overflowing_boundaries(self):
        malformed = [
            b"\x00asm\x01\x00\x00\x00\x01\x7f",
            b"\x00asm\x01\x00\x00\x00\x01\x80\x80\x80\x80\x10",
        ]
        for data in malformed:
            with self.subTest(data=data):
                self.path.write_bytes(data)
                with self.assertRaises(wasm_sections.ValidationError):
                    wasm_sections.inspect_wasm(self.path, self.policy)


if __name__ == "__main__":
    unittest.main()
