"""Explicit capture and read-only replay of bounded IO/service compatibility candidates.

Capture never replaces a baseline. Verify/run never build guests or repack originals.
This candidate is separate from guest-v1-rc1 and is not a whole-SDK stability claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "sdk/compat/transport-v1-rc1"
STEMS = {f"{language}-{kind}" for language in ("rust", "c", "cpp") for kind in ("io", "service")}
NAMES = {f"{stem}.{ext}" for stem in STEMS for ext in ("wasm", "mplugin")} | {
    "contracts/io.capnp", "contracts/service.capnp", "contracts/service_resources.capnp",
    "provenance.json", "SOURCE_SHA256SUMS",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(folder=BASELINE, expected_pin=None):
    folder = Path(folder)
    pin = expected_pin or folder.with_suffix(".sha256").read_text(encoding="ascii").strip()
    raw = (folder / "SHA256SUMS").read_bytes()
    if not re.fullmatch(r"[0-9a-f]{64}", pin) or sha(raw) != pin:
        raise ValueError("transport baseline root mismatch; do not reseal")
    entries = {}
    for line in raw.decode("ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_./-]+)", line)
        if not match or match[2] not in NAMES or match[2] in entries:
            raise ValueError("invalid transport manifest entry")
        entries[match[2]] = match[1]
    paths = list(folder.rglob("*"))
    if folder.is_symlink() or any(p.is_symlink() for p in paths):
        raise ValueError("transport baseline cannot contain symlinks")
    if set(entries) != NAMES or {p.relative_to(folder).as_posix() for p in paths if p.is_file()} != NAMES | {"SHA256SUMS"}:
        raise ValueError("unexpected or missing transport original")
    for name, digest in entries.items():
        if sha((folder / name).read_bytes()) != digest:
            raise ValueError("transport original changed: " + name)
    return len(entries)


def capture(args):
    folder = args.directory.resolve()
    if folder.exists() or folder.with_suffix(".sha256").exists():
        raise ValueError("capture requires a new baseline ID; existing originals are immutable")
    sources = ["sdk/rust", "sdk/c", "sdk/cpp", "sdk/examples", "core/examples/plugin_package.rs",
               "tool/morrow_plugin.py", "tool/plugin_transport_baseline.py"]
    subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", *sources], cwd=ROOT, check=True)
    names = subprocess.check_output(["git", "ls-files", "--", *sources], cwd=ROOT, text=True).splitlines()
    files = {}
    qualifications = {}
    for kind, source in (("io", args.io), ("service", args.service)):
        source = source.resolve()
        records = json.loads((source / "packages.json").read_text(encoding="utf-8"))
        if set(records) != {"rust", "c", "cpp"}:
            raise ValueError("all three qualified original packages required")
        for language, record in records.items():
            package = Path(record["path"]).read_bytes()
            if sha(package) != record["sha256"]:
                raise ValueError("qualified original package changed")
            files[f"{language}-{kind}.mplugin"] = package
            files[f"{language}-{kind}.wasm"] = (source / language / "build/plugin.wasm").read_bytes()
        qualifications[kind] = records
    for name in ("io.capnp", "service.capnp", "service_resources.capnp"):
        files["contracts/" + name] = (ROOT / "sdk/rust/contracts" / name).read_bytes()
    files["SOURCE_SHA256SUMS"] = "".join(sha((ROOT / name).read_bytes()) + "  " + name + "\n" for name in sorted(names)).encode()
    files["provenance.json"] = (json.dumps({
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "platform": sys.platform, "packages": qualifications,
        "scope": "Bounded HTTP IO and finite service runs. Resource codec is recorded, not exercised by the echo guest. No whole-SDK, native ABI, Flutter or cross-platform promise.",
        "tools": {name: subprocess.check_output(command, cwd=ROOT, text=True).strip()
                  for name, command in {"rustc": ["rustc", "-Vv"], "cargo": ["cargo", "-V"], "clang": ["clang", "--version"], "capnp": ["capnp", "--version"]}.items()},
    }, indent=2) + "\n").encode()
    assert set(files) == NAMES
    folder.mkdir(parents=True)
    for name, data in files.items():
        path = folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as out:
            out.write(data)
    raw = "".join(sha(files[name]) + "  " + name + "\n" for name in sorted(files)).encode()
    (folder / "SHA256SUMS").write_bytes(raw)
    with folder.with_suffix(".sha256").open("x", encoding="ascii") as out:
        out.write(sha(raw) + "\n")
    print("Captured", verify(folder), "original files at", folder)


def run(args):
    verify(args.directory)
    env = os.environ.copy()
    folder = args.directory.resolve()
    env["MORROW_TRANSPORT_BASELINE"] = str(folder)
    for language in ("rust", "c", "cpp"):
        env["MORROW_SERVICE_PACKAGE_" + language.upper()] = str(folder / (language + "-service.mplugin"))
        env["MORROW_SDK_IO_PACKAGE_" + language.upper()] = str(folder / (language + "-io.mplugin"))
    cargo = ["cargo", "test", "--locked", *([] if args.allow_network else ["--offline"])]
    subprocess.run([*cargo, "--manifest-path", "plugin_runtime/Cargo.toml", "--features", "packages",
                    "--target-dir", str(args.build_root / "target-runtime"), "--test", "sdk_transport_frozen"],
                   cwd=ROOT, env=env, check=True)
    for test, selector in (("sdk_service_guest", None), ("managed_http", "sdk_http_guest")):
        command = [*cargo, "--manifest-path", "network_node/Cargo.toml", "--features", "plugin-adapter",
                   "--target-dir", str(args.build_root / "target-network"), "--test", test]
        if selector:
            command.append(selector)
        subprocess.run([*command, "--", "--ignored", "--nocapture"], cwd=ROOT, env=env, check=True)
    verify(args.directory)
    print("PASS: six original transport packages through current host; no guest build/repack")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for action in ("verify", "run", "capture"):
        command = sub.add_parser(action)
        command.add_argument("--directory", type=Path, default=BASELINE)
        if action == "capture":
            command.add_argument("--io", type=Path, required=True)
            command.add_argument("--service", type=Path, required=True)
        elif action == "run":
            command.add_argument("--build-root", type=Path, default=ROOT / "build/transport-baseline")
            command.add_argument("--allow-network", action="store_true")
    args = parser.parse_args()
    if args.command == "capture":
        capture(args)
    elif args.command == "run":
        run(args)
    else:
        print("PASS:", verify(args.directory), "pinned transport originals")


if __name__ == "__main__":
    main()
