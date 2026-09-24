"""Qualify generated SDK projects in a fresh retained directory (trusted local builds)."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "tool/morrow_plugin.py"

def project_command(arguments, sysroot=None):
    command = [sys.executable, "-B", "-X", "utf8", TOOL, arguments[0]]
    if sysroot is not None:
        command += ["--sysroot", sysroot]
    return [*command, *arguments[1:]]


def check_result(returncode, diagnostic, *, success=True, required=()):
    if not success and not required:
        raise ValueError("negative qualification must specify its expected diagnostic")
    if (returncode == 0) != success:
        raise RuntimeError(f"unexpected exit {returncode}")
    for text in required:
        if text not in diagnostic:
            raise RuntimeError("expected diagnostic missing: " + text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--sysroot", type=Path, help="WASI sysroot for C/C++ project builds")
    args = parser.parse_args()
    output = (args.output_root or ROOT / "build" / ("SDK projects 空间 " + uuid.uuid4().hex)).absolute()
    if output.exists() or output.is_symlink():
        parser.error("output root must not exist; previous evidence is never overwritten")
    output.mkdir(parents=True, exist_ok=False)
    output = output.resolve()
    print("EVIDENCE", output, flush=True)

    def run(name, arguments, success=True, required=()):
        with (output / (name + ".log")).open("x", encoding="utf-8") as log:
            result = subprocess.run([str(item) for item in arguments], cwd=ROOT, shell=False,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=1800)
        diagnostic = (output / (name + ".log")).read_text(encoding="utf-8", errors="replace")
        try:
            check_result(result.returncode, diagnostic, success=success, required=required)
        except (RuntimeError, ValueError) as error:
            raise RuntimeError(f"{name}: {error}; see {output}") from error

    def cli(name, *arguments, success=True, required=()):
        run(name, project_command(arguments, args.sysroot), success, required)

    run("unit", [sys.executable, "-B", "-X", "utf8", "-m", "unittest", "discover", "-s", ROOT / "tool/tests", "-p", "test_plugin_project*.py", "-v"])
    cli("doctor", "doctor")
    packages = {}
    for language in ("rust", "c", "cpp"):
        for kind in ("content", "transform", "ui", "dependency", "io"):
            key = f"{language}-{kind}"
            project = output / key
            cli(key + "-new", "new", project, "--language", language, "--kind", kind,
                "--id", "org.example." + key)
            cli(key + "-pack", "pack", project)
            archives = list((project / "dist").glob("*.mplugin"))
            if len(archives) != 1:
                raise RuntimeError("expected exactly one generated package: " + key)
            archive = archives[0]
            if archive.stem != hashlib.sha256(archive.read_bytes()).hexdigest():
                raise RuntimeError("package filename digest mismatch: " + key)
            packages[key] = archive
            print("PACK PASS", key, flush=True)

    run("execution", ["cargo", "run", "--locked", "--offline", "--release", "--manifest-path",
                      ROOT / "plugin_runtime/Cargo.toml", "--features", "packages", "--example",
                      "qualify_sdk_projects", "--", output])
    print("EXECUTION PASS 12 original generated packages; 3 IO packages prepared only", flush=True)
    binary = output / "input.bin"
    binary.write_bytes(b"a\0\xffz")
    for language in ("rust", "c", "cpp"):
        package = packages[language + "-transform"]
        target = output / (language + "-output.bin")
        cli(language + "-transform-success", "transform", package, "bytes.reverse", "bytes", "bytes", binary, target)
        if target.read_bytes() != b"z\xff\0a":
            raise RuntimeError("binary output mismatch")
        cli(language + "-transform-no-clobber", "transform", package, "bytes.reverse", "bytes", "bytes", binary, target, success=False, required=("output already exists; refusing to overwrite", "(exit code: 5)"))
        if target.read_bytes() != b"z\xff\0a":
            raise RuntimeError("existing output changed")
        failure = output / (language + "-failed-output.bin")
        cli(language + "-business-failure", "transform", package, "bytes.require-ascii", "bytes", "bytes", binary, failure, success=False, required=("BUSINESS_FAILURE code=unsupportedInput", "(exit code: 3)"))
        if failure.exists():
            raise RuntimeError("business failure published output")

    project = output / "rust-transform"
    def inventory():
        return {path.name: path.read_bytes() for path in (project / "dist").iterdir()}
    before = inventory()
    cli("repeat-pack", "pack", project)
    if inventory() != before:
        raise RuntimeError("unchanged rebuild changed package bytes")
    source = project / "src/lib.rs"
    source_bytes = source.read_bytes()
    module = project / "build/plugin.wasm"
    module_bytes = module.read_bytes()
    try:
        source.write_bytes(source_bytes + b'\ncompile_error!("intentional qualification failure");\n')
        cli("failed-current-build", "pack", project, success=False, required=("error: intentional qualification failure", "failed (exit 101)"))
        if inventory() != before or module.read_bytes() != module_bytes:
            raise RuntimeError("failed build modified prior artifacts")
    finally:
        source.write_bytes(source_bytes)
    archive = packages["rust-transform"]
    original = archive.read_bytes()
    try:
        archive.write_bytes(b"intentional corrupted existing archive")
        cli("corrupted-existing-package", "pack", project, success=False, required=('Error: Invalid("container header")', "PREPARED: checked only;", " install "))
        if archive.read_bytes() != b"intentional corrupted existing archive":
            raise RuntimeError("corrupted existing package overwritten")
    finally:
        archive.write_bytes(original)
    (output / "RESULT.txt").write_text(
        "PASS_SCOPED: 12 original generated packages executed and 3 IO packages prepared; binary CLI transforms, no-clobber, business failures, "
        "identical rebuild, failed-build stale-artifact rejection and corrupt-package preservation passed.\n"
        "Trusted local builds. Not Flutter pixels, independent third-party adoption or all-platform qualification.\n",
        encoding="utf-8")
    print("PASS_SCOPED", output / "RESULT.txt", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
