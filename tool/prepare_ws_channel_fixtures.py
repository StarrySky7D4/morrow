#!/usr/bin/env python3
"""Prepare NEW three-language W15 WS guests outside the repository; never build or reseal originals.

The independent small-message Capnp layout is checked by tests/ws_envelope.rs against
native encode. These fixtures exercise the old opaque channel ABI, not a public WS grant.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "network_node_stream_001/tests/fixtures/ws_guest"
SCHEMA = ROOT / "network_node_stream_001/schemas/ws_message.capnp"


def envelope(kind: int, payload: bytes, digest: bytes, close_code: int | None = None) -> bytes:
    if len(digest) != 32 or len(payload) > 125 or kind not in (0, 1, 4):
        raise ValueError("only three bounded qualification vectors are supported")
    if close_code is not None and kind != 4:
        raise ValueError("close code requires Close")
    total = 72 + ((len(payload) + 7) & ~7)
    raw = bytearray(total)
    for at, value in (
        (0, (total // 8 - 1) << 32), (8, (1 << 32) | (2 << 48)),
        (16, 1 | (kind << 16) | (int(close_code is not None) << 32) | ((close_code or 0) << 48)),
        (24, 5 | (2 << 32) | (32 << 35)), (32, 17 | (2 << 32) | (len(payload) << 35)),
    ):
        struct.pack_into("<Q", raw, at, value)
    raw[40:72] = digest
    raw[72:72 + len(payload)] = payload
    return bytes(raw)


def prepare(destination: Path) -> dict:
    if ".." in destination.parts:
        raise ValueError("destination must not contain parent traversal")
    destination = destination.absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError("destination must be new; no existing source or artifacts are replaced")
    for parent in destination.parents:
        if parent.is_symlink():
            raise ValueError("destination ancestors must not be symlinks")
    if destination == ROOT or ROOT in destination.parents:
        raise ValueError("generated projects must be outside the source repository")
    digest = hashlib.sha256(SCHEMA.read_bytes()).digest()
    vectors = [envelope(0, "W15 雪 🙂".encode(), digest), envelope(1, bytes([0, 255, 128, 10, 13, 0, 37]), digest), envelope(4, b"", digest, 1000)]
    arrays = [", ".join(str(x) for x in b) for b in vectors]
    rust = "// Generated NEW fixture payloads; verify against native ws_envelope tests.\n"
    rust += "const PAYLOADS: [&[u8]; 3] = [\n" + "".join("    &[" + x + "],\n" for x in arrays) + "];\n"
    c = "/* Generated NEW fixture payloads; not SDK contracts or frozen originals. */\n#include <stdint.h>\n"
    c += "".join(f"static const uint8_t ws_payload_{i}[] = {{{x}}};\n" for i, x in enumerate(arrays))
    c += "static const uint8_t *const ws_payloads[] = {ws_payload_0, ws_payload_1, ws_payload_2};\n"
    c += "static const uint32_t ws_lengths[] = {" + ", ".join(str(len(x)) for x in vectors) + "};\n"
    destination.mkdir(parents=True)
    for language, template, source in (("rust", "plugin.rs", "lib.rs"), ("c", "plugin.c", "plugin.c"), ("cpp", "plugin.cpp", "plugin.cpp")):
        project = destination / language
        subprocess.run([sys.executable, str(ROOT / "tool/morrow_plugin.py"), "new", str(project),
                        "--language", language, "--kind", "channel", "--lock-sdk", "--id", "org.example.w15.ws." + language], check=True)
        (project / "src" / source).write_bytes((FIXTURES / template).read_bytes())
        (project / "src" / ("payloads.rs" if language == "rust" else "payloads.h")).write_text(rust if language == "rust" else c, encoding="utf-8")
        config = (project / "plugin.toml").read_text()
        config = config.replace('name = "channel.exercise"', 'name = "channel.ws.duplex"').replace("host_calls = 16", "host_calls = 64")
        (project / "plugin.toml").write_text(config, encoding="utf-8")
        subprocess.run([sys.executable, str(ROOT / "tool/morrow_plugin.py"), "validate", str(project), "--require-sdk-lock"], check=True)
    records = {p.relative_to(destination).as_posix(): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
               for p in sorted(destination.rglob("*")) if p.is_file()}
    result = {"status": "NEW_SOURCE_PREPARED_NOT_COMPILED_OR_EXECUTED", "schema_sha256": digest.hex(),
              "vectors": [{"bytes": len(v), "sha256": hashlib.sha256(v).hexdigest(), "hex": v.hex()} for v in vectors],
              "source_files": records, "sdk_originals_rebuilt": False}
    (destination / "source-manifest.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    result = prepare(args.destination)
    print(json.dumps({"status": result["status"], "source_files": len(result["source_files"])}))


if __name__ == "__main__":
    main()
