"""Check a prepared controller against its actual four role files, before VM I/O.

This mechanical check has no runtime, credential, policy or cleanup authority.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re


def pin(path: Path) -> dict:
    raw = path.read_bytes()
    return {"path": str(path), "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def verify_pin(row: dict) -> None:
    actual = pin(Path(row["path"]))
    if any(actual[key] != row[key] for key in ("bytes", "sha256")):
        raise ValueError(f"File bytes differ: {row['path']}")


def verify(stage: Path) -> dict:
    plan = json.loads((stage / "stage-plan.json").read_bytes())
    roles = {row["role"]: row for row in plan["artifacts"] if "role" in row}
    if set(roles) != set("PCRS"):
        raise ValueError("Exactly four actual roles are required")
    for row in plan["artifacts"] + plan["source_inputs"]:
        verify_pin(row)
    source = stage / "controller-source"
    io = (source / "Root47Controller1725.IO.cs").read_text(encoding="utf-8-sig")
    shas = re.findall(r'internal static readonly string\[\] Sha=\{([^}]+)\};', io)
    sizes = re.findall(r'internal static readonly ulong\[\] Bytes=\{([0-9,]+)\};', io)
    if len(shas) != 1 or len(sizes) != 1:
        raise ValueError("Controller role table is not unique")
    actual_shas = re.findall(r'"([0-9a-f]{64})"', shas[0])
    actual_sizes = [int(value) for value in sizes[0].split(",")]
    if actual_shas != [roles[key]["sha256"] for key in "PCRS"]:
        raise ValueError("Controller embeds a different role SHA256 table")
    if actual_sizes != [roles[key]["bytes"] for key in "PCRS"]:
        raise ValueError("Controller embeds a different role byte-count table")
    pure = (source / "RootPPrebirth1682.Pure.cs").read_text(encoding="utf-8-sig")
    inventories = re.findall(r'FrozenInventory\s*=\s*"([0-9a-f]{64})"', pure)
    if inventories != [roles["P"]["inventory"]]:
        raise ValueError("Controller source inventory differs from actual P inputs")
    # P supplies the harness argv through its original private launcher. The
    # controller's own selector alone cannot rebind that separately compiled
    # identity. Compare it before opening a guest connection.
    build_sets = json.loads((source / "BUILD_SETS2762.json").read_bytes())
    build = Path(build_sets["original_build"])
    original = json.loads((build / "canonical-snapshot.json").read_bytes())
    source_manifest = original["files"]
    source_digest = hashlib.sha256(
        json.dumps(source_manifest, sort_keys=True).encode()
    ).hexdigest()
    if source_digest != roles["P"]["inventory"]:
        raise ValueError("Original P snapshot differs from its staged inventory")
    native_name = (
        "companions/morrow-codex/derived/c28-cleanup-com-002/codex-rs/"
        "windows-sandbox-rs/src/asuser_parent_native.rs"
    )
    native_path = Path(original["root"]) / native_name
    native = native_path.read_bytes()
    if hashlib.sha256(native).hexdigest() != source_manifest[native_name]:
        raise ValueError("Original P launcher source changed after compilation")
    expected = re.findall(
        rb'"--expected-guest-uuid"\.into\(\),"([0-9a-f-]{36})"\.into\(\)',
        native,
    )
    if len(expected) != 1 or expected[0].decode() != plan["firmware_uuid"]:
        raise ValueError("Original P harness UUID differs from selected VM firmware")
    controller_pure = (source / "Root47Controller1725.Pure.cs").read_text(
        encoding="utf-8-sig"
    )
    controller_uuid = re.findall(
        r'internal const string Firmware="([0-9a-f-]{36})";', controller_pure
    )
    if controller_uuid != [plan["firmware_uuid"]]:
        raise ValueError("Compiled controller UUID differs from selected VM firmware")
    original_p = json.loads((build / "P/result.json").read_bytes())
    verify_pin(original_p["artifact"])
    if (original_p["exit_code"] != 0
        or original_p["input_manifest_sha256"] != source_digest
        or any(
        original_p["artifact"][key] != roles["P"][key]
        for key in ("bytes", "sha256")
    )):
        raise ValueError("Staged P differs from original compiler artifact")
    dependency_file = (build / "P/original-dependencies.d").read_text(
        encoding="utf-8"
    ).replace("\\", "/")
    if str(native_path).replace("\\", "/") not in dependency_file:
        raise ValueError("P compiler did not record the original launcher input")
    report = json.loads((stage / "controller/report.json").read_bytes())
    controllers = [row for row in report["rows"] if row["kind"] == "controller"]
    if len(controllers) != 1 or controllers[0]["compiler_exit"] != 0:
        raise ValueError("Original controller compilation did not return zero")
    compiled = controllers[0]
    verify_pin(compiled["artifact"])
    staged = next(row for row in plan["artifacts"] if row["name"] == "Root47Controller1725.exe")
    if any(compiled["artifact"][key] != staged[key] for key in ("bytes", "sha256")):
        raise ValueError("Staged controller differs from its compilation")
    inputs = {Path(row["path"]).name: row for row in compiled["sources"]}
    for row in inputs.values():
        verify_pin(row)
    for name in (
        "Root47Controller1725.IO.cs",
        "RootPPrebirth1682.Pure.cs",
        "Root47Controller1725.Pure.cs",
    ):
        if name not in inputs or Path(inputs[name]["path"]).resolve() != (source / name).resolve():
            raise ValueError("Role/inventory constants are outside original compiler inputs")
    return {
        "status": "PASS_MECHANICAL_ROLE_AND_COMPILER_INPUT_BINDING",
        "tag": plan["tag"],
        "vm_id": plan["vm_id"],
        "plan": pin(stage / "stage-plan.json"),
        "controller": compiled["artifact"],
        "parent_harness_firmware_uuid": expected[0].decode(),
        "vm_calls": 0,
        "source_accepted": False,
        "release_eligible": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.stage)
        code = 0
    except (OSError, ValueError, KeyError, StopIteration) as error:
        result = {"status": "REJECTED", "reason": str(error), "vm_calls": 0}
        code = 2
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        with args.output.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(text)
    print(text, end="")
    return code


if __name__ == "__main__":
    raise SystemExit(main())
