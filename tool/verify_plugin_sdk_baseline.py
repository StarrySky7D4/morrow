"""Read-only integrity gate for the original SDK guest compatibility fixtures."""
import hashlib
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "sdk/compat/guest-v1-rc1"
CONTRACT_NAMES = {"runtime.capnp", "content.proto", "task.capnp", "ui.capnp", "dependency_call.capnp", "version.txt", "task-version.txt", "ui-version.txt"}
STEMS = {f"{language}-{profile}" for language in ("rust", "c", "cpp") for profile in ("task", "transform", "ui", "dependency")} | {"rust-provider"}
BINARY_NAMES = {f"{stem}.{suffix}" for stem in STEMS for suffix in ("wasm", "mplugin")}
EXPECTED_NAMES = BINARY_NAMES | {f"contracts/{name}" for name in CONTRACT_NAMES} | {"SOURCE_SHA256SUMS", "provenance.txt"}

def verify(folder=BASELINE, expected_pin=None):
    folder = Path(folder)
    if expected_pin is None:
        expected_pin = BASELINE.with_suffix(".sha256").read_text(encoding="ascii").strip()
    if not re.fullmatch(r"[0-9a-f]{64}", expected_pin):
        raise ValueError("Invalid baseline root pin")
    raw = (folder / "SHA256SUMS").read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected_pin:
        raise ValueError("Baseline manifest root mismatch; do not refresh/reseal fixtures")
    entries = {}
    for line in raw.decode("ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_./-]+)", line)
        if not match:
            raise ValueError("Invalid checksum entry")
        digest, name = match.groups()
        if name not in EXPECTED_NAMES or name in entries:
            raise ValueError("Unexpected or duplicate fixture: " + name)
        entries[name] = digest
    if set(entries) != EXPECTED_NAMES:
        raise ValueError("Missing baseline fixtures")
    paths = list(folder.rglob("*"))
    if folder.is_symlink() or any(p.is_symlink() for p in paths):
        raise ValueError("Baseline must not contain symbolic links")
    actual = {p.relative_to(folder).as_posix() for p in paths if p.is_file()}
    if actual != EXPECTED_NAMES | {"SHA256SUMS"}:
        raise ValueError("Unexpected or missing files in baseline directory")
    for name, digest in entries.items():
        if hashlib.sha256((folder / name).read_bytes()).hexdigest() != digest:
            raise ValueError("Frozen fixture changed: " + name)
    return len(entries)

if __name__ == "__main__":
    print(f"PASS: {verify()} pinned files, 13 original Wasm/package pairs. No guest build or repack performed.")
