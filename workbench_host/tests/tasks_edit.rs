#![cfg(target_os = "windows")]
use morrow_audit::session::{OpenMode, Session as AuditSession};
use morrow_core::{
    content::CardRecord,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{Capability, TransformHandler},
        registry::Registry,
    },
    task_evidence::{self, Evidence},
    transaction::{self, Lookup},
};
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
};
use morrow_workbench_host::tasks_edit::{self, Plan, ProjectedEdit};
use morrow_workbench_plugin::{
    Idea, persistence,
    tasks_v2::{self, Baseline, Command as TaskCommand, Completion},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Path,
    process::{Command, Output},
};

struct Fixture {
    owner: Option<AuditSession>,
    pool: Pool,
    manager: Manager,
    session: Session,
    package: Package,
    source: CardRecord,
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let module = std::fs::read(
            std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled guest path"),
        )
        .unwrap();
        let mut manifest = Package::manifest_for_transform(
            "test.tasks-edit",
            morrow_workbench_plugin::PACKAGE_VERSION,
            &module,
            vec![TransformHandler {
                handler: "workbench.tasks.v2".into(),
                input_type: "morrow.workbench.tasks.request.v2".into(),
                output_type: "morrow.workbench.tasks.response.v2".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        );
        manifest.requested_capabilities = vec![Capability::EditContent as i32];
        let package = Package::build(manifest, &module).unwrap();
        let old = persistence::encode(
            &Idea {
                id: "card".into(),
                title: "Original".into(),
                category: "进行中".into(),
                stage: "计划中".into(),
                todos: vec!["same".into(); count],
                completed: vec!["same".into()],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let base = Baseline::capture("card", 1, &old).unwrap();
        let mut body = tasks_v2::migrate(&base, "card", 1, "Original", &old).unwrap();
        body.extend_from_slice(&[0xb8, 0x0c, 7]);
        let card = CardRecord::new("card", "org.morrow.idea", 2, "Original", body).unwrap();
        let mut raw = card.encode();
        raw.extend_from_slice(&[0xaa, 0x06, 3, 255, 0, 42]);
        let source = CardRecord::decode(&raw).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve(
                "test.tasks-edit",
                package.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(
                "test.tasks-edit",
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut owner = AuditSession::open(
            &dir.path().join("db"),
            Default::default(),
            OpenMode::Initialize,
        )
        .unwrap();
        let host = owner.runtime();
        let mut seed = host.connect().unwrap();
        host.grant(&mut seed, GrantKind::CreateContent, "card", 10000, 0)
            .unwrap();
        host.create_content(&seed, "seed", &source, || 1).unwrap();
        host.disconnect(&seed).unwrap();
        let mut pool = Pool::new(host, Default::default()).unwrap();
        let revision = manager.revision();
        let session = pool
            .start(&mut manager, host, "test.tasks-edit", &[], revision)
            .unwrap();
        pool.grant_root(host, &session, GrantKind::EditContent, "card", 10000, 1)
            .unwrap();
        Self {
            owner: Some(owner),
            pool,
            manager,
            session,
            package,
            source,
            dir,
        }
    }
    fn card(&mut self) -> CardRecord {
        self.owner
            .as_mut()
            .unwrap()
            .runtime()
            .store_local()
            .card("card")
            .unwrap()
            .unwrap()
    }
    fn capture_from(
        &mut self,
        source: &CardRecord,
        operation: &str,
        command: &TaskCommand,
    ) -> (Plan, ProjectedEdit) {
        let plan = Plan::prepare(source, &self.package, operation, command).unwrap();
        let observed = self
            .pool
            .record_transform(
                &self.manager,
                self.owner.as_mut().unwrap().runtime(),
                &self.session,
                plan.invocation(),
            )
            .unwrap();
        assert_eq!(observed.report().execution.outcome, Ok(0), "{operation}");
        let projection = plan.capture(observed.evidence()).unwrap();
        assert_eq!(projection.source_card(), source.encode());
        assert!(
            projection
                .matches_intent(operation, source.summary().revision, command)
                .unwrap()
        );
        (plan, projection)
    }
    fn capture(&mut self, operation: &str, command: &TaskCommand) -> (Plan, ProjectedEdit) {
        let source = self.card();
        self.capture_from(&source, operation, command)
    }
    fn commit(
        &mut self,
        projection: &ProjectedEdit,
    ) -> morrow_workbench_host::Result<transaction::Receipt> {
        projection.commit(
            self.owner.as_mut().unwrap().runtime(),
            self.pool.root(&self.session).unwrap().connection(),
            || 1,
        )
    }
    fn close_pool(&mut self) {
        self.pool
            .close_all(self.owner.as_mut().unwrap().runtime())
            .unwrap();
    }
    fn commit_bytes(&self, operation: &str) -> Vec<u8> {
        let sql = rusqlite::Connection::open_with_flags(
            self.dir.path().join("db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        sql.query_row(
            "SELECT payload FROM operations WHERE id=?1",
            [operation],
            |r| r.get(0),
        )
        .unwrap()
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn verify_cli(directory: &Path, commit: &[u8], evidence: &Evidence, bad_pin: bool) -> Output {
    let cp = directory.join("edit-commit.bin");
    let ep = directory.join("edit-evidence.bin");
    std::fs::write(&cp, commit).unwrap();
    std::fs::write(&ep, evidence.container()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-content-replay"));
    command
        .arg(cp)
        .arg(if bad_pin {
            "0".repeat(64)
        } else {
            hex(&Sha256::digest(commit))
        })
        .arg(ep)
        .arg(hex(&evidence.digest()));
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}

#[test]
fn real_guest_edit_commands_preserve_identity_and_replay_after_reopen() {
    let mut f = Fixture::new(2);
    let initial = tasks_v2::decode("card", "Original", &f.source.body()).unwrap();
    let first_id = initial.tasks[0].id.clone();
    let second_id = initial.tasks[1].id.clone();
    assert_ne!(first_id, second_id);
    assert!(
        initial
            .tasks
            .iter()
            .all(|task| task.completion == Completion::LegacyAmbiguous as i32)
    );
    let first_command = TaskCommand::SetCompletion {
        id: first_id.clone(),
        complete: true,
    };
    let (first_plan, first) = f.capture("edit-one", &first_command);
    let first_receipt = f.commit(&first).unwrap();
    assert_eq!(first_receipt.revision, 2);
    let one = tasks_v2::decode("card", "Original", &f.card().body()).unwrap();
    assert_eq!(one.tasks[0].completion, Completion::Complete as i32);
    assert_eq!(one.tasks[1].completion, Completion::LegacyAmbiguous as i32);
    let (_, second) = f.capture(
        "edit-two",
        &TaskCommand::SetCompletion {
            id: second_id.clone(),
            complete: false,
        },
    );
    assert_eq!(f.commit(&second).unwrap().revision, 3);
    let resolved = tasks_v2::decode("card", "Original", &f.card().body()).unwrap();
    assert_eq!(resolved.tasks[0].completion, Completion::Complete as i32);
    assert_eq!(resolved.tasks[1].completion, Completion::Incomplete as i32);
    let original_ids = [first_id.clone(), second_id.clone()];
    for (operation, command) in [
        ("edit-stage", TaskCommand::SetStage("推进中".into())),
        (
            "edit-order",
            TaskCommand::Reorder(vec![second_id.clone(), first_id.clone()]),
        ),
        (
            "edit-rename",
            TaskCommand::Rename {
                id: first_id.clone(),
                text: "renamed".into(),
            },
        ),
        (
            "edit-add",
            TaskCommand::Add {
                id: "owner-new-task".into(),
                text: "same".into(),
            },
        ),
        ("edit-remove", TaskCommand::Remove("owner-new-task".into())),
        (
            "edit-complete",
            TaskCommand::CompleteAllAndSetStage("已完成".into()),
        ),
    ] {
        let (_, projection) = f.capture(operation, &command);
        let before = f.card();
        let prior = tasks_v2::decode("card", "Original", &before.body()).unwrap();
        let receipt = f.commit(&projection).unwrap();
        assert_eq!(receipt.revision, before.summary().revision + 1);
        let next = f.card();
        assert_eq!(next.encode(), projection.card().encode());
        let view = tasks_v2::decode("card", "Original", &next.body()).unwrap();
        if operation == "edit-stage" {
            assert_eq!(view.stage, "推进中");
            assert_eq!(
                view.tasks
                    .iter()
                    .map(|task| task.completion)
                    .collect::<Vec<_>>(),
                prior
                    .tasks
                    .iter()
                    .map(|task| task.completion)
                    .collect::<Vec<_>>()
            );
        }
        if operation == "edit-order" {
            assert_eq!(
                view.tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>(),
                vec![second_id.as_str(), first_id.as_str()]
            );
        }
        if operation == "edit-rename" {
            assert_eq!(
                view.tasks
                    .iter()
                    .find(|task| task.id == first_id)
                    .unwrap()
                    .text,
                "renamed"
            );
        }
        if operation == "edit-remove" {
            assert!(view.retired_task_ids.contains(&"owner-new-task".to_owned()));
            assert!(
                Plan::prepare(
                    &next,
                    &f.package,
                    "reuse-retired",
                    &TaskCommand::Add {
                        id: "owner-new-task".into(),
                        text: "reused".into()
                    }
                )
                .is_err()
            );
        }
        if operation == "edit-complete" {
            assert_eq!(view.stage, "已完成");
            assert!(
                view.tasks
                    .iter()
                    .all(|task| task.completion == Completion::Complete as i32)
            );
        }
        for id in &original_ids {
            assert!(view.tasks.iter().any(|task| task.id == *id));
        }
        let host = f.owner.as_mut().unwrap().runtime();
        let (raw, saved_receipt) = host
            .store_local()
            .operation_commit("card", operation)
            .unwrap()
            .unwrap();
        assert_eq!(saved_receipt, receipt);
        tasks_edit::verify_commit(&raw, projection.evidence()).unwrap();
        assert_eq!(
            host.store_local()
                .operation_evidence("card", operation)
                .unwrap()[0]
                .raw(),
            projection.evidence().raw()
        );
    }
    assert_eq!(f.card().summary().revision, 9);
    assert_eq!(f.commit(&first).unwrap(), first_receipt);
    assert_eq!(f.card().summary().revision, 9);
    f.owner.as_mut().unwrap().flush(16).unwrap();
    f.close_pool();
    drop(f.owner.take());
    f.owner = Some(
        AuditSession::open(
            &f.dir.path().join("db"),
            Default::default(),
            OpenMode::Existing,
        )
        .unwrap(),
    );
    let host = f.owner.as_mut().unwrap().runtime();
    let mut connection = host.connect_package(&f.package).unwrap();
    host.grant(&mut connection, GrantKind::EditContent, "card", 10000, 0)
        .unwrap();
    let saved = host
        .store_local()
        .operation_evidence("card", first_plan.operation_id())
        .unwrap()
        .remove(0);
    let restored = tasks_edit::derive(&saved).unwrap();
    assert!(
        restored
            .matches_intent("edit-one", 1, &first_command)
            .unwrap()
    );
    assert_eq!(
        restored.commit(host, &connection, || 1).unwrap(),
        first_receipt
    );
    assert_eq!(
        host.store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        9
    );
    host.store_local().integrity_check().unwrap();
    host.disconnect(&connection).unwrap();
    let raw = f.commit_bytes("edit-one");
    let source_path = f.dir.path().to_owned();
    drop(f);
    assert!(!source_path.exists());
    let verify = tempfile::tempdir().unwrap();
    let output = verify_cli(verify.path(), &raw, &saved, false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("MATCH:"));
    let wrong_pin = verify_cli(verify.path(), &raw, &saved, true);
    assert_eq!(wrong_pin.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong_pin.stderr).contains("integrity pin mismatch"));

    // The archived facts and commit can be made self-consistent, while the
    // reported Wasm fuel remains false. Only actual replay detects that lie.
    let mut data = saved.data().clone();
    let observation = &mut data.batch.as_mut().unwrap().observations[0];
    assert!(observation.fuel_remaining > 0);
    observation.fuel_remaining -= 1;
    let forged = task_evidence::encode(data).unwrap();
    let derived = tasks_edit::derive(&forged).unwrap();
    let alternative = transaction::encode_commit_with_evidence(
        derived.command().to_vec(),
        derived.card(),
        &[forged.digest()],
    )
    .unwrap();
    tasks_edit::verify_commit(
        &transaction::decode_commit(&alternative).unwrap().0,
        &forged,
    )
    .unwrap();
    let mismatch = verify_cli(verify.path(), &alternative, &forged, false);
    assert_eq!(mismatch.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&mismatch.stdout).starts_with("MISMATCH:"));
}

#[test]
fn real_guest_edit_rejects_stale_source_revoked_grant_and_wrong_package() {
    for case in 0..5 {
        let mut f = Fixture::new(2);
        let id = tasks_v2::decode("card", "Original", &f.source.body())
            .unwrap()
            .tasks[0]
            .id
            .clone();
        let (plan, projection) = f.capture(
            "edit-reject",
            &TaskCommand::SetCompletion { id, complete: true },
        );
        if case == 0 {
            f.owner
                .as_mut()
                .unwrap()
                .runtime()
                .store_local_mut()
                .set_attachments_local("concurrent", "card", 1, &[])
                .unwrap();
            let changed = f.card().encode();
            assert!(f.commit(&projection).is_err());
            assert_eq!(f.card().encode(), changed);
        } else if case == 1 {
            f.pool
                .revoke_root(
                    f.owner.as_mut().unwrap().runtime(),
                    &f.session,
                    GrantKind::EditContent,
                    "card",
                )
                .unwrap();
            assert!(f.commit(&projection).is_err());
            assert_eq!(f.card().encode(), f.source.encode());
        } else if case == 2 {
            let module = std::fs::read(std::env::var("MORROW_WORKBENCH_WASM").unwrap()).unwrap();
            let mut manifest = f.package.manifest().clone();
            manifest.package_id = "test.foreign-edit".into();
            let foreign = Package::build(manifest, &module).unwrap();
            let host = f.owner.as_mut().unwrap().runtime();
            let mut wrong = host.connect_package(&foreign).unwrap();
            host.grant(&mut wrong, GrantKind::EditContent, "card", 10000, 1)
                .unwrap();
            assert!(projection.commit(host, &wrong, || 1).is_err());
            host.disconnect(&wrong).unwrap();
            assert_eq!(f.card().encode(), f.source.encode());
        } else if case == 3 {
            let host = f.owner.as_mut().unwrap().runtime();
            let bound_without_grant = host.connect_package(&f.package).unwrap();
            assert!(projection.commit(host, &bound_without_grant, || 1).is_err());
            host.disconnect(&bound_without_grant).unwrap();
            assert_eq!(f.card().encode(), f.source.encode());
        } else {
            let revocation = f
                .owner
                .as_mut()
                .unwrap()
                .runtime()
                .revocation(f.pool.root(&f.session).unwrap().connection())
                .unwrap();
            let manager = &mut f.manager;
            let digest = f.package.digest();
            let mut ticks = 0;
            let result = projection.commit(
                f.owner.as_mut().unwrap().runtime(),
                f.pool.root(&f.session).unwrap().connection(),
                || {
                    ticks += 1;
                    if ticks == 2 {
                        manager
                            .set_enabled("test.tasks-edit", digest, false, manager.revision())
                            .unwrap();
                    }
                    1
                },
            );
            assert!(result.is_err());
            assert_eq!(ticks, 2);
            assert!(revocation.is_revoked());
            assert_eq!(f.card().encode(), f.source.encode());
        }
        let host = f.owner.as_mut().unwrap().runtime();
        assert_eq!(
            host.store_local().lookup(plan.operation_id()).unwrap(),
            Lookup::Absent
        );
        host.store_local().integrity_check().unwrap();
        f.close_pool();
    }
}

#[test]
fn same_operation_rejects_changed_command_and_evidence() {
    let mut f = Fixture::new(2);
    let initial = tasks_v2::decode("card", "Original", &f.source.body()).unwrap();
    let command = TaskCommand::SetCompletion {
        id: initial.tasks[0].id.clone(),
        complete: true,
    };
    let (_, original) = f.capture("edit-fixed-operation", &command);
    let receipt = f.commit(&original).unwrap();
    let newer = f.card();
    let (_, different) = f.capture_from(
        &newer,
        "edit-fixed-operation",
        &TaskCommand::SetStage("推进中".into()),
    );
    assert!(f.commit(&different).is_err());
    assert_eq!(f.card().encode(), newer.encode());
    let mut data = original.evidence().data().clone();
    data.batch.as_mut().unwrap().observations[0].fuel_remaining -= 1;
    let forged = task_evidence::encode(data).unwrap();
    let rejected = match tasks_edit::derive(&forged) {
        Ok(alternative) => alternative
            .commit(
                f.owner.as_mut().unwrap().runtime(),
                f.pool.root(&f.session).unwrap().connection(),
                || 1,
            )
            .is_err(),
        Err(_) => true,
    };
    assert!(
        rejected,
        "mutated evidence must not replay an existing operation"
    );
    assert_eq!(f.commit(&original).unwrap(), receipt);
    assert_eq!(f.card().encode(), newer.encode());
    f.close_pool();
}
