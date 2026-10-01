"""Explicit trusted-host discovery; bounded output, no authority or guest run."""
import hashlib
import json
from pathlib import Path
import subprocess
import threading
import time

MAX_OUTPUT = 65536


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate descriptor field: " + key)
        result[key] = value
    return result


def strings(value, label):
    if not isinstance(value, list) or len(value) > 32 or any(not isinstance(v, str) or not v or len(v) > 256 or any(ord(c) < 32 for c in v) for v in value) or len(set(value)) != len(value):
        raise ValueError("invalid " + label)


def positive(value, label):
    if type(value) is not int or not 1 <= value <= (1 << 63) - 1:
        raise ValueError("invalid " + label)


def validate_descriptor(value):
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 1:
        raise ValueError("unsupported SDK descriptor schema")
    if value.get("authority") != "none" or value.get("package_preflight_required") is not True:
        raise ValueError("discovery grants no authority and requires package preflight")
    if not isinstance(value.get("host_version"), str) or not value["host_version"] or len(value["host_version"]) > 128:
        raise ValueError("invalid host version")
    platform = value.get("platform")
    if not isinstance(platform, dict) or any(not isinstance(platform.get(k), str) or not platform[k] or len(platform[k]) > 64 for k in ("os", "arch")):
        raise ValueError("invalid host platform")
    if value.get("backend") != "wasmi" or not isinstance(value.get("profiles"), list) or not 1 <= len(value["profiles"]) <= 16:
        raise ValueError("invalid backend or profile list")
    seen = set()
    for profile in value["profiles"]:
        if not isinstance(profile, dict) or not isinstance(profile.get("id"), str) or not profile["id"] or len(profile["id"]) > 256 or profile["id"] in seen:
            raise ValueError("invalid or duplicate profile id")
        seen.add(profile["id"])
        if profile.get("status") not in ("candidate", "stable", "experimental"):
            raise ValueError("invalid profile stability")
        for field in ("guest_abi_version", "package_schema_version"):
            positive(profile.get(field), field)
        contracts = profile.get("contracts")
        if not isinstance(contracts, dict) or not contracts or len(contracts) > 16:
            raise ValueError("missing profile contracts")
        if profile["id"] == "morrow.guest-task.v3" and not {"runtime", "content", "task", "ui", "dependency_call"}.issubset(contracts):
            raise ValueError("missing base guest contracts")
        if profile["id"] == "morrow.channel.v1":
            if not {"runtime", "task", "channel"}.issubset(contracts):
                raise ValueError("missing channel profile contracts")
            scope = profile.get("channel_scope")
            expected = {"trusted_local_sources": True, "managed_binding_required": True,
                        "workbench_binding": False, "network_backend": False,
                        "cloud_account": False, "automatic_replay": False}
            if (not isinstance(scope, dict) or any(type(scope.get(k)) is not bool or scope[k] != v for k, v in expected.items())
                    or profile.get("workbench_routes") != []):
                raise ValueError("unsupported channel profile scope")
        for name, contract in contracts.items():
            if not isinstance(contract, dict) or not isinstance(contract.get("sha256"), str) or len(contract["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in contract["sha256"]):
                raise ValueError("invalid contract digest")
            if "version" in contract:
                positive(contract["version"], "contract version")
            elif name != "content":
                raise ValueError("missing contract version")
        for field in ("runtime_task_modes", "workbench_routes", "runtime_supported_required_features"):
            strings(profile.get(field), field)
        constraints = profile.get("workbench_constraints")
        if not isinstance(constraints, dict) or any(type(constraints.get(k)) is not bool for k in ("content_task", "dependency_calls", "required_dependencies")):
            raise ValueError("invalid Workbench route constraints")
        limits = profile.get("hard_byte_limits")
        if not isinstance(limits, dict) or not limits or len(limits) > 32:
            raise ValueError("missing hard byte limits")
        for limit in limits.values():
            positive(limit, "hard byte limit")
        defaults = profile.get("runtime_defaults")
        if not isinstance(defaults, dict):
            raise ValueError("missing runtime defaults")
        positive(defaults.get("fuel"), "default fuel")
        positive(defaults.get("memory_bytes"), "default memory")
        if type(defaults.get("host_calls")) is not int or defaults["host_calls"] < 0:
            raise ValueError("invalid default host calls")
    strings(value.get("unsupported_requirements"), "unsupported requirements")
    experiments = value.get("experimental_extensions")
    if not isinstance(experiments, dict) or experiments.get("status") != "recognized_experimental_not_fully_discovered":
        raise ValueError("invalid experimental extension status")
    strings(experiments.get("feature_names"), "experimental features")
    return value


def bounded_process(argv):
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    outputs = [bytearray(), bytearray()]
    exceeded = threading.Event()
    errors = []

    def read(index, stream):
        try:
            while len(outputs[index]) <= MAX_OUTPUT:
                chunk = stream.read(min(4096, MAX_OUTPUT + 1 - len(outputs[index])))
                if not chunk:
                    return
                outputs[index].extend(chunk)
                if len(outputs[index]) > MAX_OUTPUT:
                    exceeded.set()
                    return
        except OSError as error:
            errors.append(error)

    threads = [threading.Thread(target=read, args=(i, stream), daemon=True) for i, stream in enumerate((process.stdout, process.stderr))]
    for thread in threads:
        thread.start()
    deadline = time.monotonic() + 5
    try:
        while process.poll() is None:
            if exceeded.is_set():
                raise ValueError("SDK profile output exceeds 64 KiB")
            if time.monotonic() >= deadline:
                raise ValueError("SDK profile query timed out")
            time.sleep(0.01)
        for thread in threads:
            thread.join(timeout=max(0, deadline - time.monotonic()))
        if any(thread.is_alive() for thread in threads):
            raise ValueError("SDK profile pipe did not close")
        if exceeded.is_set():
            raise ValueError("SDK profile output exceeds 64 KiB")
        if errors:
            raise ValueError("SDK profile pipe read failed")
        if process.returncode:
            raise ValueError("SDK profile query failed (exit " + str(process.returncode) + ")")
        return bytes(outputs[0])
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=1)
        for thread in threads:
            thread.join(timeout=0.5)
        for stream in (process.stdout, process.stderr):
            if not any(thread.is_alive() for thread in threads):
                stream.close()


def query(host):
    host = Path(host).resolve(strict=True)
    if not host.is_file():
        raise ValueError("--host must name an explicitly trusted executable")
    before = hashlib.sha256(host.read_bytes()).hexdigest()
    raw = bounded_process([str(host), "--sdk-capabilities"])
    if hashlib.sha256(host.read_bytes()).hexdigest() != before:
        raise ValueError("host artifact changed during SDK profile query")
    try:
        value = validate_descriptor(json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object))
    except (UnicodeError, RecursionError, ValueError) as error:
        raise ValueError("invalid SDK profile descriptor: " + str(error)) from error
    return {"host_sha256": before, "descriptor_sha256": hashlib.sha256(raw).hexdigest(), "descriptor": value}
