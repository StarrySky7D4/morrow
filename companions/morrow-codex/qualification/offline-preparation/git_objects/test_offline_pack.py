"""Synthetic memory-only packs. No Git, transport, filesystem input or cache."""
from dataclasses import replace
import hashlib
import struct
import unittest
import zlib
import offline_pack as subject


def size_header(kind, size):
    first = (kind << 4) | (size & 15)
    size >>= 4
    output = bytearray([first | (128 if size else 0)])
    while size:
        next_byte = size & 127
        size >>= 7
        output.append(next_byte | (128 if size else 0))
    return bytes(output)


def varint(value):
    result = bytearray()
    while True:
        byte = value & 127
        value >>= 7
        result.append(byte | (128 if value else 0))
        if not value:
            return bytes(result)


def ofs_distance(value):
    result = bytearray([value & 127])
    while value >> 7:
        value = (value >> 7) - 1
        result.insert(0, (value & 127) | 128)
    return bytes(result)


def literal_delta(base, result):
    output = bytearray(varint(len(base)) + varint(len(result)))
    for start in range(0, len(result), 127):
        chunk = result[start:start + 127]
        output.append(len(chunk))
        output.extend(chunk)
    return bytes(output)


def pack(records, version=2):
    """Records: (kind,content,None) or delta kind/program/base offsetindex-or-OID."""
    output = bytearray(b"PACK" + struct.pack(">II", version, len(records)))
    positions = []
    for kind, content, base in records:
        start = len(output)
        positions.append(start)
        output.extend(size_header(kind, len(content)))
        if kind == 6:
            output.extend(ofs_distance(start - positions[base]))
        elif kind == 7:
            output.extend(base)
        output.extend(zlib.compress(content))
    output.extend(hashlib.sha1(output).digest())
    return bytes(output)


def retag(data):
    return bytes(data[:-20]) + hashlib.sha1(data[:-20]).digest()


def tree(entries):
    return b"".join(mode + b" " + name + b"\0" + oid for mode, name, oid in entries)


def commit(tree_oid, parent=None, extra=b""):
    return (b"tree " + tree_oid.hex().encode() + b"\n" +
            (b"parent " + parent.hex().encode() + b"\n" if parent else b"") +
            b"author Synthetic <synthetic@example.invalid> 0 +0000\n"
            b"committer Synthetic <synthetic@example.invalid> 0 +0000\n" +
            extra + b"\nsynthetic data; never executed\n")


def fixture(blob=b"synthetic", mode=b"100644", name=b"file.txt", parent=None):
    blob_oid = subject.object_oid("blob", blob)
    root = tree([(mode, name, blob_oid)])
    root_oid = subject.object_oid("tree", root)
    target = commit(root_oid, parent)
    oid = subject.object_oid("commit", target).hex()
    return [(3, blob, None), (2, root, None), (1, target, None)], oid


class OfflinePackTests(unittest.TestCase):
    def reject(self, records=None, oid=None, data=None, limits=None, code=None):
        if records is None:
            records, default_oid = fixture()
            oid = oid or default_oid
        data = data if data is not None else pack(records)
        with self.assertRaises(subject.Rejected) as caught:
            subject.validate_pack(data, oid, limits or subject.DEFAULT_LIMITS)
        if code:
            self.assertEqual(str(caught.exception), code)

    def test_pack_v2_target_oid_and_tree_blob_complete(self):
        records, oid = fixture()
        result = subject.validate_pack(pack(records), oid)
        self.assertEqual((result["packed_objects"], result["tree_entries"], result["blob_visits"]), (3, 1, 1))
        self.assertEqual(result["transport"], "NOT_IMPLEMENTED")
        self.assertEqual(result["sha1_collision_detection"], "NOT_IMPLEMENTED_NOT_SHA1DC")

    def test_pack_v3(self):
        records, oid = fixture()
        self.assertEqual(subject.validate_pack(pack(records, 3), oid)["pack_version"], 3)

    def test_pack_header_and_trailer_rejected(self):
        records, oid = fixture()
        data = pack(records)
        for changed in (b"FAIL" + data[4:], data[:-1] + bytes([data[-1] ^ 1]), data[:-1]):
            self.reject(records, oid, changed)

    def test_pack_version_count_and_trailing_rejected(self):
        records, oid = fixture()
        data = pack(records)
        for changed in (pack(records, 1), retag(data[:8] + struct.pack(">I", 4) + data[12:]),
                        retag(data[:-20] + b"unexpected" + data[-20:])):
            self.reject(records, oid, changed)

    def test_wrong_target_commit_and_noncommit_oid(self):
        records, oid = fixture()
        for expected in ("0" * 40, subject.object_oid("blob", records[0][1]).hex()):
            self.reject(records, expected, code="expected-commit-absent-or-type")
        for expected in (oid.upper(), " prefix " + oid, oid + "\n", None, 17):
            self.reject(records, expected, code="expected-commit-format")

    def test_fixed_entry_never_accepts_synthetic_or_unknown_repo(self):
        records, _ = fixture()
        for package in subject.FIXED:
            with self.assertRaises(subject.Rejected):
                subject.validate_fixed_pack(package, pack(records))
        for package in ("https://example.invalid", "runfiles ", None):
            with self.assertRaises(subject.Rejected):
                subject.validate_fixed_pack(package, pack(records))

    def test_nonbytes_and_pack_bytes_limit(self):
        records, oid = fixture()
        data = pack(records)
        for changed in (bytearray(data), b"", "PACK"):
            self.reject(records, oid, changed, code="pack-input-bound")
        self.reject(records, oid, data, replace(subject.DEFAULT_LIMITS, max_pack_bytes=len(data) - 1), "pack-input-bound")

    def test_object_count_declared_and_expanded_limits(self):
        records, oid = fixture()
        self.reject(records, oid, limits=replace(subject.DEFAULT_LIMITS, max_objects=2), code="pack-object-count")
        self.reject(records, oid, limits=replace(subject.DEFAULT_LIMITS, max_object_bytes=3), code="object-declared-bound")
        self.reject(records, oid, limits=replace(subject.DEFAULT_LIMITS, max_expanded_bytes=5), code="expanded-bound")

    def test_limits_cannot_exceed_fixed_ceilings_or_use_bool(self):
        records, oid = fixture()
        for limits in (replace(subject.DEFAULT_LIMITS, max_pack_bytes=999999999),
                       replace(subject.DEFAULT_LIMITS, max_objects=True), object()):
            self.reject(records, oid, limits=limits)

    def test_zlib_invalid_truncated_and_output_overrun(self):
        records, oid = fixture()
        rest = pack(records)[12:-20]
        for first in (size_header(3, 1) + zlib.compress(b"too long"),
                      size_header(3, 9) + b"not zlib", size_header(3, 9) + zlib.compress(b"synthetic")[:-3]):
            data = b"PACK" + struct.pack(">II", 3, 3) + first + rest[len(size_header(3, 9) + zlib.compress(b"synthetic")):]
            data += hashlib.sha1(data).digest()
            self.reject(records, oid, data)

    def test_zero_length_blob_and_tree(self):
        records, oid = fixture(b"")
        subject.validate_pack(pack(records), oid)
        root = b""
        target = commit(subject.object_oid("tree", root))
        oid = subject.object_oid("commit", target).hex()
        subject.validate_pack(pack([(2, root, None), (1, target, None)]), oid)

    def test_ofs_delta_literal_resolution(self):
        records, oid = fixture(b"updated")
        modified = [(3, b"base", None), (6, literal_delta(b"base", b"updated"), 0)] + records[1:]
        result = subject.validate_pack(pack(modified), oid)
        self.assertGreater(result["delta_instructions"], 0)

    def test_ofs_multibyte_distance(self):
        base = bytes(range(256)) * 3
        records, oid = fixture(b"updated")
        modified = [(3, base, None), (6, literal_delta(base, b"updated"), 0)] + records[1:]
        subject.validate_pack(pack(modified), oid)

    def test_ref_delta_forward_base_resolution(self):
        records, oid = fixture(b"updated")
        modified = [(7, literal_delta(b"base", b"updated"), subject.object_oid("blob", b"base")),
                    (3, b"base", None)] + records[1:]
        subject.validate_pack(pack(modified), oid)

    def test_delta_copy_and_insert(self):
        records, oid = fixture(b"abcXYZ")
        program = varint(6) + varint(6) + bytes([0x90, 3, 3]) + b"XYZ"
        modified = [(3, b"abcdef", None), (7, program, subject.object_oid("blob", b"abcdef"))] + records[1:]
        subject.validate_pack(pack(modified), oid)

    def test_delta_copy_zero_size_means_65536(self):
        blob = b"a" * 65536
        records, oid = fixture(blob)
        program = varint(len(blob)) + varint(len(blob)) + bytes([0x80])
        modified = [(3, blob, None), (6, program, 0)] + records[1:]
        subject.validate_pack(pack(modified), oid)

    def test_delta_zero_opcode_wrong_base_size_and_copy_bounds(self):
        records, oid = fixture(b"updated")
        for program in (varint(4) + varint(7) + b"\0", varint(99) + varint(0),
                        varint(4) + varint(1) + bytes([0x91, 255, 1]),
                        varint(4) + varint(7) + b"\x90", varint(4) + varint(7) + b"\x03ab",
                        b"\x80" * 10, varint(4) + varint(7)):
            modified = [(3, b"base", None), (6, program, 0)] + records[1:]
            self.reject(modified, oid)

    def test_delta_result_and_instruction_bounds(self):
        records, oid = fixture(b"updated")
        modified = [(3, b"base", None), (6, literal_delta(b"base", b"updated"), 0)] + records[1:]
        # Two literal instructions, complete target tree/commit remain ordinary objects.
        program = varint(4) + varint(7) + b"\x03upd\x04ated"
        modified[1] = (6, program, 0)
        self.reject(modified, oid, limits=replace(subject.DEFAULT_LIMITS, max_delta_instructions=1), code="delta-instruction-bound")
        oversized = varint(4) + varint(33554433)
        modified[1] = (6, oversized, 0)
        self.reject(modified, oid, code="delta-result-bound")

    def test_delta_depth_bound_and_chain(self):
        records, oid = fixture(b"final")
        modified = [(3, b"base", None), (6, literal_delta(b"base", b"middle"), 0),
                    (6, literal_delta(b"middle", b"final"), 1)] + records[1:]
        subject.validate_pack(pack(modified), oid)
        self.reject(modified, oid, limits=replace(subject.DEFAULT_LIMITS, max_delta_depth=1), code="delta-depth-bound")

    def test_thin_pack_missing_base_rejected(self):
        records, oid = fixture(b"updated")
        modified = [(7, literal_delta(b"base", b"updated"), b"\xff" * 20)] + records[1:]
        self.reject(modified, oid, code="thin-missing-or-cyclic-base")

    def test_missing_and_wrong_tree_blob_types(self):
        records, oid = fixture()
        self.reject(records[1:], oid, code="blob-closure-missing-or-type")
        self.reject([records[0], records[2]], oid, code="tree-closure-missing-or-type")
        badtree = tree([(b"100644", b"a", subject.object_oid("tree", b""))])
        target = commit(subject.object_oid("tree", badtree))
        self.reject([(2, b"", None), (2, badtree, None), (1, target, None)],
                    subject.object_oid("commit", target).hex(), code="blob-closure-missing-or-type")

    def test_symlink_gitlink_and_noncanonical_modes_rejected(self):
        for mode in (b"120000", b"160000", b"040000", b"100664"):
            records, oid = fixture(mode=mode)
            self.reject(records, oid, code="tree-mode-disallowed")

    def test_tree_traversal_windows_devices_ads_reserved_names_rejected(self):
        for name in (b"", b".", b"..", b".GIT", b"../x", b"x\\y", b"C:x", b"x:y", b"x.", b"x ",
                     b"CON.txt", b"LPT9", b"COM1", b"A?", b"a\n", b"\xff", "COM¹".encode()):
            records, oid = fixture(name=name)
            self.reject(records, oid)

    def test_casefold_collisions_and_tree_order_rejected(self):
        blob = b"data"
        child = subject.object_oid("blob", blob)
        for names in ((b"A", b"a"), (b"a", b"a"), (b"z", b"b")):
            root = tree([(b"100644", name, child) for name in names])
            target = commit(subject.object_oid("tree", root))
            self.reject([(3, blob, None), (2, root, None), (1, target, None)], subject.object_oid("commit", target).hex())

    def test_tree_git_order_directory_terminator(self):
        blob = b"data"
        boid = subject.object_oid("blob", blob)
        empty = b""
        root = tree([(b"100644", b"a.c", boid), (b"40000", b"a", subject.object_oid("tree", empty))])
        target = commit(subject.object_oid("tree", root))
        subject.validate_pack(pack([(3, blob, None), (2, empty, None), (2, root, None), (1, target, None)]),
                              subject.object_oid("commit", target).hex())

    def test_tree_depth_entry_path_bounds(self):
        records, _ = fixture(name=b"long-name")
        inner = records[1][1]
        outer = tree([(b"40000", b"dir", subject.object_oid("tree", inner))])
        top = tree([(b"40000", b"top", subject.object_oid("tree", outer))])
        target = commit(subject.object_oid("tree", top))
        modified = records[:2] + [(2, outer, None), (2, top, None), (1, target, None)]
        oid = subject.object_oid("commit", target).hex()
        subject.validate_pack(pack(modified), oid)
        for limits, code in ((replace(subject.DEFAULT_LIMITS, max_tree_depth=1), "tree-depth-bound"),
                             (replace(subject.DEFAULT_LIMITS, max_tree_entries=1), "tree-visit-bound"),
                             (replace(subject.DEFAULT_LIMITS, max_path_bytes=5), "tree-path-bound")):
            self.reject(modified, oid, limits=limits, code=code)

    def test_shared_tree_counted_per_path_and_bounded(self):
        records, _ = fixture()
        inner_oid = subject.object_oid("tree", records[1][1])
        root = tree([(b"40000", b"a", inner_oid), (b"40000", b"b", inner_oid)])
        target = commit(subject.object_oid("tree", root))
        modified = records[:2] + [(2, root, None), (1, target, None)]
        oid = subject.object_oid("commit", target).hex()
        self.assertEqual(subject.validate_pack(pack(modified), oid)["blob_visits"], 2)
        self.reject(modified, oid, limits=replace(subject.DEFAULT_LIMITS, max_tree_entries=3))

    def test_parent_header_does_not_fabricate_full_history(self):
        records, oid = fixture(parent=b"\xff" * 20)
        result = subject.validate_pack(pack(records), oid)
        self.assertEqual(result["parent_headers_not_history_verified"], 1)

    def test_commit_malformed_required_duplicate_and_header_bounds(self):
        records, _ = fixture()
        good = records[2][1]
        variants = (good.replace(b"tree ", b"TREE ", 1), good.replace(b"author ", b"invalid ", 1),
                    good.replace(b"committer ", b"invalid ", 1), good.replace(b"\n\n", b"\n"),
                    good.replace(b"\n\n", b"\ntree " + b"a" * 40 + b"\n\n"),
                    good.replace(b"author ", b"author \0", 1))
        for bad in variants:
            self.reject(records[:2] + [(1, bad, None)], subject.object_oid("commit", bad).hex())
        self.reject(records, subject.object_oid("commit", good).hex(),
                    limits=replace(subject.DEFAULT_LIMITS, max_commit_header_bytes=10), code="commit-header-bound")

    def test_duplicate_same_objects_allowed_and_no_payload_in_report(self):
        sentinel = b"PRIVATE_SENTINEL_SYNTHETIC"
        records, oid = fixture(sentinel)
        result = subject.validate_pack(pack([records[0]] + records), oid)
        self.assertEqual(result["unique_objects"], 3)
        self.assertNotIn(sentinel.decode(), repr(result))

    def test_cli_function_is_harddisabled(self):
        from contextlib import redirect_stderr
        import io
        stream = io.StringIO()
        with redirect_stderr(stream):
            self.assertEqual(subject.main(), 78)
        self.assertIn("HARD_DISABLED", stream.getvalue())


if __name__ == "__main__":
    unittest.main(verbosity=2)
