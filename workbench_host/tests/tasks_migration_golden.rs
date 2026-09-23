//! Migrator 1 fixture: these expected bytes and digests were generated once, inspected,
//! and then fixed here. Changing request shaping or output rules requires a new version.
use morrow_core::{
    content::CardRecord,
    plugin_package::{Package, proto::TransformHandler},
    task_evidence::{
        self,
        proto::{ExecutionBudget, TaskEvidence},
    },
};
use morrow_workbench_host::tasks_migration::Plan;
use morrow_workbench_plugin::{tasks_v2, tasks_v2_codec};
use sha2::{Digest, Sha256};

// Hand-encoded Card protobuf: V1 idea, revision 2^53+1, category "进行中",
// stage "计划中", todos ["same", "same", "open"], completed ["same"].
// Body field 100 and outer Card field 101 carry opaque source bytes.
const CARD_HEX: &str = "0801120b676f6c64656e2d636172641a0f6f72672e6d6f72726f772e696465612001288180808080808010320d46726f7a656e20736f757263653a3508011a09e8bf9be8a18ce4b8ad2209e8aea1e58892e4b8ad420473616d65420473616d6542046f70656e4a0473616d65a2060201ff52060a046b657074582aaa060202ff";
const SOURCE_SHA256: &str = "267ee78309474b86fd250b7851fddc467129c0052f65701fd5c082dc63602ccb";
const SOURCE_BODY_SHA256: &str = "e0b9c1435a0dbedb9f4a4bbc87b6f687322fe662f635775d22ce2f87c0ae23d2";
const REQUEST_SHA256: &str = "68b27b6dec4090fcdd032e31300226545f327b8a6bf7b5626699918a2e44cb40";
const RESPONSE_SHA256: &str = "070e79d91165c24e9f2f728ff2d9deaa2b88909efc73d792bcf4b05a555a6ed4";
const TARGET_BODY_SHA256: &str = "06e6709890d2e284e737a49d4a3c91f81858decdc08bd1177dd758daccfec47d";
const COMMAND_SHA256: &str = "87f158c7739aea4088e1ad7c0e3bc8392304819c053f8e84d4c97dd68ac1995b";
const OPERATION_ID: &str =
    "migrate-tasks-cfcfab5ba331fe70dbf37859721eb255f64723262423940ed299e6312606242b";
const TASK_IDS: [&str; 3] = [
    "task-21bbaa7899eeadf06523dc2cc50ad7f0fdf719088b55b508022fa992717065b9",
    "task-033a32f4aaf361ecabe54b1b82e21b9fbe7ce516e4f1bf340338ea9446d7b255",
    "task-4f40720731781345bb10aec16ecfeb1984774df70cc6e9289c013282483d497b",
];

fn decode_hex(raw: &str) -> Vec<u8> {
    raw.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash(raw: &[u8]) -> String {
    hex(&Sha256::digest(raw))
}

#[test]
fn migrator_one_historical_bytes_and_semantics_are_frozen() {
    let source_raw = decode_hex(CARD_HEX);
    assert_eq!(hash(&source_raw), SOURCE_SHA256);
    let source = CardRecord::decode(&source_raw).unwrap();
    assert_eq!(source.encode(), source_raw);
    assert_eq!(source.summary().revision, 9_007_199_254_740_993);
    assert_eq!(source.summary().preview_text, "kept");
    assert_eq!(hash(&source.body()), SOURCE_BODY_SHA256);

    // This synthetic observation tests deterministic host projection only.
    // The separate guest integration test checks actual Wasm execution and replay.
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for_transform(
            "test.migration-golden",
            "1.0.0",
            module,
            vec![TransformHandler {
                handler: "workbench.tasks.v2".into(),
                input_type: "morrow.workbench.tasks.request.v2".into(),
                output_type: "morrow.workbench.tasks.response.v2".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        ),
        module,
    )
    .unwrap();
    let plan = Plan::prepare(&source, &package).unwrap();
    assert_eq!(plan.operation_id(), OPERATION_ID);
    let input = &plan.invocation().transform().unwrap().input;
    assert_eq!(hash(input), REQUEST_SHA256);

    let output = tasks_v2_codec::process(input).unwrap();
    assert_eq!(hash(&output), RESPONSE_SHA256);
    let capture = task_evidence::encode(TaskEvidence {
        schema_version: task_evidence::VERSION,
        package_archive: package.archive().to_vec(),
        invocation: plan.invocation().bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 20_000_000,
            memory_bytes: 16 * 1024 * 1024,
            host_calls: 16,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: plan.invocation().output_completion(&output).unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 19_000_000,
        batch: None,
    })
    .unwrap();
    let projected = plan.capture(&capture).unwrap();
    let body = projected.card().body();
    assert_eq!(hash(&body), TARGET_BODY_SHA256);
    assert_eq!(hash(projected.command()), COMMAND_SHA256);
    assert_eq!(projected.card().summary().format_version, 2);
    assert_eq!(projected.card().summary().revision, 9_007_199_254_740_994);
    assert_eq!(projected.card().summary().preview_text, "kept");
    assert!(
        projected
            .card()
            .encode()
            .windows(5)
            .any(|bytes| bytes == [0xaa, 0x06, 0x02, 0x02, 0xff]),
        "outer unknown Card field must survive"
    );

    let migrated = tasks_v2::decode("golden-card", "Frozen source", &body).unwrap();
    assert_eq!(migrated.stage, "计划中");
    assert_eq!(
        migrated
            .tasks
            .iter()
            .map(|task| (
                task.text.as_str(),
                task.completion,
                task.legacy_completed,
                task.legacy_duplicates
            ))
            .collect::<Vec<_>>(),
        [
            (
                "same",
                tasks_v2::Completion::LegacyAmbiguous as i32,
                true,
                2
            ),
            (
                "same",
                tasks_v2::Completion::LegacyAmbiguous as i32,
                true,
                2
            ),
            ("open", tasks_v2::Completion::Incomplete as i32, false, 1),
        ]
    );
    assert_eq!(
        migrated
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        TASK_IDS
    );
    let origin = migrated.origin.unwrap();
    assert_eq!(origin.source_revision, 9_007_199_254_740_993);
    assert_eq!(hex(&origin.source_sha256), SOURCE_BODY_SHA256);
    assert_eq!(origin.original_properties, source.body());
    assert!(
        origin
            .original_properties
            .windows(5)
            .any(|bytes| bytes == [0xa2, 0x06, 0x02, 0x01, 0xff]),
        "unknown V1 property field must remain in Origin"
    );
    assert_eq!(origin.original_title, "Frozen source");
    assert_eq!(origin.historical_project_stage, "计划中");
    assert_eq!(origin.migrator_version, 1);
    assert_eq!(origin.target_version, 2);
}
