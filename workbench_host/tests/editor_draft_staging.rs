#![cfg(target_os = "windows")]
//! Real host/store coverage for durable editor-draft import ownership.
use morrow_core::plugin_package::proto::{Capability, TransformHandler};
use morrow_core::plugin_package::Package;
use morrow_workbench_host::{
    editor_draft::model::proto::{AssetSelection, TextValue, Values, WriteRequest},
    editor_draft_staging::{proto, DraftImportPhase},
    versioned_record::VersionedRecord,
    Workbench,
};
use morrow_workbench_plugin::{Idea, PACKAGE_VERSION};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

const CARD: &str = "durable-import-card";
const DRAFT: &str = "durable-import-draft";
const BYTES: &[u8] = b"durable draft source bytes\x00\xff";

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("compiled first-party Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.editor-draft-staging",
        PACKAGE_VERSION,
        &module,
        [
            (
                "studio.command",
                "morrow.studio.request.v1",
                "morrow.studio.response.v1",
            ),
            (
                "workbench.command",
                "morrow.workbench.request.v1",
                "morrow.workbench.response.v1",
            ),
            (
                "workbench.tasks.v2",
                "morrow.workbench.tasks.request.v2",
                "morrow.workbench.tasks.response.v2",
            ),
            (
                "workbench.cards.v2",
                "morrow.workbench.cards.request.v2",
                "morrow.workbench.cards.response.v2",
            ),
            (
                "capture.convert",
                "morrow.capture.request.v1",
                "morrow.capture.response.v1",
            ),
        ]
        .into_iter()
        .map(|(handler, input_type, output_type)| TransformHandler {
            handler: handler.into(),
            input_type: input_type.into(),
            output_type: output_type.into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        })
        .collect(),
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    Package::build(manifest, &module).unwrap()
}

fn seed(path: &Path) -> Workbench {
    let mut host = Workbench::open(path, Some(package())).unwrap();
    host.create(
        "durable-import-business-seed",
        Idea {
            id: CARD.into(),
            title: "Formal title".into(),
            description: "Formal body".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["keep TaskId".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let migration = host.plan_tasks_migration(CARD).unwrap();
    host.migrate_tasks(&migration.operation, CARD, migration.source_revision)
        .unwrap();
    assert_eq!(business_revision(&host), 2);
    host
}

fn business_revision(host: &Workbench) -> u64 {
    match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(card) => card.revision,
        VersionedRecord::Legacy(_) => panic!("expected V2 business source"),
    }
}
fn text(value: &str) -> TextValue {
    TextValue {
        text: value.into(),
        selection_base: 0,
        selection_extent: 0,
        affinity: 0,
        directional: false,
        composing_start: -1,
        composing_end: -1,
    }
}
fn draft(operation: &str, generation: u64, assets: Vec<AssetSelection>) -> WriteRequest {
    WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: generation,
        source_revision: 2,
        predecessor_operation: String::new(),
        predecessor_sha256: vec![],
        source_kind: 0,
        values: Some(Values {
            title: Some(text("")),
            description: Some(text("raw, unsubmitted")),
            hypothesis: Some(text("")),
            conclusion: Some(text("")),
            todos: Some(text("")),
            category: "进行中".into(),
            stage: "计划中".into(),
        }),
        assets,
    }
}
fn active_draft(host: &mut Workbench) {
    host.save_editor_draft(&draft("durable-import-first-draft", 0, vec![]))
        .unwrap();
}
fn sha(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}
fn import_request(operation: &str, generation: u64, bytes: &[u8]) -> proto::ImportRequest {
    proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: generation,
        name: "selected.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: sha(bytes),
    }
}
fn assert_sealed(host: &mut Workbench, request: &proto::ImportRequest) {
    let historical = host
        .import_editor_draft_asset_durable(request, &mut ThrowReader)
        .unwrap();
    assert_eq!(historical.phase, DraftImportPhase::Retired);
    assert!(!historical.current_active);
}
struct ThrowReader;
impl Read for ThrowReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("confirmed import reread removed original source")
    }
}

#[test]
fn pending_restart_ready_retry_missing_source_and_package_free_inspection() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let source = directory.path().join("selected-original.bin");
    std::fs::write(&source, BYTES).unwrap();
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-import-one", 1, BYTES);
    let pending = host.begin_editor_draft_import(&request).unwrap();
    assert_eq!(pending.phase, DraftImportPhase::Pending);
    assert!(!pending.current_active);
    assert!(!pending.bytes_retained);
    assert_eq!(business_revision(&host), 2);
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let restored = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(restored.phase, DraftImportPhase::Pending);
    assert_eq!(restored.request, request);
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        1
    );
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert_eq!(
        host.inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .phase,
        DraftImportPhase::Pending
    );
    let ready = host
        .import_editor_draft_asset_durable(&request, &mut std::fs::File::open(&source).unwrap())
        .unwrap();
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    assert!(ready.current_active);
    assert!(ready.bytes_retained);
    assert!(!ready.asset_id.is_empty());
    assert_eq!(
        host.export_editor_draft_import(CARD, DRAFT, 1, &request.operation_id)
            .unwrap(),
        BYTES
    );
    std::fs::remove_file(&source).unwrap();
    let repeated = host
        .import_editor_draft_asset_durable(&request, &mut ThrowReader)
        .unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.asset_id, ready.asset_id);
    assert_eq!(repeated.phase, DraftImportPhase::Ready);
    let mut changed = request.clone();
    changed.name = "changed-name.bin".into();
    assert!(host
        .import_editor_draft_asset_durable(&changed, &mut ThrowReader)
        .is_err());
    assert_eq!(
        host.inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .asset_id,
        ready.asset_id
    );
    assert_eq!(business_revision(&host), 2);
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let loaded = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.phase, DraftImportPhase::Ready);
    assert_eq!(loaded.asset_id, ready.asset_id);
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        1
    );
    assert_eq!(
        host.export_editor_draft_import(CARD, DRAFT, 1, &request.operation_id)
            .unwrap(),
        BYTES
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn wrong_hash_or_foreign_operation_does_not_acquire_an_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-bad-bytes", 1, b"expected bytes");
    host.begin_editor_draft_import(&request).unwrap();
    assert!(host
        .import_editor_draft_asset_durable(&request, &mut &b"wrong bytes"[..])
        .is_err());
    let pending = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.phase, DraftImportPhase::Pending);
    assert!(!pending.bytes_retained);
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 1, &request.operation_id)
        .is_err());
    let sql = rusqlite::Connection::open(&path).unwrap();
    let retained: i64 = sql
        .query_row(
            "SELECT count(*) FROM retentions WHERE kind=?1",
            [morrow_core::attachment::RetentionKind::Snapshot as i32],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 0, "failed source hash retained a blob owner");
    let ready = host
        .import_editor_draft_asset_durable(&request, &mut &b"expected bytes"[..])
        .unwrap();
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    assert!(ready.bytes_retained);
    assert_eq!(business_revision(&host), 2);

    let mut foreign = Idea::default();
    foreign.id = "durable-import-unrelated-business".into();
    foreign.title = "Unrelated business".into();
    foreign.category = "灵感".into();
    foreign.stage = "待整理".into();
    host.create("durable-foreign-business-op", foreign).unwrap();
    let collision = import_request("durable-foreign-business-op", 1, BYTES);
    assert!(host.begin_editor_draft_import(&collision).is_err());
    assert!(host
        .inspect_editor_draft_import(CARD, DRAFT, &collision.operation_id)
        .is_err());
}

#[test]
fn selected_import_is_consumed_once_and_old_stage_id_cannot_be_reclaimed_after_removal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-selected", 1, BYTES);
    let ready = host
        .import_editor_draft_asset_durable(&request, &mut &BYTES[..])
        .unwrap();
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    let selected = AssetSelection {
        origin: 2,
        asset_id: ready.asset_id.clone(),
        aliases: vec!["local:selected".into()],
    };
    let second = draft("durable-draft-select", 1, vec![selected.clone()]);
    let saved = host.save_editor_draft(&second).unwrap();
    assert_eq!(saved.slot.generation, 2);
    assert_eq!(saved.slot.assets.len(), 1);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &ready.asset_id)
            .unwrap(),
        BYTES
    );
    let consumed = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(consumed.phase, DraftImportPhase::Retired);
    assert!(!consumed.current_active);
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 2, &request.operation_id)
        .is_err());
    assert_sealed(&mut host, &request);
    assert_eq!(business_revision(&host), 2);
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .phase,
        DraftImportPhase::Retired
    );
    assert_sealed(&mut host, &request);
    let third = draft("durable-draft-remove-selection", 2, vec![]);
    host.save_editor_draft(&third).unwrap();
    let fourth = draft("durable-draft-old-stage-claim", 3, vec![selected]);
    assert!(host.save_editor_draft(&fourth).is_err());
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        3
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn ready_import_can_be_abandoned_but_late_original_cannot_reacquire_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-abandoned", 1, BYTES);
    let ready = host
        .import_editor_draft_asset_durable(&request, &mut &BYTES[..])
        .unwrap();
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    let retired = host
        .abandon_editor_draft_import(CARD, DRAFT, 1, &request.operation_id, "durable-abandon-op")
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(!retired.current_active);
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert!(
        !host
            .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .bytes_retained
    );
    let same = host
        .abandon_editor_draft_import(CARD, DRAFT, 1, &request.operation_id, "durable-abandon-op")
        .unwrap();
    assert!(same.repeated);
    assert!(host
        .abandon_editor_draft_import(
            CARD,
            DRAFT,
            1,
            &request.operation_id,
            "durable-foreign-abandon"
        )
        .is_err());
    assert_sealed(&mut host, &request);
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 1, &request.operation_id)
        .is_err());
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .phase,
        DraftImportPhase::Retired
    );
    assert_sealed(&mut host, &request);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn different_drafts_share_physical_bytes_but_never_import_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let mut other = draft("durable-other-draft-first", 0, vec![]);
    other.draft_id = "other-draft".into();
    host.save_editor_draft(&other).unwrap();
    let first = import_request("durable-first-owner", 1, BYTES);
    let mut second = import_request("durable-second-owner", 1, BYTES);
    second.draft_id = "other-draft".into();
    let one = host
        .import_editor_draft_asset_durable(&first, &mut &BYTES[..])
        .unwrap();
    let two = host
        .import_editor_draft_asset_durable(&second, &mut &BYTES[..])
        .unwrap();
    assert_eq!(one.phase, DraftImportPhase::Ready);
    assert_eq!(two.phase, DraftImportPhase::Ready);
    assert_ne!(
        one.asset_id, two.asset_id,
        "distinct imports need distinct selection identities"
    );
    let cross_claim = draft(
        "durable-cross-draft-claim",
        1,
        vec![AssetSelection {
            origin: 2,
            asset_id: two.asset_id.clone(),
            aliases: vec![],
        }],
    );
    assert!(host.save_editor_draft(&cross_claim).is_err());
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    let physical: i64 = sql
        .query_row(
            "SELECT count(*) FROM blobs WHERE digest=?1 AND size=?2",
            rusqlite::params![sha(BYTES), BYTES.len() as i64],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(physical, 1);
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 1, &second.operation_id)
        .is_err());
    assert!(host
        .export_editor_draft_import(CARD, "other-draft", 1, &first.operation_id)
        .is_err());
    host.abandon_editor_draft_import(CARD, DRAFT, 1, &first.operation_id, "durable-first-abandon")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let retired = host
        .inspect_editor_draft_import(CARD, DRAFT, &first.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(
        !retired.bytes_retained,
        "first owner's Snapshot must be released"
    );
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 1, &first.operation_id)
        .is_err());
    drop(sql);
    drop(host);

    let mut host = Workbench::open(&path, None).unwrap();
    let retired = host
        .inspect_editor_draft_import(CARD, DRAFT, &first.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(!retired.current_active);
    assert!(!retired.bytes_retained);
    assert_sealed(&mut host, &first);
    let surviving = host
        .inspect_editor_draft_import(CARD, "other-draft", &second.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(surviving.phase, DraftImportPhase::Ready);
    assert!(surviving.bytes_retained);
    assert_eq!(
        host.export_editor_draft_import(CARD, "other-draft", 1, &second.operation_id)
            .unwrap(),
        BYTES
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn pending_imports_count_toward_per_draft_capacity_without_reading_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    for index in 0..20 {
        let bytes = [index as u8];
        let request = import_request(&format!("durable-pending-{index}"), 1, &bytes);
        let pending = host.begin_editor_draft_import(&request).unwrap();
        assert_eq!(pending.phase, DraftImportPhase::Pending);
        assert!(!pending.bytes_retained);
    }
    let overflow = import_request("durable-pending-20", 1, b"x");
    assert!(host.begin_editor_draft_import(&overflow).is_err());
    assert!(host
        .inspect_editor_draft_import(CARD, DRAFT, &overflow.operation_id)
        .unwrap()
        .is_none());
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        20
    );
    assert_eq!(business_revision(&host), 2);
    drop(host);
    let host = Workbench::open(&path, None).unwrap();
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        20
    );
}

#[cfg(feature = "fault-injection")]
#[test]
fn durable_import_child() {
    let Ok(path) = std::env::var("MORROW_DURABLE_IMPORT_CHILD_DB") else {
        return;
    };
    let source = std::env::var("MORROW_DURABLE_IMPORT_CHILD_SOURCE").unwrap();
    let mut host = Workbench::open(Path::new(&path), Some(package())).unwrap();
    let request = import_request("durable-crash-import", 1, BYTES);
    host.import_editor_draft_asset_durable(&request, &mut std::fs::File::open(source).unwrap())
        .unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn core_commit_crash_recovers_pending_into_ready_without_rereading_original() {
    use std::process::Command;
    for (point, committed) in [
        ("stage-retained-before-commit", false),
        ("stage-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workbench.db");
        let source = directory.path().join("source.bin");
        std::fs::write(&source, BYTES).unwrap();
        let mut host = seed(&path);
        active_draft(&mut host);
        let request = import_request("durable-crash-import", 1, BYTES);
        host.begin_editor_draft_import(&request).unwrap();
        drop(host);
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .arg("--exact")
            .arg("durable_import_child")
            .arg("--nocapture")
            .env("MORROW_DURABLE_IMPORT_CHILD_DB", &path)
            .env("MORROW_DURABLE_IMPORT_CHILD_SOURCE", &source)
            .env("MORROW_TEST_CRASH_AT", point);
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x08000000);
        let output = child.output().unwrap();
        assert_eq!(output.status.code(), Some(86), "{point}: {output:?}");
        let mut host = Workbench::open(&path, Some(package())).unwrap();
        let before = host
            .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(before.phase, DraftImportPhase::Pending);
        assert_eq!(
            before.bytes_retained, committed,
            "{point}: owner and blob crossed commit separately"
        );
        host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
        let after = host
            .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(after.phase, DraftImportPhase::Pending);
        if committed {
            std::fs::remove_file(&source).unwrap();
            let repeated = host
                .import_editor_draft_asset_durable(&request, &mut ThrowReader)
                .unwrap();
            assert!(repeated.repeated);
            assert_eq!(
                host.export_editor_draft_import(CARD, DRAFT, 1, &request.operation_id)
                    .unwrap(),
                BYTES
            );
        } else {
            let ready = host
                .import_editor_draft_asset_durable(
                    &request,
                    &mut std::fs::File::open(&source).unwrap(),
                )
                .unwrap();
            assert_eq!(ready.phase, DraftImportPhase::Ready);
        }
        assert_eq!(business_revision(&host), 2);
    }
}

#[test]
fn discarding_main_draft_retires_unselected_import_and_seals_old_operation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-orphan-on-discard", 1, BYTES);
    let ready = host
        .import_editor_draft_asset_durable(&request, &mut &BYTES[..])
        .unwrap();
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    assert!(ready.bytes_retained);
    let discarded = host
        .discard_editor_draft(CARD, DRAFT, 1, "durable-main-draft-discard")
        .unwrap();
    assert!(!discarded.slot.active);
    let retired = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(!retired.current_active);
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert!(
        !host
            .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .bytes_retained
    );
    assert_sealed(&mut host, &request);
    assert!(host
        .export_editor_draft_import(CARD, DRAFT, 2, &request.operation_id)
        .is_err());
    assert_eq!(business_revision(&host), 2);
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
            .unwrap()
            .unwrap()
            .phase,
        DraftImportPhase::Retired
    );
    assert_sealed(&mut host, &request);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn automatic_retire_operation_cannot_impersonate_explicit_abandon_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    active_draft(&mut host);
    let request = import_request("durable-retire-alias", 1, BYTES);
    host.import_editor_draft_asset_durable(&request, &mut &BYTES[..])
        .unwrap();
    host.discard_editor_draft(CARD, DRAFT, 1, "durable-retire-alias-main-discard")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let mut hasher = Sha256::new();
    for part in ["retire", CARD, DRAFT, request.operation_id.as_str()] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let internal = format!(
        "draft-import-stage-{}",
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert!(
        host.abandon_editor_draft_import(CARD, DRAFT, 2, &request.operation_id, &internal,)
            .is_err(),
        "automatic retire must not count as caller-authorized abandon"
    );
    let historical = host
        .inspect_editor_draft_import(CARD, DRAFT, &request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(historical.phase, DraftImportPhase::Retired);
    assert!(!historical.current_active);
    assert!(!historical.bytes_retained);
}
