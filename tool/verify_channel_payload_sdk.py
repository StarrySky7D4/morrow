"""Standalone bounded source inventory, not a signature, grant or build sandbox.

Python 3.11+. This verifier never invokes a compiler, Core, host or network.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tomllib
import unicodedata
import zipfile
import zlib

PROFILE = "morrow-channel-payload-sdk-source-v1"
MANIFEST = "CHANNEL_PAYLOAD_SDK_MANIFEST.json"
MAX_FILES = 256
MAX_FILE_BYTES = 4 * 1024 * 1024
MAX_TOTAL_BYTES = 16 * 1024 * 1024
MAX_MANIFEST_BYTES = 256 * 1024
MAX_ARCHIVE_BYTES = MAX_TOTAL_BYTES + 1024 * 1024
# Fixed bytes of the unchanged SDK lock. Unsigned source observation, not trust.
SDK_LOCK_SHA256 = "395ffb285db0cf70876872b1f98c5fea95d84db0e1ccc7a798de7f648b6ed164"
CONTRACTS = {
    "ws-message-v1": {"version": 1, "schema_sha256": "2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d"},
    "sse-event-v1": {"version": 1, "schema_sha256": "bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863"},
}
STEMS = ("morrow_channel_v1", "morrow_plugin_codec", "morrow_plugin_dependency",
         "morrow_plugin_io", "morrow_plugin_mutation", "morrow_plugin_sdk",
         "morrow_plugin_service", "morrow_plugin_task", "morrow_plugin_ui")
SDK_CONTRACTS = ("runtime.capnp", "content.proto", "task.capnp", "ui.capnp",
                 "dependency_call.capnp", "io.capnp", "channel.capnp", "mutation.capnp",
                 "service.capnp", "service_resources.capnp", "version.txt",
                 "task-version.txt", "ui-version.txt")
SDK_MODULES = ("lib", "protocol", "task", "ui", "dependency_call", "io", "service",
               "service_resources", "mutation", "channel", "descriptor_prefix", "ffi",
               "io_ffi", "service_ffi", "mutation_ffi", "channel_ffi", "wasm", "wasm_alloc")
SDK_NAMES = frozenset(
    {"sdk/LICENSE", "sdk/rust/LICENSE", "sdk/rust/Cargo.toml", "sdk/rust/Cargo.lock", "sdk/rust/build.rs",
     "sdk/c/include/morrow_plugin_wasm.h", "sdk/cpp/src/morrow_plugin_wasm_runtime.cpp",
     "sdk/rust/src/channel/transport.rs"}
    | {"sdk/" + lang + "/include/" + stem + suffix for stem in STEMS for lang, suffix in (("c", ".h"), ("cpp", ".hpp"))}
    | {"sdk/rust/contracts/" + name for name in SDK_CONTRACTS}
    | {"sdk/rust/src/" + name + ".rs" for name in SDK_MODULES}
    | {"sdk/c/src/" + name + ".c" for name in ("morrow_channel_v1", "morrow_plugin_sdk", "morrow_plugin_task", "morrow_plugin_wasm", "morrow_plugin_wasm_libc")}
)


class BundleError(ValueError):
    pass


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(value):
    if (not isinstance(value, str) or not value
            or value != unicodedata.normalize("NFC", value)
            or "\\" in value or ":" in value
            or any(unicodedata.category(c) in ("Cc", "Cs") for c in value)):
        raise BundleError("unsafe bundle path")
    if len(value.encode("utf-8")) > 512:
        raise BundleError("bundle path exceeds byte budget")
    for part in value.split("/"):
        if (part in ("", ".", "..") or len(part.encode("utf-8")) > 255
                or part != part.rstrip(" .")
                or re.fullmatch(r"(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", part, re.I)):
            raise BundleError("unsafe bundle path: " + repr(value))
    return value


def regular(path, directory=False):
    info = Path(path).lstat()
    if (stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400
            or not (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode))):
        raise BundleError("ordinary " + ("directory" if directory else "file") + " required: " + str(path))
    return info


def checked_root(path):
    # Check lexical ancestors before resolving; a junction cannot be hidden by resolve().
    path = Path(os.path.abspath(path))
    for ancestor in reversed((path, *path.parents)):
        regular(ancestor, True)
    return path


def read_file(root, name, limit=MAX_FILE_BYTES):
    safe_name(name)
    path = root
    for part in name.split("/")[:-1]:
        path /= part
        regular(path, True)
    path /= name.split("/")[-1]
    size = regular(path).st_size
    if size > limit:
        raise BundleError("file exceeds byte budget: " + name)
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) != size or len(data) > limit:
        raise BundleError("file changed or exceeds byte budget: " + name)
    return data


def extension_names(extension):
    stem = extension.replace("-", "_")
    schema = "ws_message.capnp" if extension == "ws-message-v1" else "sse_event.capnp"
    prefix = "extensions/" + extension + "/"
    return {prefix + name for name in ("Cargo.toml", "Cargo.lock", "build.rs",
            "src/lib.rs", "src/ffi.rs", "contracts/" + schema,
            "include/morrow_" + stem + ".h", "include/morrow_" + stem + ".hpp",
            "guests/c/plugin.c", "guests/cpp/plugin.cpp", "guests/rust/Cargo.toml", "guests/rust/src/lib.rs")}


NAMES = SDK_NAMES | frozenset().union(*(extension_names(ext) for ext in CONTRACTS)) | {
    "LICENSE", "README.md", "NOTICE.md", "tool/verify_channel_payload_sdk.py"}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise BundleError("duplicate JSON member")
        result[key] = value
    return result


def parse_manifest(data):
    if len(data) > MAX_MANIFEST_BYTES:
        raise BundleError("manifest exceeds byte budget")
    try:
        value = json.loads(data.decode("utf-8"), object_pairs_hook=unique_object,
                           parse_constant=lambda _: (_ for _ in ()).throw(BundleError("non-finite JSON")))
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise BundleError("invalid manifest JSON") from error
    if (not isinstance(value, dict) or set(value) != {"schema", "profile", "authority", "qualification", "contracts", "files", "source_bytes"}
            or type(value["schema"]) is not int or value["schema"] != 1
            or value["profile"] != PROFILE or value["authority"] != "none"
            or value["qualification"] != "experimental-source-only"
            or value["contracts"] != CONTRACTS):
        raise BundleError("unsupported manifest profile or boundary")
    # bool is not an acceptable integer version in the contract observations either.
    for item in value["contracts"].values():
        if type(item["version"]) is not int:
            raise BundleError("invalid contract version")
    files = value["files"]
    if not isinstance(files, dict) or set(files) != NAMES or len(files) > MAX_FILES:
        raise BundleError("manifest is outside the exact v1 source closure")
    total, case_names = 0, set()
    for name, record in files.items():
        safe_name(name)
        if name.casefold() in case_names:
            raise BundleError("case collision in manifest")
        case_names.add(name.casefold())
        if (not isinstance(record, dict) or set(record) != {"size", "sha256"}
                or type(record["size"]) is not int or not 0 <= record["size"] <= MAX_FILE_BYTES
                or not isinstance(record["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", record["sha256"])):
            raise BundleError("invalid file record")
        total += record["size"]
    if type(value["source_bytes"]) is not int or value["source_bytes"] != total or total + len(data) > MAX_TOTAL_BYTES:
        raise BundleError("invalid source byte total")
    return value


def build_manifest(files):
    value = {"schema": 1, "profile": PROFILE, "authority": "none", "qualification": "experimental-source-only",
             "contracts": CONTRACTS, "files": {name: {"size": len(data), "sha256": sha(data)} for name, data in sorted(files.items())},
             "source_bytes": sum(len(data) for data in files.values())}
    data = (json.dumps(value, ensure_ascii=True, indent=2, sort_keys=True) + "\n").encode("utf-8")
    parse_manifest(data)
    return data


def expected_cargo(name):
    sdk = name == "sdk/rust/Cargo.toml"
    guest = "/guests/rust/" in name
    extension = next((ext for ext in CONTRACTS if name.startswith("extensions/" + ext + "/")), None)
    crate = "morrow-plugin-sdk" if sdk else "morrow-" + extension
    package = {"name": crate.replace("-v1", "-guest") if guest else crate, "version": "0.1.9-test.50" if sdk else "0.1.0",
               "edition": "2024", "publish": False, "license": "AGPL-3.0-only"}
    if guest:
        return {"package": package, "lib": {"crate-type": ["cdylib"]}, "dependencies": {
            crate: {"path": "../.."}, "morrow-plugin-sdk": {"path": "../../../../sdk/rust", "features": ["wasm-guest"]}}}
    if sdk:
        return {"workspace": {"resolver": "2"}, "package": package, "lib": {"crate-type": ["rlib", "cdylib"]},
                "dependencies": {"sha2": "=0.10.9", "capnp": "=0.24.1"},
                "build-dependencies": {"capnpc": "=0.24.0", "sha2": "=0.10.9"},
                "features": {"wasm-guest": [], "wasm-c": []}}
    return {"workspace": {"resolver": "2", "members": ["guests/rust"]}, "package": package,
            "lib": {"crate-type": ["rlib", "staticlib"]}, "dependencies": {"capnp": "=0.24.1",
             "morrow-plugin-sdk": {"path": "../../sdk/rust", "optional": True}},
            "build-dependencies": {"capnpc": "=0.24.0", "sha2": "=0.10.9"},
            "features": {"c-transport": ["dep:morrow-plugin-sdk", "morrow-plugin-sdk/wasm-c"]}}


def parse_toml(data):
    try:
        return tomllib.loads(data.decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError, RecursionError) as error:
        raise BundleError("invalid Cargo input") from error


def registry_inputs(data, path_edges):
    lock = parse_toml(data)
    if set(lock) != {"version", "package"} or type(lock["version"]) is not int or lock["version"] != 4 or not isinstance(lock["package"], list):
        raise BundleError("unsupported Cargo lock")
    registry, paths, seen = [], set(), set()
    for record in lock["package"]:
        if not isinstance(record, dict) or set(record) - {"name", "version", "source", "checksum", "dependencies"}:
            raise BundleError("unsupported Cargo lock entry")
        name, version = record.get("name"), record.get("version")
        if not isinstance(name, str) or not isinstance(version, str) or (name, version) in seen:
            raise BundleError("duplicate or invalid Cargo lock entry")
        seen.add((name, version))
        dependencies = record.get("dependencies", [])
        if not isinstance(dependencies, list) or any(not isinstance(item, str) for item in dependencies):
            raise BundleError("invalid lock dependency list")
        if "source" in record:
            checksum = record.get("checksum")
            if (record["source"] != "registry+https://github.com/rust-lang/crates.io-index"
                    or not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum)):
                raise BundleError("unsupported Cargo source")
            registry.append((name, version, record["source"], record["checksum"], tuple(dependencies)))
        else:
            if ("checksum" in record or (name, version) not in path_edges
                    or dependencies != path_edges[(name, version)]):
                raise BundleError("unexpected Cargo path package")
            paths.add((name, version))
    if paths != set(path_edges) or len(registry) != 13:
        raise BundleError("unexpected Cargo dependency closure")
    return sorted(registry)


def typed_equal(left, right):
    # TOML bool and integer are distinct even though Python True == 1.
    if type(left) is not type(right):
        return False
    if isinstance(right, dict):
        return set(left) == set(right) and all(typed_equal(left[key], right[key]) for key in right)
    if isinstance(right, list):
        return len(left) == len(right) and all(typed_equal(a, b) for a, b in zip(left, right))
    return left == right


def validate_compiler_inputs(files):
    # An exact v1 graph catches target-specific escape, inherited workspace and patches.
    # It does not audit Rust/build.rs semantics or sandbox an ordinary trusted build.
    for name in sorted(n for n in NAMES if n.endswith("/Cargo.toml")):
        if not typed_equal(parse_toml(files[name]), expected_cargo(name)):
            raise BundleError("Cargo graph differs from the supported v1 closure: " + name)
    if sha(files["sdk/rust/Cargo.lock"]) != SDK_LOCK_SHA256:
        raise BundleError("original SDK lock bytes differ from fixed v1 input")
    sdk_paths = {("morrow-plugin-sdk", "0.1.9-test.50"): ["capnp", "capnpc", "sha2"]}
    registry = registry_inputs(files["sdk/rust/Cargo.lock"], sdk_paths)
    for extension, observation in CONTRACTS.items():
        prefix = "extensions/" + extension + "/"
        name = "ws_message.capnp" if extension == "ws-message-v1" else "sse_event.capnp"
        if sha(files[prefix + "contracts/" + name]) != observation["schema_sha256"]:
            raise BundleError("payload schema differs from native raw contract")
        expected_paths = {**sdk_paths,
            ("morrow-" + extension, "0.1.0"): ["capnp", "capnpc", "morrow-plugin-sdk", "sha2"],
            ("morrow-" + extension.replace("-v1", "-guest"), "0.1.0"): ["morrow-plugin-sdk", "morrow-" + extension]}
        if registry_inputs(files[prefix + "Cargo.lock"], expected_paths) != registry:
            raise BundleError("extension lock registry differs from the unchanged SDK")


def verify_payload(files, manifest_data):
    manifest = parse_manifest(manifest_data)
    if set(files) != NAMES:
        raise BundleError("payload is outside the exact source closure")
    for name, record in manifest["files"].items():
        data = files[name]
        if len(data) != record["size"] or sha(data) != record["sha256"]:
            raise BundleError("source integrity mismatch: " + name)
    validate_compiler_inputs(files)
    return {"profile": PROFILE, "status": "VERIFIED_SOURCE_INVENTORY_ONLY", "authority": "none",
            "files": len(files), "source_bytes": manifest["source_bytes"], "manifest_sha256": sha(manifest_data),
            "build_execution": "NOT_RUN", "semantics": "NOT_PROVED"}


def directory_names(root):
    result, pending, visited, directories = set(), [root], 0, set()
    expected_directories = {PurePosixPath(name).parent.as_posix() for name in NAMES | {MANIFEST}}
    expected_directories |= {ancestor.as_posix() for name in NAMES for ancestor in PurePosixPath(name).parents}
    while pending:
        for path in sorted(pending.pop().iterdir()):
            visited += 1
            if visited > MAX_FILES * 4:
                raise BundleError("directory traversal budget exceeded")
            info = path.lstat()
            name = safe_name(path.relative_to(root).as_posix())
            if stat.S_ISDIR(info.st_mode):
                regular(path, True)
                if name not in expected_directories or name.casefold() in directories:
                    raise BundleError("unexpected/colliding source directory")
                directories.add(name.casefold())
                pending.append(path)
            else:
                regular(path)
                if name.casefold() in {item.casefold() for item in result}:
                    raise BundleError("case collision in directory")
                result.add(name)
                if len(result) > MAX_FILES + 1:
                    raise BundleError("file count exceeds budget")
    return result


def verify_directory(root):
    root = checked_root(root)
    if directory_names(root) != NAMES | {MANIFEST}:
        raise BundleError("directory differs from exact source inventory")
    data = read_file(root, MANIFEST, MAX_MANIFEST_BYTES)
    manifest = parse_manifest(data)
    files = {name: read_file(root, name, record["size"]) for name, record in manifest["files"].items()}
    return verify_payload(files, data)


def read_zip(path):
    path = Path(os.path.abspath(path))
    checked_root(path.parent)
    if regular(path).st_size > MAX_ARCHIVE_BYTES:
        raise BundleError("archive exceeds compressed budget")
    try:
        with zipfile.ZipFile(path) as archive:
            entries, seen, total = archive.infolist(), set(), 0
            if len(entries) != len(NAMES) + 1:
                raise BundleError("archive file count differs from source closure")
            for entry in entries:
                name = safe_name(entry.filename)
                mode = (entry.external_attr >> 16) & 0o170000
                if (name.casefold() in seen or entry.is_dir() or mode not in (0, stat.S_IFREG)
                        or entry.external_attr & 0x400
                        or entry.flag_bits & 1 or entry.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                        or entry.file_size < 0 or entry.file_size > (MAX_MANIFEST_BYTES if name == MANIFEST else MAX_FILE_BYTES)):
                    raise BundleError("unsupported/duplicate archive entry")
                seen.add(name.casefold())
                total += entry.file_size
            if total > MAX_TOTAL_BYTES or set(archive.namelist()) != NAMES | {MANIFEST}:
                raise BundleError("archive exceeds or differs from source closure")
            files = {}
            for entry in entries:
                limit = MAX_MANIFEST_BYTES if entry.filename == MANIFEST else MAX_FILE_BYTES
                with archive.open(entry) as stream:
                    data = stream.read(limit + 1)
                if len(data) != entry.file_size or len(data) > limit:
                    raise BundleError("archive entry changed or exceeds byte budget")
                files[entry.filename] = data
    except (zipfile.BadZipFile, RuntimeError, EOFError, zlib.error) as error:
        raise BundleError("invalid archive") from error
    return files


def verify_zip(path):
    files = read_zip(path)
    data = files.pop(MANIFEST)
    return verify_payload(files, data)


def materialize(files, manifest_data, output):
    verify_payload(files, manifest_data)
    output = Path(os.path.abspath(output))
    checked_root(output.parent)
    # Fails if any output exists, including a broken link. Never replace a user path.
    output.mkdir(exist_ok=False)
    for name, data in sorted({**files, MANIFEST: manifest_data}.items()):
        path = output
        for part in name.split("/")[:-1]:
            path /= part
            path.mkdir(exist_ok=True)
            regular(path, True)
        path /= name.split("/")[-1]
        with path.open("xb") as stream:
            stream.write(data)
    # Failure leaves an unqualified output for diagnosis; unknown content is not deleted.
    return verify_directory(output)


def extract_zip(path, output):
    files = read_zip(path)
    manifest = files.pop(MANIFEST)
    return materialize(files, manifest, output)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("verify-directory", "verify-zip", "extract-zip"):
        entry = sub.add_parser(command)
        entry.add_argument("input", type=Path)
        if command == "extract-zip":
            entry.add_argument("output", type=Path)
    args = parser.parse_args(argv)
    try:
        result = (verify_directory(args.input) if args.command == "verify-directory" else
                  verify_zip(args.input) if args.command == "verify-zip" else extract_zip(args.input, args.output))
    except (BundleError, OSError) as error:
        parser.exit(2, "Source bundle verification refused: " + str(error) + "\n")
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
