import importlib.util
import unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("adapter", ROOT / "tool/capnp_web_metadata.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class AdapterTests(unittest.TestCase):
    def source(self):
        return (ROOT / "packages/morrow_core_client/lib/src/generated/ui.capnp.native.dart").read_text(encoding="utf-8").removesuffix("\nconst schemaReflectionAvailable = true;\n")

    def test_native_source_and_exact_ids_preserved(self):
        source = self.source()
        result = module.adapt("ui.capnp.dart", source)
        self.assertEqual(result["ui.capnp.native.dart"], source + "\nconst schemaReflectionAvailable = true;\n")
        self.assertIn("'kindSchema': '9374a4065674ce61'", result["ui.capnp.ids.dart"])
        self.assertIn("int get generation => getUint64Field(8);", result["ui.capnp.web.dart"])
        self.assertIn("setUint64Field(24, v);", result["ui.capnp.web.dart"])
        self.assertNotIn("SchemaInfo", result["ui.capnp.web.dart"])

    def test_unknown_large_wire_literal_refuses_instead_of_rounding(self):
        with self.assertRaises(ValueError):
            module.adapt("ui.capnp.dart", self.source()+"\nconst unsafeWire = 0xffffffffffffffff;\n")

    def test_unknown_reflection_shape_refuses(self):
        with self.assertRaises(ValueError):
            module.adapt("ui.capnp.dart", self.source().replace("EnumSchemaInfo kindSchema", "InterfaceSchemaInfo kindSchema"))

    def test_external_reference_requires_explicit_support(self):
        with self.assertRaises(ValueError):
            module.adapt("ui.capnp.dart", self.source().replace("EnumRefTypeSchemaInfo(0x9374a4065674ce61)", "EnumRefTypeSchemaInfo(0xffffffffffffffff)"))

    def test_exact64_splits_words_and_rejects_unknown_defaults(self):
        result = module.adapt("ui.capnp.dart", self.source(), exact64=True)
        for platform in ("native", "web"):
            output = result[f"ui.capnp.{platform}.dart"]
            self.assertIn("BigInt get generationBigInt", output)
            self.assertIn("getUint32Field(12)", output)
            self.assertIn("setUint32Field(8,", output)
        self.assertIn("_checkedWireInt(generationBigInt)", result["ui.capnp.web.dart"])
        with self.assertRaisesRegex(ValueError, "64-bit field shape"):
            module.adapt("ui.capnp.dart", self.source().replace("getUint64Field(8)", "getUint64Field(8, defaultValue: 1)"), exact64=True)

    def test_unknown_float_shape_refuses(self):
        with self.assertRaisesRegex(ValueError, 'Float64 field shape'):
            module.float_fields('getFloat64Field(offset, defaultValue: mask)')

if __name__ == "__main__": unittest.main()
