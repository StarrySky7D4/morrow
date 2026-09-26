"""Local service SDK gate: generated original packages through a real TCP node.

Uses trusted local toolchains. Does not publish, enable installed plugins or use CI.
All three languages are required; missing tools/builds fail rather than skip.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]


def load_original_packages(manifest, package_root=None):
    """Validate every explicit original before running tools or creating evidence."""
    manifest = Path(manifest)
    if manifest.stat().st_size > 65536:
        raise ValueError("original manifest exceeds 64 KiB")
    records = json.loads(manifest.read_text(encoding="utf-8"))
    if not isinstance(records, dict) or set(records) != {"rust", "c", "cpp"}:
        raise ValueError("all three original packages required")
    packages = {}
    for language, record in records.items():
        if not isinstance(record, dict) or set(record) != {"path", "sha256"}:
            raise ValueError("invalid original package entry")
        digest = record["sha256"]
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError("invalid original package digest")
        if not isinstance(record["path"], str) or not record["path"]:
            raise ValueError("invalid original package path")
        # Explicit relocation uses a fixed layout and the pinned hash, never a
        # file picked by recency or an arbitrary basename from the old machine.
        path = (Path(package_root) / language / "dist" / (digest + ".mplugin") if package_root else Path(record["path"]))
        if not path.is_absolute():
            path = manifest.resolve().parent / path
        if not path.is_file() or path.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("missing or oversized original package: " + language)
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError("original package hash mismatch: " + language)
        packages[language] = {"path": str(path.resolve()), "sha256": digest}
    return packages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sysroot", type=Path, help="required only when generating new guests")
    parser.add_argument("--original-packages", type=Path, help="replay a previously qualified packages.json; never build or repack these guests")
    parser.add_argument("--package-root", type=Path, help="explicit relocation root containing LANGUAGE/dist/SHA256.mplugin")
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--build-root", type=Path, help="optional reusable Cargo cache; evidence still uses a new output root")
    parser.add_argument("--service-http", action="store_true", help="qualify the public-SDK outbound service starter instead of echo")
    parser.add_argument("--native", action="store_true", help="also require Linux native C/C++ handle checks (cc/c++)")
    parser.add_argument("--allow-network", action="store_true", help="allow Cargo dependency downloads")
    args = parser.parse_args()
    if not args.original_packages and not args.sysroot:
        parser.error("--sysroot required unless --original-packages is supplied")
    if args.package_root and not args.original_packages:
        parser.error("--package-root requires --original-packages")
    packages = load_original_packages(args.original_packages, args.package_root.resolve() if args.package_root else None) if args.original_packages else {}
    if args.native and sys.platform != "linux":
        parser.error("--native currently qualifies the Linux shared-library ABI only")
    output = (args.output_root or ROOT / "build" / ("service-sdk-" + uuid.uuid4().hex)).resolve()
    if output.exists():
        parser.error("output root must not exist; preserve previous evidence")
    output.mkdir(parents=True)
    env = os.environ.copy()
    build = (args.build_root or output).resolve()

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
    run("transport-originals", [sys.executable, "tool/plugin_transport_baseline.py", "run", "--build-root", build,
                                *(["--allow-network"] if args.allow_network else [])])
    run("sdk", [*cargo, "--manifest-path", "sdk/rust/Cargo.toml", "--target-dir", build / "target-sdk"])
    run("host-codec-and-frozen", [*cargo, "--manifest-path", "plugin_runtime/Cargo.toml", "--features", "packages",
                                 "--target-dir", build / "target-runtime",
                                 "--test", "sdk_service_codec", "--test", "sdk_frozen_compat", "--test", "sdk_frozen_dependency"])
    if args.native:
        run("native-library", ["cargo", "build", "--locked", *([] if args.allow_network else ["--offline"]),
                               "--manifest-path", "sdk/rust/Cargo.toml", "--target-dir", build / "target-sdk"])
        for language, compiler, standard in (("c", "/usr/bin/cc", "c11"), ("cpp", "/usr/bin/c++", "c++17")):
            executable = output / (language + "-service-native")
            library = build / "target-sdk/debug"
            run(language + "-native-build", [compiler, "-std=" + standard, "-Wall", "-Wextra", "-Werror",
                "-I", "sdk/c/include", "-I", "sdk/cpp/include", "sdk/tests/" + language + "_service." + language,
                "-L", library, "-lmorrow_plugin_sdk", "-Wl,-rpath," + str(library), "-o", executable])
            env["MORROW_SDK_SERVICE_NATIVE_" + language.upper()] = str(executable)
        run("native-codec", [*cargo, "--manifest-path", "plugin_runtime/Cargo.toml", "--features", "packages",
                             "--target-dir", build / "target-runtime", "--test", "sdk_service_codec", "--", "--ignored"])
    if not args.original_packages:
        for language in ("rust", "c", "cpp"):
            project = output / language
            common = ["--sysroot", args.sysroot] + (["--allow-network"] if args.allow_network else [])
            run(language + "-new", [sys.executable, "tool/morrow_plugin.py", "new", project, *common,
                                    "--language", language, "--kind", "service", "--id", "org.example.service." + language,
                                    "--service-run-ms", "120000", "--service-run-jobs", "64" if args.service_http else "16", "--service-run-bytes", "4194304",
                                    *(["--service-http"] if args.service_http else [])])
            run(language + "-pack", [sys.executable, "tool/morrow_plugin.py", "pack", project, *common])
            archives = list((project / "dist").glob("*.mplugin"))
            if len(archives) != 1:
                raise RuntimeError("expected exactly one generated original package: " + language)
            archive = archives[0]
            packages[language] = {"path": str(archive), "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}
    for language, package in packages.items():
        env["MORROW_SERVICE_PACKAGE_" + language.upper()] = package["path"]
    run("real-node", [*cargo, "--manifest-path", "network_node/Cargo.toml", "--features", "plugin-adapter",
                      "--target-dir", build / "target-network",
                      "--test", "sdk_service_http" if args.service_http else "sdk_service_guest", "--", "--ignored", "--nocapture"])
    for package in packages.values():
        if hashlib.sha256(Path(package["path"]).read_bytes()).hexdigest() != package["sha256"]:
            raise RuntimeError("original package changed during qualification")
    run("frozen-after", [sys.executable, "tool/verify_plugin_sdk_baseline.py"])
    run("transport-after", [sys.executable, "tool/plugin_transport_baseline.py", "verify"])
    (output / "packages.json").write_text(json.dumps(packages, indent=2) + "\n", encoding="utf-8")
    exclusions = (["Synthetic credential resolver only; this gate does not qualify Windows DPAPI. Both low-level and configured service wiring are exercised with synthetic credentials."] if args.service_http else []) + ["No Flutter UI, TLS or other-platform qualification; recovery covers observed/unknown history, not application transaction reconciliation."]
    if not args.native:
        exclusions.append("Native C/C++ executable codec test requires --native or a separate explicit run.")
    if sys.platform != "win32":
        exclusions.append("Windows-only frozen dependency tests were not run; this is not the full guest-v1-rc1 gate.")
    (output / "scope.json").write_text(json.dumps({"platform": sys.platform, "template": "service-http" if args.service_http else "service", "guest_source": "original-packages" if args.original_packages else "generated", "exclusions": exclusions}, indent=2) + "\n", encoding="utf-8")
    print("PASS: three original service packages, real node and available frozen guest checks; evidence:", output)
    for exclusion in exclusions:
        print("OUTSIDE THIS GATE:", exclusion)


if __name__ == "__main__":
    main()
