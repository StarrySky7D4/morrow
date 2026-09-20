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

if __name__ == "__main__": unittest.main()
