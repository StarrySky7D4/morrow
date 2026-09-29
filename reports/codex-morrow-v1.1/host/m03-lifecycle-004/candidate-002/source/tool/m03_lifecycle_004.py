"""Prepare a new host candidate and run one explicitly selected M03 case.

Uses the unchanged fixture002 and harness011 for their existing Core-revocation
and OS-backpressure cases. They do not qualify passive deadline/disconnect
scenarios. No retries, public network targets, Git writes or product-G0 credit.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
INPUTS = (
    "core", "native_session_stream_001", "network_node_stream_001",
    "native_pipe_win_001", "contracts/experimental/agent_host_v3_http_stream",
)
HELPERS = {
    "tool/m03_revoke_barriers_011.py": "641dece216bda441067123b288ae62565deb79478cdb951d307bdb04c2dc536e",
    "tool/m03_http_matrix_008.py": "9deef213e4ea7765395fbaf5a0aede45b7cdfc7ff2d75faca662782fa3af39e1",
}
GUEST_ROOT = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex")
GUEST_MANIFEST = GUEST_ROOT / "receipts/m03-fixture-002/native-candidate-fixture-002.json"
GUEST_MANIFEST_SHA = "6a8d4d07aab156d72f5fce73dea7750f8b5424707dae84595182ff7f55c211a5"
GUEST_EXE_SHA = "b895a7d78cd6c3c3641e502ad63b167cf5b78bdfaab75bfde079710efb0a08bc"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def inputs():
    paths = [Path(__file__).resolve()]
    for name in INPUTS:
        paths += [p for p in (ROOT / name).rglob("*") if p.is_file()
                  and not {"target", ".git"}.intersection(p.relative_to(ROOT / name).parts)]
    return {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted(paths)}


def verify_guest():
    assert sha(GUEST_MANIFEST) == GUEST_MANIFEST_SHA, "fixture manifest drift"
    manifest = json.loads(GUEST_MANIFEST.read_text())
    executable = Path(manifest["executable"])
    assert sha(executable) == manifest["executable_sha256"] == GUEST_EXE_SHA
    for rel, digest in manifest["input_sha256"].items():
        assert sha(GUEST_ROOT / rel) == digest, rel
    return executable


def build_environment(output):
    # Test roots are new and contain no user content. Runtime child/HTTP tests
    # use the separately restricted environment in the frozen harness.
    env = os.environ.copy()
    vcroot = Path(r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC")
    version = (vcroot / "Auxiliary/Build/Microsoft.VCToolsVersion.default.txt").read_text().strip()
    vc = vcroot / "Tools/MSVC" / version
    sdk = Path(r"C:\Program Files (x86)\Windows Kits\10")
    sdk_version = sorted((sdk / "Lib").iterdir(), key=lambda p: p.name)[-1].name
    env["PATH"] = str(vc / "bin/Hostx64/x64") + os.pathsep + env["PATH"]
    env["LIB"] = ";".join(str(p) for p in (vc / "lib/x64", sdk / "Lib" / sdk_version / "ucrt/x64", sdk / "Lib" / sdk_version / "um/x64"))
    env["INCLUDE"] = ";".join(str(p) for p in (vc / "include", sdk / "Include" / sdk_version / "ucrt", sdk / "Include" / sdk_version / "um", sdk / "Include" / sdk_version / "shared"))
    for name, key in (("owner-profiles", "MORROW_OWNER_TEST_ROOT"), ("api-profiles", "MORROW_HTTP_TEST_ROOT")):
        directory = output / name
        directory.mkdir()
        env[key] = str(directory)
    env["MORROW_OWNER_TEST_CLIENT"] = str(ROOT / "target/m03-stream-001-native/debug/morrow-native-close-peer.exe")
    return env


def prepare(output):
    output = output.resolve()
    assert not any(output.is_relative_to(ROOT / name) for name in INPUTS)
    output.mkdir(parents=True, exist_ok=False)
    before = inputs()
    receipt = {"sources_before": before, "commands": [], "status": "failed",
               "scope": "host build/library tests; runtime HTTP/Core cases not run"}
    env = build_environment(output)
    for label, flags in (("build", ["build", "--bins"]), ("tests", ["test", "--lib", "--", "--test-threads=1"])):
        command = [shutil.which("cargo"), flags[0], "--locked", "--offline", "--manifest-path", "native_session_stream_001/Cargo.toml",
                   "--target-dir", "target/m03-stream-001-native", *flags[1:]]
        start = time.monotonic()
        with (output / (label + ".log")).open("xb") as log:
            process = subprocess.Popen(command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
            # A timeout retains its process handle in the receipt. It is not
            # cleanup confirmation, and no candidate is accepted in that case.
            try:
                code = process.wait(timeout=600)
            except subprocess.TimeoutExpired:
                receipt["live_process_pid"] = process.pid
                code = None
        receipt["commands"].append({"command": command, "exit_code": code, "seconds": time.monotonic() - start})
        if code != 0:
            receipt["sources_after"] = inputs()
            write(output / "build-receipt.json", receipt)
            raise RuntimeError("candidate preparation failed; inspect the retained attempt")
    after = inputs()
    assert before == after, "source changed during candidate build"
    for rel, digest in before.items():
        destination = output / "source" / rel
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / rel, destination)
        assert sha(destination) == digest
    binary = output / "morrow-native-stream-host.exe"
    shutil.copyfile(ROOT / "target/m03-stream-001-native/debug/morrow-native-stream-host.exe", binary)
    receipt.update(status="passed", sources_after=after, source_unchanged=True)
    write(output / "build-receipt.json", receipt)
    manifest = {
        "executable": str(binary), "executable_sha256": sha(binary), "source_files": before,
        "build_receipt_sha256": sha(output / "build-receipt.json"),
        "helper_sha256": HELPERS, "guest_manifest_sha256": GUEST_MANIFEST_SHA,
        "guest_exe_sha256": GUEST_EXE_SHA, "http_runtime_tested": False,
        "product_G0": "not_run", "sdk_frozen": False,
    }
    write(output / "manifest.json", manifest)
    print(json.dumps({"manifest": str(output / "manifest.json"), "sha256": sha(output / "manifest.json")}))


def execute(manifest_path, expected_hash, mode):
    manifest_path = manifest_path.resolve()
    assert sha(manifest_path) == expected_hash, "candidate manifest drift"
    manifest = json.loads(manifest_path.read_text())

    def verify(_mode):
        assert sha(manifest_path) == expected_hash
        assert sha(manifest_path.parent / "build-receipt.json") == manifest["build_receipt_sha256"]
        for rel, digest in manifest["source_files"].items():
            assert sha(manifest_path.parent / "source" / rel) == sha(ROOT / rel) == digest, rel
        for rel, digest in HELPERS.items():
            assert sha(ROOT / rel) == digest, rel
        binary = Path(manifest["executable"])
        assert sha(binary) == manifest["executable_sha256"]
        return binary, verify_guest()

    verify(mode)  # No socket/process before pinned input verification.
    loader = importlib.util.spec_from_file_location("m03_frozen_harness011", ROOT / "tool/m03_revoke_barriers_011.py")
    harness = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(harness)
    # Reuse the unchanged case logic/strict assertions, with explicitly pinned
    # new host inputs. Every execution gets its own profile/operation/nonce.
    harness.BASE = manifest_path.parent / "runs"
    harness.BASE.mkdir(exist_ok=True)
    # fixture002 deliberately allows evidence only below its own repository's
    # out directory. Keep that frozen path rule and let its per-run nonce/stamp
    # create a fresh batch; evidence-manifest.json binds those external files.
    harness.PLUGIN = GUEST_ROOT
    harness.verify = verify
    harness.HOSTS = {name: ("lifecycle004", expected_hash, manifest["executable_sha256"])
                     for name in ("core-revoke", "data-pending")}
    before = set(harness.BASE.iterdir())
    code = harness.run(mode)
    created = set(harness.BASE.iterdir()) - before
    assert len(created) == 1
    run = created.pop()
    write(run / "candidate-binding.json", {
        "manifest": str(manifest_path), "manifest_sha256": expected_hash,
        "runner_sha256": sha(Path(__file__)), "helper_sha256": HELPERS,
        "mode": mode, "exit_code": code, "automatic_retry": False,
        "new_host_full_chain_case": True, "sdk_freeze_credit": False,
    })
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--prepare", type=Path, metavar="NEW_DIRECTORY")
    group.add_argument("--case", choices=("core-revoke", "data-pending"))
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--manifest-sha256")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("actual Windows required")
    if args.prepare:
        prepare(args.prepare)
        return 0
    if not args.manifest or not args.manifest_sha256:
        parser.error("one case requires --manifest and --manifest-sha256")
    return execute(args.manifest, args.manifest_sha256, args.case)


if __name__ == "__main__":
    sys.exit(main())
