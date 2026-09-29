"""Expand checksum-verified public archives for locked, offline qualification.

The conservative upstream lock graph and reviewed host-kit lock are the only
sources. This does not execute package code or assert a successful build.
"""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / "out/p02-native-probe-001"
VENDOR = BASE / "vendor"
RECEIPT = Path(__file__).with_name("vendor-result.json")
INVENTORY = Path(__file__).with_name("result.json")
KIT_LOCK = ROOT / "sdk/host-kit-003-copy/Cargo.lock"
REGISTRY_ID = "index.crates.io-1949cf8c6b5b557f"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if VENDOR.exists() or RECEIPT.exists():
        raise SystemExit("Fresh vendor attempt required")
    rows = json.loads(INVENTORY.read_text(encoding="utf-8"))["archives"]
    entries = {(r["name"], r["version"]): r["expected_sha256"] for r in rows}
    for p in tomllib.loads(KIT_LOCK.read_text(encoding="utf-8"))["package"]:
        if p.get("source") == "registry+https://github.com/rust-lang/crates.io-index":
            key = (p["name"], p["version"])
            if key in entries and entries[key] != p["checksum"]:
                raise RuntimeError("Conflicting pinned package checksum")
            entries[key] = p["checksum"]
    VENDOR.mkdir(parents=True)
    result_rows = []
    total = 0
    for (name, version), checksum in sorted(entries.items()):
        filename = f"{name}-{version}.crate"
        archive = BASE / "cargo-home/registry/cache" / REGISTRY_ID / filename
        if not archive.is_file():
            prior_archive = ROOT / "out/host-kit-003-consumer/cargo-home/registry/cache" / REGISTRY_ID / filename
            if not prior_archive.is_file():
                raise RuntimeError(f"Exact pinned archive missing: {filename}")
            if sha(prior_archive) != checksum:
                raise RuntimeError(f"Host-kit pinned archive checksum mismatch: {filename}")
            shutil.copyfile(prior_archive, archive)
        if sha(archive) != checksum:
            raise RuntimeError(f"Pinned archive checksum mismatch: {filename}")
        package_root = f"{name}-{version}"
        destination = VENDOR / package_root
        destination.mkdir()
        files = {}
        seen = set()
        package_total = 0
        with tarfile.open(archive, "r:gz") as reader:
            for member in reader:
                path = PurePosixPath(member.name)
                if path.is_absolute() or not path.parts or path.parts[0] != package_root:
                    raise RuntimeError(f"Invalid crate root: {filename}:{member.name}")
                if any(p in ("", ".", "..") or ":" in p or "\\" in p or p.endswith((".", " ")) for p in path.parts):
                    raise RuntimeError(f"Unsafe member path: {filename}:{member.name}")
                if not (member.isdir() or member.isfile()):
                    raise RuntimeError(f"Unsupported crate link/special member: {filename}:{member.name}")
                relative = PurePosixPath(*path.parts[1:])
                if relative == PurePosixPath("."):
                    if not member.isdir():
                        raise RuntimeError("Crate root must be a directory")
                    continue
                key = relative.as_posix().casefold()
                if key in seen:
                    raise RuntimeError(f"Duplicate Windows path: {filename}:{relative}")
                seen.add(key)
                target = destination.joinpath(*relative.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                if member.size < 0 or member.size > 64 * 1024**2:
                    raise RuntimeError("Member byte budget exceeded")
                package_total += member.size
                total += member.size
                if package_total > 512 * 1024**2 or total > 3 * 1024**3:
                    raise RuntimeError("Expansion byte budget exceeded")
                if relative.as_posix() == ".cargo-checksum.json":
                    raise RuntimeError("Archive unexpectedly owns Cargo checksum metadata")
                target.parent.mkdir(parents=True, exist_ok=True)
                with reader.extractfile(member) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
                if target.stat().st_size != member.size:
                    raise RuntimeError("Extracted file length mismatch")
                files[relative.as_posix()] = sha(target)
        (destination / ".cargo-checksum.json").write_text(json.dumps({"files": files, "package": checksum}, separators=(",", ":")) + "\n", encoding="utf-8")
        result_rows.append({"name": name, "version": version, "archive_sha256": checksum, "files": len(files), "expanded_bytes": package_total})
        if len(result_rows) % 100 == 0:
            print(json.dumps({"prepared_packages": len(result_rows), "expanded_bytes": total}), flush=True)
    config = BASE / "cargo-home/config.toml"
    if config.exists():
        raise RuntimeError("Refusing to replace existing Cargo configuration")
    config.write_text('[source.crates-io]\nreplace-with = "qualification-pinned-vendor"\n\n[source.qualification-pinned-vendor]\ndirectory = ' + json.dumps(str(VENDOR).replace("\\", "/")) + '\n\n[net]\noffline = true\n', encoding="utf-8")
    result = {
        "status": "fixed_registry_source_prepared",
        "scope": "Conservative codex-api lock graph plus reviewed host-kit-003 lock; not resolved target/feature closure",
        "entry_sha256": sha(Path(__file__)),
        "inventory_sha256": sha(INVENTORY),
        "host_kit_lock_sha256": sha(KIT_LOCK),
        "generated_config_sha256": sha(config),
        "vendor": str(VENDOR),
        "packages": len(result_rows),
        "expanded_bytes": total,
        "package_records": result_rows,
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "build_passed": False,
        "P-02": "not_claimed",
    }
    RECEIPT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k != "package_records"}, indent=2), flush=True)


if __name__ == "__main__":
    main()
