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
use morrow_workbench_host::cards_edit::{self, Plan, ProjectedEdit, Undo};
use morrow_workbench_plugin::{
    Asset, Idea,
    cards_v2::{Command as CardCommand, Fields},
    persistence,
    tasks_v2::{self, Baseline},
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
    fn new() -> Self {
        let module = std::fs::read(
            std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled guest path"),
        )
        .unwrap();
        let mut manifest = Package::manifest_for_transform(
            "test.cards-edit",
            morrow_workbench_plugin::PACKAGE_VERSION,
            &module,
            vec![TransformHandler {
                handler: "workbench.cards.v2".into(),
                input_type: "morrow.workbench.cards.request.v2".into(),
                output_type: "morrow.workbench.cards.response.v2".into(),
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
                todos: vec!["one".into()],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let body = tasks_v2::migrate(
            &Baseline::capture("card", 1, &old).unwrap(),
            "card",
            1,
            "Original",
            &old,
        )
        .unwrap();
        let mut body = body;
        body.extend([0xb8, 0x0c, 7]); // V2 unknown field 199
        let card = CardRecord::new("card", "org.morrow.idea", 2, "Original", body).unwrap();
        let mut raw = card.encode();
        raw.extend([0xaa, 0x06, 3, 255, 0, 42]); // outer unknown field
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
                "test.cards-edit",
                package.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(
                "test.cards-edit",
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
            .start(&mut manager, host, "test.cards-edit", &[], revision)
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
        command: &CardCommand,
        undo: Option<Undo>,
    ) -> ProjectedEdit {
        let plan = Plan::prepare(
            source,
            &self.package,
            operation,
            command,
            &source.attachments(),
            undo,
        )
        .unwrap();
        let captured = self
            .pool
            .record_transform(
                &self.manager,
                self.owner.as_mut().unwrap().runtime(),
                &self.session,
                plan.invocation(),
            )
            .unwrap();
        assert_eq!(captured.report().execution.outcome, Ok(0));
        let projection = plan.capture(captured.evidence()).unwrap();
        assert_eq!(projection.source_card(), source.encode());
        assert!(
            projection
                .matches_intent(operation, source.summary().revision, command)
                .unwrap()
        );
        projection
    }
    fn capture(
        &mut self,
        operation: &str,
        command: &CardCommand,
        undo: Option<Undo>,
    ) -> ProjectedEdit {
        let source = self.card();
        self.capture_from(&source, operation, command, undo)
    }
    fn commit(
        &mut self,
        projection: &ProjectedEdit,
        clock: u64,
    ) -> morrow_workbench_host::Result<transaction::Receipt> {
        projection.commit(
            self.owner.as_mut().unwrap().runtime(),
            self.pool.root(&self.session).unwrap().connection(),
            || clock,
        )
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
    fn close_pool(&mut self) {
        self.pool
            .close_all(self.owner.as_mut().unwrap().runtime())
            .unwrap();
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn verify_cli(directory: &Path, commit: &[u8], evidence: &Evidence, wrong_pin: bool) -> Output {
    let commit_path = directory.join("cards-commit.bin");
    let evidence_path = directory.join("cards-evidence.bin");
    std::fs::write(&commit_path, commit).unwrap();
    std::fs::write(&evidence_path, evidence.container()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-content-replay"));
    command
        .arg(commit_path)
        .arg(if wrong_pin {
            "0".repeat(64)
        } else {
            hex(&Sha256::digest(commit))
        })
        .arg(evidence_path)
        .arg(hex(&evidence.digest()));
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}
#[test]
fn real_guest_common_edit_persists_unknowns_and_replays_with_pins() {
    let mut f = Fixture::new();
    let before = tasks_v2::decode("card", "Original", &f.source.body()).unwrap();
    let command = CardCommand::Edit(Fields {
        title: "Renamed".into(),
        description: "A new description".into(),
        hypothesis: "H".into(),
        conclusion: "C".into(),
        icon: 1,
        color: 42,
        assets: vec![],
    });
    let first = f.capture("cards-edit-one", &command, None);
    assert!(!first.matches_intent("cards-edit-one", 0, &command).unwrap());
    assert!(
        !first
            .matches_intent("cards-edit-one", 1, &CardCommand::SetFavorite(true))
            .unwrap()
    );
    let receipt = f.commit(&first, 1).unwrap();
    assert_eq!(receipt.revision, 2);
    let current = f.card();
    assert_eq!(current.encode(), first.card().encode());
    assert_eq!(current.summary().title, "Renamed");
    let after = tasks_v2::decode("card", "Renamed", &current.body()).unwrap();
    assert_eq!(after.tasks, before.tasks);
    assert_eq!(after.origin, before.origin);
    assert_eq!(after.retired_task_ids, before.retired_task_ids);
    assert!(current.body().windows(3).any(|w| w == [0xb8, 0x0c, 7]));
    assert!(
        current
            .encode()
            .windows(6)
            .any(|w| w == [0xaa, 0x06, 3, 255, 0, 42])
    );
    let saved = f
        .owner
        .as_mut()
        .unwrap()
        .runtime()
        .store_local()
        .operation_evidence("card", "cards-edit-one")
        .unwrap()
        .remove(0);
    let (commit, saved_receipt) = f
        .owner
        .as_mut()
        .unwrap()
        .runtime()
        .store_local()
        .operation_commit("card", "cards-edit-one")
        .unwrap()
        .unwrap();
    assert_eq!(saved_receipt, receipt);
    cards_edit::verify_commit(&commit, &saved).unwrap();
    assert_eq!(f.commit(&first, 9999).unwrap(), receipt); // historical receipt
    assert_eq!(f.card().encode(), current.encode());
    let altered = f.capture_from(
        &current,
        "cards-edit-one",
        &CardCommand::SetFavorite(true),
        None,
    );
    assert!(f.commit(&altered, 1).is_err()); // same operation, different source and payload
    assert_eq!(f.card().encode(), current.encode());
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
    let restored = cards_edit::derive(&saved).unwrap();
    assert_eq!(
        restored.commit(host, &connection, || 9999).unwrap(),
        receipt
    );
    host.store_local().integrity_check().unwrap();
    host.disconnect(&connection).unwrap();
    let raw = f.commit_bytes("cards-edit-one");
    let verify = tempfile::tempdir().unwrap();
    let match_result = verify_cli(verify.path(), &raw, &saved, false);
    assert!(
        match_result.status.success(),
        "{}",
        String::from_utf8_lossy(&match_result.stderr)
    );
    assert!(String::from_utf8_lossy(&match_result.stdout).starts_with("MATCH:"));
    let wrong_pin = verify_cli(verify.path(), &raw, &saved, true);
    assert_eq!(wrong_pin.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong_pin.stderr).contains("integrity pin mismatch"));
    let mut data = saved.data().clone();
    let observation = &mut data.batch.as_mut().unwrap().observations[0];
    assert!(observation.fuel_remaining > 0);
    observation.fuel_remaining -= 1;
    let forged = task_evidence::encode(data).unwrap();
    let projected = cards_edit::derive(&forged).unwrap();
    let self_consistent = transaction::encode_commit_with_evidence(
        projected.command().to_vec(),
        projected.card(),
        &[forged.digest()],
    )
    .unwrap();
    cards_edit::verify_commit(
        &transaction::decode_commit(&self_consistent).unwrap().0,
        &forged,
    )
    .unwrap();
    let mismatch = verify_cli(verify.path(), &self_consistent, &forged, false);
    assert_eq!(mismatch.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&mismatch.stdout).starts_with("MISMATCH:"));
}
#[test]
fn real_guest_delete_and_restore_obey_second_clock_guard() {
    let mut f = Fixture::new();
    let deleted = f.capture("cards-delete", &CardCommand::Delete { now_ms: 100 }, None);
    let delete_receipt = f.commit(&deleted, 100).unwrap();
    let tombstone = f.card();
    assert_eq!(delete_receipt.revision, 2);
    let view = tasks_v2::decode("card", "Original", &tombstone.body()).unwrap();
    assert!(view.deleted);
    assert_eq!(view.deleted_at, 100);
    assert!(
        Plan::prepare(
            &tombstone,
            &f.package,
            "edit-deleted",
            &CardCommand::SetFavorite(true),
            &[],
            None,
        )
        .is_err()
    );
    let restore_command = CardCommand::Restore { now_ms: 101 };
    let restore = f.capture(
        "cards-restore",
        &restore_command,
        Some(Undo {
            revision: 2,
            deadline: 8100,
        }),
    );
    assert!(f.commit(&restore, 8100).is_err()); // core guard runs at commit time
    assert_eq!(f.card().encode(), tombstone.encode());
    assert!(matches!(
        f.owner
            .as_mut()
            .unwrap()
            .runtime()
            .store_local()
            .lookup_for_card("card", "cards-restore")
            .unwrap(),
        Lookup::Absent
    ));
    // A failed late attempt advances the trusted monotonic clock. Use a fresh
    // fixture to prove an in-window restore and its historical late retry.
    let mut in_window = Fixture::new();
    let deleted = in_window.capture("cards-delete", &CardCommand::Delete { now_ms: 100 }, None);
    in_window.commit(&deleted, 100).unwrap();
    let restore = in_window.capture(
        "cards-restore",
        &CardCommand::Restore { now_ms: 101 },
        Some(Undo {
            revision: 2,
            deadline: 8100,
        }),
    );
    let receipt = in_window.commit(&restore, 102).unwrap();
    assert_eq!(receipt.revision, 3);
    let restored = in_window.card();
    assert!(
        !tasks_v2::decode("card", "Original", &restored.body())
            .unwrap()
            .deleted
    );
    in_window.owner.as_mut().unwrap().flush(16).unwrap();
    in_window.close_pool();
    drop(in_window.owner.take());
    in_window.owner = Some(
        AuditSession::open(
            &in_window.dir.path().join("db"),
            Default::default(),
            OpenMode::Existing,
        )
        .unwrap(),
    );
    let host = in_window.owner.as_mut().unwrap().runtime();
    let mut connection = host.connect_package(&in_window.package).unwrap();
    host.grant(&mut connection, GrantKind::EditContent, "card", 10000, 0)
        .unwrap();
    assert_eq!(restore.commit(host, &connection, || 9000).unwrap(), receipt);
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        restored.encode()
    );
    host.disconnect(&connection).unwrap();
}
#[test]
fn new_attachment_metadata_requires_selected_outer_attachment() {
    let f = Fixture::new();
    let command = CardCommand::Edit(Fields {
        title: "Original".into(),
        description: String::new(),
        hypothesis: String::new(),
        conclusion: String::new(),
        icon: 0,
        color: 0,
        assets: vec![Asset {
            id: "unselected".into(),
            name: "not-selected.txt".into(),
            kind: "file".into(),
            bytes: 7,
        }],
    });
    assert!(Plan::prepare(&f.source, &f.package, "unselected", &command, &[], None).is_err());
}
#[test]
fn delete_cannot_commit_after_edit_authority_is_revoked() {
    let mut f = Fixture::new();
    let deletion = f.capture(
        "cards-no-grant-delete",
        &CardCommand::Delete { now_ms: 100 },
        None,
    );
    f.pool
        .revoke_root(
            f.owner.as_mut().unwrap().runtime(),
            &f.session,
            GrantKind::EditContent,
            "card",
        )
        .unwrap();
    assert!(f.commit(&deletion, 100).is_err());
    assert_eq!(f.card().encode(), f.source.encode());
    assert!(matches!(
        f.owner
            .as_mut()
            .unwrap()
            .runtime()
            .store_local()
            .lookup_for_card("card", "cards-no-grant-delete")
            .unwrap(),
        Lookup::Absent,
    ));
}
