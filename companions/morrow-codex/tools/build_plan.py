#!/usr/bin/env python3
"""Local, read-only input qualification. No builds or downstream stages yet.

Source lock v1 is plugin-owned provenance, never a replacement host Schema.
Every invocation reserves a fresh local output directory before checking inputs.
Only fixed read-only Git/tool-version commands are executed, with no shell.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
import uuid

STAGES = ("preflight", "generate", "deps", "test", "build", "inspect", "pack",
          "bundle", "qualify", "release-plan")
PINNED = {
    "codex": ("https://github.com/openai/codex", "44fe510ce3ee61c8ef623adcbf89b901c73ddd61"),
    "cc-switch": ("https://github.com/farion1231/cc-switch", "846de29c13ac4d65f164db8c15dd5fd58e29f972"),
}


class Blocked(Exception):
    def __init__(self, code: str, message: str):
        self.code, self.message = code, message
        super().__init__(message)


def fail(code, message):
    raise Blocked(code, message)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def no_links(path: Path):
    # Resolve nothing until every existing component is checked. Windows junctions
    # and other reparse points are rejected, even if they currently stay in-root.
    for component in (*reversed(path.parents), path):
        try:
            info = component.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            fail("path_link", f"Linked/reparse path is not accepted: {component}")


def path_at(root: Path, value, *, external=False, must_exist=True) -> Path:
    if not isinstance(value, str) or not value.strip():
        fail("path_missing", "A required path is absent")
    candidate = Path(value)
    if not candidate.is_absolute():
        candidate = root / candidate
    candidate = Path(os.path.abspath(candidate))
    no_links(candidate)
    if not external:
        try:
            candidate.relative_to(root)
        except ValueError:
            fail("path_escape", f"Path leaves plugin repository: {value}")
    if must_exist and not candidate.exists():
        fail("input_missing", f"Required input is missing: {candidate}")
    return candidate


def child_at(root: Path, value) -> Path:
    if not isinstance(value, str) or Path(value).is_absolute():
        fail("path_escape", "Source member paths must be relative")
    return path_at(root, value)


def read_json(path: Path):
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, ValueError) as error:
        fail("invalid_json", f"Cannot read JSON {path}: {error}")
    if not isinstance(value, dict):
        fail("invalid_json", f"Expected JSON object: {path}")
    return value


def digest_file(path: Path, expected, label):
    if not isinstance(expected, str) or not re.fullmatch(r"[a-f0-9]{64}", expected):
        fail("digest_missing", f"Missing/invalid SHA-256 for {label}")
    actual = sha256(path)
    if actual != expected:
        fail("digest_mismatch", f"SHA-256 mismatch: {label}")
    return actual


def run_fixed(args, cwd: Path, *, toolchain=None):
    # -C does not override inherited Git repository/config redirections.
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("GIT_")}
    env.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                "GIT_OPTIONAL_LOCKS": "0", "GIT_TERMINAL_PROMPT": "0",
                "GIT_NO_LAZY_FETCH": "1", "RUSTUP_AUTO_INSTALL": "0"})
    if toolchain:
        env["RUSTUP_TOOLCHAIN"] = toolchain
    try:
        result = subprocess.run(args, cwd=cwd, env=env, capture_output=True,
                                timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        fail("tool_unavailable", f"{args[0]} failed: {type(error).__name__}")
    if result.returncode:
        fail("tool_failed", f"{args[0]} returned {result.returncode}")
    return result.stdout


def git(root, *args):
    return run_fixed(["git", "--no-optional-locks", "-c", "core.fsmonitor=false",
                      "-c", "core.untrackedCache=false", "-C", str(root), *args], root)


def normalize_url(value):
    return str(value).removesuffix("/").removesuffix(".git")


def tree_files(root):
    files = {}
    for base, dirs, names in os.walk(root, followlinks=False):
        for name in (*dirs, *names):
            item = Path(base) / name
            no_links(item)
        for name in names:
            item = Path(base) / name
            if not item.is_file():
                fail("unsupported_file", f"Source member is not a regular file: {item}")
            files[item.relative_to(root).as_posix()] = sha256(item)
    return files


def manifest_entries(document):
    if document.get("schema_version") != 1 or not isinstance(document.get("files"), list):
        fail("manifest_invalid", "Expected files manifest v1")
    output = {}
    for entry in document["files"]:
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str):
            fail("manifest_invalid", "Invalid files manifest entry")
        key = entry["path"]
        if key in output or not re.fullmatch(r"[a-f0-9]{64}", str(entry.get("sha256"))):
            fail("manifest_invalid", "Duplicate path or invalid digest in files manifest")
        output[key] = entry["sha256"]
    if not output:
        fail("manifest_invalid", "Source files manifest cannot be empty")
    return output


def verify_archive_members(archive_path, source, expected):
    """Tie the extracted tree to original bytes without extracting any member."""
    top = normalize_url(source["url"]).rsplit("/", 1)[-1] + "-" + source["commit"]
    actual, names, total = {}, set(), 0
    try:
        with tarfile.open(archive_path, mode="r|gz") as archive:
            for member in archive:
                name = member.name.rstrip("/")
                parts = PurePosixPath(name).parts
                if not name or "\\" in name or name.startswith("/") or name != "/".join(parts) or any(part in (".", "..") for part in name.split("/")):
                    fail("archive_path", "Unsafe archive member path")
                if not parts or parts[0] != top or name in names:
                    fail("archive_path", "Archive root or duplicate member does not match fixed source")
                names.add(name)
                if member.isdir():
                    continue
                if not member.isreg() or len(parts) < 2:
                    fail("archive_member", "Only regular files/directories are accepted in source archives")
                total += member.size
                if member.size > 512 * 1024 * 1024 or total > 2 * 1024 * 1024 * 1024:
                    fail("archive_budget", "Source archive exceeds first-slice inspection budget")
                stream = archive.extractfile(member)
                if stream is None:
                    fail("archive_member", "Cannot read source archive member")
                h = hashlib.sha256()
                for data in iter(lambda: stream.read(1024 * 1024), b""):
                    h.update(data)
                actual["/".join(parts[1:])] = h.hexdigest()
    except (tarfile.TarError, EOFError) as error:
        fail("archive_invalid", f"Unreadable source archive: {type(error).__name__}")
    if actual != expected:
        fail("archive_tree_mismatch", "Recorded file manifest does not match the original archive members")


def verify_dirty(repo, source, source_root):
    patch = git(source_root, "diff", "--no-ext-diff", "--no-textconv", "--binary", "HEAD", "--")
    untracked = git(source_root, "ls-files", "--others", "--exclude-standard", "-z").split(b"\0")
    # Include ignored files: they can change build behavior and must be recorded.
    ignored = git(source_root, "ls-files", "--others", "--ignored", "--exclude-standard", "-z").split(b"\0")
    actual_untracked = {}
    for raw in untracked + ignored:
        if raw:
            name = os.fsdecode(raw)
            file = child_at(source_root, name)
            actual_untracked[name.replace("\\", "/")] = sha256(file)
    dirty = bool(patch or actual_untracked)
    recorded = source.get("dirty", {})
    if not isinstance(recorded, dict):
        fail("dirty_unrecorded", "dirty must be an object")
    if dirty and recorded.get("allowed") is not True:
        fail("dirty_unrecorded", f"Unrecorded dirty source: {source['id']}")
    if recorded.get("allowed") is True:
        patch_path = path_at(repo, recorded.get("patch_path"))
        digest_file(patch_path, recorded.get("patch_sha256"), "dirty patch")
        if patch_path.read_bytes() != patch:
            fail("dirty_patch_mismatch", "Recorded patch does not match current tracked changes")
        entries = recorded.get("untracked")
        if not isinstance(entries, list):
            fail("dirty_unrecorded", "Dirty source requires an explicit untracked digest list")
        expected = {}
        for entry in entries:
            if not isinstance(entry, dict) or entry.get("path") in expected:
                fail("dirty_unrecorded", "Invalid untracked digest list")
            expected[entry.get("path")] = entry.get("sha256")
        if expected != actual_untracked:
            fail("dirty_untracked_mismatch", "Untracked/ignored file set or digest differs")
    return {"dirty": dirty, "tracked_diff_sha256": hashlib.sha256(patch).hexdigest(),
            "untracked": actual_untracked}


def verify_source(repo, source):
    if not isinstance(source, dict) or source.get("id") not in PINNED:
        fail("source_id", "Expected the pinned codex or cc-switch source")
    name = source["id"]
    url, commit = PINNED[name]
    if normalize_url(source.get("url")) != url or source.get("commit") != commit:
        fail("source_pin", f"Unexpected source URL or commit: {name}")
    root = path_at(repo, source.get("path"))
    if not root.is_dir():
        fail("source_missing", f"Source is not a directory: {name}")
    kind = source.get("kind", "git")
    details = {"id": name, "kind": kind, "path": str(root), "url": url, "commit": commit}
    if kind == "git":
        if Path(git(root, "rev-parse", "--show-toplevel").decode().strip()) != root:
            fail("source_root", "Source path must be the Git working tree root")
        if git(root, "rev-parse", "HEAD").decode().strip() != commit:
            fail("source_commit", f"Git HEAD mismatch: {name}")
        origin = git(root, "remote", "get-url", "origin").decode().strip()
        if normalize_url(origin) != url:
            fail("source_origin", f"Git origin mismatch: {name}")
        # Reject links in the checked-out tracked set as well as untracked files.
        for raw in git(root, "ls-files", "-z").split(b"\0"):
            if raw:
                no_links(root / os.fsdecode(raw))
        details.update(verify_dirty(repo, source, root))
    elif kind == "archive":
        archive = source.get("archive", {})
        archive_path = path_at(repo, archive.get("path"))
        details["archive_sha256"] = digest_file(archive_path, archive.get("sha256"), name + " archive")
        manifest = source.get("files_manifest", {})
        manifest_path = path_at(repo, manifest.get("path"))
        details["files_manifest_sha256"] = digest_file(manifest_path, manifest.get("sha256"), name + " files manifest")
        expected = manifest_entries(read_json(manifest_path))
        verify_archive_members(archive_path, source, expected)
        for member in expected:
            child_at(root, member)
        actual = tree_files(root)
        if actual != expected:
            fail("source_tree_mismatch", f"Source tree differs from recorded archive file set: {name}")
        details.update({"dirty": False, "files_verified": len(actual),
                        "provenance_limit": "Archive receipt pins bytes; it does not independently authenticate GitHub."})
    elif kind == "partial_snapshot":
        manifest = source.get("files_manifest", {})
        manifest_path = path_at(repo, manifest.get("path"))
        details["files_manifest_sha256"] = digest_file(manifest_path, manifest.get("sha256"), name + " partial manifest")
        document = read_json(manifest_path)
        expected = manifest_entries(document)
        for member in expected:
            child_at(root, member)
        if tree_files(root) != expected:
            fail("source_tree_mismatch", f"Partial snapshot differs from recorded file set: {name}")
        blobs_verified = 0
        for entry in document["files"]:
            if "git_blob_sha1" in entry:
                content = child_at(root, entry["path"]).read_bytes()
                blob = hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content).hexdigest()
                if blob != entry["git_blob_sha1"]:
                    fail("source_blob_mismatch", f"Git blob digest differs: {name}/{entry['path']}")
                blobs_verified += 1
        details.update({"complete": False, "files_verified": len(expected), "git_blobs_verified": blobs_verified,
                        "dirty": "unavailable_without_complete_original", "qualification": "partial_snapshot_only"})
    else:
        fail("source_kind", f"Unsupported source kind: {kind}")
    licenses = source.get("license_files")
    if not isinstance(licenses, list) or not licenses:
        fail("license_missing", f"No license evidence: {name}")
    details["licenses"] = []
    for license_file in licenses:
        path = child_at(root, license_file.get("path"))
        digest = digest_file(path, license_file.get("sha256"), name + " license")
        details["licenses"].append({"path": license_file["path"], "sha256": digest})
    return details


def check_baseline(repo, lock):
    baseline = lock.get("baseline")
    if not isinstance(baseline, dict):
        fail("baseline_missing", "Missing Morrow baseline candidate")
    path = path_at(repo, baseline.get("path"), external=True)
    head = git(path, "rev-parse", "HEAD").decode().strip()
    branch = git(path, "branch", "--show-current").decode().strip()
    if head != baseline.get("commit") or branch != baseline.get("branch"):
        fail("baseline_mismatch", "Morrow baseline candidate HEAD/branch mismatch")
    return {"path": str(path), "commit": head, "branch": branch,
            "status": "candidate_only", "m00_delivery_verified": False}


def check_kit(repo, args):
    if not args.host_kit:
        fail("kit_missing", "A reviewed host-kit has not been supplied")
    kit = path_at(repo, args.host_kit, external=True)
    if not kit.is_dir():
        fail("kit_missing", "Host-kit must be an existing directory")
    manifest = child_at(kit, args.host_kit_manifest)
    digest_file(manifest, args.host_kit_sha256, "host-kit manifest")
    if not args.host_kit_review:
        fail("kit_unreviewed", "A digest-bound local host-kit review is required")
    review_path = path_at(repo, args.host_kit_review)
    review = read_json(review_path)
    if review.get("manifest_sha256") != args.host_kit_sha256 or review.get("status") not in ("ready", "qualification_only"):
        fail("kit_unreviewed", "Review must bind this manifest digest and readiness")
    return {"path": str(kit), "manifest": str(manifest), "manifest_sha256": args.host_kit_sha256,
            "review_sha256": sha256(review_path), "review_status": review["status"],
            "schema_validation": "not_implemented", "scope": "manifest_digest_and_local_review_binding_only"}


def check_graphs(repo, contract):
    # This is intentionally a lock/availability check, never cargo metadata/build.
    toolchain = contract.get("toolchain_lock")
    if not isinstance(toolchain, dict):
        fail("toolchain_lock_missing", "Plugin toolchain lock has not been established")
    lock_path = path_at(repo, toolchain.get("path"))
    digest_file(lock_path, toolchain.get("sha256"), "toolchain lock")
    lock = read_json(lock_path)
    toolchain_name = lock.get("toolchain")
    if not isinstance(toolchain_name, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*", toolchain_name):
        fail("toolchain_lock_invalid", "An explicit installed toolchain name is required")
    available = run_fixed(["rustup", "toolchain", "list"], repo).decode().splitlines()
    if toolchain_name not in {line.split()[0] for line in available if line.strip()}:
        fail("toolchain_missing", "The pinned toolchain is not installed; no installation was attempted")
    versions = {}
    for tool in ("rustup", "rustc", "cargo"):
        command = ["rustup", "--version"] if tool == "rustup" else ["rustup", "run", toolchain_name, tool, "--version"]
        value = run_fixed(command, repo, toolchain=toolchain_name).decode("utf-8", errors="replace").strip()
        if value != lock.get("versions", {}).get(tool):
            fail("toolchain_mismatch", f"Installed {tool} version differs from lock")
        versions[tool] = value
    installed = set(run_fixed(["rustup", "target", "list", "--installed", "--toolchain", toolchain_name], repo).decode().split())
    graphs = contract.get("graphs", {})
    evidence, outputs, manifests, locks = {}, set(), set(), set()
    for name in ("native", "wasm"):
        graph = graphs.get(name)
        if not isinstance(graph, dict) or graph.get("status") != "implemented":
            fail("graph_not_implemented", f"{name} build graph is not implemented")
        manifest = path_at(repo, graph.get("manifest"))
        lockfile = path_at(repo, graph.get("lock"))
        if manifest in manifests or lockfile in locks:
            fail("graph_inputs_shared", "Native and Wasm require separate manifests and locks")
        manifests.add(manifest)
        locks.add(lockfile)
        digest_file(manifest, graph.get("manifest_sha256"), name + " manifest")
        digest_file(lockfile, graph.get("lock_sha256"), name + " lock")
        if graph.get("target") not in installed:
            fail("target_missing", f"Required target unavailable: {graph.get('target')}")
        target_dir = path_at(repo, graph.get("target_dir"), must_exist=False)
        if any(target_dir == prior or target_dir in prior.parents or prior in target_dir.parents for prior in outputs):
            fail("graph_output_shared", "Native and Wasm cannot share target directories")
        outputs.add(target_dir)
        evidence[name] = {"manifest_sha256": sha256(manifest), "lock_sha256": sha256(lockfile),
                          "target": graph["target"], "target_dir": str(target_dir)}
    return {"versions": versions, "graphs": evidence,
            "dependency_cache": "not_validated_no_deps_stage", "lock_freshness": "hash_only_no_cargo_resolution"}


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("stage", choices=STAGES)
    p.add_argument("--scope", choices=("baseline", "sources", "full"), default="full")
    p.add_argument("--source-lock", default="sources.lock.json")
    p.add_argument("--contract", default="tools/build-contract.json")
    p.add_argument("--output-dir", "--output", dest="output_dir")
    p.add_argument("--host-kit")
    p.add_argument("--host-kit-manifest")
    p.add_argument("--host-kit-sha256")
    p.add_argument("--host-kit-review")
    p.add_argument("--locked", action="store_true", help="Reads only; this first slice never updates locks")
    p.add_argument("--offline", action="store_true", help="Reads only; this first slice never fetches")
    # Plan examples remain parseable but cannot activate unimplemented behavior.
    for option in ("suite", "component", "profile", "target", "plugin", "members", "host", "input"):
        p.add_argument("--" + option)
    p.add_argument("--check", action="store_true")
    p.add_argument("--require-no-fixture-exports", action="store_true")
    return p


def main(argv=None, *, repo_root=None):
    args = parser().parse_args(argv)
    repo = Path(os.path.abspath(repo_root or Path(__file__).parent.parent))
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    run_id = stamp + "-" + uuid.uuid4().hex[:12]
    receipt = {"schema_version": 1, "run_id": run_id, "started_utc": stamp,
               "entry_sha256": sha256(Path(__file__)),
               "stage": args.stage, "scope": args.scope, "status": "blocked", "checks": [],
               "artifacts": [], "gates": {"P-00": "not_claimed", "G0": "not_claimed"},
               "network_used": False, "locks_updated": False, "old_dist_reused": False}
    output = None
    code = 2
    try:
        no_links(repo)
        output = path_at(repo, args.output_dir or "out/runs/" + run_id, must_exist=False)
        if output == repo or output.exists():
            fail("output_exists", "Output must be a new directory; existing output is never reused")
        output.mkdir(parents=True, exist_ok=False)
        receipt["output_dir"] = str(output)
        contract_path = path_at(repo, args.contract)
        contract = read_json(contract_path)
        receipt["contract_sha256"] = sha256(contract_path)
        if contract.get("schema_version") != 1 or set(contract.get("stages", {})) != set(STAGES):
            fail("contract_invalid", "Expected complete build stage contract v1")
        if args.stage != "preflight":
            receipt["status"] = "not_implemented"
            fail("not_implemented", f"Stage {args.stage} is not implemented; no artifact was produced")
        lock_path = path_at(repo, args.source_lock)
        lock = read_json(lock_path)
        receipt["source_lock_sha256"] = sha256(lock_path)
        if lock.get("schema_version") != 1:
            fail("source_lock_invalid", "Expected source lock v1")
        if args.scope in ("baseline", "full"):
            receipt["checks"].append({"baseline": check_baseline(repo, lock)})
        if args.scope in ("sources", "full"):
            sources = lock.get("sources")
            if not isinstance(sources, list) or len(sources) != 2 or {s.get('id') for s in sources if isinstance(s, dict)} != set(PINNED):
                fail("sources_missing", "Both pinned Codex and CC Switch sources are required")
            for source in sources:
                receipt["checks"].append({"source": verify_source(repo, source)})
            if any(check.get("source", {}).get("complete") is False for check in receipt["checks"]):
                fail("source_incomplete", "Partial source snapshots were verified; complete pinned upstream sources are still missing")
        if args.scope == "full":
            receipt["checks"].append({"host_kit": check_kit(repo, args)})
            receipt["checks"].append({"build_inputs": check_graphs(repo, contract)})
            fail("full_preflight_incomplete", "Dependency cache, resolved lock freshness and host schema checks are not implemented")
        receipt["status"] = "passed_scoped_checks"
        receipt["qualification_limit"] = "Scoped input checks only; P-00/M-00/G0 not completed"
        code = 0
    except Blocked as error:
        receipt["error"] = {"code": error.code, "message": error.message}
    except (OSError, ValueError, TypeError, KeyError, AttributeError) as error:
        receipt["error"] = {"code": "invalid_input", "message": f"{type(error).__name__}: {error}"}
    receipt["exit_code"] = code
    receipt["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    # Never touch a rejected/existing output directory, even to write a failure.
    if output is not None and receipt.get("output_dir"):
        status_path = output / "status.json"
        try:
            status_path.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        except OSError as error:
            receipt["status"] = "blocked"
            receipt["error"] = {"code": "receipt_write_failed", "message": str(error)}
            receipt["exit_code"] = code = 2
    print(json.dumps(receipt, ensure_ascii=False, indent=2))
    return code


if __name__ == "__main__":
    sys.exit(main())
