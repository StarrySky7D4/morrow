"""Prepare checksum-verified public registry archives in a fresh local cache.

This is dependency preparation, not a resolved build or product test.  The
Cargo.lock graph includes dev, optional and non-Windows packages; it is an
explicit conservative inventory, not the actual target/feature closure.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import tomllib
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "upstream/reference/codex/codex-rs/Cargo.lock"
OUTPUT = ROOT / "out/p02-native-probe-001/cargo-home"
RECEIPT = Path(__file__).with_name("result.json")
REGISTRY = Path(r"C:\Users\Administrator\.cargo\registry")
REGISTRY_ID = "index.crates.io-1949cf8c6b5b557f"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def index_key(name: str) -> str:
    if len(name) == 1:
        return "1/" + name
    if len(name) == 2:
        return "2/" + name
    if len(name) == 3:
        return "3/" + name[0] + "/" + name
    return name[:2] + "/" + name[2:4] + "/" + name


def main() -> None:
    if OUTPUT.exists() or RECEIPT.exists():
        raise SystemExit("Fresh output and receipt are required")
    packages = tomllib.loads(LOCK.read_text(encoding="utf-8"))["package"]
    by_name: dict[str, list[dict]] = {}
    for package in packages:
        by_name.setdefault(package["name"], []).append(package)
    selected: dict[tuple[str, str, str], dict] = {}

    def walk(package: dict) -> None:
        key = (package["name"], package["version"], package.get("source", ""))
        if key in selected:
            return
        selected[key] = package
        for reference in package.get("dependencies", []):
            parts = reference.split(" ", 2)
            candidates = by_name[parts[0]]
            if len(parts) > 1:
                candidates = [p for p in candidates if p["version"] == parts[1]]
            if len(parts) > 2:
                candidates = [p for p in candidates if p.get("source", "") == parts[2][1:-1]]
            if len(candidates) != 1:
                raise RuntimeError(f"Ambiguous lock edge: {reference}")
            walk(candidates[0])

    walk(by_name["codex-api"][0])
    OUTPUT.mkdir(parents=True)
    index = OUTPUT / "registry/index" / REGISTRY_ID
    index.mkdir(parents=True)
    (index / "config.json").write_text(
        '{"dl":"https://static.crates.io/crates","api":"https://crates.io"}\n',
        encoding="utf-8",
    )
    archive_rows = []
    index_rows = {}
    total_bytes = 0
    for key, package in sorted(selected.items()):
        source = package.get("source", "")
        if source != "registry+https://github.com/rust-lang/crates.io-index":
            continue
        name, version = package["name"], package["version"]
        filename = name + "-" + version + ".crate"
        original = REGISTRY / "cache" / REGISTRY_ID / filename
        row = {"name": name, "version": version, "expected_sha256": package["checksum"]}
        if original.is_file():
            actual = digest(original)
            if actual != package["checksum"]:
                raise RuntimeError(f"Public archive checksum mismatch: {filename}")
            total_bytes += original.stat().st_size
            if total_bytes > 2 * 1024**3:
                raise RuntimeError("Cache copy budget exceeded")
            destination = OUTPUT / "registry/cache" / REGISTRY_ID / filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(original, destination)
            if digest(destination) != actual:
                raise RuntimeError(f"Copied archive checksum mismatch: {filename}")
            row.update(status="copied_checksum_verified", bytes=destination.stat().st_size)
        else:
            row["status"] = "missing"
        archive_rows.append(row)
        if name not in index_rows:
            original_index = REGISTRY / "index" / REGISTRY_ID / ".cache" / index_key(name)
            index_row = {"name": name}
            if original_index.is_file():
                destination_index = index / ".cache" / index_key(name)
                destination_index.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(original_index, destination_index)
                index_row.update(status="copied_public_index_metadata", sha256=digest(destination_index))
            else:
                index_row["status"] = "missing"
            index_rows[name] = index_row
    result = {
        "schema_version": 1,
        "status": "partial_dependency_cache_prepared",
        "scope": "Conservative codex-api Cargo.lock graph, including dev/optional/all-target edges; not resolved native closure",
        "lock_path": str(LOCK),
        "lock_sha256": digest(LOCK),
        "entry_sha256": digest(Path(__file__)),
        "cargo_home": str(OUTPUT),
        "network_downloads": 0,
        "personal_configuration_or_credentials_read": False,
        "public_registry_archives_only": True,
        "packages_in_conservative_graph": len(selected),
        "copied_archives": sum(r["status"] == "copied_checksum_verified" for r in archive_rows),
        "missing_archives": sum(r["status"] == "missing" for r in archive_rows),
        "bytes_copied": total_bytes,
        "archives": archive_rows,
        "public_index_metadata": list(index_rows.values()),
        "git_dependencies": [dict(name=p["name"], version=p["version"], source=p["source"]) for p in selected.values() if p.get("source", "").startswith("git+")],
        "completed_utc": datetime.now(timezone.utc).isoformat(),
        "build_passed": False,
        "P-02": "not_claimed",
    }
    RECEIPT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k not in ["archives", "public_index_metadata", "git_dependencies"]}, indent=2))


if __name__ == "__main__":
    main()
