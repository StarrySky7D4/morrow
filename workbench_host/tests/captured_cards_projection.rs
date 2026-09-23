use capnp::{message::Builder, serialize};
use morrow_core::{
    content::CardRecord,
    content_migration::ContentMigration,
    plugin_package::{Package, proto::TransformHandler},
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, Observation, TaskEvidence},
    },
    transaction,
};
use morrow_workbench_host::{
    capture_provenance::EditorSnapshot,
    captured_cards, cards_edit,
    projection::proto::{CaptureParent, ContentProjectionV2, PasteApplication, PastePart},
};
use morrow_workbench_plugin::{
    Idea, capture, capture_capnp,
    cards_v2::{Command, Fields},
    cards_v2_codec, persistence,
    tasks_v2::Baseline,
};
use prost::Message;

fn fixture() -> (CardRecord, Package) {
    let idea = Idea {
        id: "card".into(),
        title: "Original".into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["task".into()],
        ..Default::default()
    };
    let mut old = persistence::encode(&idea, None).unwrap();
    // An unknown source field must survive migration and captured common-card edits.
    old.extend_from_slice(&[0xa2, 0x06, 0x01, 0xff]);
    let legacy = CardRecord::new("card", "org.morrow.idea", 1, "Original", old.clone()).unwrap();
    let baseline = Baseline::capture("card", 1, &old).unwrap();
    let body =
        morrow_workbench_plugin::tasks_v2::migrate(&baseline, "card", 1, "Original", &old).unwrap();
    let source = ContentMigration {
        operation_id: "migration".into(),
        source_card: legacy.encode(),
        target_format_version: 2,
        body,
        preview_text: String::new(),
    }
    .propose(&legacy)
    .unwrap();
    let wasm = b"\0asm\x01\0\0\0";
    let handlers = vec![
        TransformHandler {
            handler: "workbench.cards.v2".into(),
            input_type: "morrow.workbench.cards.request.v2".into(),
            output_type: "morrow.workbench.cards.response.v2".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        },
        TransformHandler {
            handler: "capture.convert".into(),
            input_type: "morrow.capture.request.v1".into(),
            output_type: "morrow.capture.response.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        },
    ];
    let package = Package::build(
        Package::manifest_for_transform("test.captured-card", "1.0.0", wasm, handlers),
        wasm,
    )
    .unwrap();
    (source, package)
}
fn single(package: &Package, invocation: &Invocation, output: &[u8]) -> Evidence {
    // Synthetic completion exercises pure correspondence only, not guest execution.
    task_evidence::encode(TaskEvidence {
        schema_version: task_evidence::VERSION,
        package_archive: package.archive().to_vec(),
        invocation: invocation.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 20_000_000,
            memory_bytes: 16 * 1024 * 1024,
            host_calls: 16,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: invocation.output_completion(output).unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 19_999_999,
        batch: None,
    })
    .unwrap()
}
fn captured(package: &Package, task_id: &str, source: &str) -> Observation {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<capture_capnp::request::Builder>();
        r.set_version(1);
        r.set_digest(&capture::digest());
        r.set_format("plain");
        r.set_source(source);
        r.init_nodes(0);
    }
    let input = serialize::write_message_to_words(&message);
    let invocation = Invocation::new_transform(
        task_id,
        Transform {
            handler: "capture.convert".into(),
            input_type: "morrow.capture.request.v1".into(),
            output_type: "morrow.capture.response.v1".into(),
            input: input.clone(),
        },
    )
    .unwrap();
    let output = capture::process(&input).unwrap();
    let evidence = single(package, &invocation, &output);
    let actual = evidence.data();
    Observation {
        invocation: actual.invocation.clone(),
        budget: actual.budget,
        backend: actual.backend.clone(),
        completion: actual.completion.clone(),
        fault: actual.fault,
        exit_code: actual.exit_code,
        observed_host_calls: actual.observed_host_calls,
        fuel_remaining: actual.fuel_remaining,
    }
}
fn projected() -> (
    captured_cards::ProjectedEdit,
    cards_edit::ProjectedEdit,
    Command,
    EditorSnapshot,
) {
    let (source, package) = fixture();
    let fields = Fields {
        title: "Edited".into(),
        description: "prefix converted".into(),
        hypothesis: String::new(),
        conclusion: String::new(),
        icon: 0,
        color: 0,
        assets: vec![],
    };
    let command = Command::Edit(fields);
    let plan =
        cards_edit::Plan::prepare(&source, &package, "edit-captured", &command, &[], None).unwrap();
    let input = plan.invocation().transform().unwrap().input.clone();
    let output = cards_v2_codec::process(&input).unwrap();
    let original = plan
        .capture(&single(&package, plan.invocation(), &output))
        .unwrap();
    let snapshot = EditorSnapshot {
        title: " Edited ".into(),
        description: "prefix converted".into(),
        hypothesis: String::new(),
        conclusion: String::new(),
        todos: String::new(),
        aliases: vec![],
    };
    let applications = vec![PasteApplication {
        id: "paste-1".into(),
        field: "description".into(),
        before: "prefix ".into(),
        start_utf16: 7,
        end_utf16: 7,
        parts: vec![PastePart {
            observation: Some(0),
            literal: String::new(),
            selection: "outputMarkdown".into(),
        }],
        after: "prefix converted".into(),
    }];
    let projected = captured_cards::compose(
        &original,
        "scope-1",
        &snapshot,
        vec![CaptureParent { parent: None }],
        applications,
        vec![captured(&package, "capture-1", "converted")],
    )
    .unwrap();
    (projected, original, command, snapshot)
}
fn tamper(base: &Evidence, edit: impl FnOnce(&mut ContentProjectionV2)) -> Evidence {
    let mut raw = base.data().clone();
    let batch = raw.batch.as_mut().unwrap();
    let mut facts = ContentProjectionV2::decode(batch.intent.as_slice()).unwrap();
    edit(&mut facts);
    batch.intent = facts.encode_to_vec();
    task_evidence::encode(raw).unwrap()
}

#[test]
fn exact_capture_and_final_edit_form_one_distinct_commit_evidence() {
    let (value, original, command, snapshot) = projected();
    assert_eq!(value.operation_id(), "edit-captured");
    assert_eq!(value.card().summary().revision, 3);
    assert_eq!(value.card().summary().title, "Edited");
    assert_eq!(value.command(), original.command());
    let source = CardRecord::decode(value.source_card()).unwrap();
    let summary = source.summary();
    let properties =
        morrow_workbench_plugin::tasks_v2::decode(&summary.id, &summary.title, &source.body())
            .unwrap();
    assert!(
        properties
            .origin
            .unwrap()
            .original_properties
            .ends_with(&[0xa2, 0x06, 0x01, 0xff])
    );
    assert_ne!(value.evidence().digest(), original.evidence().digest());
    let batch = value.evidence().data().batch.as_ref().unwrap();
    assert_eq!(batch.intent_type, captured_cards::INTENT_TYPE);
    assert_eq!(batch.observations.len(), 2);
    assert_eq!(batch.total_fuel, 1_000_000_000);
    assert!(
        value
            .matches_intent("edit-captured", 2, &command, "scope-1", &snapshot)
            .unwrap()
    );
    assert!(
        !value
            .matches_intent("edit-captured", 2, &command, "other", &snapshot)
            .unwrap()
    );
    let replay = captured_cards::derive(value.evidence()).unwrap();
    assert_eq!(replay.command(), value.command());
    assert_eq!(replay.card().encode(), value.card().encode());
    let event = transaction::encode_commit_with_evidence(
        value.command().to_vec(),
        value.card(),
        &[value.evidence().digest()],
    )
    .unwrap();
    let (commit, _) = transaction::decode_commit(&event).unwrap();
    captured_cards::verify_commit(&commit, value.evidence()).unwrap();
    assert!(cards_edit::verify_commit(&commit, original.evidence()).is_err());
}
#[test]
fn reject_unadopted_wrong_parent_todos_wrong_field_and_bad_replacement() {
    let (value, _, _, _) = projected();
    for change in 0..5 {
        let changed = tamper(value.evidence(), |facts| match change {
            0 => facts.applications.clear(),
            1 => facts.captures[0].parent = Some(0),
            2 => facts.snapshot.as_mut().unwrap().todos = "invented".into(),
            3 => facts.applications[0].field = "todos".into(),
            _ => facts.applications[0].after = "wrong".into(),
        });
        assert!(captured_cards::derive(&changed).is_err(), "damage {change}");
    }
}
#[test]
fn reject_changed_final_observation_and_scope_snapshot_retry() {
    let (value, _, command, snapshot) = projected();
    let mut raw = value.evidence().data().clone();
    let batch = raw.batch.as_mut().unwrap();
    batch.observations.pop();
    assert!(task_evidence::encode(raw).is_ok_and(|e| captured_cards::derive(&e).is_err()));
    let mut other = snapshot.clone();
    other.description = "other".into();
    assert!(
        !value
            .matches_intent("edit-captured", 2, &command, "scope-1", &other)
            .unwrap()
    );
    assert!(
        !value
            .matches_intent("edit-captured", 3, &command, "scope-1", &snapshot)
            .unwrap()
    );
}

#[test]
fn valid_parent_chain_is_adopted_and_utf16_surrogates_cannot_be_split() {
    let (_, final_edit, _, snapshot) = projected();
    let (_, package) = fixture();
    let applications = vec![PasteApplication {
        id: "paste-chain".into(),
        field: "description".into(),
        before: "prefix ".into(),
        start_utf16: 7,
        end_utf16: 7,
        parts: vec![PastePart {
            observation: Some(1),
            literal: String::new(),
            selection: "outputMarkdown".into(),
        }],
        after: "prefix converted".into(),
    }];
    let value = captured_cards::compose(
        &final_edit,
        "scope-chain",
        &snapshot,
        vec![
            CaptureParent { parent: None },
            CaptureParent { parent: Some(0) },
        ],
        applications,
        vec![
            captured(&package, "capture-1", "converted"),
            captured(&package, "capture-2", "converted"),
        ],
    )
    .unwrap();
    assert_eq!(
        value
            .evidence()
            .data()
            .batch
            .as_ref()
            .unwrap()
            .observations
            .len(),
        3
    );
    let split = tamper(value.evidence(), |facts| {
        let app = &mut facts.applications[0];
        app.before = "😀".into();
        app.start_utf16 = 1;
        app.end_utf16 = 1;
        app.after = "😀converted".into();
    });
    assert!(captured_cards::derive(&split).is_err());
    let broken_parent = tamper(value.evidence(), |facts| {
        facts.captures[1].parent = Some(1);
    });
    assert!(captured_cards::derive(&broken_parent).is_err());
}
