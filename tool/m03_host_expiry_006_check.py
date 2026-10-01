"""Local H1/H2 compilation and pipe tests. Does not launch an HTTP request."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
CARGO = Path(r"C:\Users\Administrator\.cargo\bin\cargo.exe")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    names = ["native_session_stream_001", "network_node_stream_001", "native_pipe_win_001",
             "contracts/experimental/agent_host_v3_http_stream", "core", "sdk/rust/contracts"]
    paths = []
    for name in names:
        for directory, folders, files in os.walk(ROOT / name):
            folders[:] = [f for f in folders if f not in {"target", ".git", "build"}]
            for file in files:
                p = Path(directory) / file
                if p.suffix in {".rs", ".toml", ".lock", ".proto", ".capnp", ".h", ".md"}:
                    paths.append(p)
    return {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted(paths)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    parser.add_argument("--build-only", action="store_true")
    args = parser.parse_args()
    out = (ROOT / args.out).resolve()
    out.relative_to(ROOT)
    out.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    vcroot = Path(r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC")
    version = (vcroot / "Auxiliary/Build/Microsoft.VCToolsVersion.default.txt").read_text().strip()
    vc = vcroot / "Tools/MSVC" / version
    sdk = Path(r"C:\Program Files (x86)\Windows Kits\10")
    sv = sorted((sdk / "Lib").iterdir(), key=lambda p: p.name)[-1].name
    env["PATH"] = str(vc / "bin/Hostx64/x64") + os.pathsep + env["PATH"]
    env["LIB"] = ";".join(str(p) for p in [vc / "lib/x64", sdk / "Lib" / sv / "ucrt/x64", sdk / "Lib" / sv / "um/x64"])
    env["INCLUDE"] = ";".join(str(p) for p in [vc / "include", sdk / "Include" / sv / "ucrt", sdk / "Include" / sv / "um", sdk / "Include" / sv / "shared"])
    target = ROOT / "target/m03-expiry-006-host"
    test_root = out / "local-test-data"
    test_root.mkdir()
    env["MORROW_QUALIFICATION_TEST_ROOT"] = str(test_root)
    http_root = out / "simulated-owner-data"
    http_root.mkdir()
    env["MORROW_HTTP_TEST_ROOT"] = str(http_root)
    receipt = {"scope": "native H1/H2 build and local pipe tests only; no HTTP; no full-product qualification",
               "sources_before": inputs(), "commands": []}
    steps = [("default-build", [], ["build", "--bins"]),
             ("feature-build", ["--features", "qualification-pipe-fault"], ["build", "--bins"])]
    if not args.build_only:
        steps += [("default-pipe-tests", [], ["test", "--lib", "pipe_driver::", "--", "--test-threads=1", "--nocapture"]),
                  ("default-write-state-tests", [], ["test", "--lib", "write_state::", "--", "--test-threads=1", "--nocapture"]),
                  ("default-revocation-tests", [], ["test", "--lib", "revocation_interleaving_tests::", "--", "--test-threads=1", "--nocapture"]),
                  ("feature-qualification-tests", ["--features", "qualification-pipe-fault"], ["test", "--lib", "qualification", "--", "--test-threads=1", "--nocapture"]),
                  ("feature-ordinary-pipe-tests", ["--features", "qualification-pipe-fault"], ["test", "--lib", "pipe_driver::", "--", "--test-threads=1", "--nocapture"]),
                  ("feature-write-state-tests", ["--features", "qualification-pipe-fault"], ["test", "--lib", "write_state::", "--", "--test-threads=1", "--nocapture"]),
                  ("feature-revocation-tests", ["--features", "qualification-pipe-fault"], ["test", "--lib", "revocation_interleaving_tests::", "--", "--test-threads=1", "--nocapture"]),
                  ("default-cli-tests", [], ["test", "--bin", "morrow-native-stream-host", "--", "--test-threads=1", "--nocapture"]),
                  ("feature-cli-tests", ["--features", "qualification-pipe-fault"], ["test", "--bin", "morrow-native-stream-host", "--", "--test-threads=1", "--nocapture"])]
    if not args.build_only:
        steps += [("default-expiry-tests", [], ["test", "--lib", "expiry_teardown::", "--", "--test-threads=1"]),
                  ("feature-expiry-tests", ["--features", "qualification-pipe-fault"], ["test", "--lib", "expiry_teardown::", "--", "--test-threads=1"])]
    for label, features, flags in steps:
        command = [str(CARGO), flags[0], "--locked", "--offline", "--manifest-path",
                   "native_session_stream_001/Cargo.toml", "--target-dir", str(target), *features, *flags[1:]]
        started = time.monotonic()
        log = out / (label + ".log")
        with log.open("wb") as output:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
        item = {"label": label, "command": command, "exit_code": result.returncode,
                "seconds": time.monotonic() - started, "log_sha256": sha(log),
                "features": ["qualification-pipe-fault"] if features else []}
        if result.returncode == 0 and flags[0] == "build":
            candidate = out / label
            candidate.mkdir()
            for binary in ["morrow-native-stream-host.exe", "morrow-native-close-peer.exe"]:
                src = target / "debug" / binary
                if src.is_file():
                    shutil.copy2(src, candidate / binary)
            item["executable"] = (candidate / "morrow-native-stream-host.exe").relative_to(out).as_posix()
            item["executable_sha256"] = sha(candidate / "morrow-native-stream-host.exe")
        receipt["commands"].append(item)
        receipt["sources_after"] = inputs()
        receipt["source_unchanged"] = receipt["sources_before"] == receipt["sources_after"]
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        print(label, result.returncode, flush=True)
        if result.returncode:
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
