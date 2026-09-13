"""One-time sealing after explicit guest build and capture; never used by verify."""
import argparse
import hashlib
import platform
import re
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding="utf-8").strip()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="new capture directory (26 original binary files)")
    args = parser.parse_args()
    folder = args.directory.resolve()
    pin = folder.with_suffix(".sha256")
    if pin.exists() or (folder / "SHA256SUMS").exists():
        raise SystemExit("Refusing to reseal an existing baseline. Create a new reviewed baseline ID.")
    from verify_plugin_sdk_baseline import BINARY_NAMES, CONTRACT_NAMES
    if {p.name for p in folder.iterdir()} != BINARY_NAMES:
        raise SystemExit("Capture must contain exactly the 13 Wasm/package pairs")
    subprocess.run(["git", "diff", "--exit-code", "HEAD", "--", "sdk/rust", "sdk/c", "sdk/cpp", "sdk/examples"], cwd=ROOT, check=True)
    (folder / "contracts").mkdir()
    for name in CONTRACT_NAMES:
        text = (ROOT / "sdk/rust/contracts" / name).read_text(encoding="utf-8")
        (folder / "contracts" / name).write_text(text, encoding="utf-8", newline="\n")
    source_names = output("git", "ls-files", "sdk/rust", "sdk/c", "sdk/cpp", "sdk/examples", "tool/build_plugin_c_wasm.ps1", "tool/build_plugin_dependency_wasm.ps1", "core/examples/capture_sdk_baseline.rs", "tool/seal_plugin_sdk_baseline.py").splitlines()
    source_sums = "".join(hashlib.sha256((ROOT / n).read_bytes()).hexdigest() + "  " + n + "\n" for n in sorted(source_names))
    (folder / "SOURCE_SHA256SUMS").write_text(source_sums, encoding="utf-8", newline="\n")
    def crate_version(path):
        match = re.search(r'^version\s*=\s*"([^"\n]+)"', (ROOT / path).read_text(encoding="utf-8"), re.MULTILINE)
        if match is None:
            raise ValueError("Missing source crate version: " + path)
        return match.group(1)
    def contract_version(name):
        return str(int((ROOT / "sdk/rust/contracts" / name).read_text(encoding="ascii").strip()))
    provenance = ["baseline=" + folder.name, "sdk_source_commit=" + output("git", "rev-parse", "HEAD"),
                  "sdk_crate=" + crate_version("sdk/rust/Cargo.toml"), "package_semver=1.0.0 (fixture identity only)",
                  "guest_abi=2", "runtime_protocol=" + contract_version("version.txt"), "task_protocol=" + contract_version("task-version.txt"), "ui_protocol=" + contract_version("ui-version.txt"), "dependency_call_protocol=1 (capture profile)",
                  "host_at_capture=" + platform.platform() + "; core=" + crate_version("core/Cargo.toml") + "; canonical contract snapshots use LF",
                  "capture=Fresh guest builds; no deterministic/reproducible-build claim",
                  "rustc:\n" + output("rustc", "-Vv"), "cargo: " + output("cargo", "-V"),
                  "clang:\n" + output("clang", "--version"), "capnp: " + output("capnp", "--version"),
                  "wasi_sysroot=34.0; C++ no exceptions/no RTTI; no WASI runtime imports",
                  "build=tool/build_plugin_c_wasm.ps1; tool/build_plugin_dependency_wasm.ps1",
                  "rust_build=cargo build --offline --locked --manifest-path sdk/examples/rust-{task,transform,ui,dependency-caller,chain-provider}/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/plugin-guest",
                  "pack=cargo run --offline --locked --release --manifest-path core/Cargo.toml --example capture_sdk_baseline -- NEW_DIRECTORY",
                  "seal=python tool/seal_plugin_sdk_baseline.py NEW_DIRECTORY"]
    (folder / "provenance.txt").write_text("\n".join(provenance) + "\n", encoding="utf-8", newline="\n")
    sums = "".join(hashlib.sha256(p.read_bytes()).hexdigest() + "  " + p.relative_to(folder).as_posix() + "\n" for p in sorted(folder.rglob("*")) if p.is_file())
    (folder / "SHA256SUMS").write_text(sums, encoding="utf-8", newline="\n")
    with pin.open("x", encoding="utf-8", newline="\n") as f:
        f.write(hashlib.sha256(sums.encode()).hexdigest() + "\n")
    print("Sealed", folder, "root", pin.read_text().strip())

if __name__ == "__main__":
    main()
