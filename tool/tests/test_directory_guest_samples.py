"""Pure-byte negative fixtures for the directory sample build's static ABI gate.

These tests invoke no compiler, network, guest execution, or existing gate suite.
"""
import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "build_directory_guest_samples.py"
SPEC = importlib.util.spec_from_file_location("directory_guest_build", SCRIPT)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def u32(value):
    data = bytearray()
    while value >= 128:
        data.append((value & 127) | 128)
        value >>= 7
    data.append(value)
    return bytes(data)


def name(value):
    encoded = value.encode("utf-8")
    return u32(len(encoded)) + encoded


def section(kind, data):
    return bytes([kind]) + u32(len(data)) + data


class BytesPath:
    """Give the inspector immutable bytes without creating a fixture file."""
    def __init__(self, data):
        self.data = data

    def read_bytes(self):
        return self.data


def module(*, signatures=None, imports=None, functions=(2,), memory=True,
           exports=None, start=False, duplicate_types=False):
    if signatures is None:
        signatures = [([0x7f, 0x7f], [0x7f]), ([0x7f] * 4, [0x7f]), ([], [0x7f])]
    if imports is None:
        imports = [("morrow_task_v1", "read_input", 0, 0),
                   ("morrow_task_v1", "complete", 0, 0),
                   ("morrow_fs_directory_v1", "call", 0, 1)]
    if exports is None:
        exports = [("memory", 2, 0), ("morrow_run", 0, len(imports))]
    types = u32(len(signatures)) + b"".join(
        b"\x60" + u32(len(params)) + bytes(params) + u32(len(results)) + bytes(results)
        for params, results in signatures)
    data = b"\0asm\x01\0\0\0" + section(1, types)
    if duplicate_types:
        data += section(1, types)
    data += section(2, u32(len(imports)) + b"".join(
        name(owner) + name(field) + bytes([category]) + u32(index)
        for owner, field, category, index in imports))
    data += section(3, u32(len(functions)) + b"".join(u32(index) for index in functions))
    if memory:
        data += section(5, b"\x01\x00\x01")  # One unshared memory, minimum one page.
    data += section(7, u32(len(exports)) + b"".join(
        name(field) + bytes([category]) + u32(index) for field, category, index in exports))
    if start:
        data += section(8, u32(len(imports)))
    body = b"\x00\x41\x00\x0b"  # No locals; i32.const 0; end.
    data += section(10, u32(len(functions)) + b"".join(u32(len(body)) + body for _ in functions))
    return data


class DirectoryGuestStaticAbiTests(unittest.TestCase):
    def inspect(self, data):
        return gate.inspect_wasm(BytesPath(data))

    def refuse(self, data):
        with self.assertRaises(ValueError):
            self.inspect(data)

    def test_fixed_three_function_imports_and_signatures_are_bound(self):
        result = self.inspect(module())
        self.assertEqual(result["imports"], [
            {"module": "morrow_task_v1", "name": "read_input", "params": [0x7f, 0x7f], "results": [0x7f]},
            {"module": "morrow_task_v1", "name": "complete", "params": [0x7f, 0x7f], "results": [0x7f]},
            {"module": "morrow_fs_directory_v1", "name": "call", "params": [0x7f] * 4, "results": [0x7f]},
        ])
        self.assertEqual(result["exports"], ["memory", "morrow_run"])
        self.assertIs(result["start_section"], False)
        for index, signatures in enumerate((
                [([0x7e, 0x7f], [0x7f]), ([0x7f] * 4, [0x7f]), ([], [0x7f])],
                [([0x7f, 0x7f], [0x7f]), ([0x7f] * 3, [0x7f]), ([], [0x7f])],
                [([0x7f, 0x7f], []), ([0x7f] * 4, [0x7f]), ([], [0x7f])])):
            with self.subTest(signature=index):
                self.refuse(module(signatures=signatures))

    def test_missing_unknown_and_duplicate_imports_are_rejected(self):
        fixed = [("morrow_task_v1", "read_input", 0, 0),
                 ("morrow_task_v1", "complete", 0, 0),
                 ("morrow_fs_directory_v1", "call", 0, 1)]
        for imports in (fixed[:-1], fixed + [("wasi_snapshot_preview1", "fd_write", 0, 0)],
                        fixed + [fixed[0]], fixed[:2] + [("morrow_fs_directory_v1", "other", 0, 1)]):
            with self.subTest(imports=imports):
                self.refuse(module(imports=imports))

    def test_nonfunction_import_is_rejected(self):
        for category in (1, 2, 3, 4):
            with self.subTest(category=category):
                self.refuse(module(imports=[("morrow_task_v1", "read_input", category, 0)]))

    def test_start_section_is_rejected(self):
        self.refuse(module(start=True))

    def test_missing_or_wrong_memory_export_is_rejected(self):
        for exports in ([("morrow_run", 0, 3)], [("memory", 0, 3), ("morrow_run", 0, 3)]):
            with self.subTest(exports=exports):
                self.refuse(module(exports=exports))

    def test_memory_export_requires_an_existing_memory(self):
        self.refuse(module(memory=False))

    def test_memory_export_index_must_exist(self):
        self.refuse(module(exports=[("memory", 2, 1), ("morrow_run", 0, 3)]))

    def test_missing_wrong_or_out_of_bounds_run_export_is_rejected(self):
        for exports in ([("memory", 2, 0)], [("memory", 2, 0), ("morrow_run", 2, 0)],
                        [("memory", 2, 0), ("morrow_run", 0, 99)]):
            with self.subTest(exports=exports):
                self.refuse(module(exports=exports))
        self.refuse(module(functions=(0,)))

    def test_duplicate_export_names_are_rejected(self):
        for extra in (("memory", 2, 0), ("morrow_run", 0, 3)):
            with self.subTest(export=extra):
                self.refuse(module(exports=[("memory", 2, 0), ("morrow_run", 0, 3), extra]))

    def test_truncated_headers_blocks_and_section_fields_are_rejected(self):
        for data in (b"", module()[:7], module()[:-1], b"\0asm\x01\0\0\0\x01\x05\x01",
                     b"\0asm\x01\0\0\0" + section(1, b"\x01\x60\x02\x7f")):
            with self.subTest(data=data):
                self.refuse(data)

    def test_invalid_import_type_index_is_rejected(self):
        self.refuse(module(imports=[("morrow_task_v1", "read_input", 0, 99)]))

    def test_invalid_exported_function_type_index_is_rejected_as_wasm(self):
        self.refuse(module(functions=(99,)))

    def test_invalid_unexported_function_type_index_is_rejected(self):
        self.refuse(module(functions=(2, 99)))

    def test_duplicate_noncustom_sections_are_rejected(self):
        self.refuse(module(duplicate_types=True))

    def test_oversized_and_unterminated_u32_are_rejected(self):
        for data in (b"\x80" * 5, b"\xff\xff\xff\xff\x10", b"\x80"):
            with self.subTest(data=data), self.assertRaises(ValueError):
                gate.Wire(data).uint()


if __name__ == "__main__":
    unittest.main()
