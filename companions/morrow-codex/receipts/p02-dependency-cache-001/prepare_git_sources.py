"""Prepare exact Git directory replacements locally; never invokes Cargo/Git/network."""
from pathlib import Path, PurePosixPath, PureWindowsPath
import datetime as dt
import hashlib
import json
import os
import shutil
import stat
import sys
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[2]
INPUTS = ROOT / "upstream/p02-dependencies-001"
PROOFS = ROOT / "receipts/p02-dependencies-001"
OUTPUT = ROOT / "out/p02-native-probe-001/git-directory-sources"
CONFIG = ROOT / "out/p02-native-probe-001/cargo-home/config.toml"
SPECS = [
    {"id": "tokio-tungstenite", "git": "https://github.com/openai-oss-forks/tokio-tungstenite",
     "rev": "0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186", "version": "0.28.0",
     "source": "tokio-tungstenite-0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186",
     "proof": "tokio-tungstenite-verification.json", "proof_status": "complete_verified",
     "copy_relative": "tokio-tungstenite/tokio-tungstenite-0.28.0", "package_relative": ".",
     "directory_relative": "tokio-tungstenite"},
    {"id": "tungstenite", "git": "https://github.com/openai-oss-forks/tungstenite-rs",
     "rev": "4fffad30fe373adbdcffab9545e9e9bf4f2fc19f", "version": "0.27.0",
     "source": "tungstenite-rs-4fffad30fe373adbdcffab9545e9e9bf4f2fc19f",
     "proof": "tungstenite-rs-verification.json", "proof_status": "complete_verified",
     "copy_relative": "tungstenite/tungstenite-0.27.0", "package_relative": ".",
     "directory_relative": "tungstenite"},
    {"id": "runfiles", "git": "https://github.com/dzbarsky/rules_rust",
     "rev": "b56cbaa8465e74127f1ea216f813cd377295ad81", "version": "0.1.0",
     "source": "rules_rust-b56cbaa8465e74127f1ea216f813cd377295ad81-runfiles-package",
     "proof": "runfiles-package-verification.json", "proof_status": "complete_package_subset_verified",
     "copy_relative": "rules-rust-package", "package_relative": "rust/runfiles",
     "directory_relative": "rules-rust-package/rust"},
]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def safe(path):
    path = Path(os.path.abspath(path))
    path.relative_to(ROOT)
    for item in (*reversed(path.parents), path):
        try:
            metadata = item.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise RuntimeError("Linked/reparse path refused: " + str(item))
    return path


def member(root, name):
    windows = PureWindowsPath(name)
    if windows.drive or windows.root or ".." in PurePosixPath(name).parts or "\\" in name or ":" in name:
        raise RuntimeError("Unsafe source member: " + name)
    path = safe(root / name)
    path.relative_to(root)
    return path


def tree(root):
    safe(root)
    result = {}
    for base, dirs, names in os.walk(root, followlinks=False):
        for name in dirs + names:
            safe(Path(base) / name)
        for name in names:
            path = Path(base) / name
            result[path.relative_to(root).as_posix()] = sha(path)
    return dict(sorted(result.items()))


def verify_original(spec):
    root = safe(INPUTS / spec["source"])
    proof_path = safe(PROOFS / spec["proof"])
    proof = json.loads(proof_path.read_text(encoding="utf-8"))
    if proof["commit"] != spec["rev"] or proof["status"] != spec["proof_status"]:
        raise RuntimeError("Fixed source proof identity/status mismatch")
    expected = {}
    for entry in proof["files"]:
        path = member(root, entry["path"])
        raw = path.read_bytes()
        blob = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
        if entry["path"] in expected or len(raw) != entry["size"] or blob != entry["git_blob_sha1"] or hashlib.sha256(raw).hexdigest() != entry["sha256"]:
            raise RuntimeError("Original source proof mismatch: " + entry["path"])
        expected[entry["path"]] = entry["sha256"]
    if tree(root) != expected:
        raise RuntimeError("Original source file set mismatch: " + spec["id"])
    return root, expected, {"path": str(proof_path), "sha256": sha(proof_path)}


def main():
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    receipt = safe(ROOT / "receipts/p02-dependency-cache-001" / ("git-sources-" + stamp))
    receipt.mkdir(parents=True, exist_ok=False)
    result = {"status": "blocked", "script_sha256": sha(Path(__file__)), "cargo_invoked": False,
              "network_used": False, "original_sources_modified": False,
              "documentation": "https://doc.rust-lang.org/cargo/reference/source-replacement.html",
              "output": str(OUTPUT), "mappings": []}
    originals = []
    try:
        safe(OUTPUT)
        safe(CONFIG)
        if OUTPUT.exists():
            raise RuntimeError("Refusing to overwrite an existing Git directory-source preparation")
        original_config = CONFIG.read_bytes()
        original_hash = hashlib.sha256(original_config).hexdigest()
        parsed_config = tomllib.loads(original_config.decode("utf-8"))
        if parsed_config.get("net", {}).get("offline") is not True:
            raise RuntimeError("Existing isolated config must remain offline")
        (receipt / "config-before.toml").write_bytes(original_config)
        result["config_before_sha256"] = original_hash
        OUTPUT.mkdir()
        additions = ["", "# Exact public Git revisions; source bytes unchanged; prepared by prepare_git_sources.py."]
        new_names = set()
        for spec in SPECS:
            root, expected, proof = verify_original(spec)
            originals.append((root, expected))
            destination = safe(OUTPUT / spec["copy_relative"])
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(root, destination)
            if tree(destination) != expected:
                raise RuntimeError("Copied source differs: " + spec["id"])
            package = safe(destination / spec["package_relative"])
            manifest = tomllib.loads((package / "Cargo.toml").read_text(encoding="utf-8"))
            if manifest["package"]["name"] != spec["id"] or manifest["package"]["version"] != spec["version"]:
                raise RuntimeError("Package identity mismatch: " + spec["id"])
            checksum = package / ".cargo-checksum.json"
            if checksum.exists():
                raise RuntimeError("Original unexpectedly contains Cargo checksum metadata")
            package_files = tree(package)
            checksum.write_text(json.dumps({"package": None, "files": package_files}, sort_keys=True, indent=2) + "\n", encoding="utf-8")
            after = tree(destination)
            checksum_relative = checksum.relative_to(destination).as_posix()
            if set(after) != set(expected) | {checksum_relative} or any(after[name] != digest for name, digest in expected.items()):
                raise RuntimeError("Copy gained changes beyond Cargo checksum metadata")
            source_name = "morrow-p02-fixed-" + spec["id"]
            directory_name = "morrow-p02-directory-" + spec["id"]
            if {source_name, directory_name} & (set(parsed_config.get("source", {})) | new_names):
                raise RuntimeError("Source configuration name collision")
            new_names.update((source_name, directory_name))
            directory = safe(OUTPUT / spec["directory_relative"])
            additions += [f'[source."{source_name}"]', f'git = "{spec["git"]}"', f'rev = "{spec["rev"]}"',
                          f'replace-with = "{directory_name}"', "", f'[source."{directory_name}"]',
                          f'directory = "{directory.as_posix()}"', ""]
            result["mappings"].append({"name": source_name, "git": spec["git"], "rev": spec["rev"],
                "replace_with": directory_name, "directory": str(directory), "package_directory": str(package),
                "original_source": str(root), "original_files": len(expected), "package_checksum_files": len(package_files),
                "only_added_file": str(checksum), "checksum_metadata_sha256": sha(checksum), "proof": proof,
                "source_bytes_unchanged": True, "complete_repository": spec["id"] != "runfiles"})
            (receipt / (spec["id"] + "-files.json")).write_text(json.dumps({"original": expected, "copy": after}, indent=2) + "\n", encoding="utf-8")
        for root, expected in originals:
            if tree(root) != expected:
                raise RuntimeError("Original source changed while preparing copies")
        appended = "\n".join(additions).encode("utf-8")
        updated = original_config + (b"\n" if not original_config.endswith(b"\n") else b"") + appended
        config_after = tomllib.loads(updated.decode("utf-8"))
        for name, section in parsed_config.items():
            if name != "source" and config_after[name] != section:
                raise RuntimeError("Existing config section changed")
        for name, section in parsed_config.get("source", {}).items():
            if config_after["source"][name] != section:
                raise RuntimeError("Existing source replacement changed")
        (receipt / "config-after.toml").write_bytes(updated)
        temporary = safe(CONFIG.parent / ("config.git-sources-" + stamp + ".tmp"))
        temporary.write_bytes(updated)
        if sha(CONFIG) != original_hash:
            raise RuntimeError("Isolated Cargo config changed concurrently; refusing replacement")
        os.replace(temporary, CONFIG)
        result.update({"status": "exact_git_directory_sources_prepared_not_cargo_verified", "config_after_sha256": sha(CONFIG),
                       "config": str(CONFIG), "limits": ["No Cargo resolution/build/runtime was performed", "runfiles is a complete package subset, not the complete rules_rust repository", "Cargo checksum metadata detects accidental changes; original Git blob proofs establish source identity"]})
    except Exception as error:
        result["error"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        result["original_sources_unchanged"] = all(tree(root) == expected for root, expected in originals)
        result["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        (receipt / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"status": result["status"], "receipt": str(receipt / "result.json"), "mappings": result["mappings"], "error": result.get("error")}, ensure_ascii=False, indent=2))
    return 0 if result["status"] == "exact_git_directory_sources_prepared_not_cargo_verified" else 2


if __name__ == "__main__":
    sys.exit(main())
