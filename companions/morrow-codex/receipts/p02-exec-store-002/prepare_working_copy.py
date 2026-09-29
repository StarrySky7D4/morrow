from pathlib import Path
import hashlib
import json
import os
import shutil

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = Path(__file__).resolve().parent
DEST = ROOT / "upstream/p02-exec-store-002"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    if DEST.exists():
        raise RuntimeError("Fresh working copy required")
    DEST.mkdir()
    original = ROOT / "upstream/p02-source-batch-001/codex-source"
    target = DEST / "codex-work"
    shutil.copytree(original, target, symlinks=True)
    manifest_path = ROOT / "receipts/p02-source-batch-001/codex-content-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected = {entry["path"] for entry in manifest["files"]}
    for entry in manifest["files"]:
        source, copied = original / entry["path"], target / entry["path"]
        if entry["mode"] == "120000":
            source_bytes, copied_bytes = os.readlink(source).encode(), os.readlink(copied).encode()
        else:
            source_bytes, copied_bytes = source.read_bytes(), copied.read_bytes()
        if hashlib.sha256(source_bytes).hexdigest() != entry["sha256"] or source_bytes != copied_bytes:
            raise RuntimeError("Fixed source copy differs: " + entry["path"])
    actual = {p.relative_to(target).as_posix() for p in target.rglob("*") if p.is_file() or p.is_symlink()}
    if actual != expected:
        raise RuntimeError("Working-copy file set differs")
    copies = {}
    for source, destination in [
        (ROOT / "sdk/host-kit-003-copy", DEST / "host-kit-003"),
        (ROOT / "out/p02-native-probe-001/build-forks", ROOT / "out/p02-exec-store-002/build-forks"),
    ]:
        shutil.copytree(source, destination)
        records = {}
        for item in source.rglob("*"):
            if item.is_file():
                relative = item.relative_to(source)
                if sha(item) != sha(destination / relative):
                    raise RuntimeError("Input copy differs: " + str(relative))
                records[relative.as_posix()] = sha(item)
        copies[str(destination.relative_to(ROOT))] = records
    result = {
        "status": "new_working_copy_verified_before_patches",
        "fixed_commit": "44fe510ce3ee61c8ef623adcbf89b901c73ddd61",
        "fixed_tree": "3b868fad63be6ac5db91402b579fab37f587d7d5",
        "source_manifest_sha256": sha(manifest_path),
        "working_copy": str(target.relative_to(ROOT)),
        "codex_files": len(expected),
        "other_copies": copies,
        "originals_modified": False,
        "qualification_build_patch_note": "Copied prior exact fork build manifests; tokio dependency path patch retained and will be recorded separately",
    }
    (RECEIPTS / "working-copy-before.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": result["status"], "codex_files": len(expected), "working_copy": str(target)}))


if __name__ == "__main__":
    main()
