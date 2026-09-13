#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, proto::TransformHandler},
    store::Store,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, TaskEvidence},
    },
    transaction::{self, proto::command::Action},
};
use rusqlite::{Connection, params};
// Synthetic observations qualify storage/query semantics, not actual Wasm execution.
fn evidence(task: &str) -> Evidence {
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for_transform(
            "test.operation.commit",
            "1.0.0",
            module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 1024,
                max_output_bytes: 1024,
            }],
        ),
        module,
    )
    .unwrap();
    let input = Invocation::new_transform(
        task,
        Transform {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![1],
        },
    )
    .unwrap();
    task_evidence::encode(TaskEvidence {
        schema_version: 1,
        package_archive: package.archive().to_vec(),
        invocation: input.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: input.output_completion(&[2]).unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
    })
    .unwrap()
}
fn card() -> CardRecord {
    CardRecord::new("card", "note", 1, "original", b"original body".to_vec()).unwrap()
}
struct Fixture {
    dir: tempfile::TempDir,
    host: HostRuntime,
    evidence: Vec<Evidence>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut c = host.connect().unwrap();
        host.grant(&mut c, GrantKind::CreateContent, "card", 100, 0)
            .unwrap();
        let evidence = vec![evidence("first"), evidence("second")];
        host.create_content_with_evidence(&c, "create", &card(), &evidence, || 1)
            .unwrap();
        Self {
            dir,
            host,
            evidence,
        }
    }
    fn sql(&self) -> Connection {
        let connection = Connection::open(self.dir.path().join("db")).unwrap();
        // Deliberate out-of-band corruption of synthetic data, bypassing engine constraints.
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
    }
}
#[test]
fn original_command_receipt_and_ordered_refs_survive_later_edits_and_reopen() {
    let mut f = Fixture::new();
    let before = f
        .host
        .store_local()
        .operation_commit("card", "create")
        .unwrap()
        .unwrap();
    assert_eq!(before.0.schema_version, 2);
    assert_eq!(before.1.revision, 1);
    assert_eq!(
        before.0.task_evidence_sha256,
        f.evidence
            .iter()
            .map(|e| e.digest().to_vec())
            .collect::<Vec<_>>()
    );
    let mut c = f.host.connect().unwrap();
    f.host
        .grant(&mut c, GrantKind::EditContent, "card", 100, 1)
        .unwrap();
    let change = ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "later title".into(),
        body: b"later body".to_vec(),
        preview_text: "later".into(),
        attachments: None,
    };
    let receipt = f
        .host
        .edit_content_with_evidence(&c, &change, &f.evidence, || 2)
        .unwrap();
    assert_eq!(receipt.revision, 2);
    let after = f
        .host
        .store_local()
        .operation_commit("card", "create")
        .unwrap()
        .unwrap();
    assert_eq!(before, after);
    let command = transaction::decode_command(&after.0.command).unwrap();
    let Action::CreateCard(bytes) = command.action.unwrap() else {
        panic!("original creation");
    };
    let original = CardRecord::decode(&bytes).unwrap();
    assert_eq!(original.encode(), card().encode());
    assert_ne!(
        original.body(),
        f.host.store_local().card("card").unwrap().unwrap().body()
    );
    let edit = f
        .host
        .store_local()
        .operation_commit("card", "edit")
        .unwrap()
        .unwrap();
    assert_eq!(edit.1, receipt);
    assert!(
        matches!(transaction::decode_command(&edit.0.command).unwrap().action,Some(Action::SetContent(v)) if v.body==change.body && v.expected_revision==1)
    );
    drop(f.host);
    let store = Store::open_existing(&f.dir.path().join("db"), Default::default()).unwrap();
    assert_eq!(
        store.operation_commit("card", "create").unwrap().unwrap(),
        before
    );
}
#[test]
fn absent_foreign_card_and_noncontent_operations_return_none_without_payload_access() {
    let f = Fixture::new();
    let store = f.host.store_local();
    assert!(store.operation_commit("card", "absent").unwrap().is_none());
    assert!(
        store
            .operation_commit("foreign", "create")
            .unwrap()
            .is_none()
    );
    f.sql()
        .execute(
            "UPDATE operations SET object_kind=1,payload=zeroblob(?1) WHERE id='create'",
            [(transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 129) as i64],
        )
        .unwrap();
    assert!(store.operation_commit("card", "create").unwrap().is_none());
    f.sql()
        .execute("UPDATE operations SET object_kind=0 WHERE id='create'", [])
        .unwrap();
    assert!(
        store
            .operation_commit("foreign", "create")
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        store.operation_commit("card", "create"),
        Err(Error::Limit)
    ));
    for (card, op) in [("", "create"), ("card", "../create")] {
        assert!(store.operation_commit(card, op).is_err());
    }
}
#[test]
fn empty_refs_legacy_commits_are_queryable_without_inventing_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
    let receipt = store.create_local("legacy", &card()).unwrap();
    let (commit, found) = store.operation_commit("card", "legacy").unwrap().unwrap();
    assert_eq!(commit.schema_version, 1);
    assert!(commit.task_evidence_sha256.is_empty());
    assert_eq!(receipt, found);
    assert!(
        store
            .operation_evidence("card", "legacy")
            .unwrap()
            .is_empty()
    );
}
#[test]
fn missing_corrupt_reordered_and_extra_evidence_refs_are_never_returned_as_a_commit() {
    for case in 0..5 {
        let f = Fixture::new();
        let sql = f.sql();
        match case {
            0 => {
                sql.execute(
                    "DELETE FROM task_evidence WHERE digest=?1",
                    [f.evidence[0].digest().as_slice()],
                )
                .unwrap();
            }
            1 => {
                sql.execute(
                    "UPDATE task_evidence SET payload=x'00' WHERE digest=?1",
                    [f.evidence[0].digest().as_slice()],
                )
                .unwrap();
            }
            2 => {
                sql.execute("UPDATE operation_evidence SET digest=?1 WHERE operation_id='create' AND ordinal=0",[f.evidence[1].digest().as_slice()]).unwrap();
            }
            3 => {
                sql.execute(
                    "DELETE FROM operation_evidence WHERE operation_id='create' AND ordinal=1",
                    [],
                )
                .unwrap();
            }
            _ => {
                sql.execute("INSERT INTO operation_evidence(operation_id,ordinal,digest) VALUES('create',2,?1)",[f.evidence[0].digest().as_slice()]).unwrap();
            }
        }
        assert!(
            f.host
                .store_local()
                .operation_commit("card", "create")
                .is_err(),
            "case {case}"
        );
    }
}
#[test]
fn decoded_commit_ids_must_match_the_scoped_sql_row() {
    for different_card in [true, false] {
        let f = Fixture::new();
        let other = if different_card {
            CardRecord::new("other", "note", 1, "different", vec![]).unwrap()
        } else {
            card()
        };
        let operation = if different_card {
            "create"
        } else {
            "different-operation"
        };
        let raw = transaction::encode_commit(
            transaction::create_command(operation, &other).unwrap(),
            &other,
        )
        .unwrap();
        f.sql()
            .execute(
                "UPDATE operations SET payload=?1 WHERE id=?2",
                params![raw, "create"],
            )
            .unwrap();
        assert!(matches!(
            f.host.store_local().operation_commit("card", "create"),
            Err(Error::Integrity)
        ));
    }
}
