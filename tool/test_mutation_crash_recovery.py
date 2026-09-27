"""Run isolated Windows host mutation crash/recovery cases via real Flutter IO.

The caller supplies separately built normal/fault hosts and checked local plugin
fixtures. No builds, package downloads, user-store access or process discovery.
Each test creates and cleans its own temporary protected library.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
POINTS = ("after-claim", "after-effect", "after-observe")


def file_path(value: str) -> Path:
    try:
        path = Path(value).resolve(strict=True)
    except OSError as error:
        raise argparse.ArgumentTypeError(str(error)) from error
    if not path.is_file():
        raise argparse.ArgumentTypeError(f"Not a file: {path}")
    return path


def sha256(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dart", type=file_path, required=True)
    parser.add_argument("--flutter-tool", type=file_path, required=True)
    parser.add_argument("--host", type=file_path, required=True,
                        help="Dedicated fault-injection host executable")
    parser.add_argument("--normal-host", type=file_path, required=True,
                        help="Normal host executable, for a negative control")
    parser.add_argument("--package", type=file_path, required=True,
                        help="Built-in workbench package")
    parser.add_argument("--fixture", type=file_path, required=True,
                        help="Local plugin declaring file-create and file-delete")
    parser.add_argument("--suite", choices=("native", "widget"), default="native",
                        help="Exercise the recovery session or actual recovery widget buttons")
    parser.add_argument("--output", type=Path,
                        help="Evidence directory (separate defaults for native and widget suites)")
    parser.add_argument("--case", choices=[
        *(f"{kind}-{point}" for kind in ("create", "delete") for point in POINTS),
        "normal-create-after-claim",
    ], help="Run one case when diagnosing; omit for the complete matrix")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This qualification requires Windows and real native file effects")
    if args.host == args.normal_host:
        parser.error("Normal and fault-injection hosts must be separate executables")
    hashes = {"fault_host": sha256(args.host), "normal_host": sha256(args.normal_host),
              "package": sha256(args.package), "fixture": sha256(args.fixture)}
    if hashes["fault_host"] == hashes["normal_host"]:
        parser.error("Normal and fault host hashes are identical; build separate configurations")
    test_file = ("test/mutation_recovery_crash_native_test.dart" if args.suite == "native"
                 else "test/mutation_recovery_crash_widget_native_test.dart")
    output = (args.output or ROOT / ("build/mutation-crash-matrix" if args.suite == "native"
                                  else "build/mutation-crash-widget-matrix")).resolve()
    output.mkdir(parents=True, exist_ok=True)
    cases = [(f"{kind}-{point}", kind, point, True)
             for kind in ("create", "delete") for point in POINTS]
    cases.append(("normal-create-after-claim", "create", "after-claim", False))
    if args.case:
        cases = [case for case in cases if case[0] == args.case]
    results = []
    for name, kind, point, crash in cases:
        env = os.environ.copy()
        for key in ("MORROW_FILE_CREATE_FAULT", "MORROW_FILE_DELETE_FAULT"):
            env.pop(key, None)
        env.update({
            "MORROW_MUTATION_CRASH_HOST": str(args.host if crash else args.normal_host),
            "MORROW_WORKBENCH_PACKAGE": str(args.package),
            "MORROW_MUTATION_TASK_PACKAGE": str(args.fixture),
            "MORROW_MUTATION_CRASH_KIND": kind,
            "MORROW_MUTATION_CRASH_POINT": point,
            "MORROW_MUTATION_EXPECT_CRASH": "1" if crash else "0",
            f"MORROW_FILE_{kind.upper()}_FAULT": point,
        })
        command = [str(args.dart), str(args.flutter_tool), "--no-version-check",
                   "--suppress-analytics", "test", "--no-pub",
                   test_file]
        log = output / f"{name}.log"
        print(f"Running {name}", flush=True)
        # A timeout is a failed case, not evidence of a host crash. The test
        # itself owns its process and asserts actual exit before recovery.
        try:
            with log.open("w", encoding="utf-8") as stream:
                result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream,
                                        stderr=subprocess.STDOUT, timeout=240)
            code = result.returncode
        except subprocess.TimeoutExpired:
            code = -1
        evidence = log.read_text(encoding="utf-8", errors="replace")
        passed = code == 0 and "All tests passed!" in evidence and "Skip:" not in evidence
        results.append({"case": name, "passed": passed, "exit_code": code,
                        "log": str(log)})
        print(f"{name}: {'PASS' if passed else 'FAIL'}", flush=True)
    summary = {"scope": "Windows real native mutation crash recovery",
               "suite": args.suite, "test_file": test_file,
               "complete_matrix": args.case is None, "artifacts": hashes,
               "results": results}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n",
                                          encoding="utf-8")
    return 0 if all(row["passed"] for row in results) else 1


if __name__ == "__main__":
    sys.exit(main())
