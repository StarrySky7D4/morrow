"""G0-only host contract preflight, export and verification. No production build."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "contracts/experimental/agent_host_v1"
EXPECTED_HEAD = "88557916aabf2e10619b1022110035178498898a"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, data: object) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")


def safe_child(root: Path, relative: str) -> Path:
    path = root / relative
    if not relative or Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise ValueError(f"unsafe manifest path: {relative}")
    if not path.resolve().is_relative_to(root.resolve()) or path.is_symlink():
        raise ValueError(f"escaped or linked manifest path: {relative}")
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
        if path.is_symlink():
            raise ValueError(f"linked input not supported: {path}")
        if path.is_file():
            relative = path.relative_to(root).as_posix()
            if excluded and relative in excluded:
                continue
            entries.append({"path": relative, "bytes": path.stat().st_size, "sha256": sha(path)})
    return entries


def verify_kit(path: Path) -> dict:
    manifest = json.loads((path / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("status") != "complete" or manifest.get("qualification_only") is not True:
        raise ValueError("not a completed qualification kit")
    entries = manifest.get("files", [])
    if not entries or manifest.get("schema_sha256") != sha(path / "agent_host.capnp"):
        raise ValueError("missing files or schema mismatch")
    seen = set()
    for entry in entries:
        relative = entry["path"]
        if relative == "manifest.json" or relative in seen:
            raise ValueError("duplicate or recursive manifest entry")
        seen.add(relative)
        file = safe_child(path, relative)
        if sha(file) != entry["sha256"] or file.stat().st_size != entry["bytes"]:
            raise ValueError(f"kit input mismatch: {relative}")
    # Additional files can obscure stale/partial copies, so require exact membership.
    actual = {x["path"] for x in inventory(path, {"manifest.json"})}
    if actual != seen:
        raise ValueError("unmanifested kit file")
    required = {"agent_host.capnp", "Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs", "src/fake.rs",
                "generated/agent_host_capnp.rs", "generated/identity.rs", "generated/manifest.json", "vectors/manifest.json"}
    if not required.issubset(seen) or manifest.get("wire_major") != 1 or manifest.get("wire_revision") != 1:
        raise ValueError("incomplete kit shape or unsupported identity")
    return {"status": "verified", "file_count": len(seen), "manifest_sha256": sha(path / "manifest.json"),
            "schema_sha256": manifest["schema_sha256"], "qualification_only": True}


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
            result = verify_kit(args.input.resolve())
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
