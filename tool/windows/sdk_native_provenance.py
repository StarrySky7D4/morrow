"""Source/command/artifact identity for the two documented native corrections."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

MODE = "schema-reference-and-msvc-utf8"
ROLES = {
    "dll": ("sdk-target/debug/morrow_plugin_sdk.dll", "sdk-native-dll-build"),
    "runtime_dll": ("native-msvc/morrow_plugin_sdk.dll", "sdk-native-dll-build"),
    "import_lib": ("sdk-target/debug/morrow_plugin_sdk.dll.lib", "sdk-native-dll-build"),
    "sdk_wrapper": ("native-msvc/morrow_plugin_sdk.obj", "native-msvc-morrow_plugin_sdk"),
    "channel_wrapper": ("native-msvc/morrow_channel_v1.obj", "native-msvc-morrow_channel_v1"),
}
class Rejected(RuntimeError):
    pass
def require(condition, message):
    if not condition:
        raise Rejected(message)
def digest(data):
    return hashlib.sha256(data).hexdigest()
def file_identity(path):
    data = Path(path).read_bytes()
    return {"bytes": len(data), "sha256": digest(data)}
def json_bytes(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode("utf-8")
def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args])

def snapshot(repo, pin, tool_commit):
    repo = Path(repo).resolve()
    require(re.fullmatch(r"[0-9a-f]{40}", pin) is not None, "Invalid baseline pin")
    require(re.fullmatch(r"[0-9a-f]{40}", tool_commit) is not None, "Invalid tool commit")
    require(git(repo, "rev-parse", "HEAD").decode().strip() == tool_commit, "Tool HEAD drift")
    eol = {}
    for record in git(repo, "ls-files", "--eol", "-z", "--", "sdk", "core/schemas", ".gitattributes", "tool/windows").split(b"\0"):
        if record:
            info, path = record.split(b"\t", 1)
            eol[path.decode()] = info.decode()
    entries = []
    for commit, scopes in [(pin, ["sdk", "core/schemas", ".gitattributes"]), (tool_commit, ["tool/windows"])]:
        for record in git(repo, "ls-tree", "-r", "-z", commit, "--", *scopes).split(b"\0"):
            if record:
                info, path = record.split(b"\t", 1)
                mode, kind, oid = info.split()
                require(kind == b"blob" and mode in (b"100644", b"100755"), "Unsupported source entry")
                entries.append((path.decode(), oid.decode()))
    requests = b"".join((oid + "\n").encode() for _, oid in entries)
    process = subprocess.Popen(["git", "-C", str(repo), "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    packed, _ = process.communicate(requests)
    require(process.returncode == 0, "Cannot read pinned source blobs")
    offset, files = 0, {}
    for name, oid in entries:
        end = packed.index(b"\n", offset)
        header = packed[offset:end].split()
        require(header[0].decode() == oid and header[1] == b"blob", "Pinned blob identity mismatch")
        length = int(header[2]); canonical = packed[end + 1:end + 1 + length]
        offset = end + 2 + length
        path = repo / name
        require(path.is_file() and not path.is_symlink(), "Missing/link source: " + name)
        working = path.read_bytes()
        if working == canonical:
            normalization = "git_bytes"
        else:
            allowed = "w/crlf" in eol.get(name, "") and "attr/-text" not in eol.get(name, "")
            require(allowed and working.replace(b"\r\n", b"\n") == canonical, "Source drift: " + name)
            normalization = "crlf_to_lf"
        files[name] = {"git_blob": oid, "canonical_sha256": digest(canonical),
            "working_sha256": digest(working), "working_bytes": len(working), "normalization": normalization}
    for scope in ("sdk", "core/schemas"):
        expected = {name for name in files if name.startswith(scope + "/")}
        actual = set()
        for path in (repo / scope).rglob("*"):
            require(not path.is_symlink() and not path.is_junction(), "Source link/junction: " + str(path))
            if path.is_file():
                actual.add(path.relative_to(repo).as_posix())
        require(actual == expected, "Extra/missing source inventory: " + scope)
    sdk = [value for name, value in files.items() if name.startswith("sdk/")]
    require("core/schemas/dependency_call.capnp" in files, "Required schema missing from baseline")
    return {"baseline_commit": pin, "baseline_tree": git(repo, "rev-parse", pin + "^{tree}").decode().strip(),
        "sdk_tree": git(repo, "rev-parse", pin + ":sdk").decode().strip(), "tool_commit": tool_commit,
        "sdk_normalization_counts": {"git_bytes": sum(v["normalization"] == "git_bytes" for v in sdk),
            "crlf_to_lf": sum(v["normalization"] == "crlf_to_lf" for v in sdk)}, "files": files}

def codec_recipe(compiler, repo, root, artifact_paths, utf8):
    repo, root = Path(repo), Path(root)
    return [str(compiler), "/nologo", *(["/utf-8"] if utf8 else []), "/std:c++17", "/W4", "/WX", "/MD", "/EHsc",
        "/I" + str(repo / "sdk/c/include"), "/I" + str(repo / "sdk/cpp/include"), str(repo / "sdk/tests/cpp_codec.cpp"),
        str(artifact_paths["sdk_wrapper"]), str(artifact_paths["channel_wrapper"]), str(artifact_paths["import_lib"]),
        "/Fo" + str(root / "cpp_codec.obj"), "/Fe" + str(root / "cpp_codec.exe")]

def emit(root, repo, source, records, mode, built_artifacts):
    root, repo = Path(root), Path(repo)
    by = {record["name"]: record for record in records}
    require(set(built_artifacts) == set(ROLES), "Missing first-build artifact capture")
    artifacts = built_artifacts
    for role, (relative, command) in ROLES.items():
        require(command in by and by[command]["exit_code"] == 0, "No successful artifact build: " + role)
        require(artifacts[role]["relative_path"] == relative and artifacts[role]["build_command"] == command,
            "First-build artifact role mismatch")
        require(file_identity(root / relative) == {"bytes": artifacts[role]["bytes"], "sha256": artifacts[role]["sha256"]},
            "Artifact changed since first build: " + role)
        require(artifacts[role]["producer_log_sha256"] == file_identity(root / by[command]["log"])["sha256"],
            "First-build command log changed")
    require(artifacts["dll"]["sha256"] == artifacts["runtime_dll"]["sha256"], "Runtime DLL copy differs from built DLL")
    record_names = ["commands.json", "vs-toolset-setup.log", "source-metadata.json", "compiler-versions.json", "results.json"]
    record_names.extend(record["log"] for record in records)
    files = {name: file_identity(root / name) for name in record_names}
    tools = {}
    for name in ["rust-native-tests", "msvc-cpp_codec-build"]:
        if name in by:
            path = Path(by[name]["command"][0])
            tools[name] = {"path": str(path), **file_identity(path)}
    value = {"version": 1, "producer_mode": mode, "producer_repo": str(repo.resolve()), "producer_root": str(root.resolve()),
        "source_snapshot": source, "artifacts": artifacts, "record_files": files, "tools": tools}
    data = json_bytes(value)
    (root / "provenance.json").write_bytes(data)
    receipt = {"manifest_sha256": digest(data), "manifest": str(root / "provenance.json"), "producer_mode": mode}
    (root / "provenance-receipt.json").write_bytes(json_bytes(receipt))
    return receipt

def load_verified(prior, expected_digest, repo, pin, tool_commit):
    prior = Path(prior).resolve()
    path = prior / "provenance.json"
    require(path.is_file(), "Legacy directory has no build-bound artifact provenance; cannot backfill it")
    raw = path.read_bytes()
    require(re.fullmatch(r"[0-9a-f]{64}", expected_digest) is not None and digest(raw) == expected_digest, "Provenance digest drift")
    value = json.loads(raw)
    require(value.get("version") == 1 and value.get("producer_mode") == MODE, "Unsupported producer/correction mode")
    source = snapshot(repo, pin, tool_commit)
    require(source == value["source_snapshot"], "Current source/schema/tool snapshot drift")
    required_records = {"commands.json", "vs-toolset-setup.log", "source-metadata.json", "compiler-versions.json", "results.json"}
    for name, identity in value["record_files"].items():
        require(Path(name).name == name and (name in required_records or name.endswith(".log")), "Unknown provenance record")
        require(file_identity(prior / name) == identity, "Record drift: " + name)
    require(required_records.issubset(value["record_files"]), "Missing provenance record")
    require(set(value["artifacts"]) == set(ROLES), "Unknown/missing artifact role")
    for role, (relative, command) in ROLES.items():
        item = value["artifacts"][role]
        require(item["relative_path"] == relative and item["build_command"] == command, "Artifact role/path drift")
        require(item.get("capture_phase") == "immediately_after_build_or_verified_copy", "No first-build artifact capture")
        require(file_identity(prior / relative) == {"bytes": item["bytes"], "sha256": item["sha256"]}, "Artifact drift: " + role)
    require(value["artifacts"]["dll"]["sha256"] == value["artifacts"]["runtime_dll"]["sha256"], "Recorded runtime DLL differs")
    for name, tool in value["tools"].items():
        require(name in {"rust-native-tests", "msvc-cpp_codec-build"}, "Unknown tool role")
        require(file_identity(tool["path"]) == {"bytes": tool["bytes"], "sha256": tool["sha256"]}, "Tool executable drift")
    require(set(value["tools"]) == {"rust-native-tests", "msvc-cpp_codec-build"}, "Missing tool identity")
    commands = json.loads((prior / "commands.json").read_text("utf-8"))
    names = [record["name"] for record in commands]
    require(len(names) == len(set(names)), "Duplicate command name")
    by = {record["name"]: record for record in commands}
    expected_names = {"msvc-version", "rust-native-tests", "sdk-native-dll-build", "channel-vectors", "dll-exports",
        "native-msvc-morrow_plugin_sdk", "native-msvc-morrow_channel_v1", "msvc-cpp_codec-build", "msvc-cpp_codec-run"}
    require(set(by) == expected_names, "Unknown/missing fixture command")
    require(set(value["record_files"]) == required_records | {name + ".log" for name in expected_names}, "Missing/extra command log binding")
    require(all(row["log"] == name + ".log" for name, row in by.items()), "Unknown command log shape")
    require({record["name"] for record in commands if record["exit_code"] != 0 and record["name"] != "msvc-version"}
        == {"rust-native-tests", "msvc-cpp_codec-run"}, "Only the two documented failures are allowed")
    origin_repo, origin_root = Path(value["producer_repo"]), Path(value["producer_root"])
    require(all(row["cwd"] == str(origin_repo) for name, row in by.items() if name != "rust-native-tests"), "Unknown historical working-directory shape")
    cargo, compiler = value["tools"]["rust-native-tests"]["path"], value["tools"]["msvc-cpp_codec-build"]["path"]
    require(by["msvc-version"]["command"] == [compiler, "/Bv"] and by["msvc-version"]["exit_code"] in (0, 2), "Unknown compiler metadata command")
    require(by["sdk-native-dll-build"]["command"] == [cargo, "build", "--offline", "--locked", "--manifest-path", str(origin_repo / "sdk/rust/Cargo.toml")], "Unknown DLL build shape")
    require(by["channel-vectors"]["command"] == [cargo, "run", "--offline", "--locked", "--manifest-path", str(origin_repo / "sdk/rust/Cargo.toml"), "--example", "channel_vectors", "--", str(origin_root / "channel-vectors")], "Unknown vector command shape")
    for base in ["morrow_plugin_sdk", "morrow_channel_v1"]:
        recipe = [compiler, "/nologo", "/utf-8", "/std:c11", "/W4", "/WX", "/MD", "/I" + str(origin_repo / "sdk/c/include"), "/c", str(origin_repo / ("sdk/c/src/" + base + ".c")), "/Fo" + str(origin_root / ("native-msvc/" + base + ".obj"))]
        require(by["native-msvc-" + base]["command"] == recipe, "Unknown wrapper/object build shape")
    require(Path(by["dll-exports"]["command"][0]).name.lower() == "dumpbin.exe" and by["dll-exports"]["command"][1:] == ["/exports", str(origin_root / ROLES["dll"][0])], "Unknown DLL export command shape")
    expected_rust = [value["tools"]["rust-native-tests"]["path"], "test", "--offline", "--locked", "--manifest-path",
        str(origin_root / "qualification-source/sdk/rust/Cargo.toml")]
    require(by["rust-native-tests"]["command"] == expected_rust and by["rust-native-tests"]["cwd"] == str(origin_root / "qualification-source"), "Unknown Rust command shape")
    artifact_paths = {role: origin_root / relative for role, (relative, _) in ROLES.items()}
    expected_codec = codec_recipe(value["tools"]["msvc-cpp_codec-build"]["path"], origin_repo,
        origin_root / "native-msvc", artifact_paths, False)
    require(by["msvc-cpp_codec-build"]["command"] == expected_codec and by["msvc-cpp_codec-build"]["exit_code"] == 0, "Unknown codec build shape")
    require(by["msvc-cpp_codec-run"]["command"] == [str(origin_root / "native-msvc/cpp_codec.exe"), str(origin_repo / "sdk/tests/fixtures")], "Unknown codec run shape")
    rust_log = (prior / "rust-native-tests.log").read_text("utf-8", errors="replace")
    codec_log = (prior / "msvc-cpp_codec-run.log").read_text("utf-8", errors="replace")
    require(by["rust-native-tests"]["exit_code"] == 101 and "core/schemas/dependency_call.capnp" in rust_log.replace("\\", "/") and "couldn't read" in rust_log, "Rust failure is not missing schema reference")
    require(by["msvc-cpp_codec-run"]["exit_code"] == 3221226505 and "content_create.encode().bytes == fixture" in codec_log, "Codec failure is not the documented Unicode assertion")
    for role, (_, command) in ROLES.items():
        require(by[command]["exit_code"] == 0, "Artifact build did not succeed")
        require(value["artifacts"][role]["producer_log_sha256"] == file_identity(prior / by[command]["log"])["sha256"],
            "Artifact producer log binding drift")
    for name, identity in source["files"].items():
        if name.startswith("sdk/"):
            require(file_identity(prior / "qualification-source" / name)["sha256"] == identity["working_sha256"], "Historical SDK fixture source drift")
    require(not (prior / "qualification-source/core/schemas").exists(), "Missing-schema fixture was altered")
    return value, source, by
