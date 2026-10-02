//! Read-only compiled host profile discovery. This never opens an owner,
//! database or guest, and is neither an authorization nor a package preflight.
use morrow_core::{channel, dependency_call, plugin_package, runtime, task, ui};
use serde_json::{Value, json};

fn digest(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
        "experimental_extensions": {"status": "recognized_experimental_not_fully_discovered", "feature_names": [plugin_package::io::FEATURE, plugin_package::io::SERVICE_RUN_FEATURE, plugin_package::io::SERVICE_RUN_BUDGET_FEATURE, morrow_core::service_resources::FEATURE, plugin_package::MUTATION_FEATURE, plugin_package::MUTATION_BUDGET_FEATURE]},
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
        assert_eq!(channel_profile["workbench_constraints"]["channel_binding"], false);
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
