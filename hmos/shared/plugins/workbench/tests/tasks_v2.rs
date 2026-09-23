use morrow_workbench_plugin::{
    Idea, persistence,
    tasks_v2::{self as v2, Baseline, Command, Completion},
};
use prost::Message;
fn legacy(tasks: &[&str], complete: &[&str]) -> Vec<u8> {
    persistence::encode(
        &Idea {
            id: "card".into(),
            title: "Source title".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: tasks.iter().map(|s| (*s).into()).collect(),
            completed: complete.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        },
        None,
    )
    .unwrap()
}
fn migrate(bytes: &[u8]) -> Vec<u8> {
    v2::migrate(
        &Baseline::capture("card", 7, bytes).unwrap(),
        "card",
        7,
        "Source title",
        bytes,
    )
    .unwrap()
}
#[test]
fn migration_is_deterministic_baseline_bound_and_preserves_exact_legacy_evidence() {
    let mut old = legacy(
        &["same", "same", "unmarked", "unmarked", "unique"],
        &["same", "unique"],
    );
    old.extend([0xb8, 0x0c, 7]); // Unknown V1 extension field, preserved in its original namespace.
    let base = Baseline::capture("card", 7, &old).unwrap();
    let bytes = migrate(&old);
    assert_eq!(bytes, migrate(&old));
    assert_eq!(base.operation_id(), base.clone().operation_id());
    let p = v2::decode("card", "Source title", &bytes).unwrap();
    let origin = p.origin.unwrap();
    assert_eq!(origin.original_properties, old);
    assert_eq!(
        p.tasks.iter().map(|t| t.completion).collect::<Vec<_>>(),
        vec![2, 2, 0, 0, 1]
    );
    assert_ne!(p.tasks[0].id, p.tasks[1].id);
    assert!(v2::migrate(&base, "card", 8, "Source title", &old).is_err());
    assert!(v2::migrate(&base, "another-card", 7, "Source title", &old).is_err());
    let changed = legacy(&["unique", "same", "same"], &["same"]);
    assert!(v2::migrate(&base, "card", 7, "Source title", &changed).is_err());
    assert!(persistence::decode("card", "Source title", &bytes).is_err());
    let original = persistence::decode("card", "Source title", &old).unwrap();
    assert!(persistence::encode(&original, Some(&bytes)).is_err());
}
#[test]
fn stage_reorder_and_rename_never_change_task_completion_or_identity() {
    let bytes = migrate(&legacy(&["A", "B", "C"], &["A"]));
    let before = v2::decode("card", "Source title", &bytes).unwrap();
    let ids: Vec<_> = before.tasks.iter().rev().map(|t| t.id.clone()).collect();
    let reordered = v2::apply("card", "Source title", &bytes, Command::Reorder(ids)).unwrap();
    let staged = v2::apply(
        "card",
        "Source title",
        &reordered,
        Command::SetStage("已完成".into()),
    )
    .unwrap();
    let renamed = v2::apply(
        "card",
        "Source title",
        &staged,
        Command::Rename {
            id: before.tasks[0].id.clone(),
            text: "B".into(),
        },
    )
    .unwrap();
    let after = v2::decode("card", "New title", &renamed).unwrap();
    assert_eq!(after.stage, "已完成");
    for t in &after.tasks {
        assert_eq!(
            t.completion,
            before
                .tasks
                .iter()
                .find(|a| a.id == t.id)
                .unwrap()
                .completion
        );
    }
    let projection = v2::project("card", "New title", &renamed).unwrap();
    assert_eq!(
        (
            projection.complete,
            projection.incomplete,
            projection.ambiguous
        ),
        (1, 2, 0)
    );
    let all = v2::apply(
        "card",
        "New title",
        &renamed,
        Command::CompleteAllAndSetStage("推进中".into()),
    )
    .unwrap();
    assert_eq!(v2::project("card", "New title", &all).unwrap().complete, 3);
    assert_eq!(
        v2::project("card", "New title", &all).unwrap().stage,
        "推进中"
    );
}
#[test]
fn ambiguous_tasks_require_individual_decisions_and_task_ids_cannot_be_reused() {
    let bytes = migrate(&legacy(&["same", "same"], &["same"]));
    let p = v2::decode("card", "Source title", &bytes).unwrap();
    assert_eq!(
        v2::project("card", "Source title", &bytes)
            .unwrap()
            .ambiguous,
        2
    );
    let next = v2::apply(
        "card",
        "Source title",
        &bytes,
        Command::SetCompletion {
            id: p.tasks[0].id.clone(),
            complete: true,
        },
    )
    .unwrap();
    let view = v2::project("card", "Source title", &next).unwrap();
    assert_eq!((view.complete, view.ambiguous), (1, 1));
    let next = v2::apply(
        "card",
        "Source title",
        &next,
        Command::Add {
            id: "new-fixed-id".into(),
            text: "same".into(),
        },
    )
    .unwrap();
    let next = v2::apply(
        "card",
        "Source title",
        &next,
        Command::Remove("new-fixed-id".into()),
    )
    .unwrap();
    assert!(
        v2::apply(
            "card",
            "Source title",
            &next,
            Command::Add {
                id: "new-fixed-id".into(),
                text: "replacement".into()
            }
        )
        .is_err()
    );
    let next = v2::apply(
        "card",
        "Source title",
        &next,
        Command::Remove(p.tasks[0].id.clone()),
    )
    .unwrap();
    assert!(
        v2::apply(
            "card",
            "Source title",
            &next,
            Command::Add {
                id: p.tasks[0].id.clone(),
                text: "replacement".into()
            }
        )
        .is_err()
    );
}
#[test]
fn capacity_and_bad_legacy_state_fail_without_changing_the_original() {
    let mut value = persistence::decode("card", "Source title", &legacy(&["x"], &[])).unwrap();
    value.description = "x".repeat(40000);
    let old = persistence::encode(&value, None).unwrap();
    let keep = old.clone();
    assert!(
        v2::migrate(
            &Baseline::capture("card", 1, &old).unwrap(),
            "card",
            1,
            "Source title",
            &old
        )
        .is_err()
    );
    assert_eq!(old, keep);
    let mut invalid = legacy(&["x"], &[]);
    invalid.extend([0x4a, 1, b'y']); // completed contains a task absent from todos.
    assert!(
        v2::migrate(
            &Baseline::capture("card", 1, &invalid).unwrap(),
            "card",
            1,
            "Source title",
            &invalid
        )
        .is_err()
    );
    let small = legacy(&["x"], &[]);
    assert!(
        v2::migrate(
            &Baseline::capture("card", u64::MAX, &small).unwrap(),
            "card",
            u64::MAX,
            "Source title",
            &small
        )
        .is_err()
    );
}

#[test]
fn invalid_actions_and_adversarial_permutations_leave_the_complete_document_valid() {
    let bytes = migrate(&legacy(&["x", "y"], &[]));
    let p = v2::decode("card", "Source title", &bytes).unwrap();
    for command in [
        Command::SetStage("invalid".into()),
        Command::CompleteAllAndSetStage("invalid".into()),
        Command::Rename {
            id: p.tasks[0].id.clone(),
            text: String::new(),
        },
        Command::Add {
            id: "new/id".into(),
            text: "x".into(),
        },
        Command::Remove("absent".into()),
        Command::Reorder(vec![p.tasks[0].id.clone(), p.tasks[0].id.clone()]),
        Command::Reorder(vec![p.tasks[0].id.clone(), "absent".into()]),
    ] {
        assert!(v2::apply("card", "Source title", &bytes, command).is_err());
        assert_eq!(v2::decode("card", "Source title", &bytes).unwrap(), p);
    }
    let mut current = bytes;
    for i in 0..32 {
        let p = v2::decode("card", "Source title", &current).unwrap();
        let commands = [
            Command::SetCompletion {
                id: p.tasks[0].id.clone(),
                complete: i % 2 == 0,
            },
            Command::Rename {
                id: p.tasks[0].id.clone(),
                text: format!("text-{i}"),
            },
            Command::Reorder(p.tasks.iter().rev().map(|t| t.id.clone()).collect()),
            Command::SetStage(if i % 2 == 0 { "计划中" } else { "已完成" }.into()),
        ];
        for command in commands {
            current = v2::apply("card", "Source title", &current, command).unwrap();
            v2::decode("card", "Source title", &current).unwrap(); // Full independent output validation.
        }
    }
}
#[test]
fn unknown_v2_fields_survive_mutation_and_provenance_cannot_be_forged() {
    let mut bytes = migrate(&legacy(&["x"], &[]));
    bytes.extend([0xb8, 0x0c, 7]);
    let modified = v2::apply(
        "card",
        "Source title",
        &bytes,
        Command::SetStage("推进中".into()),
    )
    .unwrap();
    assert!(modified.windows(3).any(|v| v == [0xb8, 0x0c, 7]));
    let mut p = v2::decode("card", "Source title", &modified).unwrap();
    p.tasks[0].completion = Completion::LegacyAmbiguous as i32;
    p.tasks[0].legacy_completed = true;
    p.tasks[0].legacy_duplicates = 2;
    assert!(v2::decode("card", "Source title", &p.encode_to_vec()).is_err());
}

#[test]
fn nested_unknown_task_fields_survive_rename_and_binary_contract_checks_adjacent_revisions() {
    use morrow_workbench_plugin::{tasks_capnp as wire, tasks_v2_codec};
    use prost_reflect::{DescriptorPool, DynamicMessage, Value};
    let pool = DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/properties.descriptor.bin")).as_slice(),
    )
    .unwrap();
    let raw = migrate(&legacy(&["x"], &[]));
    let value = v2::decode("card", "Source title", &raw).unwrap();
    let id = value.tasks[0].id.clone();
    let mut task = value.tasks[0].encode_to_vec();
    task.extend([0xb8, 0x0c, 7]);
    let task = DynamicMessage::decode(
        pool.get_message_by_name("morrow.workbench.tasks.v2.Task")
            .unwrap(),
        task.as_slice(),
    )
    .unwrap();
    let mut root = DynamicMessage::decode(
        pool.get_message_by_name("morrow.workbench.tasks.v2.Properties")
            .unwrap(),
        raw.as_slice(),
    )
    .unwrap();
    root.set_field_by_name("tasks", Value::List(vec![Value::Message(task)]));
    let modified = v2::apply(
        "card",
        "Source title",
        &root.encode_to_vec(),
        Command::Rename {
            id,
            text: "renamed".into(),
        },
    )
    .unwrap();
    let root = DynamicMessage::decode(
        pool.get_message_by_name("morrow.workbench.tasks.v2.Properties")
            .unwrap(),
        modified.as_slice(),
    )
    .unwrap();
    assert!(
        root.get_field_by_name("tasks").unwrap().as_list().unwrap()[0]
            .as_message()
            .unwrap()
            .encode_to_vec()
            .windows(3)
            .any(|v| v == [0xb8, 0x0c, 7])
    );
    let old = legacy(&["x"], &[]);
    for revision in [(1u64 << 53) + 1, (1u64 << 63) + 1] {
        let mut message = capnp::message::Builder::new_default();
        {
            let mut r = message.init_root::<wire::request::Builder>();
            r.set_version(2);
            r.set_digest(&tasks_v2_codec::digest());
            r.set_action(wire::Action::Migrate);
            r.set_card_id("card");
            r.set_title("Source title");
            r.set_revision(revision);
            r.set_base_revision(revision);
            r.set_source_digest(&Baseline::capture("card", revision, &old).unwrap().sha256);
            r.set_properties(&old);
        }
        assert!(
            tasks_v2_codec::process(&capnp::serialize::write_message_to_words(&message)).is_ok()
        );
        message
            .get_root::<wire::request::Builder>()
            .unwrap()
            .set_revision(revision + 1);
        assert!(
            tasks_v2_codec::process(&capnp::serialize::write_message_to_words(&message)).is_err()
        );
    }
}
