"""Read-only current-host Codex kit gate; no adapter, build or SDK freeze.

Historical kits and consumer receipts remain immutable. A receipt only establishes
the checked kit's byte integrity, its qualification-only review binding and its
complete source identity against the current host's authoritative contract crate.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import sys

ROOT = Path(__file__).resolve().parents[1]
CANONICAL = ROOT / "contracts/experimental/agent_host_v1"
MAX_METADATA_BYTES = 1024 * 1024

# Load only the adjacent host-owned verifier, never code from the supplied kit.
_spec = importlib.util.spec_from_file_location(
    "_morrow_codex_candidate_host_build", Path(__file__).with_name("agent_host_build.py")
)
if _spec is None or _spec.loader is None:
    raise ImportError("adjacent host-kit verifier is unavailable")
host_build = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(host_build)


class CandidateError(ValueError):
    pass


def ordinary_path(path: Path, *, directory: bool = False) -> Path:
    """Reject linked/reparse ancestors before any resolve or file consumption."""
    if ".." in Path(path).parts:
        raise CandidateError("candidate input must not use parent traversal")
    path = Path(os.path.abspath(path))
    for component in (*reversed(path.parents), path):
        info = component.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise CandidateError("linked or reparse candidate input: " + str(component))
    info = path.lstat()
    expected = stat.S_ISDIR if directory else stat.S_ISREG
    if not expected(info.st_mode):
        raise CandidateError("expected ordinary candidate " + ("directory" if directory else "file"))
    return path


def read_metadata(path: Path) -> bytes:
    path = ordinary_path(path)
    if path.stat().st_size > MAX_METADATA_BYTES:
        raise CandidateError("candidate metadata byte limit exceeded: " + path.name)
    with path.open("rb") as stream:
        data = stream.read(MAX_METADATA_BYTES + 1)
    if len(data) > MAX_METADATA_BYTES:
        raise CandidateError("candidate metadata byte limit exceeded: " + path.name)
    return data


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise CandidateError("duplicate candidate metadata field: " + key)
        result[key] = value
    return result


def document(data: bytes, label: str) -> dict:
    try:
        value = json.loads(data.decode("utf-8"), object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise CandidateError("invalid " + label + " JSON") from error
    if not isinstance(value, dict):
        raise CandidateError("expected " + label + " object")
    return value


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def wire_version(source: bytes, name: str) -> int:
    try:
        text = source.decode("utf-8")
    except UnicodeError as error:
        raise CandidateError("invalid canonical Rust source") from error
    matches = re.findall(r"^pub const " + name + r": u16 = ([0-9]+);\r?$", text, re.MULTILINE)
    if len(matches) != 1 or not 0 < int(matches[0]) <= 65535:
        raise CandidateError("missing or ambiguous canonical " + name)
    return int(matches[0])


def canonical_inventory(root: Path) -> list[dict]:
    files = host_build.inventory(root)
    # Empty output directories are also contamination; never hide them with an
    # exclusion list and then claim this is the complete canonical source tree.
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if {"target", "generated", "vectors", "__pycache__"}.intersection(relative.parts):
            raise CandidateError("unexpected canonical build output: " + relative.as_posix())
    return files


def verify_candidate(kit: Path, review: Path, *, canonical: Path = CANONICAL) -> dict:
    kit = ordinary_path(Path(kit), directory=True)
    review = ordinary_path(Path(review))
    canonical = ordinary_path(Path(canonical), directory=True)
    manifest_path = kit / "manifest.json"
    schema_path = canonical / "agent_host.capnp"
    api_path = canonical / "src/lib.rs"
    # Preserve the exact checked identity through the host verifier's leaf check.
    inputs = {path: read_metadata(path) for path in (manifest_path, review, schema_path, api_path)}
    manifest = document(inputs[manifest_path], "kit manifest")
    reviewed = document(inputs[review], "consumer review")
    if type(reviewed.get("schema_version")) is not int or reviewed["schema_version"] != 1:
        raise CandidateError("unsupported consumer review schema")
    if reviewed.get("status") != "qualification_only":
        raise CandidateError("consumer review must be qualification_only")
    manifest_digest = sha(inputs[manifest_path])
    if reviewed.get("manifest_sha256") != manifest_digest:
        raise CandidateError("consumer review manifest digest mismatch")
    canonical_digest = sha(inputs[schema_path])
    if reviewed.get("schema_sha256") != canonical_digest:
        raise CandidateError("consumer review canonical schema digest mismatch")
    major = wire_version(inputs[api_path], "MAJOR")
    revision = wire_version(inputs[api_path], "REVISION")
    if (type(manifest.get("wire_major")) is not int or manifest["wire_major"] != major
            or type(manifest.get("wire_revision")) is not int or manifest["wire_revision"] != revision):
        raise CandidateError("kit and current canonical wire version mismatch")
    if manifest.get("schema_sha256") != canonical_digest:
        raise CandidateError("kit and current canonical raw schema mismatch")

    canonical_files = canonical_inventory(canonical)
    source_names = host_build.verify_inventory(canonical, manifest.get("source_files"))
    if source_names != {entry["path"] for entry in canonical_files}:
        raise CandidateError("kit and current canonical source inventory mismatch")

    # Self-described hashes/reviews cannot substitute for verification of leaves.
    verified = host_build.verify_kit(kit)
    if (verified.get("manifest_sha256") != manifest_digest
            or verified.get("schema_sha256") != canonical_digest
            or verified.get("qualification_only") is not True):
        raise CandidateError("host verifier returned a different candidate identity")
    for path, original in inputs.items():
        if read_metadata(path) != original:
            raise CandidateError("candidate identity changed while checking: " + path.name)
    if canonical_inventory(canonical) != canonical_files:
        raise CandidateError("canonical sources changed while checking")
    return {
        "schema_version": 1,
        "status": "qualification_only",
        "scope": "current_host_source_identity_kit_integrity_and_consumer_review_binding",
        "manifest_sha256": manifest_digest,
        "review_sha256": sha(inputs[review]),
        "schema_sha256": canonical_digest,
        "wire_major": major,
        "wire_revision": revision,
        "files_verified": verified["file_count"],
        "source_files_verified": len(canonical_files),
        "source_inventory_sha256": sha(json.dumps(canonical_files, sort_keys=True, separators=(",", ":")).encode("utf-8")),
        "sdk_freeze": "OPEN",
        "p02_qualification": "NOT_RUN",
        "production_binding_available": False,
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", required=True, type=Path, help="explicit exported host-kit directory")
    parser.add_argument("--review", required=True, type=Path, help="digest-bound qualification-only consumer review")
    parser.add_argument("--json", action="store_true", help="emit a machine-readable result")
    args = parser.parse_args(argv)
    try:
        result = verify_candidate(args.kit, args.review)
    except (OSError, ValueError, TypeError, KeyError) as error:
        print("FAIL:", error, file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(result, sort_keys=True))
    else:
        print(f"PASS: qualification_only; {result['files_verified']} kit files and "
              f"{result['source_files_verified']} current host source files verified. "
              "SDK freeze OPEN; P-02 qualification NOT_RUN.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
