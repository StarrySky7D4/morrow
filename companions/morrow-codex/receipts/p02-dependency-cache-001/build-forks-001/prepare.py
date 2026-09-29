"""One declared manifest-only qualification patch; no Cargo or network calls."""
from pathlib import Path
import datetime as dt
import difflib
import hashlib
import json
import os
import shutil
import stat
import sys

ROOT = Path(__file__).resolve().parents[3]
RECEIPT = Path(__file__).resolve().parent
DEST = ROOT / "out/p02-native-probe-001/build-forks"
ORIGINAL = ROOT / "upstream/p02-dependencies-001"
PROOFS = ROOT / "receipts/p02-dependencies-001"
SPECS = [
    ("tokio-tungstenite", "tokio-tungstenite-0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186", "tokio-tungstenite-verification.json", ""),
    ("tungstenite", "tungstenite-rs-4fffad30fe373adbdcffab9545e9e9bf4f2fc19f", "tungstenite-rs-verification.json", ""),
    ("runfiles", "rules_rust-b56cbaa8465e74127f1ea216f813cd377295ad81-runfiles-package", "runfiles-package-verification.json", "rust/runfiles/"),
]


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def safe(path):
    path = Path(os.path.abspath(path))
    path.relative_to(ROOT)
    for item in (*reversed(path.parents), path):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise RuntimeError("Linked/reparse path refused: " + str(item))
    return path


def snapshot(path):
    result = {}
    for base, dirs, files in os.walk(safe(path), followlinks=False):
        for name in dirs + files:
            safe(Path(base) / name)
        for name in files:
            file = Path(base) / name
            result[file.relative_to(path).as_posix()] = sha(file.read_bytes())
    return dict(sorted(result.items()))


def main():
    result = {"status": "blocked", "scope": "manifest-only dependency-source qualification patch", "cargo_invoked": False,
              "network_used": False, "script_sha256": sha(Path(__file__).read_bytes()), "packages": []}
    source_snapshots = []
    try:
        safe(DEST)
        if DEST.exists() or (RECEIPT / "result.json").exists():
            raise RuntimeError("Refusing to overwrite prior build-fork preparation")
        DEST.mkdir()
        for name, dirname, proof_name, prefix in SPECS:
            proof_path = safe(PROOFS / proof_name)
            proof = json.loads(proof_path.read_text(encoding="utf-8"))
            source = safe(ORIGINAL / dirname / prefix)
            expected = {}
            blob_records = []
            for entry in proof["files"]:
                if not entry["path"].startswith(prefix):
                    continue
                relative = entry["path"][len(prefix):]
                file = safe(source / relative)
                file.relative_to(source)
                raw = file.read_bytes()
                blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
                if sha(raw) != entry["sha256"] or blob != entry["git_blob_sha1"] or len(raw) != entry["size"]:
                    raise RuntimeError("Fixed original source mismatch: " + name + "/" + relative)
                expected[relative] = entry["sha256"]
                blob_records.append({"path": relative, "git_blob_sha1": blob, "fixed_sha256": entry["sha256"]})
            if snapshot(source) != expected or (name == "runfiles" and len(expected) != 5):
                raise RuntimeError("Unexpected fixed source file set")
            source_snapshots.append((source, expected))
            destination = DEST / name
            shutil.copytree(source, destination)
            original_manifest = (source / "Cargo.toml").read_bytes()
            (RECEIPT / (name + "-Cargo.toml.original")).write_bytes(original_manifest)
            if name == "tokio-tungstenite":
                before = (b'[dependencies.tungstenite]\n'
                          b'git = "https://github.com/openai-oss-forks/tungstenite-rs"\n'
                          b'rev = "4fffad30fe373adbdcffab9545e9e9bf4f2fc19f"\n')
                after = b'[dependencies.tungstenite]\npath = "../tungstenite"\n'
                if original_manifest.count(before) != 1:
                    raise RuntimeError("Exact nested source stanza absent or duplicated; no heuristic edit")
                patched = original_manifest.replace(before, after, 1)
                (destination / "Cargo.toml").write_bytes(patched)
                diff = "".join(difflib.unified_diff(original_manifest.decode("utf-8").splitlines(keepends=True),
                    patched.decode("utf-8").splitlines(keepends=True),
                    fromfile="fixed-upstream/tokio-tungstenite/Cargo.toml", tofile="qualification-build-forks/tokio-tungstenite/Cargo.toml"))
                (RECEIPT / "tokio-tungstenite-manifest.patch").write_text(diff, encoding="utf-8", newline="")
            actual = snapshot(destination)
            if set(actual) != set(expected):
                raise RuntimeError("Copy file set changed")
            changed = [path for path in expected if actual[path] != expected[path]]
            if changed != (["Cargo.toml"] if name == "tokio-tungstenite" else []):
                raise RuntimeError("Unexpected source change in build copy")
            for entry in blob_records:
                entry["copy_sha256"] = actual[entry["path"]]
                entry["fixed_bytes_unchanged"] = actual[entry["path"]] == entry["fixed_sha256"]
            (RECEIPT / (name + "-files.json")).write_text(json.dumps(blob_records, indent=2) + "\n", encoding="utf-8")
            result["packages"].append({"name": name, "commit": proof["commit"], "original": str(source), "copy": str(destination),
                "file_count": len(expected), "changed_files": changed, "manifest_before_sha256": sha(original_manifest),
                "manifest_after_sha256": actual["Cargo.toml"], "all_rust_sources_unchanged": all(actual[p] == expected[p] for p in expected if p.endswith(".rs")),
                "all_non_manifest_files_match_fixed_blobs": all(actual[p] == expected[p] for p in expected if p != "Cargo.toml"),
                "proof_path": str(proof_path), "proof_sha256": sha(proof_path.read_bytes())})
        if not all(snapshot(source) == expected for source, expected in source_snapshots):
            raise RuntimeError("Original source changed while preparing copy")
        result.update({"status": "declared_manifest_patch_prepared_not_cargo_verified", "original_sources_unchanged": True,
                       "manifest_patch_sha256": sha((RECEIPT / "tokio-tungstenite-manifest.patch").read_bytes()),
                       "source_identity_limit": "Build copy is explicitly patched; it is not an unmodified upstream Git source",
                       "lock_source_id_change": "Root will switch these build dependencies to path identities; no lock was changed by this script"})
    except Exception as error:
        result["error"] = {"type": type(error).__name__, "message": str(error)}
    result["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    (RECEIPT / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["status"] == "declared_manifest_patch_prepared_not_cargo_verified" else 2


if __name__ == "__main__":
    sys.exit(main())
