"""Bounded, in-memory Git PACK integrity primitive. No transport or installation.

The CLI always refuses execution (78). Callers must supply complete bytes; this
module neither contacts a repository nor opens files, spawns Git, or extracts a
tree. SHA-1 is Git's legacy object identifier, not collision-resistant trust.
"""
from collections import defaultdict, deque
from dataclasses import dataclass
import hashlib
import re
import struct
import sys
import zlib

PLAN_SHA256 = "d77e275a91424e2671c0ca14a30ab77c5f1c129aa9dcd31c4d22cbad594adff2"
FIXED = {
    "crossterm": ("https://github.com/openai-oss-forks/crossterm",
                  "efa177859fd9623d57b9fe7ae9bf491ae1ac6ec4", 134217728),
    "runfiles": ("https://github.com/dzbarsky/rules_rust",
                 "b56cbaa8465e74127f1ea216f813cd377295ad81", 268435456),
}
AGGREGATE_GIT_BYTES = 402653184
TRANSPORT = "NOT_IMPLEMENTED"
CACHE_ADOPTION = "NOT_IMPLEMENTED"
KINDS = {1: "commit", 2: "tree", 3: "blob", 4: "tag"}


class Rejected(ValueError):
    """Fixed class of rejection; input payload is never included in errors."""


@dataclass(frozen=True)
class Limits:
    max_pack_bytes: int = 268435456
    max_objects: int = 100000
    max_object_bytes: int = 33554432
    max_expanded_bytes: int = 536870912
    max_delta_depth: int = 32
    max_delta_instructions: int = 1000000
    max_tree_depth: int = 64
    max_tree_entries: int = 100000
    max_path_bytes: int = 4096
    max_commit_header_bytes: int = 1048576


DEFAULT_LIMITS = Limits()


def _reject(code):
    raise Rejected(code)


def _limits(limits):
    if type(limits) is not Limits:
        _reject("limits-type")
    for key, value in vars(limits).items():
        ceiling = getattr(DEFAULT_LIMITS, key)
        if type(value) is not int or not 1 <= value <= ceiling:
            _reject("limits-range")


def object_oid(kind, content):
    return hashlib.sha1(kind.encode("ascii") + b" " + str(len(content)).encode("ascii")
                        + b"\0" + content).digest()


def _varint(data, pos, end):
    value = 0
    for shift in range(0, 63, 7):
        if pos >= end:
            _reject("truncated-varint")
        byte = data[pos]
        pos += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
    _reject("varint-overflow")


def _inflate(data, pos, end, declared):
    decoder = zlib.decompressobj()
    output = bytearray()
    while pos < end:
        chunk = data[pos:min(end, pos + 65536)]
        try:
            piece = decoder.decompress(chunk, declared + 1 - len(output))
        except zlib.error:
            _reject("zlib-invalid")
        output.extend(piece)
        if len(output) > declared:
            _reject("object-size-overrun")
        consumed = len(chunk) - len(decoder.unused_data) - len(decoder.unconsumed_tail)
        if consumed <= 0:
            _reject("zlib-no-progress")
        pos += consumed
        if decoder.eof:
            if len(output) != declared:
                _reject("object-size-mismatch")
            return bytes(output), pos
    _reject("zlib-truncated")


def _apply_delta(base, program, limits, instruction_count):
    base_size, pos = _varint(program, 0, len(program))
    target_size, pos = _varint(program, pos, len(program))
    if base_size != len(base):
        _reject("delta-base-size")
    if target_size > limits.max_object_bytes:
        _reject("delta-result-bound")
    output = bytearray()
    while pos < len(program):
        instruction_count[0] += 1
        if instruction_count[0] > limits.max_delta_instructions:
            _reject("delta-instruction-bound")
        opcode = program[pos]
        pos += 1
        if opcode == 0:
            _reject("delta-zero-opcode")
        if opcode & 128:
            offset = length = 0
            for bit in range(7):
                if opcode & (1 << bit):
                    if pos >= len(program):
                        _reject("delta-copy-truncated")
                    value = program[pos]
                    pos += 1
                    if bit < 4:
                        offset |= value << (bit * 8)
                    else:
                        length |= value << ((bit - 4) * 8)
            length = length or 65536
            if offset + length > len(base) or len(output) + length > target_size:
                _reject("delta-copy-bound")
            output.extend(base[offset:offset + length])
        else:
            length = opcode
            if pos + length > len(program) or len(output) + length > target_size:
                _reject("delta-insert-bound")
            output.extend(program[pos:pos + length])
            pos += length
    if len(output) != target_size:
        _reject("delta-result-size")
    return bytes(output)


def _objects(pack, limits):
    if type(pack) is not bytes or len(pack) < 32 or len(pack) > limits.max_pack_bytes:
        _reject("pack-input-bound")
    if pack[:4] != b"PACK":
        _reject("pack-magic")
    version, count = struct.unpack(">II", pack[4:12])
    if version not in (2, 3):
        _reject("pack-version")
    if not 1 <= count <= limits.max_objects:
        _reject("pack-object-count")
    end = len(pack) - 20
    if hashlib.sha1(pack[:end]).digest() != pack[end:]:
        _reject("pack-trailer")
    records = []
    offsets = {}
    expanded = 0
    pos = 12
    for _ in range(count):
        start = pos
        if pos >= end:
            _reject("object-header-truncated")
        first = pack[pos]
        pos += 1
        kind = (first >> 4) & 7
        declared = first & 15
        last = first
        for shift in range(4, 67, 7):
            if last < 128:
                break
            if pos >= end:
                _reject("object-header-truncated")
            last = pack[pos]
            pos += 1
            declared |= (last & 127) << shift
        else:
            _reject("object-header-overflow")
        if kind not in (1, 2, 3, 4, 6, 7):
            _reject("object-kind")
        if declared > limits.max_object_bytes:
            _reject("object-declared-bound")
        base = None
        if kind == 6:
            if pos >= end:
                _reject("ofs-base-truncated")
            byte = pack[pos]
            pos += 1
            distance = byte & 127
            steps = 1
            while byte & 128:
                steps += 1
                if steps > 9 or pos >= end:
                    _reject("ofs-base-overflow")
                byte = pack[pos]
                pos += 1
                distance = ((distance + 1) << 7) | (byte & 127)
            base = start - distance
            if distance == 0 or base not in offsets:
                _reject("ofs-base-not-object")
        elif kind == 7:
            if pos + 20 > end:
                _reject("ref-base-truncated")
            base = pack[pos:pos + 20]
            pos += 20
        content, pos = _inflate(pack, pos, end, declared)
        expanded += len(content)
        if expanded > limits.max_expanded_bytes:
            _reject("expanded-bound")
        offsets[start] = len(records)
        records.append((start, kind, base, content))
    if pos != end:
        _reject("pack-trailing-data")
    by_oid = {}
    resolved = {}
    pending_offsets = defaultdict(list)
    pending_oids = defaultdict(list)
    queue = deque()
    for index, (start, kind, base, content) in enumerate(records):
        if kind in KINDS:
            queue.append((index, KINDS[kind], content, 0))
        elif kind == 6:
            pending_offsets[base].append(index)
        else:
            pending_oids[base].append(index)
    instruction_count = [0]
    while queue:
        index, kind, content, depth = queue.popleft()
        if index in resolved:
            _reject("duplicate-resolution")
        if depth > limits.max_delta_depth:
            _reject("delta-depth-bound")
        oid = object_oid(kind, content)
        previous = by_oid.get(oid)
        if previous is not None and previous != (kind, content):
            _reject("oid-content-conflict")
        by_oid[oid] = (kind, content)
        resolved[index] = oid
        start = records[index][0]
        children = pending_offsets.pop(start, []) + pending_oids.pop(oid, [])
        for child in children:
            result = _apply_delta(content, records[child][3], limits, instruction_count)
            expanded += len(result)
            if expanded > limits.max_expanded_bytes:
                _reject("expanded-bound")
            queue.append((child, kind, result, depth + 1))
    if len(resolved) != count:
        _reject("thin-missing-or-cyclic-base")
    return by_oid, version, count, expanded, instruction_count[0]


_HEX = re.compile(rb"[0-9a-f]{40}")
_RESERVED = {"con", "prn", "aux", "nul"} | {
    prefix + str(number) for prefix in ("com", "lpt") for number in range(1, 10)
}


def _name(name):
    if not name or len(name) > 255:
        _reject("tree-name-bound")
    try:
        text = name.decode("utf-8", "strict")
    except UnicodeError:
        _reject("tree-name-encoding")
    if (text in (".", "..") or text.casefold() == ".git" or
            text.endswith((" ", ".")) or any(ord(char) < 32 or ord(char) == 127 for char in text)
            or any(char in text for char in '/\\:<>"|?*')
            or text.split(".", 1)[0].casefold() in _RESERVED
            or any(0x80 <= ord(char) <= 0x9f for char in text)
            or text.split(".", 1)[0].casefold() in {"com¹", "com²", "com³", "lpt¹", "lpt²", "lpt³"}):
        _reject("tree-name-unsafe")
    return text.casefold()


def _tree_entries(content, limits):
    entries = []
    pos = 0
    previous_key = None
    seen = set()
    while pos < len(content):
        if len(entries) >= limits.max_tree_entries:
            _reject("tree-entry-bound")
        space = content.find(b" ", pos)
        zero = content.find(b"\0", pos)
        if space < pos or zero <= space or zero + 21 > len(content):
            _reject("tree-entry-truncated")
        mode = content[pos:space]
        if mode not in (b"40000", b"100644", b"100755"):
            _reject("tree-mode-disallowed")
        name = content[space + 1:zero]
        folded = _name(name)
        if folded in seen:
            _reject("tree-name-collision")
        seen.add(folded)
        key = name + (b"/" if mode == b"40000" else b"\0")
        if previous_key is not None and key <= previous_key:
            _reject("tree-order")
        previous_key = key
        entries.append((mode, name, content[zero + 1:zero + 21]))
        pos = zero + 21
    return entries


def _commit_tree(content, limits):
    boundary = content.find(b"\n\n", 0, limits.max_commit_header_bytes + 2)
    if boundary < 0 or boundary > limits.max_commit_header_bytes:
        _reject("commit-header-bound")
    lines = content[:boundary].split(b"\n")
    if not lines or not lines[0].startswith(b"tree ") or not _HEX.fullmatch(lines[0][5:]):
        _reject("commit-tree-header")
    author = committer = False
    parents = 0
    for line in lines[1:]:
        if line.startswith(b"tree ") or b"\0" in line or b"\r" in line:
            _reject("commit-header-invalid")
        if line.startswith(b"parent "):
            if not _HEX.fullmatch(line[7:]) or author or committer:
                _reject("commit-parent-header")
            parents += 1
            if parents > 1024:
                _reject("commit-parent-bound")
        elif line.startswith(b"author "):
            if author or committer or not line[7:]:
                _reject("commit-author-header")
            author = True
        elif line.startswith(b"committer "):
            if not author or committer or not line[10:]:
                _reject("commit-committer-header")
            committer = True
        elif not committer or (not line.startswith(b" ") and b" " not in line):
            _reject("commit-extra-header")
    if not author or not committer:
        _reject("commit-required-header")
    return bytes.fromhex(lines[0][5:].decode("ascii")), parents


def validate_pack(pack, expected_commit, limits=DEFAULT_LIMITS):
    """Offline primitive; synthetic callers may supply an expected SHA-1 commit.

    Complete target tree/blob closure is required, parents/history are not.
    Rejecting Windows-unsafe names and links is conservative; nothing is extracted.
    """
    _limits(limits)
    if type(expected_commit) is not str or not re.fullmatch(r"[0-9a-f]{40}", expected_commit):
        _reject("expected-commit-format")
    objects, version, count, expanded, instructions = _objects(pack, limits)
    target = bytes.fromhex(expected_commit)
    if target not in objects or objects[target][0] != "commit":
        _reject("expected-commit-absent-or-type")
    tree, parents = _commit_tree(objects[target][1], limits)
    stack = [(tree, (), 0, frozenset())]
    tree_count = blob_count = entries_count = 0
    parsed_trees = {}
    while stack:
        oid, path, depth, ancestors = stack.pop()
        if depth > limits.max_tree_depth:
            _reject("tree-depth-bound")
        if oid in ancestors:
            _reject("tree-cycle")
        item = objects.get(oid)
        if item is None or item[0] != "tree":
            _reject("tree-closure-missing-or-type")
        tree_count += 1
        if tree_count > limits.max_tree_entries:
            _reject("tree-visit-bound")
        if oid not in parsed_trees:
            parsed_trees[oid] = _tree_entries(item[1], limits)
        entries = parsed_trees[oid]
        entries_count += len(entries)
        if entries_count > limits.max_tree_entries:
            _reject("tree-entry-bound")
        for mode, name, child in entries:
            child_path = path + (name,)
            if sum(map(len, child_path)) + len(child_path) - 1 > limits.max_path_bytes:
                _reject("tree-path-bound")
            if mode == b"40000":
                stack.append((child, child_path, depth + 1, ancestors | {oid}))
            else:
                blob = objects.get(child)
                if blob is None or blob[0] != "blob":
                    _reject("blob-closure-missing-or-type")
                blob_count += 1
    return {
        "status": "PASS_OFFLINE_PACK_TARGET_TREE_ONLY",
        "pack_sha256_observed": hashlib.sha256(pack).hexdigest(),
        "pack_bytes": len(pack), "pack_version": version,
        "packed_objects": count, "unique_objects": len(objects),
        "expanded_accounted_bytes": expanded, "delta_instructions": instructions,
        "commit_oid_sha1": expected_commit, "tree_oid_sha1": tree.hex(),
        "tree_visits": tree_count, "tree_entries": entries_count, "blob_visits": blob_count,
        "parent_headers_not_history_verified": parents,
        "transport": TRANSPORT, "cache_adoption": CACHE_ADOPTION,
        "sha1_collision_detection": "NOT_IMPLEMENTED_NOT_SHA1DC",
        "incoming_wire_bound": "NOT_PROVEN_BY_OFFLINE_VALIDATION",
        "no_checkout_or_object_store_write": True,
    }


def validate_fixed_pack(package, pack):
    """Exact plan identities, no caller URL/commit override; never fetches bytes."""
    if type(package) is not str or package not in FIXED:
        _reject("fixed-package")
    repository, commit, ceiling = FIXED[package]
    result = validate_pack(pack, commit, Limits(max_pack_bytes=ceiling))
    return dict(result, package=package, repository_url=repository,
                plan_sha256=PLAN_SHA256, per_repository_incoming_ceiling=ceiling,
                aggregate_git_incoming_ceiling=AGGREGATE_GIT_BYTES,
                aggregate_incoming_bound="NOT_PROVEN_NO_TRANSPORT")


def main():
    sys.stderr.write("HARD_DISABLED: offline primitive only; transport/cache NOT_IMPLEMENTED\n")
    return 78


if __name__ == "__main__":
    raise SystemExit(main())
