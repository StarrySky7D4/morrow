"""Explicit trusted-host discovery; bounded output, no authority or guest run."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import threading
import time

MAX_OUTPUT = 65536
# Version-1 diagnostic bounds mirror core::plugin_package, not a caller-selected
# or untrusted advertised allocation size. A new format requires a new consumer.
MAX_MODULE_BYTES = 4 * 1024 * 1024
MAX_RAW_BYTES = MAX_MODULE_BYTES + 64 * 1024 + 256
MAX_PACKAGE_BYTES = MAX_RAW_BYTES + MAX_RAW_BYTES // 255 + 128
PREFLIGHT_MAX_OUTPUT = 16384
PREFLIGHT_COMMAND = "--sdk-preflight"
PREFLIGHT_BYTE_LIMITS = {"archive": MAX_PACKAGE_BYTES, "module": MAX_MODULE_BYTES}


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


# Exact known payload identity; this digest is SHA256 of the canonical wire
# bytes, not a normalized Cap'n Proto schema or a source-authority assertion.
CHANGES_PAYLOAD_SHA256 = "07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089"


def channel_payload_discovery(value, platform):
    envelope = {"schema_version", "status", "authority", "package_preflight_required", "profiles"}
    if (not isinstance(value, dict) or set(value) != envelope
            or type(value.get("schema_version")) is not int or value["schema_version"] != 1
            or value.get("status") != "compiled_metadata_only" or value.get("authority") != "none"
            or value.get("package_preflight_required") is not True
            or not isinstance(value.get("profiles"), list) or len(value["profiles"]) != 1):
        raise ValueError("invalid channel payload discovery")
    profile = value["profiles"][0]
    fields = {"id", "status", "required_features", "payload_contract", "wire_limits", "count_limits",
              "channel_kind", "duplex", "finite_window", "metadata_only", "runtime_static_preparation_supported",
              "native_source_required", "native_source_adapter_compiled", "native_prerequisites", "workbench_routes",
              "production_public_binding_available", "automatic_run_available", "qualification"}
    if not isinstance(profile, dict) or set(profile) != fields:
        raise ValueError("invalid changes payload profile fields")
    for key, expected in {"id": "morrow.changes-metadata.v1", "status": "experimental", "channel_kind": "Events",
                          "qualification": "not_established_by_discovery"}.items():
        if profile.get(key) != expected:
            raise ValueError("invalid changes payload " + key)
    if profile.get("required_features") != ["channel-v1", "changes-metadata-v1"]:
        raise ValueError("invalid changes payload features")
    contract = profile.get("payload_contract")
    if (not isinstance(contract, dict) or set(contract) != {"version", "sha256"}
            or type(contract.get("version")) is not int or contract["version"] != 1
            or contract.get("sha256") != CHANGES_PAYLOAD_SHA256):
        raise ValueError("unsupported changes payload contract")
    for key, expected in {"wire_limits": {"header_bytes": 150, "payload_bytes": 662, "card_id_bytes": 256,
                                        "operation_id_bytes": 256, "cursor_bytes": 32},
                          "count_limits": {"cards": 32}}.items():
        actual = profile.get(key)
        if not isinstance(actual, dict) or actual != expected or any(type(v) is not int for v in actual.values()):
            raise ValueError("invalid changes payload " + key)
    flags = {"duplex": False, "finite_window": True, "metadata_only": True,
             "runtime_static_preparation_supported": True, "native_source_required": True,
             "native_source_adapter_compiled": platform['arch'] != 'wasm32',
             "production_public_binding_available": False, "automatic_run_available": False}
    if any(type(profile.get(k)) is not bool or profile[k] != v for k, v in flags.items()):
        raise ValueError("invalid changes payload authority or native prerequisites")
    if (profile.get("workbench_routes") != [] or profile.get("native_prerequisites") != [
            "managed_channel_broker", "fresh_receiver_specific_changes_approval", "exact_package_binding", "live_store_binding"]):
        raise ValueError("invalid changes payload route prerequisites")
    return value



# Optional new directory metadata has an independent identity; legacy discovery
# records remain accepted unchanged. These checks never create a live selection.
DIRECTORY_REQUEST_SCHEMA_SHA256 = "04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df"
DIRECTORY_IO_SCHEMA_SHA256 = "78e87b3f7b7a2df7675937f3bea84d5b33399cd4559d560c7d4ab7f4da4bccf5"
DIRECTORY_PAGE_SCHEMA_SHA256 = "ade60daee77497056a3fe618b38616331b8d5de61bdb494f703592e74ec3d5f7"
DIRECTORY_REQUEST_BYTE_LIMITS = {
    "request": 512, "response": 65536, "page": 65536, "nomination_ref": 32,
    "nonce": 32, "selection_epoch": 32, "entry_id": 32,
}
DIRECTORY_SELECTION_SCOPE = {
    "host_nominated_precaptured_selection": True, "live_file_list_binding_required": True,
    "guest_paths": False, "os_handles": False, "guest_capture": False,
    "directory_session_serial": False, "new_grants": False, "automatic_replay": False,
    "native_picker_attestation": False, "ancestors_above_anchor_attested": False,
    "atomic_filesystem_snapshot": False, "finish_cancel_proves_join": False,
}


def directory_request_discovery(detail, platform, base_profiles, legacy_discovery=None):
    if (not isinstance(detail, dict) or type(detail.get("schema_version")) is not int
            or detail["schema_version"] != 1 or detail.get("status") != "compiled_metadata_only"
            or detail.get("authority") != "none" or detail.get("package_preflight_required") is not True):
        raise ValueError("invalid directory request discovery")
    profiles = detail.get("profiles")
    if not isinstance(profiles, list) or len(profiles) != 1 or not isinstance(profiles[0], dict):
        raise ValueError("invalid directory request profiles")
    profile = profiles[0]
    for key, expected in {"id": "morrow.fs-directory-request.v1", "status": "experimental",
                          "guest_abi_version": 2, "package_schema_version": 1,
                          "qualification": "not_established_by_discovery"}.items():
        if profile.get(key) != expected or (type(expected) is int and type(profile.get(key)) is not int):
            raise ValueError("invalid directory request identity")
    expected_flags = {"production_public_binding_available": False, "automatic_run_available": False,
                      "combined_dependency_import": False, "combined_channel_import": False,
                      "runtime_static_preparation_supported": True,
                      "native_source_adapter_compiled": platform["os"] == "windows" and platform["arch"] != "wasm32"}
    if any(type(profile.get(key)) is not bool or profile[key] != value for key, value in expected_flags.items()):
        raise ValueError("invalid directory request availability")
    if (profile.get("required_features") != ["io-v1", "fs-directory-request-v1"]
            or profile.get("optional_features") != [] or profile.get("workbench_routes") != []
            or profile.get("explicit_trusted_routes") != ["IoWorker::submit_directory_guest_frame"]):
        raise ValueError("invalid directory request route or features")
    if profile.get("import") != {"module": "morrow_fs_directory_v1", "name": "call",
                                "parameters": ["i32"] * 4, "result": "i32"}:
        raise ValueError("invalid directory request import")
    contracts = profile.get("contracts")
    if not isinstance(contracts, dict) or set(contracts) != {"runtime", "task", "io", "directory_request", "directory_page"}:
        raise ValueError("invalid directory request contracts")
    base = next((item for item in base_profiles if item["id"] == "morrow.guest-task.v3"), None)
    if base is None or any(contracts[key] != base["contracts"][key] for key in ("runtime", "task")):
        raise ValueError("directory request base contract mismatch")
    for key, item in contracts.items():
        contract_digest(item)
        if (type(item.get("version")) is not int or item["version"] < 1
                or (key not in ("runtime", "task") and item["version"] != 1)):
            raise ValueError("invalid directory request contract version")
    if contracts["io"]["sha256"] != DIRECTORY_IO_SCHEMA_SHA256:
        raise ValueError("directory request exact IO schema mismatch")
    if legacy_discovery is not None:
        original_io = next((item for item in legacy_discovery["profiles"] if item["id"] == "morrow.io.v1"), None)
        if original_io is None or contracts["io"] != original_io["contracts"]["io"]:
            raise ValueError("directory request legacy IO contract mismatch")
    if (contracts["directory_request"]["sha256"] != DIRECTORY_REQUEST_SCHEMA_SHA256
            or contracts["directory_page"]["sha256"] != DIRECTORY_PAGE_SCHEMA_SHA256):
        raise ValueError("directory request exact schema mismatch")
    limits = profile.get("hard_byte_limits")
    if (not isinstance(limits, dict) or limits != DIRECTORY_REQUEST_BYTE_LIMITS
            or any(type(value) is not int for value in limits.values())
            or profile.get("declaration_versions") != {"io": 1}
            or type(profile["declaration_versions"]["io"]) is not int
            or profile.get("count_limits") != {"resident_observations": 8}
            or type(profile["count_limits"]["resident_observations"]) is not int
            or profile.get("duration_limits_ms") != {}):
        raise ValueError("invalid directory request limits")
    scope = profile.get("selection_scope")
    if (not isinstance(scope, dict) or set(scope) != set(DIRECTORY_SELECTION_SCOPE)
            or any(type(scope[key]) is not bool or scope[key] != value for key, value in DIRECTORY_SELECTION_SCOPE.items())):
        raise ValueError("invalid directory selection scope")
    helper = profile.get("helper_profile")
    expected_helper = {"version": 1, "request_schema_sha256": DIRECTORY_REQUEST_SCHEMA_SHA256,
                       "response_original_request_sha256": True, "one_pending_call": True,
                       "automatic_retry": False, "authority": "none",
                       "input": "original_open_request_wire", "completion": "exact_last_directory_response_wire",
                       "standard_typed_task_helpers": False, "task_read_input_capacity": 131072}
    if (not isinstance(helper, dict) or helper != expected_helper or type(helper["version"]) is not int
            or any(type(helper[key]) is not bool for key in ("response_original_request_sha256", "one_pending_call", "automatic_retry", "standard_typed_task_helpers"))
            or type(helper["task_read_input_capacity"]) is not int):
        raise ValueError("invalid directory request helper profile")
    native = expected_flags["native_source_adapter_compiled"]
    expected_operations = [{"name": action, "route": "managed_directory_owner",
                            "implementation_compiled": native, "platform_requirement": "windows"}
                           for action in ("Open", "Next", "Finish", "Cancel")]
    operations = profile.get("implemented_operations")
    if (operations != expected_operations or not isinstance(operations, list)
            or any(type(operation.get("implementation_compiled")) is not bool for operation in operations)):
        raise ValueError("invalid directory request operation availability")
    if profile.get("unsupported_operations") != ["GuestCapture", "GuestPath", "ReadFileContents", "Watch", "Rename", "Delete", "ConditionalReplace", "DurableReopen", "NonWindowsDirectoryOwner"]:
        raise ValueError("invalid directory request unsupported operations")
    strings(profile.get("host_prerequisites"), "directory request prerequisites")
    if not profile["host_prerequisites"]:
        raise ValueError("missing directory request prerequisites")


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
            if "payload_discovery" in profile:
                channel_payload_discovery(profile["payload_discovery"], platform)
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
    if "directory_request_discovery" in experiments:
        directory_request_discovery(experiments["directory_request_discovery"], platform, value["profiles"], experiments.get("discovery"))
    if "diagnostic_capabilities" in value:
        preflight_capability(value)
    return value


def preflight_capability(descriptor):
    diagnostics = descriptor.get("diagnostic_capabilities")
    if not isinstance(diagnostics, dict) or set(diagnostics) != {"package_preflight"}:
        raise ValueError("host does not advertise a supported package preflight diagnostic")
    capability = diagnostics["package_preflight"]
    expected = {"schema_version": 1, "command": PREFLIGHT_COMMAND, "read_only": True,
                "preparation": "static_only", "max_output_bytes": PREFLIGHT_MAX_OUTPUT,
                "authority": "none", "hard_byte_limits": PREFLIGHT_BYTE_LIMITS}
    if (not isinstance(capability, dict) or capability != expected
            or type(capability.get("schema_version")) is not int
            or type(capability.get("max_output_bytes")) is not int
            or any(type(value) is not int for value in capability.get("hard_byte_limits", {}).values())
            or capability.get("read_only") is not True):
        raise ValueError("unsupported or contradictory package preflight diagnostic")
    base = next((p for p in descriptor["profiles"] if p["id"] == "morrow.guest-task.v3"), None)
    if base is None or base["hard_byte_limits"].get("module") != MAX_MODULE_BYTES:
        raise ValueError("package preflight disagrees with compiled module limit")
    validate_limits(base["runtime_defaults"], "compiled host limits")
    for profile in descriptor["profiles"]:
        if profile["runtime_defaults"] != base["runtime_defaults"]:
            raise ValueError("package preflight has inconsistent compiled host limits")
    return capability


def validate_limits(value, label):
    bounds = {"fuel": (1, 100_000_000), "memory_bytes": (65536, 64 * 1024 * 1024),
              "host_calls": (0, 1024)}
    if not isinstance(value, dict) or set(value) != set(bounds):
        raise ValueError("invalid " + label)
    for key, (low, high) in bounds.items():
        if type(value[key]) is not int or not low <= value[key] <= high:
            raise ValueError("invalid " + label + "." + key)
    if value["memory_bytes"] % 65536:
        raise ValueError("invalid " + label + ".memory_bytes")


def validate_preflight(value, descriptor, archive, exit_code=0):
    fields = {"schema_version", "diagnostic", "host_version", "platform", "backend",
              "status", "phase", "authority", "guest_executed", "installed",
              "production_qualified", "routes_qualified", "package", "host_limits",
              "effective_limits", "hard_byte_limits", "error", "preparation", "grants_created"}
    if (not isinstance(value, dict) or set(value) != fields
            or type(value.get("schema_version")) is not int or value["schema_version"] != 1
            or value.get("diagnostic") != "package_preflight"):
        raise ValueError("unsupported package preflight response schema")
    if any(value[key] != descriptor[key] for key in ("host_version", "platform", "backend")):
        raise ValueError("package preflight host identity does not match discovery")
    if (value["authority"] != "none" or type(value["grants_created"]) is not int
            or value["grants_created"] != 0 or any(value[key] is not False for key in
            ("guest_executed", "installed", "production_qualified", "routes_qualified"))):
        raise ValueError("package preflight claims authority, execution, installation or qualification")
    if (type(exit_code) is not int or exit_code not in (0, 2)
            or value["status"] != ("prepared" if exit_code == 0 else "rejected")
            or value["preparation"] != "static_only"):
        raise ValueError("package preflight exit/status or preparation mismatch")
    if (value["hard_byte_limits"] != PREFLIGHT_BYTE_LIMITS
            or any(type(limit) is not int for limit in value["hard_byte_limits"].values())):
        raise ValueError("package preflight hard limits do not match discovery")
    base = next(p for p in descriptor["profiles"] if p["id"] == "morrow.guest-task.v3")
    validate_limits(value["host_limits"], "host limits")
    if value["host_limits"] != base["runtime_defaults"]:
        raise ValueError("package preflight host limits do not match discovery")
    if exit_code == 0:
        if value["phase"] != "static_preparation" or value["error"] is not None:
            raise ValueError("package preflight did not establish static preparation")
    else:
        error = value["error"]
        errors = {"invalid_arguments": ("arguments", False),
                  "invalid_file_type": ("package_read_decode", False),
                  "package_unavailable": ("package_read_decode", False),
                  "package_rejected": ("package_read_decode", False),
                  "preparation_rejected": ("static_preparation", True),
                  "output_limit": ("static_preparation", False)}
        if (not isinstance(error, dict) or set(error) != {"code", "message"}
                or not isinstance(error["code"], str) or error["code"] not in errors
                or not isinstance(error["message"], str) or not error["message"]
                or len(error["message"].encode("utf-8")) > 4096
                or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in error["message"])):
            raise ValueError("invalid package preflight rejection error")
        phase, has_package = errors[error["code"]]
        if value["phase"] != phase or value["effective_limits"] is not None:
            raise ValueError("inconsistent package preflight rejection phase or limits")
        if not has_package:
            if value["package"] is not None:
                raise ValueError("package preflight rejection unexpectedly claims a decoded package")
            return value
    package = value["package"]
    package_fields = {"id", "version", "archive_sha256", "module_sha256", "archive_bytes",
                      "module_bytes", "manifest_schema_version", "guest_abi_version", "declared_limits"}
    if not isinstance(package, dict) or set(package) != package_fields:
        raise ValueError("invalid prepared package identity")
    for key, maximum in (("id", 256), ("version", 128)):
        strings([package[key]], "package " + key)
        if len(package[key].encode("utf-8")) > maximum:
            raise ValueError("invalid package " + key)
    if any(c in "/\\:" or 127 <= ord(c) <= 159 for c in package["id"]):
        raise ValueError("invalid package id")
    for key in ("archive_sha256", "module_sha256"):
        contract_digest({"sha256": package[key]})
    if package["archive_sha256"] != archive["sha256"]:
        raise ValueError("prepared package archive identity does not match selected file")
    for key, low, high in (("archive_bytes", 1, MAX_PACKAGE_BYTES), ("module_bytes", 8, MAX_MODULE_BYTES),
                           ("manifest_schema_version", 1, 1), ("guest_abi_version", 1, 2)):
        if type(package[key]) is not int or not low <= package[key] <= high:
            raise ValueError("invalid prepared package " + key)
    if package["archive_bytes"] != archive["bytes"]:
        raise ValueError("prepared package archive size does not match selected file")
    validate_limits(package["declared_limits"], "declared limits")
    if exit_code == 2:
        return value
    validate_limits(value["effective_limits"], "effective limits")
    expected = {key: min(value["host_limits"][key], package["declared_limits"][key])
                for key in value["host_limits"]}
    if value["effective_limits"] != expected:
        raise ValueError("package preflight effective limits are not the exact bounded intersection")
    return value


def bounded_process(argv, *, max_output=MAX_OUTPUT):
    exit_code, raw = bounded_process_result(argv, max_output=max_output)
    if exit_code:
        raise ValueError("SDK profile query failed (exit " + str(exit_code) + ")")
    return raw


def bounded_process_result(argv, *, max_output=MAX_OUTPUT):
    """Internal protocol runner; callers must validate the returned exit code."""
    process = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    outputs = [bytearray(), bytearray()]
    exceeded = threading.Event()
    errors = []

    def read(index, stream):
        try:
            while len(outputs[index]) <= max_output:
                chunk = stream.read(min(4096, max_output + 1 - len(outputs[index])))
                if not chunk:
                    return
                outputs[index].extend(chunk)
                if len(outputs[index]) > max_output:
                    exceeded.set()
                    return
        except OSError as error:
            errors.append(error)
        finally:
            # The reader owns its pipe. A descendant can keep a pipe open past
            # the caller's deadline; closing it from the caller can block on the
            # buffered reader's lock. Close here even after a late EOF/error.
            try:
                stream.close()
            except OSError as error:
                errors.append(error)

    threads = [threading.Thread(target=read, args=(i, stream), daemon=True) for i, stream in enumerate((process.stdout, process.stderr))]
    for thread in threads:
        thread.start()
    deadline = time.monotonic() + 5
    try:
        while process.poll() is None:
            if exceeded.is_set():
                raise ValueError("SDK diagnostic output exceeds " + str(max_output) + " bytes")
            if time.monotonic() >= deadline:
                raise ValueError("SDK profile query timed out")
            time.sleep(0.01)
        for thread in threads:
            thread.join(timeout=max(0, deadline - time.monotonic()))
        if any(thread.is_alive() for thread in threads):
            raise ValueError("SDK profile pipe did not close")
        if exceeded.is_set():
            raise ValueError("SDK diagnostic output exceeds " + str(max_output) + " bytes")
        if errors:
            raise ValueError("SDK profile pipe read failed")
        return process.returncode, bytes(outputs[0])
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=1)
        for thread in threads:
            thread.join(timeout=0.5)


def regular_path(path, label):
    path = Path(path)
    metadata = path.lstat()
    if (not stat.S_ISREG(metadata.st_mode)
            or getattr(metadata, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)):
        raise ValueError(label + " must be a regular file, not a symlink or reparse point")
    return path.resolve(strict=True)


def file_identity(metadata):
    # On Windows Python 3.12, path stat retains creation-time ctime while
    # descriptor stat reports change-time ctime. Birth time is consistent on
    # both APIs; Python 3.11 uses creation-time ctime on both instead.
    timestamp = (getattr(metadata, "st_birthtime_ns", metadata.st_ctime_ns)
                 if os.name == "nt" else metadata.st_ctime_ns)
    # Windows path stat synthesizes execute bits for .exe/.cmd/.bat names;
    # descriptor stat has no filename. Keep file type and read/write mode exact.
    mode = metadata.st_mode & ~0o111 if os.name == "nt" else metadata.st_mode
    return (metadata.st_dev, metadata.st_ino, mode, metadata.st_size,
            metadata.st_mtime_ns, timestamp)


def file_snapshot(path, label, maximum=None):
    # Stream the explicitly selected file. O_NONBLOCK and fstat also refuse a
    # FIFO substituted after the initial path check on platforms supporting it.
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode):
            raise ValueError(label + " must be a regular file")
        if maximum is not None and before.st_size > maximum:
            raise ValueError(label + " exceeds " + str(maximum) + " bytes")
        digest = hashlib.sha256()
        size = 0
        while True:
            chunk = stream.read(65536 if maximum is None else min(65536, maximum + 1 - size))
            if not chunk:
                break
            size += len(chunk)
            if maximum is not None and size > maximum:
                raise ValueError(label + " exceeds " + str(maximum) + " bytes")
            digest.update(chunk)
        after = os.fstat(stream.fileno())
    if file_identity(before) != file_identity(after) or file_identity(after) != file_identity(path.lstat()) or size != after.st_size:
        raise ValueError(label + " changed while reading")
    return {"sha256": digest.hexdigest(), "bytes": size, "identity": file_identity(after)}


def parse_json(raw, label):
    def invalid_constant(value):
        raise ValueError("non-JSON number: " + value)
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object, parse_constant=invalid_constant)
    except (UnicodeError, RecursionError, ValueError) as error:
        raise ValueError("invalid " + label + ": " + str(error)) from error


def query(host):
    # Preserve the profiles command's historical resolved-path behavior. The
    # new preflight route separately rejects a selected symlink/reparse entry.
    host = regular_path(Path(host).resolve(strict=True), "--host")
    before = file_snapshot(host, "host artifact")
    raw = bounded_process([str(host), "--sdk-capabilities"])
    if file_snapshot(host, "host artifact") != before:
        raise ValueError("host artifact changed during SDK profile query")
    try:
        value = validate_descriptor(parse_json(raw, "SDK profile descriptor"))
    except (UnicodeError, RecursionError, ValueError) as error:
        raise ValueError("invalid SDK profile descriptor: " + str(error)) from error
    return {"host_sha256": before["sha256"], "descriptor_sha256": hashlib.sha256(raw).hexdigest(), "descriptor": value}


def preflight(host, package):
    """Ask one explicitly trusted host to statically prepare one selected archive.

    Receipts bind before/after observations, not a sandbox against a malicious
    executable or a local actor replacing and restoring files between reads.
    """
    host = regular_path(host, "--host")
    before = file_snapshot(host, "host artifact")
    discovered = query(host)
    descriptor = discovered["descriptor"]
    preflight_capability(descriptor)  # Never send a new flag to an old host.
    package = regular_path(package, "package")
    archive = file_snapshot(package, "package archive", MAX_PACKAGE_BYTES)
    if file_snapshot(host, "host artifact") != before or discovered["host_sha256"] != before["sha256"]:
        raise ValueError("host artifact changed before package preflight")
    # Do not execute command text from a descriptor, even after validation.
    exit_code, raw = bounded_process_result([str(host), PREFLIGHT_COMMAND, str(package)], max_output=PREFLIGHT_MAX_OUTPUT)
    if exit_code not in (0, 2):
        raise ValueError("package preflight query failed (exit " + str(exit_code) + ")")
    if file_snapshot(host, "host artifact") != before:
        raise ValueError("host artifact changed during package preflight")
    if file_snapshot(package, "package archive", MAX_PACKAGE_BYTES) != archive:
        raise ValueError("package archive changed during package preflight")
    value = validate_preflight(parse_json(raw, "package preflight response"), descriptor, archive, exit_code)
    return {"host_sha256": before["sha256"], "descriptor_sha256": discovered["descriptor_sha256"],
            "archive_sha256": archive["sha256"], "response_sha256": hashlib.sha256(raw).hexdigest(),
            "preflight": value}
