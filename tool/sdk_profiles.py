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


def contract_digest(contract):
    if not isinstance(contract, dict) or not isinstance(contract.get("sha256"), str) or len(contract["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in contract["sha256"]):
        raise ValueError("invalid contract digest")


# This optional, independently versioned metadata is not a new guest ABI. Keep
# accepting schema-1 hosts that only supplied the legacy experimental names.
EXTENSION_LAYOUTS = {
    "morrow.io.v1": {
        "contracts": {"runtime", "task", "io"},
        "required": ["io-v1"], "optional": [], "versions": {"io"},
        "bytes": {"io_frame", "io_payload", "method", "relative_target", "header_name", "header_value", "headers", "credential_reference", "operation_id", "operation_response", "endpoint_reference", "declared_total", "declared_job"},
        "counts": {"headers", "declared_capabilities", "declared_handlers", "declared_resources", "declared_concurrent_jobs"},
        "durations": {"declared_io", "http_submission"},
        "operations": {"Read": "managed_file_broker", "Finish": "managed_file_broker", "Cancel": "managed_file_broker", "SubmitHttp": "managed_http_broker", "QueryOperation": "managed_operation_history"},
        "unsupported": {"SubmitFileRead", "SubmitFileList", "SubmitFileCreate", "SubmitFileReplace", "SubmitFileDelete", "SubmitHttpListen", "SubmitHttpPublish", "SubmitHttpUnpublish", "SubmitWebSocketConnect", "SubmitServiceAccept", "SubmitServiceReply", "Poll", "Write"},
    },
    "morrow.service.v1": {
        "contracts": {"runtime", "task", "io", "service"},
        "required": ["io-v1"], "optional": ["service-run-v1", "service-run-budget-v1"],
        "versions": {"io", "service_run", "service_run_budget"},
        "bytes": {"service_frame", "service_body", "headers", "declared_job", "declared_total"},
        "counts": {"headers", "declared_concurrent_jobs", "service_run_cumulative_jobs"},
        "durations": {"request", "service_run"},
        "operations": {"Invocation": "managed_service_worker", "Reply": "managed_service_worker"},
        "unsupported": {"GuestListen", "GuestPublish", "UnlimitedServiceRun", "AsyncDependencyComposition"},
    },
    "morrow.service-resources.v1": {
        "contracts": {"runtime", "task", "io", "service", "service_resources"},
        "required": ["io-v1", "service-resources-v1"], "optional": [], "versions": set(),
        "bytes": {"directory_frame"}, "counts": {"endpoints"}, "durations": set(),
        "operations": {"Directory": "host_injected_service_header"},
        "unsupported": {"GuestEndpointDiscovery", "CredentialDisclosure", "GrantFromMetadata"},
    },
    "morrow.mutation.v1": {
        "contracts": {"runtime", "task", "io", "mutation"},
        "required": ["io-v1", "mutation-v1"], "optional": ["mutation-budget-v1"], "versions": set(),
        "bytes": {"mutation_frame", "chunk", "content", "operation_id", "extended_job", "extended_total"},
        "counts": set(), "durations": {"mutation_deadline"},
        "operations": {name: "managed_mutation_worker" for name in ("PrepareCreate", "PrepareDelete", "Chunk", "Commit", "Execute", "Query", "CancelPlan", "Release")},
        "unsupported": {"ConditionalReplace", "GeneralFilesystem", "NonWindowsMutationWorker"},
    },
}


def extension_discovery(discovery, platform, base_profiles):
    if not isinstance(discovery, dict) or type(discovery.get("schema_version")) is not int or discovery["schema_version"] != 1:
        raise ValueError("unsupported extension discovery schema")
    if (discovery.get("status") != "compiled_metadata_only" or discovery.get("authority") != "none"
            or discovery.get("package_preflight_required") is not True):
        raise ValueError("invalid extension discovery authority or status")
    windows = platform["os"] == "windows"
    requirements = discovery.get("current_host_requirements")
    keys = {"protected_owner_backend_compiled", "stored_http_credentials_compiled", "protected_tls_identity_compiled"}
    if not isinstance(requirements, dict) or set(requirements) != keys or any(requirements[k] is not windows for k in keys):
        raise ValueError("invalid extension current-host prerequisites")
    strings([discovery.get("policy")], "extension discovery policy")
    profiles = discovery.get("profiles")
    if not isinstance(profiles, list) or len(profiles) != len(EXTENSION_LAYOUTS):
        raise ValueError("invalid extension profile list")
    identities = {}
    for profile in base_profiles:
        for name, contract in profile['contracts'].items():
            if name in ('runtime', 'task'):
                identity = (contract['version'], contract['sha256'])
                if name in identities and identities[name] != identity:
                    raise ValueError("inconsistent compiled contract identity")
                identities[name] = identity
    seen = set()
    for profile in profiles:
        if not isinstance(profile, dict) or not isinstance(profile.get("id"), str) or profile["id"] not in EXTENSION_LAYOUTS or profile["id"] in seen:
            raise ValueError("invalid or duplicate extension profile")
        name = profile["id"]
        seen.add(name)
        layout = EXTENSION_LAYOUTS[name]
        if (profile.get("status") != "experimental" or type(profile.get("guest_abi_version")) is not int
                or profile["guest_abi_version"] != 2 or type(profile.get("package_schema_version")) is not int
                or profile["package_schema_version"] != 1):
            raise ValueError("invalid extension profile version or status")
        if (profile.get("qualification") != "not_established_by_discovery"
                or any(profile.get(k) is not False for k in ("production_public_binding_available", "combined_dependency_import", "combined_channel_import"))):
            raise ValueError("unsupported extension scope")
        contracts = profile.get("contracts")
        if not isinstance(contracts, dict) or set(contracts) != layout["contracts"]:
            raise ValueError("missing extension contracts")
        for contract_name, contract in contracts.items():
            contract_digest(contract)
            positive(contract.get("version"), "extension contract version")
            if contract_name not in ("runtime", "task") and contract["version"] != 1:
                raise ValueError("unsupported extension contract version")
            identity = (contract['version'], contract['sha256'])
            if contract_name in identities and identities[contract_name] != identity:
                raise ValueError("inconsistent compiled contract identity")
            identities[contract_name] = identity
        for field, expected in (("required_features", layout["required"]), ("optional_features", layout["optional"])):
            strings(profile.get(field), field)
            if set(profile[field]) != set(expected):
                raise ValueError("invalid extension feature declaration")
        for field, expected in (("declaration_versions", layout["versions"]), ("hard_byte_limits", layout["bytes"]), ("count_limits", layout["counts"]), ("duration_limits_ms", layout["durations"])):
            values = profile.get(field)
            if not isinstance(values, dict) or set(values) != expected:
                raise ValueError("invalid extension " + field)
            for value in values.values():
                positive(value, "extension " + field)
                if field == "declaration_versions" and value != 1:
                    raise ValueError("unsupported declaration version")
        for field in ("unsupported_operations", "explicit_trusted_routes", "host_prerequisites"):
            strings(profile.get(field), field)
            if not profile[field]:
                raise ValueError("missing extension " + field)
        if set(profile["unsupported_operations"]) != layout["unsupported"]:
            raise ValueError("unsupported extension operation scope")
        operations = profile.get("implemented_operations")
        if not isinstance(operations, list) or len(operations) != len(layout["operations"]):
            raise ValueError("invalid extension operations")
        operation_names = set()
        mutation = name == "morrow.mutation.v1"
        for operation in operations:
            if not isinstance(operation, dict) or not isinstance(operation.get("name"), str) or operation["name"] not in layout["operations"] or operation["name"] in operation_names:
                raise ValueError("invalid or duplicate extension operation")
            operation_names.add(operation["name"])
            if (operation.get("route") != layout["operations"][operation["name"]]
                    or operation.get("platform_requirement") != ("windows" if mutation else "native")
                    or operation.get("implementation_compiled") is not (windows if mutation else True)):
                raise ValueError("invalid extension operation route or platform")
        if name == "morrow.io.v1":
            expected = ["FileRead", "FileList", "FileCreate", "FileReplace", "FileDelete", "HttpRequest", "HttpListen", "HttpPublish", "CredentialUse", "WebSocketConnect"]
            capabilities = profile.get("declared_capabilities")
            if not isinstance(capabilities, list) or len(capabilities) != len(expected):
                raise ValueError("invalid declared IO capabilities")
            for number, (capability, expected_name) in enumerate(zip(capabilities, expected), 1):
                if not isinstance(capability, dict) or capability.get("name") != expected_name or type(capability.get("number")) is not int or capability["number"] != number:
                    raise ValueError("invalid declared IO capability")
            history = profile.get("operation_history")
            flags = {"single_exact_operation": True, "fresh_grant_required": True, "status_only": True,
                     "body_available": False, "dispatch_authority": False, "automatic_replay": False,
                     "general_recovery": False, "raw_or_brokered_query_route": False}
            if not isinstance(history, dict) or history.get("capability") != "HttpRequest" or any(history.get(k) is not v for k, v in flags.items()):
                raise ValueError("unsupported operation history scope")
        if name == "morrow.service-resources.v1" and profile.get("header") != "morrow-service-resources-v1":
            raise ValueError("invalid service resources header")


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
            contract_digest(contract)
            if "version" in contract:
                positive(contract["version"], "contract version")
            elif name != "content":
                raise ValueError("missing contract version")
        for field in ("runtime_task_modes", "workbench_routes", "runtime_supported_required_features"):
            strings(profile.get(field), field)
        constraints = profile.get("workbench_constraints")
        if not isinstance(constraints, dict) or any(type(constraints.get(k)) is not bool for k in ("content_task", "dependency_calls", "required_dependencies")):
            raise ValueError("invalid Workbench route constraints")
        if profile["id"] == "morrow.channel.v1" and any(
                constraints.get(k) is not False for k in
                ("content_task", "dependency_calls", "required_dependencies", "channel_binding")):
            raise ValueError("unsupported channel Workbench route constraints")
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
    if "discovery" in experiments:
        extension_discovery(experiments["discovery"], platform, value['profiles'])
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
