"""Capture/check the exact non-ignored local source for a Windows preview."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import zipfile

def sources(root):
    raw = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root)
    names = sorted(set(raw.decode("utf-8").split("\0")) - {""})
    result = {}
    for name in names:
        path = root / name
        if path.is_symlink():
            raise ValueError(f"Source symlink requires explicit packaging: {name}")
        if not path.is_file():
            continue
        if not path.resolve().is_relative_to(root):
            raise ValueError("Source path escapes repository")
        if path.stat().st_size > 256 * 1024 * 1024:
            raise ValueError(f"Oversize source: {name}")
        result[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    current = sources(root)
    if args.check:
        with zipfile.ZipFile(args.archive) as archive:
            archived = {name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()}
        if archived != current:
            changed = sorted(set(archived) ^ set(current) | {n for n in current.keys() & archived.keys() if current[n] != archived[n]})
            raise SystemExit("Source changed during build: " + ", ".join(changed[:20]))
        print(f"Source verified: {len(current)} files")
        return
    args.archive.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for name in current:
            data = (root / name).read_bytes()
            if hashlib.sha256(data).hexdigest() != current[name]:
                raise ValueError(f"Source changed during capture: {name}")
            archive.writestr(name, data)
    if sources(root) != current:
        raise ValueError("Source changed during capture; retained incomplete snapshot")
    digest = hashlib.sha256(args.archive.read_bytes()).hexdigest()
    args.archive.with_suffix(args.archive.suffix + ".sha256").write_text(f"{digest}  {args.archive.name}\n", encoding="ascii")
    print(f"Source captured: {len(current)} files; {digest}")

if __name__ == "__main__":
    main()
