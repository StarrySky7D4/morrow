"""Pin external-encoder vectors; semantic acceptance is tested by both Rust codecs."""
import hashlib
import json
import re
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]

class MutationVectorTests(unittest.TestCase):
    def test_c_header_limits_match_schema(self):
        schema = (ROOT / "core/schemas/mutation.capnp").read_text()
        header = (ROOT / "sdk/c/include/morrow_plugin_mutation.h").read_text()
        for source, macro in {
            "version": "MP_MUTATION_ABI_VERSION",
            "maxFrameBytes": "MP_MAX_MUTATION_FRAME_BYTES",
            "maxChunkBytes": "MP_MAX_MUTATION_CHUNK_BYTES",
            "maxContentBytes": "MP_MAX_MUTATION_CONTENT_BYTES",
            "maxOperationBytes": "MP_MAX_MUTATION_OPERATION_BYTES",
            "maxDeadlineMs": "MP_MUTATION_MAX_DEADLINE_MS",
        }.items():
            with self.subTest(macro=macro):
                expected = re.search(r"const " + source + r" :UInt(?:16|32|64) = (\d+);", schema)
                actual = re.search(r"^#define " + macro + r" (\d+)[uUlL]*$", header, re.M)
                self.assertIsNotNone(expected)
                self.assertIsNotNone(actual)
                self.assertEqual(int(expected[1]), int(actual[1]))

    def test_c_enum_values_match_wire(self):
        schema = (ROOT / "core/schemas/mutation.capnp").read_text()
        header = (ROOT / "sdk/c/include/morrow_plugin_mutation.h").read_text()
        for enum, prefix in {
            "Kind": "MP_MUTATION_", "Status": "MP_MUTATION_STATUS_",
            "Phase": "MP_MUTATION_PHASE_", "Effect": "MP_MUTATION_EFFECT_",
        }.items():
            body = re.search(r"enum " + enum + r" \{([^}]+)\}", schema)[1]
            for name, value in re.findall(r"(\w+) @(\d+);", body):
                if enum == "Kind" and name == "invalid":
                    continue
                macro = prefix + re.sub(r"(?<!^)(?=[A-Z])", "_", name).upper()
                with self.subTest(macro=macro):
                    found = re.search(r"^#define " + macro + r" (\d+)u$", header, re.M)
                    self.assertIsNotNone(found)
                    self.assertEqual(int(value), int(found[1]))

    def test_schema_and_exact_vector_inventory(self):
        folder = ROOT / "sdk/vectors/mutation-v1"
        manifest = json.loads((folder / "manifest.json").read_text())
        schema = (ROOT / "core/schemas/mutation.capnp").read_text().encode()
        self.assertEqual(hashlib.sha256(schema).hexdigest(), manifest["schema_sha256"])
        self.assertEqual(schema, (ROOT / "sdk/rust/contracts/mutation.capnp").read_text().encode())
        vectors = manifest["vectors"]
        names = [v["name"] for v in vectors]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual({p.stem for p in folder.glob("*.bin")}, set(names))
        self.assertEqual(sum(v["valid"] and v["type"] == "Request" for v in vectors), 9)
        self.assertEqual(sum(not v["valid"] and v["type"] == "Request" for v in vectors), 7)
        self.assertEqual(sum(v["valid"] and v["type"] == "Response" for v in vectors), 3)
        self.assertEqual(sum(not v["valid"] and v["type"] == "Response" for v in vectors), 2)
        for v in vectors:
            with self.subTest(vector=v["name"]):
                data = (folder / (v["name"] + ".bin")).read_bytes()
                self.assertLessEqual(len(data), 128 * 1024)
                self.assertEqual(hashlib.sha256(data).hexdigest(), v["sha256"])

if __name__ == "__main__":
    unittest.main()
