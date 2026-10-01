"""Read-only G0 graph audit; optional isolated offline Cargo metadata, never a build.

The report intentionally cannot pass G0: metadata is not compilation or successful
IPC/backend/writer/exec lifecycle qualification. No source, lock or cache is edited.
"""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tomllib

NATIVE = "x86_64-pc-windows-msvc"
WASM = "wasm32-unknown-unknown"
CODEX = "upstream/p02-integration-004/codex-work/codex-rs"
ORIGINAL = "upstream/p02-source-batch-001/codex-source"
CCSWITCH = "upstream/p02-source-batch-001/cc-switch-source"
PROBE = "qualification/p02-integration-004"
RECEIPTS = "receipts/p02-integration-004"
QUALIFICATION_FEATURE = "morrow-p02-restricted-qualification"


def safe(path):
    path = Path(os.path.abspath(path))
    for part in (*reversed(path.parents), path):
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise ValueError("linked/reparse path refused: " + str(part))
    return path


def child(root, relative):
    if not isinstance(relative, str) or not relative or Path(relative).is_absolute():
        raise ValueError("expected nonempty relative path")
    result = safe(root / relative)
    result.relative_to(root)
    return result


def sha(path):
    with safe(path).open("rb") as stream:
        h = hashlib.file_digest(stream, "sha256")
    return h.hexdigest()


def read_json(path):
    return json.loads(safe(path).read_text("utf-8-sig"))


def read_toml(path):
    return tomllib.loads(safe(path).read_text("utf-8-sig"))


def inventory(root):
    result = {}
    for base, dirs, files in os.walk(safe(root), followlinks=False):
        for name in dirs:
            safe(Path(base) / name)
        for name in files:
            path = Path(base) / name
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                # Archive source contains a legitimate bubblewrap LICENSE link.
                # Bind the link text as data; never read its target or execute it.
                result[path.relative_to(root).as_posix()] = {
                    "kind": "symlink", "target": os.readlink(path)}
            else:
                result[path.relative_to(root).as_posix()] = sha(path)
    return result


def contract_audit(root, document):
    rows, used_inputs, used_outputs = [], set(), []
    for name, target in (("native", NATIVE), ("wasm", WASM)):
        graph = document.get("graphs", {}).get(name, {})
        problems = []
        if graph.get("status") != "implemented":
            problems.append("graph_not_implemented")
        if graph.get("target") != target:
            problems.append("unexpected_target")
        bindings = {}
        for field in ("manifest", "lock"):
            try:
                path = child(root, graph.get(field))
                if path in used_inputs:
                    problems.append("graph_input_shared")
                used_inputs.add(path)
                actual = sha(path)
                bindings[field] = {"path": str(path), "sha256": actual}
                if actual != graph.get(field + "_sha256"):
                    problems.append(field + "_digest_unbound")
            except (ValueError, OSError, TypeError):
                problems.append(field + "_missing_or_unsafe")
        try:
            output = child(root, graph.get("target_dir"))
            if any(output == old or output in old.parents or old in output.parents for old in used_outputs):
                problems.append("graph_output_shared")
            used_outputs.append(output)
        except (ValueError, OSError, TypeError):
            problems.append("target_dir_missing_or_unsafe")
        rows.append({"graph": name, "target": graph.get("target"), "bindings": bindings,
                     "problems": problems, "product_build_verified": False})
    return rows


def lock_audit(path, vendor):
    packages = read_toml(path).get("package", [])
    registry, git, local, missing, mismatched = [], [], [], [], []
    for package in packages:
        source = package.get("source", "")
        row = {key: package[key] for key in ("name", "version", "source", "checksum") if key in package}
        if source.startswith("registry+"):
            registry.append(row)
            member = child(vendor, package["name"] + "-" + package["version"])
            checksum = member / ".cargo-checksum.json"
            if not checksum.is_file() or not (member / "Cargo.toml").is_file():
                missing.append(row)
            elif read_json(checksum).get("package") != package.get("checksum"):
                mismatched.append(row)
        elif source.startswith("git+"):
            git.append(row)
        else:
            local.append(row)
    return {"lock": str(path), "sha256": sha(path), "package_count": len(packages),
            "registry_count": len(registry), "local_count": len(local), "git_packages": git,
            "vendor": str(vendor), "vendor_missing": missing, "vendor_checksum_mismatches": mismatched,
            "limit": "all-lock inventory, not selected-target closure; checksum receipts only, not vendor file rehash"}


def toolchain_audit(companion, contract, toolchain, output):
    binding = contract.get("toolchain_lock", {})
    lock_path = child(companion, binding.get("path"))
    lock = read_json(lock_path)
    result = {"path": str(lock_path), "sha256": sha(lock_path),
              "matches_contract": sha(lock_path) == binding.get("sha256"),
              "declared": lock, "installed": "not_checked_without_explicit_path"}
    if toolchain is not None:
        toolchain = safe(toolchain)
        run = output / "toolchain-version"
        run.mkdir(exist_ok=False)
        env = isolated_environment(run, toolchain)
        versions = {}
        for tool in ("cargo", "rustc"):
            completed = subprocess.run([str(safe(toolchain / "bin" / (tool + ".exe"))), "--version"],
                                       env=env, capture_output=True, check=False, timeout=15, shell=False)
            versions[tool] = {"exit_code": completed.returncode,
                              "stdout": completed.stdout.decode("utf-8", errors="replace").strip(),
                              "stderr": completed.stderr.decode("utf-8", errors="replace").strip()}
        result["installed"] = {"path": str(toolchain), "matches_pinned_name": toolchain.name == lock.get("toolchain"),
            "versions": versions,
            "versions_match_lock": all(row["exit_code"] == 0 and row["stdout"] == lock["versions"].get(name)
                                       for name, row in versions.items()),
            "target_std_present": {target: any((toolchain / "lib/rustlib" / target / "lib").glob("libstd-*.rlib"))
                                   for target in (NATIVE, WASM)},
            "limit": "direct cargo/rustc and std availability only; linker/Bazel/other platform qualification not checked"}
    return result


def isolated_environment(run, toolchain):
    # Explicit allowlist: never inherit personal Cargo/Rust/Git/provider configuration.
    env = {key: value for key, value in os.environ.items() if key.upper() in
           {"SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"}}
    profile, home, temp = run / "profile", run / "cargo-home", run / "tmp"
    for path in (profile, home, temp, profile / "AppData/Local", profile / "AppData/Roaming"):
        path.mkdir(parents=True, exist_ok=False)
    env.update({"PATH": str(toolchain / "bin"), "CARGO_HOME": str(home),
                "CARGO_TARGET_DIR": str(run / "target"), "CARGO_NET_OFFLINE": "true",
                "CARGO_TERM_COLOR": "never", "RUSTUP_AUTO_INSTALL": "0",
                "CARGO_BUILD_RUSTC_WRAPPER": "", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER": "",
                "RUSTC": str(toolchain / "bin/rustc.exe"),
                "RUSTDOC": str(toolchain / "bin/rustdoc.exe"),
                "HOME": str(profile), "USERPROFILE": str(profile), "TEMP": str(temp), "TMP": str(temp),
                "LOCALAPPDATA": str(profile / "AppData/Local"), "APPDATA": str(profile / "AppData/Roaming"),
                "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
                "GIT_CONFIG_GLOBAL": os.devnull, "GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1"})
    return env


def cargo_metadata(manifest, run, vendor, toolchain, timeout=45):
    run.mkdir(exist_ok=False)
    env = isolated_environment(run, toolchain)
    config = ('[source.crates-io]\nreplace-with = "pinned-readonly-vendor"\n'
              '[source.pinned-readonly-vendor]\ndirectory = ' + json.dumps(vendor.as_posix()) +
              '\n[net]\noffline = true\n')
    (run / "cargo-home/config.toml").write_text(config, encoding="utf-8")
    lock = manifest.parent / "Cargo.lock"
    before = {str(path): sha(path) for path in (manifest, lock)}
    command = [str(safe(toolchain / "bin/cargo.exe")), "metadata", "--format-version", "1",
               "--offline", "--locked", "--filter-platform", NATIVE, "--manifest-path", str(manifest)]
    try:
        completed = subprocess.run(command, cwd=run, env=env, capture_output=True,
                                   timeout=timeout, check=False, shell=False)
        code, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
    except subprocess.TimeoutExpired as error:
        code, stdout, stderr = 124, error.stdout or b"", error.stderr or b""
    (run / "stdout.json").write_bytes(stdout)
    (run / "stderr.txt").write_bytes(stderr)
    after = {str(path): sha(path) for path in (manifest, lock)}
    result = {"argv": command, "exit_code": code, "inputs_unchanged": before == after,
              "inputs_before": before, "inputs_after": after, "stdout_sha256": sha(run / "stdout.json"),
              "stderr_sha256": sha(run / "stderr.txt"), "network_allowed": False,
              "cargo_home_is_fresh": True, "build_performed": False,
              "product_build_verified": False}
    if code == 0:
        metadata = json.loads(stdout)
        result["package_count"] = len(metadata.get("packages", []))
        result["workspace_members"] = metadata.get("workspace_members", [])
        result["qualification_feature_nodes"] = [row["id"] for row in metadata.get("resolve", {}).get("nodes", [])
                                                    if QUALIFICATION_FEATURE in row.get("features", [])]
    (run / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def audit(companion, output, metadata=False, toolchain=None, _owned=None):
    companion, output = safe(companion), safe(output)
    if output == companion or companion in output.parents or output in companion.parents:
        raise ValueError("output must be separate from read-only companion")
    output.mkdir(parents=True, exist_ok=False)
    if _owned is not None:
        _owned["output"] = output
    contract_path = companion / "tools/build-contract.json"
    report = {"schema_version": 1, "captured_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
              "entry_sha256": sha(Path(__file__)), "companion": str(companion),
              "scope": "current real sources/locks/patches and bounded product metadata availability",
              "status": "blocked", "G0": "blocked", "product_graphs_verified": 0,
              "network_used": False, "build_performed": False,
              "gates": {key: "blocked" for key in ("G0-C05", "G0-C06", "G0-C07", "G0-C08")}}
    if _owned is not None:
        _owned["report"] = report
    report["contract"] = {"sha256": sha(contract_path),
                          "graphs": contract_audit(companion, read_json(contract_path)),
                          "stages": read_json(contract_path).get("stages", {})}
    report["toolchain"] = toolchain_audit(companion, read_json(contract_path), toolchain, output)
    vendor = companion / "out/p02-exec-store-002/vendor"
    locations = {"codex_product": CODEX, "qualification_probe": PROBE,
                 "cc_switch_original": CCSWITCH + "/src-tauri"}
    report["locks"] = {name: lock_audit(companion / relative / "Cargo.lock", vendor)
                       for name, relative in locations.items()}
    report["manifests"] = {name: {"path": str(companion / relative / "Cargo.toml"),
                                   "sha256": sha(companion / relative / "Cargo.toml"),
                                   "patches": read_toml(companion / relative / "Cargo.toml").get("patch", {})}
                           for name, relative in locations.items()}
    cli = read_toml(companion / CODEX / "cli/Cargo.toml")
    report["native_product_entry"] = {"package": cli["package"]["name"], "bins": cli["bin"],
                                       "manifest": str(companion / CODEX / "cli/Cargo.toml"),
                                       "feature_policy": "default product features; restricted qualification cannot establish runtime authorization"}
    report["qualification_features"] = {name: read_toml(companion / CODEX / name / "Cargo.toml").get("features", {})
                                        for name in ("core", "thread-store")}
    handoff = read_json(companion / RECEIPTS / "handoff.json")
    patched_paths = read_json(companion / RECEIPTS / "patched-inputs.json")
    report["patch_binding"] = {"handoff_sha256": sha(companion / RECEIPTS / "handoff.json"),
        "current_files": {name: sha(child(companion, name)) for name in patched_paths},
        "matches_handoff": all(sha(child(companion, name)) == handoff["input_sha256"].get(name) for name in patched_paths),
        "patch_sha256": sha(companion / RECEIPTS / "patch-after-build-001.patch"),
        "patch_matches_handoff": sha(companion / RECEIPTS / "patch-after-build-001.patch") == handoff["source_delta_sha256"]}
    original = inventory(companion / ORIGINAL)
    patched = inventory(companion / "upstream/p02-integration-004/codex-work")
    cc_switch = inventory(companion / CCSWITCH)
    delta = [{"path": name, "original_sha256": original.get(name), "patched_sha256": patched.get(name)}
             for name in sorted(original.keys() | patched.keys()) if original.get(name) != patched.get(name)]
    source_inventory = output / "source-inventory.json"
    source_inventory.write_text(json.dumps({"original": original, "patched": patched, "cc_switch": cc_switch}, indent=2) + "\n", encoding="utf-8")
    report["source_delta"] = {"original_files": len(original), "patched_files": len(patched), "delta": delta,
                              "inventory_sha256": sha(source_inventory), "archive_authentication": "not_revalidated_in_this_run",
                              "symlink_policy": "leaf symlinks inventoried as link text without dereferencing; linked directories and non-symlink reparse files refused"}
    report["qualification_receipt_scope"] = {"cases": handoff["cases_passed"], "assertions": handoff["assertions_passed"],
                                              "status": handoff["status"], "product_qualification": False}
    report["metadata"] = {}
    if metadata:
        if toolchain is None:
            raise ValueError("metadata requires explicit installed toolchain path")
        for name in ("codex_product", "cc_switch_original"):
            report["metadata"][name] = cargo_metadata(companion / locations[name] / "Cargo.toml",
                                                       output / (name + "-metadata"), vendor, safe(toolchain))
        report["metadata_sources_unchanged"] = {
            "codex_original": original == inventory(companion / ORIGINAL),
            "codex_patched": patched == inventory(companion / "upstream/p02-integration-004/codex-work"),
            "cc_switch_original": cc_switch == inventory(companion / CCSWITCH)}
    report["remaining"] = ["native/Wasm product graph contract and separate resolved locks absent",
        "CC Switch portable source slice absent", "successful same-kit Codex IPC network/auth backends unqualified",
        "durable storage/writer acquire-release and real exec/PTY close-drain lifecycle unqualified",
        "all-lock static cache coverage is not target resolution or a successful product build"]
    report["exit_code"] = 2
    (output / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--companion", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--metadata", action="store_true")
    parser.add_argument("--toolchain")
    args = parser.parse_args(argv)
    owned = {}
    try:
        result = audit(Path(args.companion), Path(args.output), args.metadata,
                       Path(args.toolchain) if args.toolchain else None, owned)
        print(json.dumps({key: result[key] for key in ("status", "G0", "gates", "product_graphs_verified", "exit_code")}, indent=2))
        return 2
    except (OSError, ValueError, KeyError, TypeError) as error:
        failure = dict(owned.get("report", {}), status="blocked", exit_code=2, error=str(error))
        if "output" in owned:
            (owned["output"] / "result.json").write_text(json.dumps(failure, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"status": "blocked", "exit_code": 2, "error": str(error)}))
        return 2


if __name__ == "__main__":
    sys.exit(main())
