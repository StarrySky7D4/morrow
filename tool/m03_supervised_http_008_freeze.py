"""Freeze a successful pinned supervisor check for passive006-compatible HTTP.

This helper copies and verifies files only. It starts no process, build or server.
An old check without all binary hashes is deliberately rejected.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil

ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOTS = ("native_session_stream_001", "network_node_stream_001", "native_pipe_win_001",
                "contracts/experimental/agent_host_v3_http_stream", "core", "sdk/rust/contracts")
SUFFIXES = {".rs", ".toml", ".lock", ".proto", ".capnp", ".h", ".md"}
BINARIES = ("morrow-native-supervisor.exe", "morrow-native-stream-host.exe",
            "morrow-native-close-peer.exe", "morrow-native-isolation-peer.exe")
REQUIRED = ("native_session_stream_001/src/bin/morrow-native-supervisor.rs",
            "native_session_stream_001/src/supervisor.rs", "native_pipe_win_001/src/job_process.rs",
            "native_pipe_win_001/src/job_stdin.rs", "native_session_stream_001/src/authority.rs")
GUEST_MANIFEST = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex\receipts\m03-fixture-006\native-candidate-fixture-006.json")
GUEST_MANIFEST_SHA = "d7c6ca55ce97adbb447270765483a838d00d7022c0787e5d4f64911bd0d8aadb"
GUEST_ROOT = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex")
BASE_SHA = "01df0107aa87bdfa4d660a32717c285c82a5075bd606c254850dbeaeda2a291e"


def require(value, message):
    if not value:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def dump(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def contained(root, relative):
    require(isinstance(relative, str) and relative and not Path(relative).is_absolute(), "relative path required")
    path = (root / relative).resolve()
    path.relative_to(root.resolve())
    return path


def inventory(root):
    files = {}
    for relative in SOURCE_ROOTS:
        for directory, folders, names in os.walk(root / relative):
            folders[:] = [n for n in folders if n not in {"target", ".git", "build"}]
            for name in names:
                path = Path(directory) / name
                if path.suffix in SUFFIXES:
                    files[path.relative_to(root).as_posix()] = sha(path)
    return dict(sorted(files.items()))


def validate_check(root, check):
    receipt = load(check / "receipt.json")
    require(receipt.get("source_unchanged") is True, "check changed source")
    sources = receipt.get("sources_before")
    require(isinstance(sources, dict) and sources and receipt.get("sources_after") == sources, "unstable/missing checked source")
    require(all(p in sources for p in REQUIRED), "supervisor source omitted")
    require(inventory(root) == sources, "current source set/hash differs from checked source")
    commands = receipt.get("commands")
    require(isinstance(commands, list) and commands and all(c.get("exit_code") == 0 for c in commands), "failed/missing check commands")
    labels = [c.get("label") for c in commands]
    require(len(set(labels)) == len(labels), "duplicate check command")
    pins = {}
    for command in commands:
        label = command.get("label")
        require(isinstance(label, str) and label and Path(label).name == label, "invalid check label")
        require(sha(contained(check, label + ".log")) == command.get("log_sha256"), "check log changed: " + label)
        if label in {"default-build", "feature-build"}:
            actual = command.get("binaries")
            expected = {label + "/" + name for name in BINARIES}
            require(isinstance(actual, dict) and set(actual) == expected, "all four checked binary pins required: " + label)
            require(command.get("features") == ([] if label == "default-build" else ["qualification-pipe-fault"]), "checked feature mismatch")
            for relative, digest in actual.items():
                require(sha(contained(check, relative)) == digest, "checked binary changed: " + relative)
                pins[relative] = digest
            require(command.get("executable") == label + "/morrow-native-stream-host.exe" and
                    command.get("executable_sha256") == actual[command["executable"]], "old host pin mismatch")
    require(set(pins) == {label + "/" + name for label in ("default-build", "feature-build") for name in BINARIES}, "both pinned builds required")
    return receipt, pins


def freeze(root, check, out, guest_manifest=GUEST_MANIFEST, guest_sha=GUEST_MANIFEST_SHA):
    root, check, out = root.resolve(), check.resolve(), out.resolve()
    require(check != out and check not in out.parents and out not in check.parents, "check and fresh output must be independent")
    receipt, pins = validate_check(root, check)
    receipt_sha = sha(check / "receipt.json")
    require(sha(root / "tool/m03_passive_fault_006.py") == BASE_SHA, "passive006 runner drift")
    require(sha(guest_manifest) == guest_sha, "frozen guest006 manifest changed")
    guest = load(guest_manifest)
    require(sha(Path(guest["executable"])) == guest["executable_sha256"], "frozen guest006 binary changed")
    for relative, digest in guest["input_sha256"].items():
        require(sha(contained(GUEST_ROOT, relative)) == digest, "guest006 input changed: " + relative)
    out.mkdir(parents=True, exist_ok=False)
    copies = {}

    def copy(source, relative, digest):
        require(sha(source) == digest, "input changed during freeze: " + str(source))
        destination = contained(out, relative)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        require(sha(source) == digest == sha(destination), "copy/hash changed during freeze")
        copies[relative] = digest

    for relative, digest in receipt["sources_before"].items():
        copy(contained(root, relative), "source/" + relative, digest)
    copy(check / "receipt.json", "build-receipt.json", receipt_sha)
    tests = {}
    for command in receipt["commands"]:
        relative = "check/" + command["label"] + ".log"
        copy(check / (command["label"] + ".log"), relative, command["log_sha256"])
        tests[command["label"]] = {"log": str(contained(out, relative)), "log_sha256": command["log_sha256"], "exit_code": 0}
    for relative, digest in pins.items():
        copy(contained(check, relative), relative, digest)
    for name in ("m03_supervised_http_008.py", "m03_supervised_http_008_freeze.py", "m03_passive_fault_006.py", "m03_revoke_barriers_011.py"):
        source = root / "tool" / name
        copy(source, "runner/" + name, sha(source))
    copy(guest_manifest, "guest006-manifest.json", guest_sha)
    manifests = {}
    for label, name in (("default-build", "default-manifest.json"), ("feature-build", "manifest.json")):
        executable = label + "/morrow-native-supervisor.exe"
        manifest = {"version": 1, "supervisor_binary": True, "source_root": "source",
                    "source_files": receipt["sources_before"], "build_receipt_sha256": receipt_sha,
                    "test_receipts": tests, "executable": str(contained(out, executable)),
                    "executable_sha256": pins[executable], "features": [] if label == "default-build" else ["qualification-pipe-fault"],
                    "default_candidate": {"executable": str(contained(out, "default-build/morrow-native-supervisor.exe")),
                                          "executable_sha256": pins["default-build/morrow-native-supervisor.exe"]},
                    "checked_binaries": pins, "check": str(check), "guest_manifest": str(guest_manifest),
                    "guest_manifest_sha256": guest_sha, "runner_sha256": copies["runner/m03_supervised_http_008.py"]}
        dump(out / name, manifest)
        manifests[name] = sha(out / name)
    final_receipt, final_pins = validate_check(root, check)
    require(final_receipt == receipt and final_pins == pins and sha(check / "receipt.json") == receipt_sha, "checked evidence changed during freeze")
    for relative, digest in copies.items():
        require(sha(contained(out, relative)) == digest, "frozen copy changed")
    require(sha(guest_manifest) == guest_sha and sha(Path(guest["executable"])) == guest["executable_sha256"], "guest changed during freeze")
    for relative, digest in guest["input_sha256"].items():
        require(sha(contained(GUEST_ROOT, relative)) == digest, "guest006 input changed during freeze: " + relative)
    result = {"check": str(check), "check_sha256": receipt_sha, "source_unchanged": True,
              "frozen_files": copies, "manifests": manifests, "runtime_started": False,
              "guest_manifest": str(guest_manifest), "guest_manifest_sha256": guest_sha}
    dump(out / "freeze-receipt.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(freeze(ROOT, args.check, args.output), indent=2))


if __name__ == "__main__":
    main()
