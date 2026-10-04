//! Read-only compiled host profile discovery. This never opens an owner,
//! database or guest, and is neither an authorization nor a package preflight.
use morrow_core::{
    channel, dependency_call, io, mutation, plugin_package, runtime, service, service_resources,
    task, ui,
};
use serde_json::{Value, json};

fn digest(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// Optional discovery metadata has its own version. The schema-1 base/channel
// profiles and legacy extension status remain unchanged for old consumers.
fn extension_profile(id: &str, contract: &str, version: u16, sha256: [u8; 32]) -> Value {
    let mut value = json!({
        "id": id,
        "status": "experimental",
        "guest_abi_version": 2,
        "package_schema_version": 1,
        "contracts": {
            "runtime": {"version": runtime::PROTOCOL_VERSION, "sha256": digest(runtime::runtime_digest())},
            "task": {"version": task::VERSION, "sha256": digest(task::schema_digest())}
        },
        "required_features": [],
        "optional_features": [],
        "declaration_versions": {},
        "hard_byte_limits": {},
        "count_limits": {},
        "duration_limits_ms": {},
        "implemented_operations": [],
        "unsupported_operations": [],
        "explicit_trusted_routes": [],
        "host_prerequisites": [],
        "production_public_binding_available": false,
        "qualification": "not_established_by_discovery",
        "combined_dependency_import": false,
        "combined_channel_import": false
    });
    value["contracts"][contract] = json!({"version": version, "sha256": digest(sha256)});
    value
}

fn operations(names: &[&str], route: &str, windows_only: bool) -> Vec<Value> {
    names.iter().map(|name| json!({
        "name": name,
        "route": route,
        "implementation_compiled": !cfg!(target_arch = "wasm32") && (!windows_only || cfg!(windows)),
        "platform_requirement": if windows_only { "windows" } else { "native" }
    })).collect()
}

fn io_profile() -> Value {
    use declaration::IoCapability;
    use plugin_package::io as declaration;
    let mut value = extension_profile("morrow.io.v1", "io", io::VERSION, io::schema_digest());
    value["required_features"] = json!([declaration::FEATURE]);
    value["declaration_versions"] = json!({"io": declaration::DECLARATION_VERSION});
    // Recognition of an enum declaration is deliberately separate from dispatch.
    value["declared_capabilities"] = json!(
        [
            ("FileRead", IoCapability::FileRead),
            ("FileList", IoCapability::FileList),
            ("FileCreate", IoCapability::FileCreate),
            ("FileReplace", IoCapability::FileReplace),
            ("FileDelete", IoCapability::FileDelete),
            ("HttpRequest", IoCapability::HttpRequest),
            ("HttpListen", IoCapability::HttpListen),
            ("HttpPublish", IoCapability::HttpPublish),
            ("CredentialUse", IoCapability::CredentialUse),
            ("WebSocketConnect", IoCapability::WebSocketConnect)
        ]
        .map(|(name, capability)| json!({"name": name, "number": capability.number()}))
    );
    value["hard_byte_limits"] = json!({
        "io_frame": io::MAX_FRAME_BYTES, "io_payload": io::MAX_PAYLOAD_BYTES,
        "method": io::MAX_METHOD_BYTES, "relative_target": io::MAX_TARGET_BYTES,
        "header_name": io::MAX_HEADER_NAME_BYTES, "header_value": io::MAX_HEADER_VALUE_BYTES,
        "headers": io::MAX_HEADER_BYTES, "credential_reference": io::MAX_CREDENTIAL_BYTES,
        "operation_id": io::MAX_OPERATION_BYTES, "operation_response": io::MAX_OPERATION_RESPONSE_BYTES,
        "endpoint_reference": io::MAX_ENDPOINT_BYTES,
        "declared_total": declaration::MAX_BYTES, "declared_job": declaration::MAX_JOB_BYTES
    });
    value["count_limits"] = json!({"headers": io::MAX_HEADERS, "declared_capabilities": declaration::MAX_CAPABILITIES,
        "declared_handlers": declaration::MAX_HANDLERS, "declared_resources": declaration::MAX_RESOURCES,
        "declared_concurrent_jobs": declaration::MAX_JOBS});
    value["duration_limits_ms"] = json!({"declared_io": declaration::MAX_DURATION_MS, "http_submission": io::MAX_SUBMIT_DEADLINE_MS});
    let mut implemented = operations(&["Read", "Finish", "Cancel"], "managed_file_broker", false);
    implemented.extend(operations(&["SubmitHttp"], "managed_http_broker", false));
    implemented.extend(operations(
        &["QueryOperation"],
        "managed_operation_history",
        false,
    ));
    value["implemented_operations"] = json!(implemented);
    value["unsupported_operations"] = json!([
        "SubmitFileRead",
        "SubmitFileList",
        "SubmitFileCreate",
        "SubmitFileReplace",
        "SubmitFileDelete",
        "SubmitHttpListen",
        "SubmitHttpPublish",
        "SubmitHttpUnpublish",
        "SubmitWebSocketConnect",
        "SubmitServiceAccept",
        "SubmitServiceReply",
        "Poll",
        "Write"
    ]);
    value["explicit_trusted_routes"] = json!([
        "FileBroker::grant_file/grant_open_file",
        "IoWorker::submit_brokered",
        "IoWorker::submit_operation_history",
        "Workbench::start_file/start_selected_file",
        "Workbench::start_http"
    ]);
    value["host_prerequisites"] = json!([
        "ABI2 IO package and separate IO handler; Runner::new_io_task; current managed instance and IoBinding",
        "File Read/Finish/Cancel need a host-selected immutable spool and exact live resource reference",
        "SubmitHttp needs approved endpoint/method/credential references and a trusted broker with durable dispatch",
        "Workbench routes require the original protected Storage owner; compiled native code alone is insufficient"
    ]);
    value["operation_history"] = json!({
        "capability": "HttpRequest", "single_exact_operation": true, "fresh_grant_required": true,
        "status_only": true, "body_available": false, "dispatch_authority": false,
        "automatic_replay": false, "general_recovery": false,
        "raw_or_brokered_query_route": false
    });
    value
}

fn service_profile() -> Value {
    use plugin_package::io as declaration;
    let mut value = extension_profile(
        "morrow.service.v1",
        "service",
        service::VERSION,
        service::schema_digest(),
    );
    value["contracts"]["io"] =
        json!({"version": io::VERSION, "sha256": digest(io::schema_digest())});
    value["required_features"] = json!([declaration::FEATURE]);
    value["optional_features"] = json!([
        declaration::SERVICE_RUN_FEATURE,
        declaration::SERVICE_RUN_BUDGET_FEATURE
    ]);
    value["declaration_versions"] = json!({"io": declaration::DECLARATION_VERSION,
        "service_run": declaration::SERVICE_RUN_VERSION, "service_run_budget": declaration::SERVICE_RUN_BUDGET_VERSION});
    value["hard_byte_limits"] = json!({"service_frame": service::MAX_FRAME_BYTES, "service_body": service::MAX_BODY_BYTES,
        "headers": service::MAX_HEADER_BYTES, "declared_job": declaration::MAX_JOB_BYTES, "declared_total": declaration::MAX_BYTES});
    value["count_limits"] = json!({"headers": service::MAX_HEADERS, "declared_concurrent_jobs": declaration::MAX_JOBS,
        "service_run_cumulative_jobs": declaration::MAX_SERVICE_RUN_JOBS});
    value["duration_limits_ms"] = json!({"request": declaration::MAX_DURATION_MS, "service_run": declaration::MAX_SERVICE_RUN_DURATION_MS});
    value["implemented_operations"] = json!(operations(
        &["Invocation", "Reply"],
        "managed_service_worker",
        false
    ));
    value["unsupported_operations"] = json!([
        "GuestListen",
        "GuestPublish",
        "UnlimitedServiceRun",
        "AsyncDependencyComposition"
    ]);
    value["explicit_trusted_routes"] = json!([
        "IoWorker::submit_service/submit_service_durable/submit_service_content",
        "Workbench::start_service"
    ]);
    value["host_prerequisites"] = json!([
        "IO declaration pins service digest and HttpPublish; finite service-run also requires HttpListen",
        "Independent ServiceGrant and ListenerGrant; trusted authentication and original managed IO owner",
        "Invocation/Reply are fixed service task frames, not IO SubmitServiceAccept/SubmitServiceReply",
        "Content commands require separately authorized ServiceContentPolicy; outbound IO requires separate endpoint grants",
        "Run lifetime and cumulative budget do not extend per-request IO deadlines; no unlimited daemon",
        "Workbench::start_service requires both service-run-v1 and service-run-budget-v1 via bind_budgeted_service_run",
        "Workbench requires protected Storage; protected TLS additionally requires its platform identity backend"
    ]);
    value
}

fn service_resources_profile() -> Value {
    let mut value = extension_profile(
        "morrow.service-resources.v1",
        "service_resources",
        service_resources::VERSION,
        service_resources::schema_digest(),
    );
    value["contracts"]["io"] =
        json!({"version": io::VERSION, "sha256": digest(io::schema_digest())});
    value["contracts"]["service"] =
        json!({"version": service::VERSION, "sha256": digest(service::schema_digest())});
    value["required_features"] = json!([plugin_package::io::FEATURE, service_resources::FEATURE]);
    value["hard_byte_limits"] = json!({"directory_frame": service_resources::MAX_FRAME_BYTES});
    value["count_limits"] = json!({"endpoints": service_resources::MAX_ENDPOINTS});
    value["implemented_operations"] = json!(operations(
        &["Directory"],
        "host_injected_service_header",
        false
    ));
    value["unsupported_operations"] = json!([
        "GuestEndpointDiscovery",
        "CredentialDisclosure",
        "GrantFromMetadata"
    ]);
    value["explicit_trusted_routes"] = json!([
        "SelectedService::resolve/approve/approve_windows",
        "PreparedService::attach",
        "ServiceHost::new_owned_with_resources",
        "Workbench::start_service_with_outbound"
    ]);
    value["header"] = json!(service_resources::HEADER);
    value["host_prerequisites"] = json!([
        "Explicit service-resources-v1 opt-in plus HttpRequest/HttpPublish declaration and exact service digest",
        "Host-selected endpoint directory is injected into the service invocation as a canonical lowercase-hex header",
        "Opaque endpoint/credential references and scope digest are metadata; original live broker reauthorizes every use",
        "No additional guest import; metadata never discovers or authorizes arbitrary endpoints"
    ]);
    value
}

fn mutation_profile() -> Value {
    let mut value = extension_profile(
        "morrow.mutation.v1",
        "mutation",
        mutation::VERSION,
        mutation::schema_digest(),
    );
    value["contracts"]["io"] =
        json!({"version": io::VERSION, "sha256": digest(io::schema_digest())});
    value["required_features"] = json!([
        plugin_package::io::FEATURE,
        plugin_package::MUTATION_FEATURE
    ]);
    value["optional_features"] = json!([plugin_package::MUTATION_BUDGET_FEATURE]);
    value["hard_byte_limits"] = json!({"mutation_frame": mutation::MAX_FRAME_BYTES, "chunk": mutation::MAX_CHUNK_BYTES,
        "content": mutation::MAX_CONTENT_BYTES, "operation_id": mutation::MAX_OPERATION_BYTES,
        "extended_job": plugin_package::MAX_MUTATION_JOB_BYTES, "extended_total": plugin_package::MAX_MUTATION_BYTES});
    value["duration_limits_ms"] = json!({"mutation_deadline": mutation::MAX_DEADLINE_MS});
    value["implemented_operations"] = json!(operations(
        &[
            "PrepareCreate",
            "PrepareDelete",
            "Chunk",
            "Commit",
            "Execute",
            "Query",
            "CancelPlan",
            "Release"
        ],
        "managed_mutation_worker",
        true
    ));
    value["unsupported_operations"] = json!([
        "ConditionalReplace",
        "GeneralFilesystem",
        "NonWindowsMutationWorker"
    ]);
    value["explicit_trusted_routes"] = json!([
        "IoWorker::submit_mutation_guest_frame",
        "Workbench::start_selected_mutation",
        "Workbench::submit_mutation"
    ]);
    value["host_prerequisites"] = json!([
        "ABI2 IO declaration with FileCreate or FileDelete, exact mutation digest and Runner::new_mutation_task",
        "Windows managed mutation owner, current IoBinding and opaque selected-target lease; no guest path authority",
        "Current reviewed plan/content and explicit execution approval; Query does not restore execution authority",
        "mutation-budget-v1 is opt-in and mutation-only; no service/dependency combination or implicit larger budget",
        "Conditional replacement is explicitly Unsupported; no weaker fallback or replay of unknown effects"
    ]);
    value
}

fn extension_discovery() -> Value {
    json!({
        "schema_version": 1,
        "status": "compiled_metadata_only",
        "authority": "none",
        "package_preflight_required": true,
        "current_host_requirements": {
            "protected_owner_backend_compiled": cfg!(windows),
            "stored_http_credentials_compiled": cfg!(windows),
            "protected_tls_identity_compiled": cfg!(windows)
        },
        "policy": "Exported ceilings and extension-specific features, not a complete Manifest recipe. Package/current grants may be stricter. Compiled routes are not live grants or platform/product qualification.",
        "profiles": [io_profile(), service_profile(), service_resources_profile(), mutation_profile()]
    })
}

pub fn descriptor() -> Value {
    let defaults = morrow_plugin_runtime::Limits::default();
    json!({
        "schema_version": 1,
        "host_version": env!("CARGO_PKG_VERSION"),
        "platform": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "backend": "wasmi",
        "authority": "none",
        "profiles": [{
            "id": "morrow.guest-task.v3",
            "status": "candidate",
            "guest_abi_version": 2,
            "package_schema_version": 1,
            "contracts": {
                "runtime": {"version": runtime::PROTOCOL_VERSION, "sha256": digest(runtime::runtime_digest())},
                "content": {"sha256": digest(runtime::content_digest())},
                "task": {"version": task::VERSION, "sha256": digest(task::schema_digest())},
                "ui": {"version": ui::VERSION, "sha256": digest(ui::schema_digest())},
                "dependency_call": {"version": dependency_call::VERSION, "sha256": digest(dependency_call::schema_digest())}
            },
            "runtime_task_modes": ["content", "transform"],
            "workbench_routes": ["external_transform", "external_ui"],
            "workbench_constraints": {"content_task": false, "dependency_calls": false, "required_dependencies": false},
            "runtime_route_constraints": "Content requires a bound trusted content adapter; dependency calls require the managed dependency router. These are not Workbench external routes.",
            "runtime_supported_required_features": [plugin_package::TRANSFORM_HANDLERS_FEATURE, plugin_package::DEPENDENCIES_FEATURE, plugin_package::DEPENDENCY_CALLS_FEATURE],
            "hard_byte_limits": {"module": morrow_plugin_runtime::MAX_MODULE_BYTES, "message": runtime::MAX_MESSAGE_BYTES, "task_frame": task::MAX_TASK_BYTES, "task_value": task::MAX_VALUE_BYTES, "failure_text": task::MAX_FAILURE_MESSAGE_BYTES, "ui_frame": ui::MAX_BYTES, "dependency_frame": dependency_call::MAX_FRAME_BYTES, "dependency_value": dependency_call::MAX_PAYLOAD_BYTES},
            "runtime_defaults": {"fuel": defaults.fuel, "memory_bytes": defaults.memory_bytes, "host_calls": defaults.host_calls},
            "policy": "Package budgets and current host grants may be stricter; discovery grants nothing.",
            "task_lifecycle": "one fixed input and one correlated completion; failure, cancellation and teardown do not prove rollback"
        }, {
            "id": "morrow.channel.v1",
            "status": "experimental",
            "guest_abi_version": 2,
            "package_schema_version": 1,
            "contracts": {
                "runtime": {"version": runtime::PROTOCOL_VERSION, "sha256": digest(runtime::runtime_digest())},
                "task": {"version": task::VERSION, "sha256": digest(task::schema_digest())},
                "channel": {"version": channel::VERSION, "sha256": digest(channel::schema_digest())}
            },
            "runtime_task_modes": ["transform"],
            "workbench_routes": [],
            "workbench_constraints": {"content_task": false, "dependency_calls": false, "required_dependencies": false, "channel_binding": false},
            "runtime_route_constraints": "Requires the managed channel broker and an explicit trusted host source; an unbound import or Workbench external task is refused. Local sources grant no network, file, credential or process authority.",
            "runtime_supported_required_features": [plugin_package::TRANSFORM_HANDLERS_FEATURE, channel::FEATURE],
            "hard_byte_limits": {"module": morrow_plugin_runtime::MAX_MODULE_BYTES, "task_frame": task::MAX_TASK_BYTES, "task_value": task::MAX_VALUE_BYTES, "channel_frame": channel::MAX_WIRE_BYTES, "channel_payload": channel::MAX_PAYLOAD_BYTES, "channel_cursor": channel::MAX_CURSOR_BYTES},
            "runtime_defaults": {"fuel": defaults.fuel, "memory_bytes": defaults.memory_bytes, "host_calls": defaults.host_calls},
            "channel_scope": {"trusted_local_sources": true, "managed_binding_required": true, "workbench_binding": false, "network_backend": false, "cloud_account": false, "automatic_replay": false},
            "native_binding_implementation": crate::channel_binding::implementation_descriptor(),
            "policy": "Finite package ceilings intersect the current host grant and original instance control; discovery grants nothing.",
            "task_lifecycle": "Multiple bounded frames within one fixed task; ACK, send acceptance, terminal cause and actual producer join are distinct. Unknown is not replayed."
        }],
        "experimental_extensions": {"status": "recognized_experimental_not_fully_discovered", "feature_names": [plugin_package::io::FEATURE, plugin_package::io::SERVICE_RUN_FEATURE, plugin_package::io::SERVICE_RUN_BUDGET_FEATURE, morrow_core::service_resources::FEATURE, plugin_package::MUTATION_FEATURE, plugin_package::MUTATION_BUDGET_FEATURE], "discovery": extension_discovery()},
        "unsupported_requirements": ["workbench_public_channel_binding", "network_sse_websocket_backend", "cloud_account_change_subscription", "arbitrary_os_access", "untrusted_native_library", "multi_version_schema_fallback"],
        "legacy_abi1": {"runtime_route_exists": true, "frozen_original_profile": false},
        "extension_policy": "New mandatory semantics require a new required feature, independent versioned contract and explicit decoder/Runner/host route. Unknown features or mismatched versions/digests are rejected; old contracts are not rewritten.",
        "package_preflight_required": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_discovery_keeps_legacy_envelope_and_output_bound() {
        let value = descriptor();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["profiles"].as_array().unwrap().len(), 2);
        assert_eq!(
            value["experimental_extensions"]["status"],
            "recognized_experimental_not_fully_discovered"
        );
        let discovery = &value["experimental_extensions"]["discovery"];
        assert_eq!(discovery["schema_version"], 1);
        assert_eq!(discovery["authority"], "none");
        assert_eq!(discovery["package_preflight_required"], true);
        assert_eq!(discovery["profiles"].as_array().unwrap().len(), 4);
        assert!(serde_json::to_vec(&value).unwrap().len() < 65536);
        for profile in discovery["profiles"].as_array().unwrap() {
            assert_eq!(profile["production_public_binding_available"], false);
            assert_eq!(profile["qualification"], "not_established_by_discovery");
            assert_eq!(profile["combined_dependency_import"], false);
            assert_eq!(profile["combined_channel_import"], false);
        }
    }

    #[test]
    fn extension_contracts_match_current_compiled_core() {
        let discovery = extension_discovery();
        for (index, name, version, sha256) in [
            (0, "io", io::VERSION, io::schema_digest()),
            (1, "service", service::VERSION, service::schema_digest()),
            (
                2,
                "service_resources",
                service_resources::VERSION,
                service_resources::schema_digest(),
            ),
            (3, "mutation", mutation::VERSION, mutation::schema_digest()),
        ] {
            let profile = &discovery["profiles"][index];
            assert_eq!(profile["contracts"][name]["version"], version);
            assert_eq!(profile["contracts"][name]["sha256"], digest(sha256));
            assert_eq!(
                profile["contracts"]["runtime"]["sha256"],
                digest(runtime::runtime_digest())
            );
            assert_eq!(profile["contracts"]["task"]["version"], task::VERSION);
        }
    }

    #[test]
    fn declared_io_capabilities_do_not_promote_unsupported_wire_operations() {
        let profile = io_profile();
        let capabilities = profile["declared_capabilities"].as_array().unwrap();
        assert_eq!(capabilities.len(), plugin_package::io::MAX_CAPABILITIES);
        for capability in capabilities {
            assert!(
                plugin_package::io::IoCapability::from_number(
                    capability["number"].as_i64().unwrap() as i32
                )
                .is_ok()
            );
        }
        let names: Vec<_> = profile["implemented_operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| op["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["Read", "Finish", "Cancel", "SubmitHttp", "QueryOperation"]
        );
        let unsupported = profile["unsupported_operations"].as_array().unwrap();
        for name in [
            "SubmitFileRead",
            "SubmitFileList",
            "SubmitFileCreate",
            "SubmitFileReplace",
            "SubmitFileDelete",
            "SubmitWebSocketConnect",
            "SubmitServiceAccept",
            "SubmitServiceReply",
            "Poll",
            "Write",
        ] {
            assert!(unsupported.contains(&json!(name)));
        }
    }

    #[test]
    fn history_is_single_operation_http_status_not_general_recovery() {
        let profile = io_profile();
        let history = &profile["operation_history"];
        assert_eq!(history["capability"], "HttpRequest");
        for name in [
            "single_exact_operation",
            "fresh_grant_required",
            "status_only",
        ] {
            assert_eq!(history[name], true);
        }
        for name in [
            "body_available",
            "dispatch_authority",
            "automatic_replay",
            "general_recovery",
            "raw_or_brokered_query_route",
        ] {
            assert_eq!(history[name], false);
        }
    }

    #[test]
    fn extension_limits_keep_wire_and_optional_budgets_separate() {
        let io = io_profile();
        assert_eq!(io["hard_byte_limits"]["io_frame"], io::MAX_FRAME_BYTES);
        assert_eq!(io["hard_byte_limits"]["io_payload"], io::MAX_PAYLOAD_BYTES);
        assert_eq!(
            io["count_limits"]["declared_concurrent_jobs"],
            plugin_package::io::MAX_JOBS
        );
        let service = service_profile();
        assert_eq!(
            service["hard_byte_limits"]["service_body"],
            service::MAX_BODY_BYTES
        );
        assert_eq!(
            service["duration_limits_ms"]["request"],
            plugin_package::io::MAX_DURATION_MS
        );
        assert_eq!(
            service["duration_limits_ms"]["service_run"],
            plugin_package::io::MAX_SERVICE_RUN_DURATION_MS
        );
        assert_eq!(
            service["required_features"],
            json!([plugin_package::io::FEATURE])
        );
        assert_eq!(
            service["optional_features"],
            json!([
                plugin_package::io::SERVICE_RUN_FEATURE,
                plugin_package::io::SERVICE_RUN_BUDGET_FEATURE
            ])
        );
        let resources = service_resources_profile();
        assert_eq!(
            resources["hard_byte_limits"]["directory_frame"],
            service_resources::MAX_FRAME_BYTES
        );
        assert_eq!(
            resources["count_limits"]["endpoints"],
            service_resources::MAX_ENDPOINTS
        );
        assert_eq!(resources["header"], service_resources::HEADER);
        let mutation = mutation_profile();
        assert_eq!(
            mutation["hard_byte_limits"]["content"],
            mutation::MAX_CONTENT_BYTES
        );
        assert_eq!(
            mutation["hard_byte_limits"]["chunk"],
            mutation::MAX_CHUNK_BYTES
        );
        assert_eq!(
            mutation["hard_byte_limits"]["extended_job"],
            plugin_package::MAX_MUTATION_JOB_BYTES
        );
        assert_eq!(
            mutation["hard_byte_limits"]["extended_total"],
            plugin_package::MAX_MUTATION_BYTES
        );
        assert_eq!(
            mutation["duration_limits_ms"]["mutation_deadline"],
            mutation::MAX_DEADLINE_MS
        );
    }

    #[test]
    fn compiled_native_routes_do_not_imply_protected_owner_or_mutation_support() {
        let discovery = extension_discovery();
        for value in discovery["current_host_requirements"]
            .as_object()
            .unwrap()
            .values()
        {
            assert_eq!(value, &json!(cfg!(windows)));
        }
        for (index, profile) in discovery["profiles"].as_array().unwrap().iter().enumerate() {
            for operation in profile["implemented_operations"].as_array().unwrap() {
                assert_eq!(
                    operation["implementation_compiled"],
                    !cfg!(target_arch = "wasm32") && (index != 3 || cfg!(windows))
                );
                assert_eq!(
                    operation["platform_requirement"],
                    if index == 3 { "windows" } else { "native" }
                );
            }
        }
        assert!(
            discovery["profiles"][3]["unsupported_operations"]
                .as_array()
                .unwrap()
                .contains(&json!("ConditionalReplace"))
        );
    }

    #[test]
    fn discovery_reports_compiled_contracts_and_no_authority() {
        let value = descriptor();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["authority"], "none");
        assert_eq!(value["package_preflight_required"], true);
        let profile = &value["profiles"][0];
        assert_eq!(profile["contracts"]["task"]["version"], task::VERSION);
        assert_eq!(
            profile["contracts"]["task"]["sha256"],
            digest(task::schema_digest())
        );
        assert_eq!(
            profile["contracts"]["runtime"]["sha256"],
            digest(runtime::runtime_digest())
        );
        assert_eq!(
            profile["hard_byte_limits"]["task_value"],
            task::MAX_VALUE_BYTES
        );
        assert!(
            value["unsupported_requirements"]
                .as_array()
                .unwrap()
                .contains(&json!("network_sse_websocket_backend"))
        );
        assert_eq!(profile["status"], "candidate");
        let channel_profile = &value["profiles"][1];
        assert_eq!(
            channel_profile["contracts"]["channel"]["sha256"],
            digest(channel::schema_digest())
        );
        assert_eq!(channel_profile["status"], "experimental");
        assert_eq!(channel_profile["workbench_routes"], json!([]));
        assert_eq!(
            channel_profile["channel_scope"]["managed_binding_required"],
            true
        );
        assert_eq!(channel_profile["channel_scope"]["network_backend"], false);
        assert_eq!(channel_profile["channel_scope"]["automatic_replay"], false);
        assert_eq!(channel_profile["channel_scope"]["workbench_binding"], false);
        assert_eq!(
            channel_profile["workbench_constraints"]["channel_binding"],
            false
        );
        assert_eq!(
            channel_profile["native_binding_implementation"]["production_public_binding_available"],
            false
        );
        assert_eq!(profile["workbench_constraints"]["dependency_calls"], false);
        assert_eq!(
            profile["workbench_constraints"]["required_dependencies"],
            false
        );
    }
}
