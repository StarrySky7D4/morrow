#!/usr/bin/env python3
"""Reexecute real Codex trait qualification and retain raw, hashed evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
from datetime import datetime, timezone


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parent
    root = source.parents[2]
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    logs = output / "logs"
    logs.mkdir(exist_ok=True)
    upstream = source.parent / "upstream/p02-integration-004/codex-work/codex-rs"

    def inputs():
        paths = set()
        # Conservative input closure: actual upstream source, not just the two
        # trait files. Registry/git dependency identities are pinned in Cargo.lock.
        for directory in (source, root / "extensions/agent-session-exec-v1-r2", upstream):
            for parent, directories, files in os.walk(directory):
                directories[:] = sorted(item for item in directories if item not in (".git", "target", "__pycache__"))
                for name in files:
                    path = Path(parent) / name
                    if path.is_symlink() and not (path == upstream / "vendor/bubblewrap/LICENSE" and os.readlink(path) == "COPYING" and path.resolve() == upstream / "vendor/bubblewrap/COPYING"):
                        raise RuntimeError(f"unexpected symlink compilation input: {path}")
                    if path.is_file():
                        paths.add(path)
        for directory in (root / "core/src", root / "core/schemas"):
            paths.update(path for path in directory.rglob("*") if path.is_file())
        paths.update(root / "core" / name for name in ("Cargo.toml", "Cargo.lock", "build.rs"))
        return {str(path.relative_to(root)): hashlib.sha256(b"symlink\0" + os.fsencode(os.readlink(path)) + b"\0" + path.read_bytes()).hexdigest() if path.is_symlink() else digest(path) for path in sorted(paths)}

    before = inputs()
    (output / "inputs-before.json").write_text(json.dumps(before, indent=2, sort_keys=True) + "\n")
    target = root / "build/codex-session-exec-r2-target"
    host_target = root / "build/codex-session-exec-r2-host-target"
    env = os.environ.copy()
    env["CARGO_PROFILE_DEV_DEBUG"] = "0"
    env["CARGO_PROFILE_TEST_DEBUG"] = "0"
    env["MORROW_SESSION_EXEC_R2_HOST_FIXTURE"] = str(host_target / "debug/morrow-codex-r2-host-fixture")
    client = str(source / "Cargo.toml")
    host = str(source / "host-fixture/Cargo.toml")
    commands = [
        ("host-build", host_target, ["cargo", "build", "--locked", "--offline", "--manifest-path", host]),
        ("client-check", target, ["cargo", "check", "--locked", "--offline", "--manifest-path", client]),
        ("actual-trait-tests", target, ["cargo", "test", "--locked", "--offline", "--manifest-path", client, "--tests"]),
        ("client-clippy", target, ["cargo", "clippy", "--locked", "--offline", "--manifest-path", client, "--all-targets", "--no-deps", "--", "-D", "warnings"]),
        ("client-fmt", target, ["cargo", "fmt", "--manifest-path", client, "--", "--check"]),
        ("host-fmt", host_target, ["cargo", "fmt", "--manifest-path", host, "--", "--check"]),
    ]
    results = []
    for name, build_target, command in commands:
        command_env = env.copy()
        command_env["CARGO_TARGET_DIR"] = str(build_target)
        started = datetime.now(timezone.utc).isoformat()
        with (logs / f"{name}.stdout").open("wb") as stdout, (logs / f"{name}.stderr").open("wb") as stderr:
            result = subprocess.run(["bash", "-c", 'source /workspace/.morrow-tools/activate.sh\nexec "$@"', "qualification", *command], cwd=root, env=command_env, stdout=stdout, stderr=stderr, check=False)
        entry = {"name": name, "command": command, "cwd": str(root), "started": started,
                 "completed": datetime.now(timezone.utc).isoformat(), "returncode": result.returncode,
                 "stdout": str((logs / f"{name}.stdout").relative_to(root)), "stdout_sha256": digest(logs / f"{name}.stdout"),
                 "stderr": str((logs / f"{name}.stderr").relative_to(root)), "stderr_sha256": digest(logs / f"{name}.stderr")}
        results.append(entry)
        print(f"{name}: {result.returncode}", flush=True)
        (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        if result.returncode:
            break
    after = inputs()
    (output / "inputs-after.json").write_text(json.dumps(after, indent=2, sort_keys=True) + "\n")
    passed = len(results) == len(commands) and all(item["returncode"] == 0 for item in results) and before == after
    evidence = {"schema": "morrow-codex-session-exec-r2-qualification-v1", "passed": passed,
                "real_thread_store_trait": True, "genuine_core_host_process": True,
                "fake_host": False, "exec_started_process_qualified": False,
                "production_plugin_routing_qualified": False,
                "inputs_unchanged": before == after, "input_count": len(before), "results": results}
    (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
