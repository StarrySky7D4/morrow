"""G0-only host contract preflight, export and verification. No production build."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import shutil
import stat
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "contracts/experimental/agent_host_v1"
EXPECTED_HEAD = "88557916aabf2e10619b1022110035178498898a"
IMPLEMENTED_FAMILIES = {"native-session-v1", "stream-io-v1", "session-events-v1", "tool-dispatch-v1"}
DRAFT_FAMILIES = {"agent-content-v1", "stream-view-v1", "native-package-v1+bundle-v1"}
# Required qualification corpus for experimental major 1 / revision 1. These
# checks bind coverage labels; recorded outputs do not prove fresh execution.
QUALIFICATION_VECTORS = dict(zip((
    "00-before-hello", "01-hello", "02-open", "03-read-before-commit", "04-write",
    "05-commit", "06-first-chunk", "07-final-chunk", "08-append", "09-append-retry",
    "10-append-conflict", "11-propose", "12-forged-permit", "13-claim", "14-claim-retry",
    "15-report-unknown", "16-drain", "17-drained-claim", "18-observe-exit",
), (
    "denied", "qualification-capabilities", "prepared", "conflict", "prepared",
    "committed", "streaming", "eof-not-model-complete", "tail-1", "original-tail-1",
    "conflict", "proposed-no-authority", "denied", "execute-once", "execute-false",
    "unknown-retained", "closing-unconfirmed", "denied", "unavailable",
)))


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, data: object) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")


def safe_child(root: Path, relative: str) -> Path:
    if ".." in root.parts:
        raise ValueError("kit root must not use parent traversal")
    if (not isinstance(relative, str) or not relative or "\\" in relative or ":" in relative
            or PurePosixPath(relative).is_absolute()
            or any(part in ("", ".", "..") for part in relative.split("/"))):
        raise ValueError(f"unsafe manifest path: {relative}")
    path = root / relative
    for component in (path, *path.parents):
        try:
            info = component.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError(f"escaped or linked manifest path: {relative}")
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f"escaped manifest path: {relative}")
    return path


def verify_inputs(root: Path, baseline: Path) -> dict:
    data = json.loads(baseline.read_text(encoding="utf-8"))
    if data.get("head") != EXPECTED_HEAD or data.get("branch") != "codex/io-safety-refactor":
        raise ValueError("unexpected baseline identity")
    inputs = data.get("legacy_inputs", [])
    if not inputs:
        raise ValueError("baseline has no inputs")
    seen = set()
    for entry in inputs:
        relative = entry["path"]
        if relative in seen:
            raise ValueError("duplicate baseline input")
        seen.add(relative)
        path = safe_child(root, relative)
        if path.stat().st_size != entry["bytes"] or sha(path) != entry["raw_sha256"]:
            raise ValueError(f"changed legacy input: {relative}")
    for entry in data.get("plan_inputs", []):
        path = Path(entry["path"])
        if path.stat().st_size != entry["bytes"] or sha(path) != entry["raw_sha256"]:
            raise ValueError(f"changed plan input: {path.name}")
    return data


def execute(args: list[str], cwd: Path, evidence: Path, name: str, stdin: bytes | None = None) -> str:
    process = subprocess.run(args, cwd=cwd, input=stdin, capture_output=True)
    (evidence / f"{name}.stdout.txt").write_bytes(process.stdout)
    (evidence / f"{name}.stderr.txt").write_bytes(process.stderr)
    write_json(evidence / f"{name}.json", {"args": args, "cwd": str(cwd), "exit_code": process.returncode})
    if process.returncode:
        raise RuntimeError(f"{name} failed with exit {process.returncode}; see evidence")
    return process.stdout.decode("utf-8", errors="replace")


def preflight(baseline: Path) -> dict:
    data = verify_inputs(ROOT, baseline)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=ROOT, text=True).strip()
    if head != data["head"] or branch != data["branch"]:
        raise ValueError("checkout no longer matches baseline")
    required = ["Cargo.toml", "Cargo.lock", "agent_host.capnp", "build.rs", "README.md",
                "src/lib.rs", "src/fake.rs", "examples/vectors.rs", "tests/qualification.rs", "tests/common/mod.rs"]
    for relative in required:
        if not (SOURCE / relative).is_file():
            raise ValueError(f"missing contract input: {relative}")
    versions = {}
    for program, option in [("rustc", "--version"), ("cargo", "--version"), ("capnp", "--version")]:
        versions[program] = subprocess.check_output([program, option], cwd=ROOT, text=True).strip()
    return {"head": head, "branch": branch, "legacy_inputs_verified": len(data["legacy_inputs"]),
            "baseline_sha256": sha(baseline), "toolchain": versions}


def inventory(root: Path, excluded: set[str] | None = None) -> list[dict]:
    entries = []
    for path in sorted(root.rglob("*")):
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError(f"linked input not supported: {path}")
        if not stat.S_ISREG(info.st_mode) and not stat.S_ISDIR(info.st_mode):
            raise ValueError(f"nonregular input not supported: {path}")
        if path.is_file():
            relative = path.relative_to(root).as_posix()
            if excluded and relative in excluded:
                continue
            entries.append({"path": relative, "bytes": path.stat().st_size, "sha256": sha(path)})
    return entries


def read_record(root: Path, relative: str) -> dict:
    path = safe_child(root, relative)
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f"nonregular kit record: {relative}")

    def unique_keys(pairs):
        value = {}
        for key, item in pairs:
            if key in value:
                raise ValueError(f"duplicate JSON key in {relative}: {key}")
            value[key] = item
        return value

    value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_keys)
    if not isinstance(value, dict):
        raise ValueError(f"invalid kit record: {relative}")
    return value


def verify_inventory(root: Path, entries: object) -> set[str]:
    if not isinstance(entries, list) or not entries:
        raise ValueError("missing file inventory")
    seen = set()
    for entry in entries:
        if (not isinstance(entry, dict) or type(entry.get("bytes")) is not int
                or entry["bytes"] < 0 or not isinstance(entry.get("sha256"), str)
                or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise ValueError("invalid file inventory entry")
        relative = entry.get("path")
        file = safe_child(root, relative)
        if relative in seen:
            raise ValueError("duplicate file inventory entry")
        seen.add(relative)
        if (not stat.S_ISREG(file.stat().st_mode) or file.stat().st_size != entry["bytes"]
                or sha(file) != entry["sha256"]):
            raise ValueError(f"kit input mismatch: {relative}")
    return seen


def verify_evidence(root: Path, name: str, program: str, action: str, *, expected_cwd=None, expected_target=None):
    record = read_record(root, f"evidence/{name}.json")
    args = record.get("args")
    if (type(record.get("exit_code")) is not int or record["exit_code"] != 0
            or not isinstance(args, list) or not all(isinstance(arg, str) for arg in args)
            or len(args) < 2 or re.split(r"[/\\]", args[0])[-1] != program
            or args[1] != action or not isinstance(record.get("cwd"), str) or not record["cwd"]):
        raise ValueError(f"missing successful command evidence: {name}")
    for stream in ("stdout", "stderr"):
        file = safe_child(root, f"evidence/{name}.{stream}.txt")
        if not stat.S_ISREG(file.stat().st_mode):
            raise ValueError(f"invalid command output: {name}")
    # Historical command paths can be Windows paths. Check their relationships
    # lexically; never open an absolute path found in untrusted evidence.
    path_type = PureWindowsPath if "\\" in record["cwd"] or ":" in record["cwd"] else PurePosixPath
    cwd = path_type(record["cwd"])
    if not cwd.is_absolute() or ".." in cwd.parts:
        raise ValueError(f"invalid command working directory: {name}")
    if expected_cwd is not None and cwd != expected_cwd:
        raise ValueError(f"command evidence belongs to a different kit: {name}")
    target = None
    if program == "capnp":
        if len(args) != 4 or path_type(args[2]) != cwd / "agent_host.capnp" or args[3] != "Frame":
            raise ValueError(f"wrong schema or root in decode evidence: {name}")
    if program == "cargo":
        prefix = ["--locked", "--offline", "--manifest-path"]
        if args[2:5] != prefix or len(args) < 8 or path_type(args[5]) != cwd / "Cargo.toml":
            raise ValueError(f"unlocked or wrong-manifest command evidence: {name}")
        position = 6
        if name in ("consumer-tests", "generate-vectors"):
            if args[position:position + 2] != ["--features", "qualification"]:
                raise ValueError(f"missing qualification feature evidence: {name}")
            position += 2
        if args[position:position + 1] != ["--target-dir"] or len(args) <= position + 1:
            raise ValueError(f"missing independent target evidence: {name}")
        target = path_type(args[position + 1])
        if (not target.is_absolute() or ".." in target.parts
                or target.is_relative_to(cwd) or cwd.is_relative_to(target)):
            raise ValueError(f"non-independent target evidence: {name}")
        if expected_target is not None and target != expected_target:
            raise ValueError(f"command evidence uses a different target: {name}")
        tail = args[position + 2:]
        if name == "consumer-tests" and tail != ["--message-format=json"]:
            raise ValueError("unexpected consumer test command options")
        if name == "default-no-fake" and tail:
            raise ValueError("default build evidence enables features or unexpected options")
        if name == "generate-vectors" and (len(tail) != 5 or tail[:4] != ["--example", "vectors", "--", "--output"]
                or path_type(tail[4]) != cwd / "vectors"):
            raise ValueError("wrong vector generation command evidence")
    if name == "consumer-tests":
        output = (root / f"evidence/{name}.stdout.txt").read_text(encoding="utf-8")
        outcomes = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", output)
        if (not outcomes or not any(int(passed) > 0 for _, passed, _ in outcomes)
                or any(result != "ok" or int(failed) for result, _, failed in outcomes)):
            raise ValueError("consumer evidence has no successful meaningful tests")
    return cwd, target


def verify_kit(path: Path) -> dict:
    manifest = read_record(path, "manifest.json")
    if manifest.get("status") != "complete" or manifest.get("qualification_only") is not True:
        raise ValueError("not a completed qualification kit")
    schema = safe_child(path, "agent_host.capnp")
    if not stat.S_ISREG(schema.stat().st_mode):
        raise ValueError("nonregular schema input")
    if manifest.get("schema_sha256") != sha(schema):
        raise ValueError("missing files or schema mismatch")
    seen = verify_inventory(path, manifest.get("files"))
    if "manifest.json" in seen:
        raise ValueError("recursive manifest entry")
    # Additional files can obscure stale/partial copies, so require exact membership.
    actual = {x["path"] for x in inventory(path, {"manifest.json"})}
    if actual != seen:
        raise ValueError("unmanifested kit file")
    source_required = {"agent_host.capnp", "Cargo.toml", "Cargo.lock", "build.rs", "README.md", "LICENSE", ".gitattributes",
                       "src/lib.rs", "src/fake.rs", "examples/vectors.rs", "tests/qualification.rs", "tests/common/mod.rs"}
    required = source_required | {"m00-inputs.json", "export-tool-source.py", "generated/agent_host_capnp.rs",
                                  "generated/identity.rs", "generated/manifest.json", "vectors/manifest.json"}
    if (not required.issubset(seen) or any(type(manifest.get(key)) is not int or manifest[key] != 1
                                        for key in ("format", "wire_major", "wire_revision"))):
        raise ValueError("incomplete kit shape or unsupported identity")
    for key, expected in (("implemented_families", IMPLEMENTED_FAMILIES), ("draft_families", DRAFT_FAMILIES)):
        families = manifest.get(key)
        if (not isinstance(families, list) or not all(isinstance(family, str) for family in families)
                or len(families) != len(expected) or set(families) != expected):
            raise ValueError(f"qualification family classification mismatch: {key}")
    source_files = verify_inventory(path, manifest.get("source_files"))
    if (not source_required.issubset(source_files) or not source_files.issubset(seen)
            or any(name.startswith(("generated/", "vectors/", "evidence/", "target/")) for name in source_files)):
        raise ValueError("incomplete or invalid source inventory")
    baseline = manifest.get("baseline")
    original = read_record(path, "m00-inputs.json")
    if (not isinstance(baseline, dict) or baseline.get("head") != EXPECTED_HEAD
            or baseline.get("branch") != "codex/io-safety-refactor"
            or baseline.get("baseline_sha256") != sha(path / "m00-inputs.json")
            or not isinstance(original.get("legacy_inputs"), list) or not original["legacy_inputs"]
            or type(baseline.get("legacy_inputs_verified")) is not int
            or baseline["legacy_inputs_verified"] != len(original["legacy_inputs"])
            or any(baseline.get(key) != original.get(key) for key in ("head", "branch"))):
        raise ValueError("export baseline identity mismatch")
    generated = read_record(path, "generated/manifest.json")
    if (generated.get("source") != "agent_host.capnp" or generated.get("source_sha256") != manifest["schema_sha256"]
            or generated.get("generator") != baseline.get("toolchain")
            or not isinstance(generated.get("generator"), dict)
            or any(not isinstance(generated["generator"].get(key), str) or not generated["generator"][key]
                   for key in ("rustc", "cargo", "capnp"))
            or verify_inventory(path / "generated", generated.get("files")) != {"agent_host_capnp.rs", "identity.rs"}):
        raise ValueError("generated bindings identity mismatch")
    expected_identity = f"pub const SCHEMA_DIGEST: [u8; 32] = {list(bytes.fromhex(manifest['schema_sha256']))};\n"
    if (path / "generated/identity.rs").read_text(encoding="utf-8") != expected_identity:
        raise ValueError("generated schema digest mismatch")
    evidence_cwd, evidence_target = verify_evidence(path, "consumer-tests", "cargo", "test")
    for name, action in (("default-no-fake", "check"), ("generate-vectors", "run")):
        verify_evidence(path, name, "cargo", action, expected_cwd=evidence_cwd, expected_target=evidence_target)
    vectors = read_record(path, "vectors/manifest.json")
    if (type(vectors.get("format")) is not int or vectors["format"] != 1
            or vectors.get("qualification_only") is not True or vectors.get("schema_sha256") != manifest["schema_sha256"]
            or not isinstance(vectors.get("vectors"), list) or not vectors["vectors"]
            or not isinstance(vectors.get("rejections"), list) or not vectors["rejections"]):
        raise ValueError("incomplete vector identity or coverage")
    vector_files = {"manifest.json"}
    names = set()
    for vector in vectors["vectors"]:
        if (not isinstance(vector, dict) or not isinstance(vector.get("name"), str) or not vector["name"]
                or vector["name"] in names or QUALIFICATION_VECTORS.get(vector["name"]) != vector.get("expected")):
            raise ValueError("invalid or duplicate vector")
        names.add(vector["name"])
        for direction in ("request", "reply"):
            relative = vector.get(direction)
            if relative != f"{vector['name']}.{direction}.capnp" or relative in vector_files:
                raise ValueError("invalid or duplicate vector wire")
            verify_inventory(path / "vectors", [{"path": relative, "sha256": vector.get(f"{direction}_sha256"),
                "bytes": safe_child(path / "vectors", relative).stat().st_size}])
            vector_files.add(relative)
            verify_evidence(path, f"decode-{relative}", "capnp", "decode", expected_cwd=evidence_cwd)
    if names != set(QUALIFICATION_VECTORS):
        raise ValueError("incomplete qualification vector coverage")
    if len(vectors["rejections"]) != 1:
        raise ValueError("incomplete unsupported-version rejection coverage")
    for vector in vectors["rejections"]:
        if (not isinstance(vector, dict) or vector.get("error") != "UnsupportedVersion"
                or vector.get("request") != "unknown-version.request.capnp"):
            raise ValueError("missing unsupported-version rejection")
        relative = vector.get("request")
        if relative in vector_files:
            raise ValueError("duplicate rejection wire")
        verify_inventory(path / "vectors", [{"path": relative, "sha256": vector.get("sha256"),
            "bytes": safe_child(path / "vectors", relative).stat().st_size}])
        vector_files.add(relative)
    if vector_files != {entry["path"] for entry in inventory(path / "vectors")}:
        raise ValueError("unmanifested vector file")
    return {"status": "verified", "file_count": len(seen), "manifest_sha256": sha(path / "manifest.json"),
            "schema_sha256": manifest["schema_sha256"], "qualification_only": True,
            "verification_scope": "qualification_kit_integrity", "evidence_execution_verified": False,
            "sdk_frozen": False}


def export_sdk(baseline: Path, output: Path, target: Path) -> dict:
    output = output.resolve()
    target = target.resolve()
    if output.exists():
        raise ValueError("output already exists; choose a new directory (no old artifact reuse)")
    if target.is_relative_to(output) or output.is_relative_to(target):
        raise ValueError("target and kit output must be separate")
    status = preflight(baseline)  # Failure here creates no candidate or success marker.
    source_files = inventory(SOURCE)
    output.mkdir(parents=True, exist_ok=False)
    evidence = output / "evidence"
    evidence.mkdir()
    try:
        for entry in source_files:
            relative = entry["path"]
            if relative.startswith(("target/", "vectors/", "generated/")) or "__pycache__" in relative:
                raise ValueError(f"unexpected canonical source artifact: {relative}")
            destination = safe_child(output, relative)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(safe_child(SOURCE, relative), destination)
        shutil.copyfile(baseline, output / "m00-inputs.json")
        shutil.copyfile(Path(__file__), output / "export-tool-source.py")
        # Build the EXPORTED consumer copy, not the canonical tree. Locked/offline only.
        cargo = ["cargo", "test", "--locked", "--offline", "--manifest-path", str(output / "Cargo.toml"),
                 "--features", "qualification", "--target-dir", str(target), "--message-format=json"]
        result = execute(cargo, output, evidence, "consumer-tests")
        out_dirs = [Path(obj["out_dir"]) for line in result.splitlines() if line.startswith("{")
                    for obj in [json.loads(line)] if obj.get("reason") == "build-script-executed"
                    and "morrow-agent-host-contract" in obj.get("package_id", "")]
        if not out_dirs:
            raise ValueError("no matching Cargo build-script output; cannot identify generated bindings")
        generated = output / "generated"
        generated.mkdir()
        for name in ["agent_host_capnp.rs", "identity.rs"]:
            shutil.copyfile(out_dirs[-1] / name, generated / name)
        write_json(generated / "manifest.json", {"source": "agent_host.capnp", "source_sha256": sha(output / "agent_host.capnp"),
                                                "generator": status["toolchain"], "files": inventory(generated)})
        execute(["cargo", "check", "--locked", "--offline", "--manifest-path", str(output / "Cargo.toml"),
                 "--target-dir", str(target)], output, evidence, "default-no-fake")
        execute(["cargo", "run", "--locked", "--offline", "--manifest-path", str(output / "Cargo.toml"),
                 "--features", "qualification", "--target-dir", str(target), "--example", "vectors", "--", "--output", str(output / "vectors")],
                output, evidence, "generate-vectors")
        vectors = json.loads((output / "vectors/manifest.json").read_text(encoding="utf-8"))
        # Independent Cap'n Proto compiler consumption proves both byte directions.
        for vector in vectors["vectors"]:
            for direction in ["request", "reply"]:
                name = vector[direction]
                raw = (output / "vectors" / name).read_bytes()
                execute(["capnp", "decode", str(output / "agent_host.capnp"), "Frame"], output,
                        evidence, f"decode-{name}", raw)
        verify_inputs(ROOT, baseline)
        if source_files != inventory(SOURCE):
            raise ValueError("canonical source changed during export")
        # Also ensure Cargo and consumer build did not mutate distributed lock/source.
        for entry in source_files:
            if sha(output / entry["path"]) != entry["sha256"]:
                raise ValueError("consumer build changed a copied source input")
        manifest = {"format": 1, "status": "complete", "qualification_only": True, "run_id": str(uuid.uuid4()),
                    "wire_major": 1, "wire_revision": 1, "schema_sha256": sha(output / "agent_host.capnp"),
                    "authority": str(SOURCE), "source_files": source_files, "baseline": status,
                    "implemented_families": ["native-session-v1", "stream-io-v1", "session-events-v1", "tool-dispatch-v1"],
                    "draft_families": ["agent-content-v1", "stream-view-v1", "native-package-v1+bundle-v1"],
                    "limitations": ["No production backend", "No G0 acceptance claim", "No Dart/C bindings", "No durable store", "No real A/B or login"],
                    "files": inventory(output)}
        write_json(output / "manifest.json", manifest)
        return verify_kit(output)
    except Exception as error:
        # A failed candidate remains inspectable; never create a completed manifest.
        if (output / "manifest.json").exists():
            (output / "manifest.json").rename(output / "failed-manifest.json")
        write_json(output / "failure.json", {"status": "failed", "error": str(error)})
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("preflight")
    p.add_argument("--baseline", required=True, type=Path)
    p.add_argument("--receipt", required=True, type=Path)
    p = commands.add_parser("export-sdk")
    p.add_argument("--baseline", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--target-dir", required=True, type=Path)
    p = commands.add_parser("verify-kit")
    p.add_argument("--input", required=True, type=Path)
    args = parser.parse_args()
    try:
        if args.command == "preflight":
            result = preflight(args.baseline.resolve())
            write_json(args.receipt, result)
        elif args.command == "export-sdk":
            result = export_sdk(args.baseline.resolve(), args.output, args.target_dir)
        else:
            result = verify_kit(args.input)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
