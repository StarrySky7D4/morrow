#![cfg(target_os = "windows")]
mod common;
use morrow_core::{plugin_package::Package, task::Invocation};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_host::{Mutation, Workbench};
use morrow_workbench_plugin::{Action, Idea, codec};
fn edit<'a>(operation: &'a str, revision: u64, proposed: Idea) -> Mutation<'a> {
    Mutation {
        operation,
        id: "card",
        revision,
        action: Action::Edit,
        proposed: Some(proposed),
        text: "stable text",
        flag: true,
    }
}
fn action<'a>(operation: &'a str, revision: u64, action: Action, flag: bool) -> Mutation<'a> {
    Mutation {
        operation,
        id: "card",
        revision,
        action,
        proposed: None,
        text: "",
        flag,
    }
}
fn configure(h: &mut Workbench, enable: bool) {
    let status = h.plugin_status();
    h.configure_plugin(status.revision, &status.digest, enable)
        .unwrap();
}
fn original_evidence(h: &Workbench, operation: &str) -> morrow_core::task_evidence::Evidence {
    let mut evidence = h.operation_evidence("card", operation).unwrap();
    assert_eq!(evidence.len(), 1);
    evidence.remove(0)
}
fn assert_same_evidence(
    h: &Workbench,
    operation: &str,
    original: &morrow_core::task_evidence::Evidence,
) {
    let stored = original_evidence(h, operation);
    assert_eq!(stored.digest(), original.digest());
    assert_eq!(stored.container(), original.container());
}
#[test]
fn actual_rust_create_and_edit_store_replayable_original_tasks_with_content() {
    let dir = tempfile::tempdir().unwrap();
    let package = common::package();
    let archive = package.archive().to_vec();
    let mut h = Workbench::open_managed(dir.path(), Some(package)).unwrap();
    let draft = common::idea("card");
    h.create("create", draft.clone()).unwrap();
    let create = original_evidence(&h, "create");
    assert_eq!(create.data().package_archive, archive);
    let invocation = Invocation::decode(&create.data().invocation).unwrap();
    let request = codec::decode_request(&invocation.transform().unwrap().input).unwrap();
    assert_eq!(request.action, Action::Create);
    assert_eq!(request.proposed, draft);
    assert!(replay::replay(&create, Limits::default()).unwrap().matches);
    let mut proposed = draft;
    proposed.title = "edited title".into();
    proposed.description = "actual **Markdown**".into();
    let saved = h.apply(edit("edit", 1, proposed.clone())).unwrap();
    assert_eq!(saved.revision, 2);
    let evidence = original_evidence(&h, "edit");
    let invocation = Invocation::decode(&evidence.data().invocation).unwrap();
    let request = codec::decode_request(&invocation.transform().unwrap().input).unwrap();
    assert_eq!(request.action, Action::Edit);
    assert_eq!(request.proposed, proposed);
    let output = invocation
        .verify_output(&evidence.data().completion)
        .unwrap();
    assert_eq!(
        codec::decode_response(&output.bytes).unwrap().idea,
        saved.idea
    );
    assert!(
        replay::replay(&evidence, Limits::default())
            .unwrap()
            .matches
    );
    h.finish().unwrap();
    drop(h);
    let h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    assert_eq!(h.read("card").unwrap().idea, saved.idea);
    assert_same_evidence(&h, "create", &create);
    assert_same_evidence(&h, "edit", &evidence);
}
#[test]
fn create_retry_compares_original_draft_not_normalized_output_and_returns_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let mut draft = common::idea("card");
    draft.stage.clear();
    draft.deleted = true;
    draft.deleted_at = 123;
    let created = h.create("create", draft.clone()).unwrap();
    assert!(!created.idea.deleted);
    assert_eq!(created.idea.deleted_at, 0);
    assert_eq!(created.idea.stage, "待整理");
    let evidence = original_evidence(&h, "create");
    let retry = h.create("create", draft.clone()).unwrap();
    assert_eq!(retry.idea, created.idea);
    assert_eq!(retry.revision, 1);
    assert!(
        h.create("create", created.idea.clone()).is_err(),
        "normalized output is a different user intent"
    );
    h.apply(action("favorite", 1, Action::Favorite, true))
        .unwrap();
    let retry = h.create("create", draft.clone()).unwrap();
    assert_eq!(retry.revision, 1);
    assert!(!retry.idea.favorite);
    assert!(h.read("card").unwrap().idea.favorite);
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    assert_eq!(h.create("create", draft).unwrap().revision, 1);
    assert_eq!(h.read("card").unwrap().revision, 2);
    assert_same_evidence(&h, "create", &evidence);
}
#[test]
fn edit_retries_ignore_new_current_state_but_cannot_overwrite_later_revision() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let mut proposed = common::idea("card");
    proposed.description = "saved edit".into();
    let saved = h.apply(edit("edit", 1, proposed.clone())).unwrap();
    let evidence = original_evidence(&h, "edit");
    assert_eq!(
        h.apply(edit("edit", 1, proposed.clone())).unwrap().idea,
        saved.idea
    );
    h.apply(action("favorite", 2, Action::Favorite, true))
        .unwrap();
    let retry = h.apply(edit("edit", 1, proposed.clone())).unwrap();
    assert_eq!(retry.revision, 2);
    assert_eq!(retry.idea, saved.idea);
    assert_eq!(h.read("card").unwrap().revision, 3);
    assert!(h.read("card").unwrap().idea.favorite);
    assert!(h.apply(edit("new-stale", 1, proposed.clone())).is_err());
    assert!(h.operation_evidence("card", "new-stale").is_err());
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let retry = h.apply(edit("edit", 1, proposed)).unwrap();
    assert_eq!(retry.revision, 2);
    assert_eq!(h.read("card").unwrap().revision, 3);
    assert_same_evidence(&h, "edit", &evidence);
}
#[test]
fn reused_operation_rejects_each_changed_user_intent_field_and_cross_card() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let proposed = common::idea("card");
    h.apply(edit("edit", 1, proposed.clone())).unwrap();
    let evidence = original_evidence(&h, "edit");
    for field in 0..6 {
        let mut changed = edit("edit", 1, proposed.clone());
        match field {
            0 => changed.action = Action::Favorite,
            1 => changed.proposed.as_mut().unwrap().title.push('!'),
            2 => changed.text = "different",
            3 => changed.flag = false,
            4 => changed.revision = 2,
            _ => changed.id = "other",
        }
        assert!(
            h.apply(changed).is_err(),
            "accepted different field {field}"
        );
    }
    assert!(h.create("create", common::idea("other")).is_err());
    assert!(h.create("edit", common::idea("other")).is_err());
    assert!(h.operation_evidence("other", "edit").is_err());
    assert_eq!(h.page("", 100).unwrap().0.len(), 1);
    assert_eq!(h.read("card").unwrap().revision, 2);
    assert!(h.writable());
    assert_same_evidence(&h, "edit", &evidence);
}
#[test]
fn committed_delete_restore_retry_survives_restart_without_reopening_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let deleted = h.apply(action("delete", 1, Action::Delete, false)).unwrap();
    assert!(deleted.idea.deleted);
    let restored = h
        .apply(action("restore", 2, Action::Restore, false))
        .unwrap();
    assert!(!restored.idea.deleted);
    h.apply(action("favorite", 3, Action::Favorite, true))
        .unwrap();
    let delete_evidence = original_evidence(&h, "delete");
    let restore_evidence = original_evidence(&h, "restore");
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let retry = h.apply(action("delete", 1, Action::Delete, false)).unwrap();
    assert_eq!(retry.revision, 2);
    assert_eq!(retry.idea, deleted.idea);
    let retry = h
        .apply(action("restore", 2, Action::Restore, false))
        .unwrap();
    assert_eq!(retry.revision, 3);
    assert_eq!(retry.idea, restored.idea);
    assert!(
        h.apply(action("new-restore", 4, Action::Restore, false))
            .is_err(),
        "historical retries must not recreate an undo window"
    );
    assert_eq!(h.read("card").unwrap().revision, 4);
    assert!(h.read("card").unwrap().idea.favorite);
    assert!(!h.read("card").unwrap().idea.deleted);
    assert_same_evidence(&h, "delete", &delete_evidence);
    assert_same_evidence(&h, "restore", &restore_evidence);
}
#[test]
fn disabled_plugin_cannot_reuse_evidence_as_write_permission() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let draft = common::idea("card");
    h.create("create", draft.clone()).unwrap();
    h.apply(action("favorite", 1, Action::Favorite, true))
        .unwrap();
    let evidence = original_evidence(&h, "create");
    configure(&mut h, false);
    assert!(h.create("create", draft.clone()).is_err());
    assert!(
        h.apply(action("favorite", 1, Action::Favorite, true))
            .is_err()
    );
    assert_same_evidence(&h, "create", &evidence);
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    assert!(h.create("create", draft.clone()).is_err());
    configure(&mut h, true);
    assert_eq!(h.create("create", draft).unwrap().revision, 1);
    assert_eq!(
        h.apply(action("favorite", 1, Action::Favorite, true))
            .unwrap()
            .revision,
        2
    );
    assert_eq!(h.read("card").unwrap().revision, 2);
}
fn trap_package() -> Package {
    let module = vec![
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 23, 2,
        10, b'm', b'o', b'r', b'r', b'o', b'w', b'_', b'r', b'u', b'n', 0, 0, 6, b'm', b'e', b'm',
        b'o', b'r', b'y', 2, 0, 10, 5, 1, 3, 0, 0, 11,
    ];
    let original = common::package();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.36",
        &module,
        original.manifest().transform_handlers.clone(),
    );
    manifest.requested_capabilities = original.manifest().requested_capabilities.clone();
    Package::build(manifest, &module).unwrap()
}
#[test]
fn retries_after_explicit_trap_package_upgrade_do_not_execute_guest() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let draft = common::idea("card");
    h.create("create", draft.clone()).unwrap();
    h.apply(action("favorite", 1, Action::Favorite, true))
        .unwrap();
    let original = original_evidence(&h, "create");
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(trap_package())).unwrap();
    assert!(!h.writable());
    configure(&mut h, true);
    assert!(h.writable());
    assert_eq!(h.create("create", draft).unwrap().revision, 1);
    assert_eq!(
        h.apply(action("favorite", 1, Action::Favorite, true))
            .unwrap()
            .revision,
        2
    );
    assert!(h.create("create", common::idea("other")).is_err());
    assert!(
        h.writable(),
        "cross-card collision must not run the faulting guest"
    );
    assert_same_evidence(&h, "create", &original);
    let error = h
        .create("new-operation", common::idea("new-card"))
        .err()
        .unwrap();
    assert!(error.to_string().contains("Trap"), "{error}");
    assert!(!h.writable());
    assert_eq!(h.read("card").unwrap().revision, 2);
    assert!(h.operation_evidence("new-card", "new-operation").is_err());
}
#[test]
fn attachment_create_retry_uses_original_commit_after_staging_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let bytes = b"synthetic office file\0\xff";
    let asset = h
        .import(
            "card",
            "source.docx",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut draft = common::idea("card");
    draft.assets.push(asset.clone());
    h.create("create", draft.clone()).unwrap();
    h.apply(edit("remove-asset", 1, common::idea("card")))
        .unwrap();
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let retry = h.create("create", draft).unwrap();
    assert_eq!(retry.revision, 1);
    assert_eq!(retry.idea.assets, [asset]);
    assert!(h.read("card").unwrap().idea.assets.is_empty());
    assert_eq!(h.read("card").unwrap().revision, 2);
}
#[test]
fn rejected_guest_result_and_stale_revision_never_attach_committed_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
    let mut invalid = common::idea("card");
    invalid.title.clear();
    assert!(h.create("retryable", invalid).is_err());
    assert!(h.operation_evidence("card", "retryable").is_err());
    assert!(h.writable());
    h.create("retryable", common::idea("card")).unwrap();
    let original = original_evidence(&h, "retryable");
    assert!(h.apply(edit("stale", 9, common::idea("card"))).is_err());
    assert!(h.operation_evidence("card", "stale").is_err());
    assert_eq!(h.read("card").unwrap().revision, 1);
    assert_same_evidence(&h, "retryable", &original);
}

#[cfg(feature = "fault-injection")]
#[test]
fn committed_evidence_crash_child() {
    let Ok(root) = std::env::var("MORROW_COMMITTED_TEST_ROOT") else {
        return;
    };
    let mode = std::env::var("MORROW_COMMITTED_TEST_MODE").unwrap();
    let mut h =
        Workbench::open_managed(std::path::Path::new(&root), Some(common::package())).unwrap();
    let mut draft = common::idea("card");
    draft.title = "after crash".into();
    if mode == "edit" {
        h.apply(edit("crash-edit", 1, draft)).unwrap();
    } else {
        h.create("crash-create", draft).unwrap();
    }
    panic!("the selected commit boundary did not terminate the child");
}
#[cfg(feature = "fault-injection")]
fn crash_process(root: &std::path::Path, mode: &str, point: &str) -> std::process::ExitStatus {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "committed_evidence_crash_child", "--nocapture"])
        .env("MORROW_COMMITTED_TEST_ROOT", root)
        .env("MORROW_COMMITTED_TEST_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .creation_flags(0x08000000)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned crash child exceeded 30 seconds: {mode}/{point}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn process_crash_and_lost_receipt_retry_preserve_original_committed_evidence() {
    for is_edit in [false, true] {
        for point in ["after-task-evidence", "before-commit", "after-commit"] {
            let dir = tempfile::tempdir().unwrap();
            let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
            if is_edit {
                h.create("seed", common::idea("card")).unwrap();
            }
            h.finish().unwrap();
            drop(h);
            let mode = if is_edit { "edit" } else { "create" };
            let operation = if is_edit {
                "crash-edit"
            } else {
                "crash-create"
            };
            assert_eq!(
                crash_process(dir.path(), mode, point).code(),
                Some(86),
                "{mode}/{point}"
            );
            let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
            let committed = point == "after-commit";
            let original = if committed {
                let e = original_evidence(&h, operation);
                let task = Invocation::decode(&e.data().invocation).unwrap();
                let request = codec::decode_request(&task.transform().unwrap().input).unwrap();
                assert_eq!(
                    request.action,
                    if is_edit {
                        Action::Edit
                    } else {
                        Action::Create
                    }
                );
                assert_eq!(request.proposed.title, "after crash");
                let output = task.verify_output(&e.data().completion).unwrap();
                assert_eq!(
                    codec::decode_response(&output.bytes).unwrap().idea,
                    h.read("card").unwrap().idea
                );
                assert_eq!(
                    h.read("card").unwrap().revision,
                    if is_edit { 2 } else { 1 }
                );
                Some(e)
            } else {
                assert!(h.operation_evidence("card", operation).is_err());
                if is_edit {
                    assert_eq!(h.read("card").unwrap().revision, 1);
                    assert_eq!(h.read("card").unwrap().idea, common::idea("card"));
                } else {
                    assert!(h.read("card").is_err());
                }
                None
            };
            let mut draft = common::idea("card");
            draft.title = "after crash".into();
            let first = if is_edit {
                h.apply(edit(operation, 1, draft.clone())).unwrap()
            } else {
                h.create(operation, draft.clone()).unwrap()
            };
            let persisted = original_evidence(&h, operation);
            if let Some(original) = original {
                assert_same_evidence(&h, operation, &original);
            }
            let second = if is_edit {
                h.apply(edit(operation, 1, draft.clone())).unwrap()
            } else {
                h.create(operation, draft.clone()).unwrap()
            };
            assert_eq!(second.revision, first.revision);
            assert_eq!(second.idea, first.idea);
            assert_same_evidence(&h, operation, &persisted);
            h.finish().unwrap();
            drop(h);
            let mut h = Workbench::open_managed(dir.path(), Some(common::package())).unwrap();
            let reopened = if is_edit {
                h.apply(edit(operation, 1, draft)).unwrap()
            } else {
                h.create(operation, draft).unwrap()
            };
            assert_eq!(reopened.revision, if is_edit { 2 } else { 1 });
            assert_eq!(h.read("card").unwrap().revision, reopened.revision);
            assert_same_evidence(&h, operation, &persisted);
        }
    }
}
