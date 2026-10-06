"""Read-only, explicitly pinned local Session/SafeExec-basic contract gate.

This verifies reviewed source and recorded evidence bytes. It does not rerun
commands, authenticate the reviewer, certify production transport, or prove an
OS sandbox. Creating a manifest is preparation, never an implicit freeze.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PROFILE = "agent-session-exec-v1"
CRATE = "extensions/" + PROFILE
SCHEMA = CRATE + "/contracts/session_exec.capnp"
SCOPES = ["session", "safe-exec-basic"]
BOUNDS = {
    "sdk26_frozen": False,
    "production_transport_verified": False,
    "os_sandbox_verified": False,
    "execution_extensions_deferred": True,
}
MAX_FILES = 4096
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024
MAX_METADATA_BYTES = 4 * 1024 * 1024
MAX_LOG_BYTES = 16 * 1024 * 1024
HEX = re.compile(r"[0-9a-f]{64}\Z")
NAME = re.compile(r"[A-Za-z_][A-Za-z_0-9]*(?:::[A-Za-z_][A-Za-z_0-9]*)*\Z")


class FreezeError(ValueError):
    pass


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def relative_name(value: str) -> str:
    if (not isinstance(value, str) or not value or len(value.encode()) > 1024
            or "\\" in value or ":" in value or PurePosixPath(value).is_absolute()
            or any(part in ("", ".", "..") for part in value.split("/"))
            or any(ord(char) < 32 or ord(char) == 127 for char in value)):
        raise FreezeError("invalid relative input path")
    return value


def ordinary_path(path: Path, *, directory: bool = False) -> Path:
    path = Path(path)
    if ".." in path.parts:
        raise FreezeError("input path contains parent traversal")
    path = Path(os.path.abspath(path))
    for component in (*reversed(path.parents), path):
        info = component.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise FreezeError("linked or reparse input: " + str(component))
    info = path.lstat()
    if not (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)):
        raise FreezeError("expected ordinary " + ("directory" if directory else "file"))
    return path


def read_regular(path: Path, limit: int) -> bytes:
    path = ordinary_path(path)
    before = path.stat()
    if before.st_size > limit:
        raise FreezeError("input byte limit exceeded: " + str(path))
    with path.open("rb") as stream:
        opened = os.fstat(stream.fileno())
        if not stat.S_ISREG(opened.st_mode) or (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise FreezeError("input identity changed before reading")
        data = stream.read(limit + 1)
    after = path.stat()
    if len(data) > limit:
        raise FreezeError("input byte limit exceeded")
    if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
            after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
        raise FreezeError("input changed while reading")
    return data


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise FreezeError("duplicate metadata field: " + key)
        result[key] = value
    return result


def document(data: bytes) -> dict:
    try:
        result = json.loads(data.decode("utf-8"), object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise FreezeError("invalid freeze JSON") from error
    if not isinstance(result, dict):
        raise FreezeError("freeze JSON must be an object")
    return result


def core_test_paths(core_tests) -> list[str]:
    if not isinstance(core_tests, (list, tuple)):
        raise FreezeError("core test paths must be a list")
    names = {"core/tests/agent_ledger.rs"}
    for name in core_tests:
        name = relative_name(name)
        if not re.fullmatch(r"core/tests/[A-Za-z_][A-Za-z_0-9]*\.rs", name):
            raise FreezeError("invalid selected Core test")
        if name in names and name != "core/tests/agent_ledger.rs":
            raise FreezeError("duplicate selected Core test")
        names.add(name)
    return sorted(names)


def source_inventory(root: Path, core_tests=()) -> dict[str, str]:
    root = ordinary_path(root, directory=True)
    allowed_top = {"Cargo.toml", "Cargo.lock", "build.rs", "README.md", "contracts", "src", "tests", "examples"}
    crate = ordinary_path(root / CRATE, directory=True)
    for path in crate.iterdir():
        if path.name not in allowed_top:
            raise FreezeError("unfrozen crate top-level input: " + path.name)
        if path.name in {"contracts", "src", "tests", "examples"}:
            ordinary_path(path, directory=True)
        else:
            ordinary_path(path)
    # Cargo walks cwd ancestors for configuration. A new local configuration
    # must receive an explicit reviewed closure policy before it is accepted.
    for directory in {root, *root.parents, root / "core", crate}:
        cargo_config = directory / ".cargo"
        if cargo_config.exists() or cargo_config.is_symlink():
            raise FreezeError("unfrozen Cargo configuration: " + str(cargo_config))
    names = [CRATE + "/" + name for name in ("Cargo.toml", "Cargo.lock", "build.rs", "README.md")]
    names += ["core/" + name for name in ("Cargo.toml", "Cargo.lock", "build.rs")]
    names += core_test_paths(core_tests)
    trees = [CRATE + "/" + name for name in ("contracts", "src", "tests")]
    # v25 migration fixtures affect the durable contract too. Bind all Core
    # tests without crediting every test target as a formal execution group.
    trees += ["core/src", "core/schemas", "core/tests"]
    # Core build.rs unconditionally compiles tests/schemas/future.proto.
    ordinary_path(root / "core/tests/schemas", directory=True)
    examples = root / CRATE / "examples"
    if examples.exists() or examples.is_symlink():
        trees.append(CRATE + "/examples")
    visited = 0
    for tree in trees:
        pending = [ordinary_path(root / tree, directory=True)]
        while pending:
            folder = pending.pop()
            for path in sorted(folder.iterdir()):
                visited += 1
                if visited > MAX_FILES * 4:
                    raise FreezeError("source traversal limit exceeded")
                relative = path.relative_to(root).as_posix()
                relative_name(relative)
                if {"target", "__pycache__", "generated"}.intersection(path.relative_to(root / tree).parts):
                    raise FreezeError("build output contaminates source closure")
                info = path.lstat()
                if stat.S_ISDIR(info.st_mode):
                    pending.append(ordinary_path(path, directory=True))
                else:
                    ordinary_path(path)
                    names.append(relative)
                if len(names) + len(pending) > MAX_FILES:
                    raise FreezeError("source file limit exceeded")
    inventory = {}
    total = 0
    for name in sorted(set(names)):
        data = read_regular(root / relative_name(name), min(MAX_FILE_BYTES, MAX_TOTAL_BYTES - total))
        total += len(data)
        inventory[name] = sha(data)
    return inventory


def contract_identity(root: Path) -> dict:
    api = read_regular(root / CRATE / "src/lib.rs", MAX_FILE_BYTES).decode("utf-8")
    constants = {}
    for name in ("VERSION", "REVISION"):
        matches = re.findall(r"^pub const " + name + r":\s*u16\s*=\s*([0-9]+);\s*$", api, re.MULTILINE)
        if len(matches) != 1 or not 0 < int(matches[0]) <= 65535:
            raise FreezeError("missing or ambiguous API " + name)
        constants[name.lower()] = int(matches[0])
    profiles = re.findall(r'^pub const PROFILE:\s*&str\s*=\s*"([^"\n]+)";\s*$', api, re.MULTILINE)
    if profiles != [PROFILE]:
        raise FreezeError("canonical API profile mismatch")
    cargo = tomllib.loads(read_regular(root / CRATE / "Cargo.toml", MAX_FILE_BYTES).decode("utf-8"))
    version = cargo.get("package", {}).get("version")
    if not isinstance(version, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?", version):
        raise FreezeError("invalid crate version")
    return {"schema_path": SCHEMA, "schema_sha256": sha(read_regular(root / SCHEMA, MAX_FILE_BYTES)),
            "crate_version": version, **constants}


def test_expectations(root: Path, expected, core_tests) -> dict[str, list[str]]:
    if not isinstance(expected, dict) or not expected:
        raise FreezeError("fixed nonzero test name lists are required")
    required = {"core:" + Path(name).stem for name in core_test_paths(core_tests)}
    for path in (root / CRATE / "tests").glob("*.rs"):
        text = read_regular(path, MAX_FILE_BYTES).decode("utf-8")
        if re.search(r"#\[\s*(?:test|[A-Za-z_:]+::test)(?:\s|\])", text):
            required.add(path.stem)
    if set(expected) != required or not any(not key.startswith("core:") for key in required):
        raise FreezeError("test groups do not cover the complete selected test targets")
    result = {}
    for key, names in expected.items():
        target = key.removeprefix("core:")
        if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", target):
            raise FreezeError("invalid test target")
        if (not isinstance(names, list) or not names or len(names) > MAX_FILES
                or any(not isinstance(name, str) or not NAME.fullmatch(name) for name in names)
                or len(set(names)) != len(names)):
            raise FreezeError("invalid, duplicate or empty fixed test names")
        result[key] = sorted(names)
    return dict(sorted(result.items()))


def evidence_log(root: Path, entry: dict, checked: dict[Path, bytes]) -> bytes:
    if not isinstance(entry, dict) or set(entry) != {"path", "sha256"}:
        raise FreezeError("invalid evidence log identity")
    path = root / relative_name(entry["path"])
    data = read_regular(path, MAX_LOG_BYTES)
    if not isinstance(entry["sha256"], str) or not HEX.fullmatch(entry["sha256"]) or sha(data) != entry["sha256"]:
        raise FreezeError("evidence log digest mismatch")
    checked[path] = data
    return data


def verify_evidence(root: Path, evidence, expected, checked: dict) -> int:
    if not isinstance(evidence, dict) or set(evidence) != {"tools", "tests"}:
        raise FreezeError("invalid evidence fields")
    tools = evidence["tools"]
    if not isinstance(tools, dict) or set(tools) != {"rustc", "cargo", "capnp"}:
        raise FreezeError("exact rustc/cargo/capnp identities are required")
    for name, record in tools.items():
        if not isinstance(record, dict) or set(record) != {"argv", "exit_code", "stdout", "stderr"}:
            raise FreezeError("invalid tool evidence")
        argv = record["argv"]
        flag = {"rustc": "-Vv", "cargo": "-V", "capnp": "--version"}[name]
        if (not isinstance(argv, list) or len(argv) != 2 or not all(isinstance(arg, str) for arg in argv)
                or Path(argv[0]).name != name or argv[1] != flag
                or type(record["exit_code"]) is not int or record["exit_code"] != 0):
            raise FreezeError("unexpected tool identity command or failure")
        output = evidence_log(root, record["stdout"], checked)
        evidence_log(root, record["stderr"], checked)
        prefix = {"rustc": b"rustc ", "cargo": b"cargo ", "capnp": b"Cap'n Proto version "}[name]
        if not output.startswith(prefix):
            raise FreezeError("missing tool version output")
    tests = evidence["tests"]
    if not isinstance(tests, dict) or set(tests) != set(expected):
        raise FreezeError("missing or extra test evidence groups")
    target_directories = {}
    count = 0
    for key, names in expected.items():
        record = tests[key]
        if not isinstance(record, dict) or set(record) != {"argv", "cwd", "exit_code", "stdout", "stderr"}:
            raise FreezeError("invalid test evidence fields")
        if record["cwd"] != str(root) or type(record["exit_code"]) is not int or record["exit_code"] != 0:
            raise FreezeError("test cwd mismatch or unsuccessful process")
        manifest = "core/Cargo.toml" if key.startswith("core:") else CRATE + "/Cargo.toml"
        argv = record["argv"]
        if not isinstance(argv, list) or not all(isinstance(arg, str) for arg in argv):
            raise FreezeError("invalid test command")
        if len(argv) != 10 or argv[:4] != [tools["cargo"]["argv"][0], "test", "--locked", "--offline"]:
            raise FreezeError("test requires exact default-feature locked offline command")
        if argv[4:7] != ["--manifest-path", manifest, "--target-dir"] or argv[8:] != ["--test", key.removeprefix("core:")]:
            raise FreezeError("test manifest/target identity mismatch")
        target = Path(argv[7])
        if not target.is_absolute():
            target = root / relative_name(argv[7])
        if ".." in target.parts or not target.is_relative_to(root / "build"):
            raise FreezeError("test target must be inside the host build directory")
        if target.exists() or target.is_symlink():
            ordinary_path(target, directory=True)
        if manifest in target_directories and target_directories[manifest] != str(target):
            raise FreezeError("test groups mix target directories for one crate")
        target_directories[manifest] = str(target)
        output = evidence_log(root, record["stdout"], checked).decode("utf-8")
        evidence_log(root, record["stderr"], checked)
        actual = re.findall(r"^test ([A-Za-z_][A-Za-z_0-9:]*) \.\.\. ok\r?$", output, re.MULTILINE)
        summaries = re.findall(r"^test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored; ([0-9]+) measured; ([0-9]+) filtered out; finished in [^\r\n]+\r?$", output, re.MULTILINE)
        if len(summaries) != 1 or tuple(map(int, summaries[0])) != (len(names), 0, 0, 0, 0):
            raise FreezeError("test evidence has zero, failed, ignored, filtered or ambiguous summary")
        if sorted(actual) != names or len(set(actual)) != len(actual):
            raise FreezeError("test evidence does not match the fixed complete name list")
        if re.search(r"^test .* \.\.\. (?:FAILED|ignored|bench)", output, re.MULTILINE):
            raise FreezeError("test evidence contains unsuccessful cases")
        count += len(actual)
    return count


def create_manifest(root: Path, evidence: dict, testexpected: dict, *, core_tests=()) -> dict:
    """Prepare an unsealed identity after the caller has actually run its tests."""
    root = ordinary_path(root, directory=True)
    selected = core_test_paths(core_tests)
    expected = test_expectations(root, testexpected, selected)
    checked = {}
    verify_evidence(root, evidence, expected, checked)
    return {"schema_version": 1, "profile": PROFILE, "scope": SCOPES.copy(), "bounds": BOUNDS.copy(),
            "contract": contract_identity(root), "core_tests": selected,
            "source_inventory": source_inventory(root, selected), "expected_test_names": expected,
            "evidence": evidence}


def verify_freeze(root: Path, baseline: Path, expected_pin: str) -> dict:
    if not isinstance(expected_pin, str) or not HEX.fullmatch(expected_pin):
        raise FreezeError("explicit reviewed expected pin is required")
    root = ordinary_path(root, directory=True)
    raw = read_regular(baseline, MAX_METADATA_BYTES)
    if sha(raw) != expected_pin:
        raise FreezeError("freeze manifest root pin mismatch; self-rehash cannot replace the reviewed pin")
    manifest = document(raw)
    fields = {"schema_version", "profile", "scope", "bounds", "contract", "core_tests", "source_inventory", "expected_test_names", "evidence"}
    if set(manifest) != fields or type(manifest["schema_version"]) is not int or manifest["schema_version"] != 1:
        raise FreezeError("unsupported freeze manifest fields/schema")
    if manifest["profile"] != PROFILE or manifest["scope"] != SCOPES:
        raise FreezeError("freeze scope cannot include execution extensions or production qualification")
    if manifest["bounds"] != BOUNDS or any(type(value) is not bool for value in manifest["bounds"].values()):
        raise FreezeError("freeze qualification bounds mismatch")
    core_tests = core_test_paths(manifest["core_tests"])
    if manifest["core_tests"] != core_tests:
        raise FreezeError("selected Core test list must be exact and sorted")
    sources = source_inventory(root, core_tests)
    if manifest["source_inventory"] != sources:
        raise FreezeError("source closure has missing, extra or changed inputs")
    contract = contract_identity(root)
    if manifest["contract"] != contract or any(type(manifest["contract"].get(name)) is not int for name in ("version", "revision")):
        raise FreezeError("raw schema, crate or API contract identity mismatch")
    expected = test_expectations(root, manifest["expected_test_names"], core_tests)
    if manifest["expected_test_names"] != expected:
        raise FreezeError("fixed test names must be sorted")
    checked = {Path(baseline): raw}
    count = verify_evidence(root, manifest["evidence"], expected, checked)
    for path, before in checked.items():
        if read_regular(path, MAX_LOG_BYTES) != before:
            raise FreezeError("pinned evidence changed while verifying")
    if source_inventory(root, core_tests) != sources:
        raise FreezeError("source closure changed while verifying")
    return {"schema_version": 1, "profile": PROFILE, "status": "local_contract_frozen", "scope": SCOPES.copy(),
            "manifest_sha256": expected_pin, "contract": contract,
            "source_files_verified": len(sources), "recorded_tests_verified": count,
            "evidence_execution_repeated": False, "current_reexecution": False,
            "reviewer_authenticated": False, **BOUNDS}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--expected-pin", required=True, help="independently reviewed raw manifest SHA256")
    parser.add_argument("--json", action="store_true")
    arguments = parser.parse_args(argv)
    try:
        result = verify_freeze(arguments.root, arguments.baseline, arguments.expected_pin)
    except (OSError, ValueError, TypeError, KeyError) as error:
        print("FAIL:", error, file=sys.stderr)
        return 1
    if arguments.json:
        print(json.dumps(result, sort_keys=True))
    else:
        print(f"PASS: local_contract_frozen session/safe-exec-basic; {result['recorded_tests_verified']} recorded tests. SDK26 OPEN; no production transport or OS sandbox qualification.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
