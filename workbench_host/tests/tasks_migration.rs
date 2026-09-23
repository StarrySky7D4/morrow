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
    store::EventBudget,
    task::Invocation,
    task_evidence::{self, Evidence},
    transaction::{self, Lookup},
};
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
    replay,
};
use morrow_workbench_host::tasks_migration::{self, Plan, ProjectedMigration};
use morrow_workbench_plugin::{Idea, persistence, tasks_v2};
use prost::Message;
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
    plan: Plan,
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new(count: usize, budget: EventBudget) -> Self {
        let module = std::fs::read(
            std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled guest path"),
        )
        .unwrap();
        let mut manifest = Package::manifest_for_transform(
            "test.task-migration",
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
                "test.task-migration",
                package.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(
                "test.task-migration",
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut body = persistence::encode(
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
        body.extend_from_slice(&[0xa2, 0x06, 3, 1, 2, 255]);
        let card = CardRecord::new("card", "org.morrow.idea", 1, "Original", body).unwrap();
        let mut raw = card.encode();
        raw.extend_from_slice(&[0xaa, 0x06, 3, 255, 0, 42]);
        let source = CardRecord::decode(&raw).unwrap();
        let plan = Plan::prepare(&source, &package).unwrap();
        let mut owner =
            AuditSession::open(&dir.path().join("db"), budget, OpenMode::Initialize).unwrap();
        let host = owner.runtime();
        let mut seed = host.connect().unwrap();
        host.grant(&mut seed, GrantKind::CreateContent, "card", 10000, 0)
            .unwrap();
        host.create_content(&seed, "seed", &source, || 1).unwrap();
        host.disconnect(&seed).unwrap();
        let mut pool = Pool::new(host, Default::default()).unwrap();
        let revision = manager.revision();
        let session = pool
            .start(&mut manager, host, "test.task-migration", &[], revision)
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
            plan,
            dir,
        }
    }
    fn capture(&mut self) -> ProjectedMigration {
        let observed = self
            .pool
            .record_transform(
                &self.manager,
                self.owner.as_mut().unwrap().runtime(),
                &self.session,
                self.plan.invocation(),
            )
            .unwrap();
        assert_eq!(observed.report().execution.outcome, Ok(0));
        self.plan.capture(observed.evidence()).unwrap()
    }
    fn commit(
        &mut self,
        projection: &ProjectedMigration,
    ) -> morrow_workbench_host::Result<transaction::Receipt> {
        projection.commit(
            self.owner.as_mut().unwrap().runtime(),
            self.pool.root(&self.session).unwrap().connection(),
            || 1,
        )
    }
    fn unchanged(&mut self) {
        let host = self.owner.as_mut().unwrap().runtime();
        assert_eq!(
            host.store_local().card("card").unwrap().unwrap().encode(),
            self.source.encode()
        );
        assert_eq!(
            host.store_local().lookup(self.plan.operation_id()).unwrap(),
            Lookup::Absent
        );
        assert_eq!(host.store_local().pending(0, 128).unwrap().len(), 1);
        host.store_local().integrity_check().unwrap();
    }
    fn commit_bytes(&self) -> Vec<u8> {
        let sql = rusqlite::Connection::open_with_flags(
            self.dir.path().join("db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        sql.query_row(
            "SELECT payload FROM operations WHERE id=?1",
            [self.plan.operation_id()],
            |r| r.get(0),
        )
        .unwrap()
    }
    fn close_pool(&mut self) {
        self.pool
            .close_all(self.owner.as_mut().unwrap().runtime())
            .unwrap();
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn verify_cli(directory: &Path, commit: &[u8], evidence: &Evidence, bad_pin: bool) -> Output {
    let cp = directory.join("commit.bin");
    let ep = directory.join("evidence.bin");
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
fn actual_guest_migration_is_atomic_audited_retryable_and_independently_replayed() {
    for count in [2, 128] {
        let mut f = Fixture::new(count, Default::default());
        let projection = f.capture();
        let receipt = f.commit(&projection).unwrap();
        assert_eq!(receipt.revision, 2);
        assert_eq!(projection.card().summary().format_version, 2);
        let p = tasks_v2::decode("card", "Original", &projection.card().body()).unwrap();
        assert_eq!(p.stage, "计划中");
        assert_eq!(p.tasks.len(), count);
        let ids: BTreeSet<_> = p.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(
            ids.len(),
            count,
            "duplicate legacy text needs distinct TaskIds"
        );
        let origin = p.origin.as_ref().unwrap();
        assert_eq!(origin.mapping.len(), count);
        for (index, task) in p.tasks.iter().enumerate() {
            assert_eq!(task.text, "same");
            assert_eq!(task.legacy_duplicates, count as u32);
            assert!(task.legacy_completed);
            assert_eq!(origin.mapping[index].source_index, index as u32);
            assert_eq!(origin.mapping[index].task_id, task.id);
        }
        assert!(
            p.tasks
                .iter()
                .all(|t| t.completion == tasks_v2::Completion::LegacyAmbiguous as i32)
        );
        assert_eq!(origin.original_properties, f.source.body());
        let host = f.owner.as_mut().unwrap().runtime();
        assert_eq!(
            host.store_local().card("card").unwrap().unwrap().encode(),
            projection.card().encode()
        );
        let (commit, historical) = host
            .store_local()
            .operation_commit("card", f.plan.operation_id())
            .unwrap()
            .unwrap();
        assert_eq!(receipt, historical);
        tasks_migration::verify_commit(&commit, projection.evidence()).unwrap();
        assert_eq!(
            host.store_local()
                .operation_evidence("card", f.plan.operation_id())
                .unwrap()[0]
                .raw(),
            projection.evidence().raw()
        );
        // A later metadata revision must survive a retry of the older migration.
        host.store_local_mut()
            .set_attachments_local("later", "card", 2, &[])
            .unwrap();
        assert_eq!(f.commit(&projection).unwrap(), receipt);
        let card_after_later_edit = f
            .owner
            .as_mut()
            .unwrap()
            .runtime()
            .store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .encode();
        let root = f.pool.root(&f.session).unwrap();
        f.manager
            .set_enabled(
                "test.task-migration",
                f.package.digest(),
                false,
                f.manager.revision(),
            )
            .unwrap();
        assert!(
            projection
                .commit(f.owner.as_mut().unwrap().runtime(), root.connection(), || 1)
                .is_err()
        );
        assert_eq!(
            f.owner
                .as_mut()
                .unwrap()
                .runtime()
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .encode(),
            card_after_later_edit
        );
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
            .operation_evidence("card", f.plan.operation_id())
            .unwrap()
            .remove(0);
        let restored = tasks_migration::derive(&saved).unwrap();
        assert_eq!(restored.commit(host, &connection, || 1).unwrap(), receipt);
        assert_eq!(
            host.store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .summary()
                .revision,
            3
        );
        host.store_local().integrity_check().unwrap();
        let card_before_denied_retry = host.store_local().card("card").unwrap().unwrap().encode();
        host.revoke(&mut connection, GrantKind::EditContent, "card")
            .unwrap();
        assert!(restored.commit(host, &connection, || 1).is_err());
        assert_eq!(
            host.store_local().card("card").unwrap().unwrap().encode(),
            card_before_denied_retry
        );
        host.disconnect(&connection).unwrap();
        let raw = f.commit_bytes();
        let source_path = f.dir.path().to_owned();
        drop(f);
        assert!(
            !source_path.exists(),
            "fixture source and keys were released"
        );
        let verify = tempfile::tempdir().unwrap();
        let output = verify_cli(verify.path(), &raw, &saved, false);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("MATCH:"));
        assert!(
            !verify_cli(verify.path(), &raw, &saved, true)
                .status
                .success()
        );
        // Projection matching is not execution proof: a forged fuel observation must fail replay.
        let mut data = saved.data().clone();
        data.batch.as_mut().unwrap().observations[0].fuel_remaining -= 1;
        let forged = task_evidence::encode(data).unwrap();
        let derived = tasks_migration::derive(&forged).unwrap();
        let alternative = transaction::encode_commit_with_evidence(
            derived.command().to_vec(),
            derived.card(),
            &[forged.digest()],
        )
        .unwrap();
        tasks_migration::verify_commit(
            &transaction::decode_commit(&alternative).unwrap().0,
            &forged,
        )
        .unwrap();
        assert_eq!(
            verify_cli(verify.path(), &alternative, &forged, false)
                .status
                .code(),
            Some(2)
        );
    }
}
#[test]
fn baseline_revocation_package_and_capacity_failures_do_not_commit() {
    for mode in 0..8 {
        let mut f = Fixture::new(
            2,
            if mode == 7 {
                EventBudget {
                    max_count: 1,
                    ..Default::default()
                }
            } else {
                Default::default()
            },
        );
        let projection = f.capture();
        if mode == 0 {
            f.owner
                .as_mut()
                .unwrap()
                .runtime()
                .store_local_mut()
                .set_attachments_local("concurrent", "card", 1, &[])
                .unwrap();
            let before = f
                .owner
                .as_mut()
                .unwrap()
                .runtime()
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .encode();
            assert!(f.commit(&projection).is_err());
            let host = f.owner.as_mut().unwrap().runtime();
            assert_eq!(
                host.store_local().card("card").unwrap().unwrap().encode(),
                before
            );
            assert_eq!(
                host.store_local().lookup(f.plan.operation_id()).unwrap(),
                Lookup::Absent
            );
        } else if mode == 1 {
            f.pool
                .revoke_root(
                    f.owner.as_mut().unwrap().runtime(),
                    &f.session,
                    GrantKind::EditContent,
                    "card",
                )
                .unwrap();
            assert!(f.commit(&projection).is_err());
            f.unchanged();
        } else if mode == 2 {
            let mut ticks = 0;
            let result = projection.commit(
                f.owner.as_mut().unwrap().runtime(),
                f.pool.root(&f.session).unwrap().connection(),
                || {
                    ticks += 1;
                    if ticks == 2 { 10_001 } else { 1 }
                },
            );
            assert!(result.is_err());
            assert_eq!(ticks, 2);
            f.unchanged();
        } else if mode == 3 {
            let host = f.owner.as_mut().unwrap().runtime();
            let mut unbound = host.connect().unwrap();
            host.grant(&mut unbound, GrantKind::EditContent, "card", 10000, 1)
                .unwrap();
            assert!(projection.commit(host, &unbound, || 1).is_err());
            host.disconnect(&unbound).unwrap();
            f.unchanged();
        } else if mode == 4 {
            let host = f.owner.as_mut().unwrap().runtime();
            let bound_without_grant = host.connect_package(&f.package).unwrap();
            assert!(projection.commit(host, &bound_without_grant, || 1).is_err());
            host.disconnect(&bound_without_grant).unwrap();
            f.unchanged();
        } else if mode == 5 {
            let module = std::fs::read(std::env::var("MORROW_WORKBENCH_WASM").unwrap()).unwrap();
            let mut foreign_manifest = f.package.manifest().clone();
            foreign_manifest.package_id = "test.foreign-migration".into();
            let foreign = Package::build(foreign_manifest, &module).unwrap();
            assert_ne!(foreign.digest(), f.package.digest());
            let host = f.owner.as_mut().unwrap().runtime();
            let mut wrong_package = host.connect_package(&foreign).unwrap();
            host.grant(&mut wrong_package, GrantKind::EditContent, "card", 10000, 1)
                .unwrap();
            assert!(projection.commit(host, &wrong_package, || 1).is_err());
            host.disconnect(&wrong_package).unwrap();
            f.unchanged();
        } else if mode == 6 {
            let mut ticks = 0;
            let revocation = f
                .owner
                .as_mut()
                .unwrap()
                .runtime()
                .revocation(f.pool.root(&f.session).unwrap().connection())
                .unwrap();
            let manager = &mut f.manager;
            let digest = f.package.digest();
            let result = projection.commit(
                f.owner.as_mut().unwrap().runtime(),
                f.pool.root(&f.session).unwrap().connection(),
                || {
                    ticks += 1;
                    if ticks == 2 {
                        manager
                            .set_enabled("test.task-migration", digest, false, manager.revision())
                            .unwrap();
                    }
                    1
                },
            );
            assert!(result.is_err());
            assert_eq!(ticks, 2);
            assert!(revocation.is_revoked());
            f.unchanged();
        } else {
            let error = f.commit(&projection).unwrap_err();
            assert_eq!(
                error.downcast_ref::<morrow_core::Error>(),
                Some(&morrow_core::Error::EventCapacity)
            );
            f.unchanged();
        }
        f.close_pool();
    }
}
#[test]
fn self_consistent_substitutions_and_ambiguous_facts_are_rejected() {
    let mut f = Fixture::new(2, Default::default());
    let projection = f.capture();
    for mode in 0..9 {
        let mut data = projection.evidence().data().clone();
        let batch = data.batch.as_mut().unwrap();
        let mut facts = tasks_migration::proto::Facts::decode(batch.intent.as_slice()).unwrap();
        match mode {
            0 => facts.operation_id = "different-operation".into(),
            1 => facts.package_sha256 = vec![42; 32],
            2 => facts.migrator_version = 2,
            3 => facts.target_format_version = 3,
            4 => facts.schema_version = 2,
            5 => facts.source_card = f.source.with_title(1, "different source").unwrap().encode(),
            6 => {
                // Full source identity remains bound even when the business body is unchanged.
                facts.source_card.extend_from_slice(&[0xb0, 0x06, 1]);
            }
            7 | 8 => {}
            _ => unreachable!(),
        }
        batch.intent = facts.encode_to_vec();
        if mode == 7 {
            batch.intent.extend_from_slice(&[8, 1]);
        }
        if mode == 8 {
            batch.intent.extend_from_slice(&[56, 1]);
        }
        let changed = task_evidence::encode(data).unwrap();
        // Mode 6 changes only an opaque outer source fact: pure projection can reconstruct
        // it, but commit must refuse because it is not the current database baseline.
        if mode == 6 {
            let p = tasks_migration::derive(&changed).unwrap();
            assert!(f.commit(&p).is_err());
        } else {
            assert!(tasks_migration::derive(&changed).is_err(), "case {mode}");
        }
        f.unchanged();
    }
    let mut data = projection.evidence().data().clone();
    let observation = &mut data.batch.as_mut().unwrap().observations[0];
    let invocation = Invocation::decode(&observation.invocation).unwrap();
    let mut output = capnp::message::Builder::new_default();
    {
        let mut response =
            output.init_root::<morrow_workbench_plugin::tasks_capnp::response::Builder>();
        response.set_version(2);
        response.set_digest(&morrow_workbench_plugin::tasks_v2_codec::digest());
        response.set_properties(&projection.card().body());
        response.set_stage("forged");
    }
    observation.completion = invocation
        .output_completion(&capnp::serialize::write_message_to_words(&output))
        .unwrap();
    let changed = task_evidence::encode(data).unwrap();
    assert!(tasks_migration::derive(&changed).is_err());
    assert!(
        replay::replay_batch(projection.evidence(), Limits::default(), 20_000_000)
            .unwrap()
            .matches
    );
    f.close_pool();
}

#[test]
fn oversized_migration_output_is_rejected_before_guest_execution() {
    let mut f = Fixture::new(2, Default::default());
    let body = persistence::encode(
        &Idea {
            id: "card".into(),
            title: "Original".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            description: "x".repeat(40_000),
            todos: vec!["same".into(); 2],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert!(body.len() < tasks_v2::MAX_BYTES);
    let card = CardRecord::new("card", "org.morrow.idea", 1, "Original", body).unwrap();
    let error = Plan::prepare(&card, &f.package).err().unwrap();
    assert!(error.to_string().contains("budget"), "{error}");
    f.unchanged();
    f.close_pool();
}
