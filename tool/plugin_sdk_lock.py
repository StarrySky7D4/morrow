"""Portable pins for selected SDK library inputs, not a build sandbox/signature."""
import hashlib
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tempfile
import tomllib
import unicodedata

LOCK_NAME = "sdk.lock.toml"
PROFILE = "morrow-sdk-source-v1"
ROOT_FILES = ("LICENSE", "rust/LICENSE", "rust/Cargo.toml", "rust/Cargo.lock", "rust/build.rs")
TREES = ("rust/src", "rust/contracts", "c/include", "c/src", "cpp/include", "cpp/src")
MAX_FILES = 2048
MAX_FILE_BYTES = 4 * 1024 * 1024
MAX_TOTAL_BYTES = 16 * 1024 * 1024
MAX_LOCK_BYTES = 1024 * 1024


class SdkLockError(ValueError):
    pass


def _metadata(path):
    value = path.lstat()
    if stat.S_ISLNK(value.st_mode) or getattr(value, "st_file_attributes", 0) & 0x400:
        raise SdkLockError("SDK lock inputs must not use symlinks or junctions: " + str(path))
    return value


def _read(path, limit):
    info = _metadata(path)
    if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
        raise SdkLockError("SDK lock input must be a bounded regular file: " + str(path))
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise SdkLockError("SDK lock input exceeds byte budget: " + str(path))
    return data


def _name(value):
    if (not isinstance(value, str) or not value or len(value.encode("utf-8")) > 1024
            or "\\" in value or ":" in value or any(unicodedata.category(c) == "Cc" for c in value)
            or PurePosixPath(value).is_absolute() or any(p in ("", ".", "..") for p in value.split("/"))
            or not (value in ROOT_FILES or any(value.startswith(tree + "/") for tree in TREES))):
        raise SdkLockError("invalid SDK lock relative path")
    return value


def inventory(root):
    root = Path(root)
    # Check parents of explicitly selected root files as well as descendants.
    for directory in (root, root / "rust"):
        if not stat.S_ISDIR(_metadata(directory).st_mode):
            raise SdkLockError("SDK lock requires regular directories")
    records = {}
    total = 0

    def add(path):
        nonlocal total
        if len(records) >= MAX_FILES:
            raise SdkLockError("SDK lock file budget exceeded")
        name = _name(path.relative_to(root).as_posix())
        data = _read(path, min(MAX_FILE_BYTES, MAX_TOTAL_BYTES - total))
        total += len(data)
        records[name] = {"size": len(data), "sha256": hashlib.sha256(data).hexdigest()}

    for name in ROOT_FILES:
        add(root / name)
    pending = []
    for tree in TREES:
        path = root
        for part in tree.split("/"):
            path = path / part
            if not stat.S_ISDIR(_metadata(path).st_mode):
                raise SdkLockError("SDK lock requires regular source trees: " + tree)
        pending.append(path)
    visited = 0
    while pending:
        directory = pending.pop()
        for path in sorted(directory.iterdir()):
            visited += 1
            if visited > MAX_FILES * 4:
                raise SdkLockError("SDK lock traversal budget exceeded")
            info = _metadata(path)
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
            else:
                add(path)
    return dict(sorted(records.items()))


def parse(data):
    if len(data) > MAX_LOCK_BYTES:
        raise SdkLockError("SDK lock exceeds metadata budget")
    try:
        config = tomllib.loads(data.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise SdkLockError("invalid SDK lock TOML") from error
    if (set(config) != {"schema", "profile", "files"} or type(config["schema"]) is not int
            or config["schema"] != 1 or config["profile"] != PROFILE):
        raise SdkLockError("unsupported SDK lock schema/profile")
    files = config["files"]
    if not isinstance(files, list) or not 1 <= len(files) <= MAX_FILES:
        raise SdkLockError("SDK lock requires a bounded file list")
    result = {}
    total = 0
    for entry in files:
        if not isinstance(entry, dict) or set(entry) != {"path", "size", "sha256"}:
            raise SdkLockError("invalid SDK lock file entry")
        name = _name(entry["path"])
        if name in result or type(entry["size"]) is not int or not 0 <= entry["size"] <= MAX_FILE_BYTES:
            raise SdkLockError("duplicate SDK lock path or invalid byte size")
        if not isinstance(entry["sha256"], str) or not re.fullmatch("[0-9a-f]{64}", entry["sha256"]):
            raise SdkLockError("invalid SDK lock digest")
        total += entry["size"]
        if total > MAX_TOTAL_BYTES:
            raise SdkLockError("SDK lock total byte budget exceeded")
        result[name] = {"size": entry["size"], "sha256": entry["sha256"]}
    if not set(ROOT_FILES).issubset(result):
        raise SdkLockError("SDK lock is missing required root files")
    return result


def encode(records):
    lines = ["schema = 1", 'profile = "' + PROFILE + '"']
    for name, value in sorted(records.items()):
        name = _name(name).replace('"', '\\"')
        lines += ["", "[[files]]", 'path = "' + name + '"',
                  "size = " + str(value["size"]), 'sha256 = "' + value["sha256"] + '"']
    data = ("\n".join(lines) + "\n").encode("utf-8")
    parse(data)
    return data


def verify(project_root, sdk_root, *, required=False):
    path = Path(project_root) / LOCK_NAME
    try:
        data = _read(path, MAX_LOCK_BYTES)
    except FileNotFoundError:
        if required:
            raise SdkLockError("SDK lock required; run lock-sdk explicitly") from None
        return {"status": "absent"}
    expected = parse(data)
    actual = inventory(sdk_root)
    if expected != actual:
        added, removed = set(actual) - set(expected), set(expected) - set(actual)
        changed = {p for p in set(expected) & set(actual) if expected[p] != actual[p]}
        raise SdkLockError(f"SDK source drift: added={len(added)} removed={len(removed)} changed={len(changed)}; review before lock-sdk --update")
    return {"status": "verified", "sha256": hashlib.sha256(data).hexdigest(), "files": len(actual)}


def write(project_root, sdk_root, *, update=False):
    target = Path(project_root) / LOCK_NAME
    previous = None
    try:
        previous = _read(target, MAX_LOCK_BYTES)
    except FileNotFoundError:
        pass
    if previous is not None and not update:
        raise SdkLockError("SDK lock already exists; review changes and use --update explicitly")
    if previous is None and update:
        raise SdkLockError("SDK lock is absent; omit --update to create it")
    data = encode(inventory(sdk_root))
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix="sdk-lock-", suffix=".tmp", dir=project_root, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        if update:
            if _read(target, MAX_LOCK_BYTES) != previous:
                raise SdkLockError("SDK lock changed while preparing update")
            os.replace(temporary, target)
        else:
            # Publish only if absent; an existing lock is never truncated.
            os.link(temporary, target)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return {"status": "locked", "sha256": hashlib.sha256(data).hexdigest(), "files": len(parse(data))}
