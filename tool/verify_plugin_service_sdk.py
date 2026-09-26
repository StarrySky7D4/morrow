"""Local service SDK gate: generated original packages through a real TCP node.

Uses trusted local toolchains. Does not publish, enable installed plugins or use CI.
All three languages are required; missing tools/builds fail rather than skip.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--allow-network", action="store_true", help="allow Cargo dependency downloads")
    args = parser.parse_args()
    output = (args.output_root or ROOT / "build" / ("service-sdk-" + uuid.uuid4().hex)).resolve()
    if output.exists():
        parser.error("output root must not exist; preserve previous evidence")
    output.mkdir(parents=True)
    env = os.environ.copy()

    def run(name, command):
        print(name, flush=True)
        with (output / (name + ".log")).open("x", encoding="utf-8") as log:
            result = subprocess.run([str(x) for x in command], cwd=ROOT, env=env,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=1800, check=False)
        if result.returncode:
            raise RuntimeError(f"{name} failed ({result.returncode}); see {output / (name + '.log')}")

    cargo = ["cargo", "test", "--locked"] + ([] if args.allow_network else ["--offline"])
    run("contracts", [sys.executable, "tool/sync_plugin_sdk_contracts.py", "--check"])
    run("frozen-before", [sys.executable, "tool/verify_plugin_sdk_baseline.py"])
    run("sdk", [*cargo, "--manifest-path", "sdk/rust/Cargo.toml", "--target-dir", output / "target-sdk"])
    run("host-codec-and-frozen", [*cargo, "--manifest-path", "plugin_runtime/Cargo.toml", "--features", "packages",
                                 "--target-dir", output / "target-runtime",
                                 "--test", "sdk_service_codec", "--test", "sdk_frozen_compat", "--test", "sdk_frozen_dependency"])
    packages = {}
    for language in ("rust", "c", "cpp"):
        project = output / language
        common = ["--sysroot", args.sysroot] + (["--allow-network"] if args.allow_network else [])
        run(language + "-new", [sys.executable, "tool/morrow_plugin.py", "new", project, *common,
                                "--language", language, "--kind", "service", "--id", "org.example.service." + language])
        run(language + "-pack", [sys.executable, "tool/morrow_plugin.py", "pack", project, *common])
        archives = list((project / "dist").glob("*.mplugin"))
        if len(archives) != 1:
            raise RuntimeError("expected exactly one generated original package: " + language)
        archive = archives[0]
        packages[language] = {"path": str(archive), "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}
        env["MORROW_SERVICE_PACKAGE_" + language.upper()] = str(archive)
    run("real-node", [*cargo, "--manifest-path", "network_node/Cargo.toml", "--features", "plugin-adapter",
                      "--target-dir", output / "target-network",
                      "--test", "sdk_service_guest", "--", "--ignored", "--nocapture"])
    for package in packages.values():
        if hashlib.sha256(Path(package["path"]).read_bytes()).hexdigest() != package["sha256"]:
            raise RuntimeError("generated package changed during qualification")
    run("frozen-after", [sys.executable, "tool/verify_plugin_sdk_baseline.py"])
    (output / "packages.json").write_text(json.dumps(packages, indent=2) + "\n", encoding="utf-8")
    exclusions = ["Native C/C++ executable codec test requires a separate explicit run.",
                  "No Flutter UI, TLS, long-running service profile, recovery or other-platform qualification."]
    if sys.platform != "win32":
        exclusions.append("Windows-only frozen dependency tests were not run; this is not the full guest-v1-rc1 gate.")
    (output / "scope.json").write_text(json.dumps({"platform": sys.platform, "exclusions": exclusions}, indent=2) + "\n", encoding="utf-8")
    print("PASS: generated original service packages, real node and available frozen guest checks; evidence:", output)
    for exclusion in exclusions:
        print("OUTSIDE THIS GATE:", exclusion)


if __name__ == "__main__":
    main()
