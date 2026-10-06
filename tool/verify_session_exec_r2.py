"""Portable, explicitly reviewed Session/SafeExec-basic revision-two source gate.

This verifies reviewed source and recorded evidence bytes. It does not rerun
commands, authenticate the reviewer, certify production transport, or prove an
OS sandbox. Creating a manifest is preparation, never an implicit freeze.
"""
from __future__ import annotations

import argparse
import ast
import gzip
import io
import tarfile
import tempfile
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
CRATE = "extensions/agent-session-exec-v1-r2"
PACKAGE = "morrow-agent-session-exec-v1-r2"
GATE = "tool/verify_session_exec_r2.py"
GATE_TEST = "tool/tests/test_session_exec_r2.py"
R1_ARCHIVE = "reports/reconstruction-2026-10-05/session-exec-v1-r1-source.tar.gz"
R1_ARCHIVE_SHA256 = "c8ca0b529f3cc33177b36a7a3c396f42168a4303532f0a93ce28fd6aa3970d0c"
R1_BASELINE = "reports/reconstruction-2026-10-05/session-exec-v1-freeze.json"
R1_PIN = "2e958fde445c37671f3100e1f880875fe4d8c9173a7014519494a55f1b9c33f5"
BUNDLE_BASELINE = "session-exec-r2-manifest.json"
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_EXECUTABLE_BYTES = 128 * 1024 * 1024
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
MAX_TOTAL_BYTES = 96 * 1024 * 1024
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
    if not directory and stat.S_ISREG(info.st_mode) and info.st_nlink != 1:
        raise FreezeError("hard-linked input: " + str(path))
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


def source_roots(extra_source_roots=()):
    if not isinstance(extra_source_roots, (list, tuple)):
        raise FreezeError("extra source roots must be a list")
    extra = [relative_name(name) for name in extra_source_roots]
    if len(set(extra)) != len(extra) or any(name in ("core", CRATE) for name in extra):
        raise FreezeError("duplicate source root")
    return sorted(["core", CRATE, *extra])


ENVIRONMENT_KEYS = {"RUSTC", "RUSTDOC", "CC", "CXX", "AR", "CARGO_HOME", "CARGO_BUILD_JOBS",
    "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "RUSTUP_TOOLCHAIN", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
    "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTFLAGS", "CFLAGS", "CXXFLAGS", "CPPFLAGS"}
FORBIDDEN_FLAGS = {"RUSTDOC", "CARGO_BUILD_TARGET", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
    "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTFLAGS"}


def reject_configuration(root: Path, roots):
    directories = {root, *root.parents}
    for name in roots:
        path = root / name
        directories.update([path, *path.parents])
    for directory in directories:
        for name in (".cargo", "rust-toolchain", "rust-toolchain.toml"):
            path = directory / name
            if path.exists() or path.is_symlink():
                raise FreezeError("unreviewed Cargo/toolchain configuration: " + str(path))
    for name in ("Cargo.toml", "Cargo.lock"):
        if (root / name).exists() or (root / name).is_symlink():
            raise FreezeError("unreviewed root workspace configuration: " + name)
    for name, value in os.environ.items():
        if value and (name in FORBIDDEN_FLAGS or name.startswith("CARGO_TARGET_") and name.endswith(("_RUSTFLAGS", "_LINKER"))):
            raise FreezeError("unreviewed build environment flags: " + name)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    if not cargo_home.is_absolute():
        raise FreezeError("Cargo home must be an absolute input path")
    if cargo_home.exists() or cargo_home.is_symlink():
        ordinary_path(cargo_home, directory=True)
    for name in ("config", "config.toml"):
        config = cargo_home / name
        if config.exists() or config.is_symlink():
            raise FreezeError("unreviewed external Cargo-home configuration: " + str(config))


def source_inventory(root: Path, extra_source_roots=(), extra_inputs=()) -> dict[str, str]:
    root = ordinary_path(root, directory=True)
    roots = source_roots(extra_source_roots)
    if not isinstance(extra_inputs, (list, tuple)) or len(extra_inputs) > MAX_FILES:
        raise FreezeError("extra reference inputs must be a bounded list")
    references = [relative_name(name) for name in extra_inputs]
    if len(set(references)) != len(references):
        raise FreezeError("duplicate reference input")
    reject_configuration(root, roots)
    names = ["README.md", "LICENSE", GATE, GATE_TEST]
    visited = 0
    for name in roots:
        folder = ordinary_path(root / name, directory=True)
        # Cargo inputs are preserved at their original relative paths. All files
        # of each declared crate are bound, not just its currently compiled src.
        required = ["Cargo.toml", "Cargo.lock", "src"]
        if name in ("core", CRATE):
            required += ["README.md", "build.rs", "tests", "schemas" if name == "core" else "contracts"]
        for item in required:
            ordinary_path(folder / item, directory=item in ("src", "tests", "schemas", "contracts"))
        pending = [folder]
        while pending:
            current = pending.pop()
            for path in sorted(current.iterdir()):
                visited += 1
                if visited > MAX_FILES * 4:
                    raise FreezeError("source traversal limit exceeded")
                relative = relative_name(path.relative_to(root).as_posix())
                if path.name in {"target", "build", "dist", "__pycache__", "generated", ".git", ".cargo"}:
                    raise FreezeError("build output/configuration contaminates source closure: " + relative)
                if path.name in {"rust-toolchain", "rust-toolchain.toml"}:
                    raise FreezeError("unreviewed crate toolchain configuration: " + relative)
                info = path.lstat()
                if stat.S_ISDIR(info.st_mode):
                    pending.append(ordinary_path(path, directory=True))
                else:
                    ordinary_path(path)
                    names.append(relative)
                if len(names) + len(pending) > MAX_FILES:
                    raise FreezeError("source file limit exceeded")
    # Reference inputs preserve reviewed callers/toolchain metadata without
    # pretending to include their entire independently qualified upstream graph.
    for name in sorted(references):
        path = root / name
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode):
            ordinary_path(path)
            names.append(name)
            continue
        pending = [ordinary_path(path, directory=True)]
        while pending:
            folder = pending.pop()
            for item in sorted(folder.iterdir()):
                visited += 1
                if visited > MAX_FILES * 4 or len(names) > MAX_FILES:
                    raise FreezeError("reference source traversal limit exceeded")
                if item.name in {"target", "build", "dist", "__pycache__", "generated", ".git", ".cargo"}:
                    raise FreezeError("build output/configuration contaminates reference inputs")
                info = item.lstat()
                if stat.S_ISDIR(info.st_mode):
                    pending.append(ordinary_path(item, directory=True))
                else:
                    ordinary_path(item)
                    names.append(relative_name(item.relative_to(root).as_posix()))
    ordinary_path(root / "core/tests/schemas", directory=True)
    inventory = {}
    total = 0
    for name in sorted(set(names)):
        data = read_regular(root / relative_name(name), min(MAX_FILE_BYTES, MAX_TOTAL_BYTES - total))
        total += len(data)
        inventory[name] = sha(data)
    # Every local path dependency, including optional and dev dependencies,
    # needs a preserved manifest. Explicit extras define, rather than imply,
    # the reviewed build closure; unrelated production graphs are not claimed.
    for name in inventory:
        if not name.endswith("/Cargo.toml") or not any(name.startswith(prefix + "/") for prefix in roots):
            continue
        cargo = tomllib.loads(read_regular(root / name, MAX_FILE_BYTES).decode("utf-8"))
        def visit(value):
            if not isinstance(value, dict):
                return
            for key, entry in value.items():
                if key in ("dependencies", "build-dependencies", "dev-dependencies") and isinstance(entry, dict):
                    for dependency in entry.values():
                        if isinstance(dependency, dict) and "path" in dependency:
                            if not isinstance(dependency["path"], str):
                                raise FreezeError("invalid local dependency path")
                            target = Path(os.path.abspath((root / name).parent / dependency["path"] / "Cargo.toml"))
                            try:
                                relative = target.relative_to(root).as_posix()
                            except ValueError as error:
                                raise FreezeError("local dependency escapes the source root") from error
                            if relative not in inventory:
                                raise FreezeError("local dependency missing from declared source closure: " + relative)
                elif isinstance(entry, dict):
                    visit(entry)
        visit(cargo)
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
    if constants != {"version": 1, "revision": 2} or cargo.get("package", {}).get("name") != PACKAGE:
        raise FreezeError("R2 requires the separate revision-two package and contract")
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


def inventory_digest(inventory) -> str:
    return sha(json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode())


def checked_inventory(value):
    if not isinstance(value, dict) or not value or len(value) > MAX_FILES:
        raise FreezeError("invalid source inventory")
    for name, digest in value.items():
        relative_name(name)
        if not isinstance(digest, str) or not HEX.fullmatch(digest):
            raise FreezeError("invalid source digest")
    return value


def archive_entries(path: Path) -> dict[str, bytes]:
    data = read_regular(path, MAX_ARCHIVE_BYTES)
    entries = {}
    total = 0
    try:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for item in archive:
                name = relative_name(item.name)
                if (not item.isreg() or item.linkname or item.pax_headers
                        or name in entries or item.size < 0 or item.size > MAX_LOG_BYTES):
                    raise FreezeError("duplicate, linked, extended or nonregular archive member")
                total += item.size
                if len(entries) >= MAX_FILES or total > MAX_ARCHIVE_BYTES:
                    raise FreezeError("archive bounds exceeded")
                stream = archive.extractfile(item)
                if stream is None:
                    raise FreezeError("missing archive member bytes")
                body = stream.read(item.size + 1)
                if len(body) != item.size:
                    raise FreezeError("archive member size mismatch")
                entries[name] = body
    except (tarfile.TarError, EOFError) as error:
        raise FreezeError("invalid source archive") from error
    if not entries:
        raise FreezeError("empty source archive")
    return entries


def log_paths(evidence):
    paths = set()
    records = [*evidence["tools"].values(), *evidence["tests"].values()]
    if "consumer" in evidence:
        records.append(evidence["consumer"])
    if "python_tests" in evidence:
        records.append(evidence["python_tests"])
    records.extend(evidence.get("external_qualifications", {}).values())
    for record in records:
        for channel in ("stdout", "stderr"):
            paths.add(relative_name(record[channel]["path"]))
    return paths


def test_log(data: bytes, names: list[str]) -> int:
    output = data.decode("utf-8")
    actual = re.findall(r"^test ([A-Za-z_][A-Za-z_0-9:]*) \.\.\. ok\r?$", output, re.MULTILINE)
    summaries = re.findall(r"^test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored; ([0-9]+) measured; ([0-9]+) filtered out; finished in [^\r\n]+\r?$", output, re.MULTILINE)
    if (len(summaries) != 1 or tuple(map(int, summaries[0])) != (len(names), 0, 0, 0, 0)
            or sorted(actual) != sorted(names) or len(set(actual)) != len(actual)
            or re.search(r"^test .* \.\.\. (?:FAILED|ignored|bench)", output, re.MULTILINE)):
        raise FreezeError("test log has failed, zero, skipped, extra or incomplete fixed cases")
    return len(actual)


def verify_r1_archive(path: Path) -> dict:
    """Validate the immutable R1 bytes and its internally pinned source/log closure."""
    if sha(read_regular(path, MAX_ARCHIVE_BYTES)) != R1_ARCHIVE_SHA256:
        raise FreezeError("historical R1 archive raw identity mismatch")
    entries = archive_entries(path)
    baseline = entries.get(R1_BASELINE)
    if baseline is None or sha(baseline) != R1_PIN:
        raise FreezeError("historical R1 external baseline pin mismatch")
    old = document(baseline)
    sources = checked_inventory(old["source_inventory"])
    if (len(sources) != 237 or old["profile"] != PROFILE or old["scope"] != SCOPES
            or old["bounds"] != BOUNDS or old["contract"]["version"] != 1
            or old["contract"]["revision"] != 1):
        raise FreezeError("historical R1 reviewed scope/closure mismatch")
    for name, digest in sources.items():
        if name not in entries or sha(entries[name]) != digest:
            raise FreezeError("historical R1 source identity mismatch")
    evidence = old["evidence"]
    records = [*evidence["tools"].values(), *evidence["tests"].values()]
    for record in records:
        for channel in ("stdout", "stderr"):
            log = record[channel]
            if set(log) != {"path", "sha256"} or sha(entries.get(log["path"], b"")) != log["sha256"]:
                raise FreezeError("historical R1 evidence log identity mismatch")
        if type(record["exit_code"]) is not int or record["exit_code"] != 0:
            raise FreezeError("historical R1 recorded command failed")
    count = sum(test_log(entries[evidence["tests"][key]["stdout"]["path"]], names)
                for key, names in old["expected_test_names"].items())
    extras = {R1_BASELINE, "reports/reconstruction-2026-10-05/session-exec-v1-freeze.sha256",
              "reports/reconstruction-2026-10-05/session-exec-v1-validation.json",
              "tool/verify_session_exec_freeze.py", "tool/tests/test_session_exec_freeze.py"}
    if count != 81 or set(entries) != set(sources) | log_paths(evidence) | extras:
        raise FreezeError("historical R1 archive has missing/extra members or test counts")
    return {"archive_sha256": R1_ARCHIVE_SHA256, "baseline_sha256": R1_PIN,
            "source_files_verified": 237, "recorded_tests_verified": 81}


def tool_identity(record, name, checked, root, check_executables):
    if not isinstance(record, dict) or set(record) != {"argv", "executable", "exit_code", "stdout", "stderr"}:
        raise FreezeError("invalid tool identity fields")
    argv = record["argv"]
    flag = {"rustc": "-Vv", "cargo": "-V", "capnp": "--version", "cc": "--version"}[name]
    if (not isinstance(argv, list) or len(argv) != 2 or not all(isinstance(arg, str) for arg in argv)
            or (name != "cc" and Path(argv[0]).name not in (name, name + ".exe")) or not Path(argv[0]).is_absolute()
            or argv[1] != flag or type(record["exit_code"]) is not int or record["exit_code"] != 0):
        raise FreezeError("tool requires its actual absolute executable and fixed version command")
    executable = record["executable"]
    if (not isinstance(executable, dict) or set(executable) != {"path", "sha256"}
            or executable["path"] != argv[0] or not isinstance(executable["sha256"], str)
            or not HEX.fullmatch(executable["sha256"])):
        raise FreezeError("missing actual executable input digest")
    if check_executables:
        path = ordinary_path(Path(argv[0]))
        if not os.access(path, os.X_OK) or sha(read_regular(path, MAX_EXECUTABLE_BYTES)) != executable["sha256"]:
            raise FreezeError("actual executable identity mismatch")
    output = evidence_log(root, record["stdout"], checked)
    evidence_log(root, record["stderr"], checked)
    prefix = {"rustc": b"rustc ", "cargo": b"cargo ", "capnp": b"Cap'n Proto version ", "cc": b""}[name]
    if not output or not output.startswith(prefix):
        raise FreezeError("missing exact tool version output")


def verify_consumer(root, record, cargo, inventory, checked):
    fields = {"argv", "cwd", "source_root", "exit_code", "stdout", "stderr", "inputs",
              "source_inventory_before", "source_inventory_after"}
    if not isinstance(record, dict) or set(record) != fields:
        raise FreezeError("invalid independent consumer fields")
    if (type(record["exit_code"]) is not int or record["exit_code"] != 0
            or not isinstance(record["cwd"], str) or not Path(record["cwd"]).is_absolute()
            or not isinstance(record["source_root"], str) or not Path(record["source_root"]).is_absolute()
            or record["source_inventory_before"] != inventory or record["source_inventory_after"] != inventory):
        raise FreezeError("independent consumer source drift or unsuccessful compilation")
    argv = record["argv"]
    if (not isinstance(argv, list) or len(argv) != 8
            or argv[:7] != [cargo, "check", "--locked", "--offline", "--manifest-path", "Cargo.toml", "--target-dir"]
            or not isinstance(argv[7], str) or not Path(argv[7]).is_absolute() or ".." in Path(argv[7]).parts):
        raise FreezeError("consumer requires exact locked offline default-feature check command")
    inputs = checked_inventory(record["inputs"])
    if len(inputs) != 3:
        raise FreezeError("consumer requires exact Cargo manifest, lock and independent source inputs")
    blobs = {}
    for name, digest in inputs.items():
        data = read_regular(root / name, MAX_FILE_BYTES)
        if sha(data) != digest:
            raise FreezeError("independent consumer input identity mismatch")
        checked[root / name] = data
        blobs[name] = data
    manifests = [name for name in inputs if name.endswith("/Cargo.toml")]
    if len(manifests) != 1:
        raise FreezeError("missing independent consumer manifest")
    parent = str(PurePosixPath(manifests[0]).parent)
    if set(inputs) not in ({parent + "/Cargo.toml", parent + "/Cargo.lock", parent + "/src/main.rs"},
                           {parent + "/Cargo.toml", parent + "/Cargo.lock", parent + "/src/lib.rs"}):
        raise FreezeError("unexpected independent consumer input set")
    target = Path(argv[7])
    source_root = Path(record["source_root"])
    cwd = Path(record["cwd"])
    if ".." in source_root.parts or ".." in cwd.parts or target.is_relative_to(source_root) or target.is_relative_to(cwd):
        raise FreezeError("consumer target must be outside exported source and independent consumer inputs")
    consumer = tomllib.loads(blobs[manifests[0]].decode("utf-8"))
    if set(consumer) - {"package", "workspace", "dependencies", "lib", "bin"} or consumer.get("workspace", {}) not in ({}, {"resolver": "2"}):
        raise FreezeError("consumer cannot add workspace, profile, patch or feature substitutions")
    if consumer.get("package", {}).get("build") or set(consumer.get("dependencies", {})) != {PACKAGE}:
        raise FreezeError("consumer requires only the reviewed client dependency and captured source")
    dependency = consumer.get("dependencies", {}).get(PACKAGE)
    if (not isinstance(dependency, dict) or not isinstance(dependency.get("path"), str)
            or set(dependency) != {"path", "default-features"} or dependency["default-features"] is not False):
        raise FreezeError("consumer must import the separate reviewed R2 package")
    actual = Path(os.path.abspath(Path(record["cwd"]) / dependency["path"]))
    if actual != Path(record["source_root"]) / CRATE:
        raise FreezeError("consumer dependency is not the exported R2 source")
    stdout = evidence_log(root, record["stdout"], checked)
    stderr = evidence_log(root, record["stderr"], checked)
    if not re.search(rb"Finished [`']?(?:dev|release|test)[`']? profile", stdout + stderr):
        raise FreezeError("independent consumer has no completed Cargo compilation evidence")


def fixed_names(value):
    if (not isinstance(value, list) or not value or len(value) > MAX_FILES
            or any(not isinstance(name, str) or not NAME.fullmatch(name) for name in value)
            or len(set(value)) != len(value)):
        raise FreezeError("fixed nonzero complete test names required")
    return sorted(value)


def group_key(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9.:-]*", value):
        raise FreezeError("invalid evidence group identity")
    return value


def group_definition(value):
    if not isinstance(value, dict) or set(value) != {"manifest", "kind", "target", "expected_names"}:
        raise FreezeError("invalid extra test group definition")
    manifest = relative_name(value["manifest"])
    kind = value["kind"]
    target = value["target"]
    if not manifest.endswith("/Cargo.toml"):
        raise FreezeError("test group requires a Cargo manifest")
    if kind == "lib":
        if target is not None:
            raise FreezeError("lib test group target must be null")
    elif kind == "test":
        if not isinstance(target, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", target):
            raise FreezeError("invalid integration test target")
    else:
        raise FreezeError("test group kind must be lib or test")
    return {"manifest": manifest, "kind": kind, "target": target,
            "expected_names": fixed_names(value["expected_names"])}


def reviewed_test_groups(root, extra, inventory, declared_roots):
    if not isinstance(extra, dict):
        raise FreezeError("extra test groups must be an object")
    result = {}
    for key, value in extra.items():
        group_key(key)
        definition = group_definition(value)
        manifest = definition["manifest"]
        crate = manifest.removesuffix("/Cargo.toml")
        if crate not in declared_roots or manifest not in inventory:
            raise FreezeError("extra test manifest is outside declared reviewed source roots")
        if definition["kind"] == "test":
            target_source = crate + "/tests/" + definition["target"] + ".rs"
        else:
            cargo = tomllib.loads(read_regular(root / manifest, MAX_FILE_BYTES).decode("utf-8"))
            path = cargo.get("lib", {}).get("path", "src/lib.rs")
            target_source = crate + "/" + relative_name(path)
        if target_source not in inventory:
            raise FreezeError("extra test target source is missing from the reviewed inventory")
        result[key] = definition
    return dict(sorted(result.items()))


def test_command(record, definition, cargo, execution_root, targets):
    if not isinstance(record, dict) or set(record) != {"argv", "cwd", "exit_code", "stdout", "stderr"}:
        raise FreezeError("invalid test evidence fields")
    if record["cwd"] != execution_root or type(record["exit_code"]) is not int or record["exit_code"] != 0:
        raise FreezeError("test cwd identity mismatch or failed process")
    argv = record["argv"]
    suffix = ["--lib"] if definition["kind"] == "lib" else ["--test", definition["target"]]
    if (not isinstance(argv, list) or not all(isinstance(arg, str) for arg in argv)
            or len(argv) != 8 + len(suffix)
            or argv[:7] != [cargo, "test", "--locked", "--offline", "--manifest-path", definition["manifest"], "--target-dir"]
            or argv[8:] != suffix):
        raise FreezeError("requires exact locked offline lib-or-test command without extra flags")
    target = Path(argv[7])
    if not target.is_absolute():
        target = Path(execution_root) / relative_name(argv[7])
    if ".." in target.parts or not target.is_relative_to(Path(execution_root) / "build"):
        raise FreezeError("test target must be inside recorded host build directory")
    manifest = definition["manifest"]
    if manifest in targets and targets[manifest] != str(target):
        raise FreezeError("one test crate mixes target directories")
    targets[manifest] = str(target)


def python_test_names(root):
    tree = ast.parse(read_regular(root / GATE_TEST, MAX_FILE_BYTES).decode("utf-8"))
    names = []
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            names.extend(node.name + "." + method.name for method in node.body
                         if isinstance(method, (ast.FunctionDef, ast.AsyncFunctionDef)) and method.name.startswith("test_"))
    if not names or len(names) != len(set(names)):
        raise FreezeError("missing or ambiguous complete Python gate test names")
    return sorted(names)


def verify_python_tests(root, record, execution_root, checked, check_executables):
    fields = {"argv", "cwd", "exit_code", "stdout", "stderr", "executable", "expected_names"}
    if not isinstance(record, dict) or set(record) != fields:
        raise FreezeError("invalid Python gate test evidence fields")
    argv = record["argv"]
    if (not isinstance(argv, list) or len(argv) != 3 or not all(isinstance(arg, str) for arg in argv)
            or argv[1:] != [GATE_TEST, "-v"] or not Path(argv[0]).is_absolute()
            or not re.fullmatch(r"python(?:3(?:\.[0-9]+)?)?(?:\.exe)?", Path(argv[0]).name)
            or record["cwd"] != execution_root or type(record["exit_code"]) is not int or record["exit_code"] != 0):
        raise FreezeError("Python tests require the complete fixed verbose gate suite command")
    executable = record["executable"]
    if (not isinstance(executable, dict) or set(executable) != {"path", "sha256"}
            or executable["path"] != argv[0] or not isinstance(executable["sha256"], str)
            or not HEX.fullmatch(executable["sha256"])):
        raise FreezeError("missing actual Python executable input digest")
    if check_executables:
        path = ordinary_path(Path(argv[0]))
        if not os.access(path, os.X_OK) or sha(read_regular(path, MAX_EXECUTABLE_BYTES)) != executable["sha256"]:
            raise FreezeError("actual Python executable identity mismatch")
    expected = python_test_names(root)
    if record["expected_names"] != expected:
        raise FreezeError("Python expected names do not cover the full reviewed gate suite")
    output = (evidence_log(root, record["stdout"], checked) + b"\n" + evidence_log(root, record["stderr"], checked)).decode("utf-8")
    actual = re.findall(r"^test_[A-Za-z_0-9]+ \((?:__main__\.)?([A-Za-z_][A-Za-z_0-9]*\.test_[A-Za-z_0-9]+)\) \.\.\. ok\r?$", output, re.MULTILINE)
    summaries = re.findall(r"^Ran ([0-9]+) tests? in [^\r\n]+\r?$", output, re.MULTILINE)
    if (sorted(actual) != expected or len(set(actual)) != len(actual) or summaries != [str(len(expected))]
            or len(re.findall(r"^OK\r?$", output, re.MULTILINE)) != 1
            or re.search(r"\.\.\. (?:skipped|expected failure|unexpected success|FAIL|ERROR)|^FAILED|^OK \(", output, re.MULTILINE)):
        raise FreezeError("Python test log has failed, skipped, missing or extra methods")
    return len(expected)


def reference_member(name, references):
    return any(name == item or name.startswith(item + "/") for item in references)


def verify_external_qualifications(root, qualifications, cargo, execution_root, inventory, references, checked, targets):
    if not isinstance(qualifications, dict):
        raise FreezeError("external qualifications must be an object")
    result = {}
    fields = {"manifest", "kind", "target", "expected_names", "reference_inputs", "argv", "cwd", "exit_code", "stdout", "stderr"}
    for key, record in qualifications.items():
        group_key(key)
        if not isinstance(record, dict) or set(record) != fields:
            raise FreezeError("invalid external qualification fields")
        definition = group_definition({name: record[name] for name in ("manifest", "kind", "target", "expected_names")})
        inputs = record["reference_inputs"]
        if (not isinstance(inputs, list) or not inputs or inputs != sorted(set(inputs))
                or definition["manifest"] not in inputs
                or any(name not in inventory or not reference_member(relative_name(name), references) for name in inputs)):
            raise FreezeError("external qualification lacks explicitly reviewed reference inputs")
        test_record = {name: record[name] for name in ("argv", "cwd", "exit_code", "stdout", "stderr")}
        test_command(test_record, definition, cargo, execution_root, targets)
        count = test_log(evidence_log(root, record["stdout"], checked), definition["expected_names"])
        evidence_log(root, record["stderr"], checked)
        result[key] = {"status": "recorded_external_qualification_only", "manifest": definition["manifest"],
                       "recorded_tests_verified": count, "upstream_build_graph_source_closed": False,
                       "production_qualification": False, "os_qualification": False}
    return dict(sorted(result.items()))


def verify_evidence(root, evidence, expected, inventory, checked, *, extra_groups=None, references=(), check_executables=True):
    if not isinstance(evidence, dict) or set(evidence) != {"execution_root", "environment", "tools", "tests", "consumer", "python_tests", "external_qualifications"}:
        raise FreezeError("invalid R2 evidence fields")
    execution_root = evidence["execution_root"]
    if not isinstance(execution_root, str) or not Path(execution_root).is_absolute() or ".." in Path(execution_root).parts:
        raise FreezeError("invalid recorded execution root")
    tools = evidence["tools"]
    if not isinstance(tools, dict) or set(tools) != {"rustc", "cargo", "capnp", "cc"}:
        raise FreezeError("exact rustc/cargo/capnp/cc command identities required")
    for name, record in tools.items():
        tool_identity(record, name, checked, root, check_executables)
    environment = evidence["environment"]
    if not isinstance(environment, dict) or set(environment) != ENVIRONMENT_KEYS or any(value is not None and not isinstance(value, str) for value in environment.values()):
        raise FreezeError("fixed captured build environment identities required")
    if any(environment[name] for name in FORBIDDEN_FLAGS):
        raise FreezeError("unreviewed recorded build flags/compiler target")
    if environment["RUSTC"] not in (None, "", tools["rustc"]["executable"]["path"]):
        raise FreezeError("recorded Rust compiler override differs from its executable identity")
    if environment["CC"] and environment["CC"] != tools["cc"]["executable"]["path"]:
        raise FreezeError("recorded C compiler override lacks its actual executable identity")
    if check_executables and environment != {name: os.environ.get(name) for name in ENVIRONMENT_KEYS}:
        raise FreezeError("current build environment differs from recorded inputs")
    tests = evidence["tests"]
    if not isinstance(tests, dict) or set(tests) != set(expected):
        raise FreezeError("missing or extra fixed test evidence groups")
    groups = extra_groups or {}
    targets = {}
    count = 0
    for key, names in expected.items():
        definition = groups.get(key)
        if definition is None:
            definition = {"manifest": "core/Cargo.toml" if key.startswith("core:") else CRATE + "/Cargo.toml",
                          "kind": "test", "target": key.removeprefix("core:"), "expected_names": names}
        record = tests[key]
        test_command(record, definition, tools["cargo"]["argv"][0], execution_root, targets)
        count += test_log(evidence_log(root, record["stdout"], checked), names)
        evidence_log(root, record["stderr"], checked)
    consumer = evidence["consumer"]
    if Path(consumer["source_root"]).is_relative_to(Path(execution_root)) or Path(consumer["cwd"]).is_relative_to(Path(execution_root)):
        raise FreezeError("consumer must compile independently exported source")
    verify_consumer(root, consumer, tools["cargo"]["argv"][0], inventory, checked)
    python_count = verify_python_tests(root, evidence["python_tests"], execution_root, checked, check_executables)
    external = verify_external_qualifications(root, evidence["external_qualifications"], tools["cargo"]["argv"][0],
        execution_root, inventory, references, checked, targets)
    return {"rust_tests": count, "python_tests": python_count, "external_qualifications": external}


EXPORT_NOTES = """# Reviewed session/safe-exec-basic R2 source bundle

Keep this directory layout: the independent R2 crate references ../../core.
README.md and LICENSE retain repository documentation and licensing. This is
source and recorded evidence, without a dependency cache, production transport,
OS execution-domain qualification, or a full SDK26 freeze.

Verify with an independently reviewed raw manifest SHA256, never a value chosen
by this bundle itself:

    python3 tool/verify_session_exec_r2.py verify --root . --baseline session-exec-r2-manifest.json --expected-pin REVIEWED_SHA256 --recorded-executables-only --json

The recorded-executables-only switch checks historical input digests without
requiring the original machine's executable paths. It performs no compilation
and never authenticates a reviewer or recorded execution.

For a separate client crate, use this dependency (relative to its Cargo.toml):

    morrow-agent-session-exec-v1-r2 = { path = "../source/extensions/agent-session-exec-v1-r2", default-features = false }

Then run cargo check --locked --offline --manifest-path Cargo.toml --target-dir
an-explicit-build-directory using prepared pinned dependencies. A Cargo.lock
must already exist. Host adapters are separate declared source roots; enable
and validate their reviewed feature selection separately. No build outputs are
source inputs, and there is no fallback to an upstream execution backend.
"""


def create_manifest(root: Path, evidence: dict, testexpected: dict, *, core_tests=(),
                    extra_source_roots=(), extra_inputs=(), extra_test_groups=None, r1_archive=None) -> dict:
    """Prepare unreviewed identity; never choose or authenticate a reviewer pin."""
    root = ordinary_path(root, directory=True)
    extra = sorted(extra_source_roots)
    source_roots(extra)
    selected = core_test_paths(core_tests)
    references = sorted(extra_inputs)
    inventory = source_inventory(root, extra, references)
    base_expected = test_expectations(root, testexpected, selected)
    extra_groups = reviewed_test_groups(root, extra_test_groups or {}, inventory, source_roots(extra))
    if set(base_expected) & set(extra_groups):
        raise FreezeError("duplicate base/extra evidence group")
    expected = dict(sorted({**base_expected, **{key: value["expected_names"] for key, value in extra_groups.items()}}.items()))
    checked = {}
    verify_evidence(root, evidence, expected, inventory, checked, extra_groups=extra_groups, references=references)
    historical = verify_r1_archive(root / R1_ARCHIVE if r1_archive is None else r1_archive)
    if r1_archive is not None and ordinary_path(r1_archive) != ordinary_path(root / R1_ARCHIVE):
        raise FreezeError("historical archive must retain its documented relative path")
    if source_inventory(root, extra, references) != inventory:
        raise FreezeError("source closure changed while preparing manifest")
    for path, data in checked.items():
        if read_regular(path, MAX_LOG_BYTES) != data:
            raise FreezeError("evidence changed while preparing manifest")
    return {"schema_version": 2, "profile": PROFILE, "scope": SCOPES.copy(), "bounds": BOUNDS.copy(),
            "contract": contract_identity(root), "core_tests": selected, "extra_source_roots": extra, "extra_inputs": references, "extra_test_groups": extra_groups,
            "source_inventory": inventory, "expected_test_names": expected, "evidence": evidence,
            "history": {"path": R1_ARCHIVE, **historical}, "export_notes_sha256": sha(EXPORT_NOTES.encode())}


def pinned_manifest(baseline: Path, expected_pin: str):
    if not isinstance(expected_pin, str) or not HEX.fullmatch(expected_pin):
        raise FreezeError("explicit externally reviewed pin is mandatory")
    raw = read_regular(baseline, MAX_METADATA_BYTES)
    if sha(raw) != expected_pin:
        raise FreezeError("manifest root pin mismatch; self-rehash cannot replace independent review")
    manifest = document(raw)
    fields = {"schema_version", "profile", "scope", "bounds", "contract", "core_tests", "extra_source_roots", "extra_inputs", "extra_test_groups",
              "source_inventory", "expected_test_names", "evidence", "history", "export_notes_sha256"}
    if set(manifest) != fields or type(manifest["schema_version"]) is not int or manifest["schema_version"] != 2:
        raise FreezeError("unsupported R2 manifest fields/schema")
    if (manifest["profile"] != PROFILE or manifest["scope"] != SCOPES or manifest["bounds"] != BOUNDS
            or any(type(value) is not bool for value in manifest["bounds"].values())):
        raise FreezeError("R2 scope cannot imply SDK26, production transport or OS qualification")
    if manifest["export_notes_sha256"] != sha(EXPORT_NOTES.encode()):
        raise FreezeError("unreviewed export/consumer instructions")
    checked_inventory(manifest["source_inventory"])
    return raw, manifest


def verify_freeze(root: Path, baseline: Path, expected_pin: str, *, check_executables=True) -> dict:
    root = ordinary_path(root, directory=True)
    raw, manifest = pinned_manifest(baseline, expected_pin)
    extra = manifest["extra_source_roots"]
    if not isinstance(extra, list) or extra != sorted(extra):
        raise FreezeError("extra source roots must be exact and sorted")
    references = manifest["extra_inputs"]
    if not isinstance(references, list) or references != sorted(references):
        raise FreezeError("reference inputs must be exact and sorted")
    inventory = source_inventory(root, extra, references)
    if manifest["source_inventory"] != inventory:
        raise FreezeError("source closure has missing, extra or changed inputs")
    contract = contract_identity(root)
    if (manifest["contract"] != contract
            or any(type(manifest["contract"].get(name)) is not int for name in ("version", "revision"))):
        raise FreezeError("raw R2 schema, package or contract identity mismatch")
    selected = core_test_paths(manifest["core_tests"])
    if manifest["core_tests"] != selected:
        raise FreezeError("selected Core tests must be exact and sorted")
    extra_groups = reviewed_test_groups(root, manifest["extra_test_groups"], inventory, source_roots(extra))
    if extra_groups != manifest["extra_test_groups"]:
        raise FreezeError("extra test groups and names must be exact and sorted")
    base_names = {key: names for key, names in manifest["expected_test_names"].items() if key not in extra_groups}
    base_expected = test_expectations(root, base_names, selected)
    expected = dict(sorted({**base_expected, **{key: value["expected_names"] for key, value in extra_groups.items()}}.items()))
    if expected != manifest["expected_test_names"]:
        raise FreezeError("fixed test names must be sorted")
    historical = {"path": R1_ARCHIVE, **verify_r1_archive(root / R1_ARCHIVE)}
    if manifest["history"] != historical:
        raise FreezeError("historical R1 archive identity mismatch")
    checked = {Path(baseline): raw}
    count = verify_evidence(root, manifest["evidence"], expected, inventory, checked,
                            extra_groups=extra_groups, references=references, check_executables=check_executables)
    for path, data in checked.items():
        if read_regular(path, MAX_LOG_BYTES) != data:
            raise FreezeError("pinned evidence changed while verifying")
    if source_inventory(root, extra, references) != inventory:
        raise FreezeError("source closure changed while verifying")
    return {"schema_version": 2, "profile": PROFILE, "status": "local_contract_frozen", "scope": SCOPES.copy(),
            "manifest_sha256": expected_pin, "contract": contract,
            "source_files_verified": len(inventory), "declared_source_roots": source_roots(extra), "reference_inputs": references,
            "recorded_tests_verified": count["rust_tests"], "recorded_python_tests_verified": count["python_tests"],
            "external_qualifications": count["external_qualifications"],
            "recorded_commands_verified": len(manifest["evidence"]["tools"]) + len(expected) + 2 + len(count["external_qualifications"]),
            "recorded_consumer_compilations_verified": 1, "historical_r1": historical,
            "current_executable_inputs_verified": bool(check_executables),
            "executable_verification_scope": "recorded_tool_version_command_binaries_only",
            "tool_identity_scope": "recorded_tool_version_command_binaries_only",
            "evidence_execution_repeated": False, "current_reexecution": False,
            "reviewer_authenticated": False, **BOUNDS}


def bundle_names(manifest):
    return (set(manifest["source_inventory"]) | log_paths(manifest["evidence"])
            | set(manifest["evidence"]["consumer"]["inputs"]) | {R1_ARCHIVE, BUNDLE_BASELINE, "EXPORT.md"})


def write_archive(output: Path, entries):
    output = Path(output)
    ordinary_path(output.parent, directory=True)
    if output.exists() or output.is_symlink():
        raise FreezeError("refuse existing source export output")
    # USTAR, ordinary files only: no symlinks, extracted permissions, ownership,
    # timestamps or extended member metadata control the recipient filesystem.
    buffer = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buffer, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name, body in sorted(entries.items()):
                relative_name(name)
                info = tarfile.TarInfo(name)
                info.size = len(body)
                info.mode = 0o644
                info.mtime = 0
                archive.addfile(info, io.BytesIO(body))
    data = buffer.getvalue()
    if len(data) > MAX_ARCHIVE_BYTES:
        raise FreezeError("source export archive limit exceeded")
    with output.open("xb") as stream:
        stream.write(data)
    return {"path": str(output), "sha256": sha(data), "files": len(entries)}


def export_candidate(root: Path, output: Path, *, extra_source_roots=(), extra_inputs=()) -> dict:
    """Unreviewed preparation bundle for compiling an independent consumer first."""
    root = ordinary_path(root, directory=True)
    inventory = source_inventory(root, extra_source_roots, extra_inputs)
    entries = {name: read_regular(root / name, MAX_FILE_BYTES) for name in inventory}
    if any(sha(entries[name]) != digest for name, digest in inventory.items()):
        raise FreezeError("source bytes changed during candidate export")
    entries["EXPORT.md"] = EXPORT_NOTES.encode()
    entries["candidate-source.json"] = (json.dumps({"status": "unreviewed_source_candidate",
        "scope": SCOPES, "bounds": BOUNDS, "source_inventory": inventory}, sort_keys=True, indent=2) + "\n").encode()
    if source_inventory(root, extra_source_roots, extra_inputs) != inventory:
        raise FreezeError("source closure changed during candidate export")
    result = write_archive(output, entries)
    result["status"] = "unreviewed_source_candidate"
    return result


def export_source(root: Path, baseline: Path, expected_pin: str, output: Path) -> dict:
    result = verify_freeze(root, baseline, expected_pin)
    raw, manifest = pinned_manifest(baseline, expected_pin)
    entries = {}
    for name in bundle_names(manifest) - {BUNDLE_BASELINE, "EXPORT.md"}:
        limit = MAX_ARCHIVE_BYTES if name == R1_ARCHIVE else MAX_LOG_BYTES
        entries[name] = read_regular(root / name, limit)
    entries[BUNDLE_BASELINE] = raw
    entries["EXPORT.md"] = EXPORT_NOTES.encode()
    expected = dict(manifest["source_inventory"])
    expected.update(manifest["evidence"]["consumer"]["inputs"])
    for record in [*manifest["evidence"]["tools"].values(), *manifest["evidence"]["tests"].values(), manifest["evidence"]["consumer"], manifest["evidence"]["python_tests"], *manifest["evidence"]["external_qualifications"].values()]:
        for channel in ("stdout", "stderr"):
            entry = record[channel]
            if entry["path"] in expected and expected[entry["path"]] != entry["sha256"]:
                raise FreezeError("conflicting export input digest")
            expected[entry["path"]] = entry["sha256"]
    expected[R1_ARCHIVE] = R1_ARCHIVE_SHA256
    if any(sha(entries[name]) != digest for name, digest in expected.items()):
        raise FreezeError("source/evidence bytes changed during reviewed export")
    result["export"] = write_archive(output, entries)
    verified = verify_source_archive(output, baseline, expected_pin)
    result["export"]["portable_source_archive_verified"] = verified["portable_source_archive_verified"]
    return result


def verify_source_archive(archive: Path, baseline: Path, expected_pin: str) -> dict:
    raw, manifest = pinned_manifest(baseline, expected_pin)
    entries = archive_entries(archive)
    if set(entries) != bundle_names(manifest) or entries.get(BUNDLE_BASELINE) != raw:
        raise FreezeError("source archive has missing/extra members or a different pinned manifest")
    if entries.get("EXPORT.md") != EXPORT_NOTES.encode():
        raise FreezeError("source archive consumer instructions mismatch")
    # Extraction is only into a disposable directory after rejecting every
    # nonregular/linked/traversal member. No archive permissions are applied.
    with tempfile.TemporaryDirectory(prefix="session-exec-r2-verify-") as temporary:
        root = Path(temporary) / "source"
        root.mkdir()
        for name, body in entries.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(body)
        result = verify_freeze(root, root / BUNDLE_BASELINE, expected_pin, check_executables=False)
    result["archive_sha256"] = sha(read_regular(archive, MAX_ARCHIVE_BYTES))
    result["portable_source_archive_verified"] = True
    return result


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("create", "export-candidate", "verify", "export", "verify-archive", "verify-r1"):
        command = commands.add_parser(name)
        command.add_argument("--json", action="store_true")
        if name not in ("verify-archive", "verify-r1"):
            command.add_argument("--root", type=Path, default=ROOT)
        if name in ("verify", "export", "verify-archive"):
            command.add_argument("--baseline", type=Path, required=True)
            command.add_argument("--expected-pin", required=True)
        if name == "verify":
            command.add_argument("--recorded-executables-only", action="store_true")
        if name in ("create", "export-candidate"):
            command.add_argument("--extra-source-root", action="append", default=[])
            command.add_argument("--extra-input", action="append", default=[])
        if name == "create":
            command.add_argument("--validation", type=Path, required=True,
                                 help="JSON containing evidence, expected_test_names and optional core_tests")
        if name in ("create", "export", "export-candidate"):
            command.add_argument("--output", type=Path, required=True)
        if name in ("verify-archive", "verify-r1"):
            command.add_argument("--source-archive", type=Path, required=True)
    arguments = parser.parse_args(argv)
    try:
        if arguments.command == "create":
            validation = document(read_regular(arguments.validation, MAX_METADATA_BYTES))
            if set(validation) - {"evidence", "expected_test_names", "core_tests", "extra_test_groups"}:
                raise FreezeError("unknown validation preparation fields")
            result = create_manifest(arguments.root, validation["evidence"], validation["expected_test_names"],
                core_tests=validation.get("core_tests", ()), extra_source_roots=arguments.extra_source_root, extra_inputs=arguments.extra_input, extra_test_groups=validation.get("extra_test_groups", {}))
            ordinary_path(arguments.output.parent, directory=True)
            with arguments.output.open("x", encoding="utf-8") as stream:
                stream.write(json.dumps(result, sort_keys=True, indent=2) + "\n")
            result = {"status": "unreviewed_manifest_prepared", "path": str(arguments.output), "scope": SCOPES, **BOUNDS}
        elif arguments.command == "verify":
            result = verify_freeze(arguments.root, arguments.baseline, arguments.expected_pin,
                                   check_executables=not arguments.recorded_executables_only)
        elif arguments.command == "export":
            result = export_source(arguments.root, arguments.baseline, arguments.expected_pin, arguments.output)
        elif arguments.command == "export-candidate":
            result = export_candidate(arguments.root, arguments.output, extra_source_roots=arguments.extra_source_root, extra_inputs=arguments.extra_input)
        elif arguments.command == "verify-archive":
            result = verify_source_archive(arguments.source_archive, arguments.baseline, arguments.expected_pin)
        else:
            result = verify_r1_archive(arguments.source_archive)
    except (OSError, ValueError, TypeError, KeyError) as error:
        print("FAIL:", error, file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True) if arguments.json else result["status"] if "status" in result else "PASS: immutable R1 source/evidence identity verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
