#![cfg(target_os = "windows")]
mod common;
use morrow_core::{
    content::CardRecord,
    task_evidence::{self, Evidence},
    transaction,
};
use morrow_workbench_host::{Mutation, Workbench, projection};
use morrow_workbench_plugin::Action;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Output},
};

fn hex(value: &[u8]) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}
fn run(dir: &Path, commit: &[u8], evidence: &Evidence, bad_pin: bool) -> Output {
    let cp = dir.join("commit.bin");
    let ep = dir.join("evidence.bin");
    std::fs::write(&cp, commit).unwrap();
    std::fs::write(&ep, evidence.container()).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_morrow-content-replay"));
    cmd.arg(cp)
        .arg(if bad_pin {
            "0".repeat(64)
        } else {
            hex(&Sha256::digest(commit))
        })
        .arg(ep)
        .arg(hex(&evidence.digest()));
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000);
    cmd.output().unwrap()
}
fn fixture() -> (tempfile::TempDir, Vec<(Vec<u8>, Evidence)>) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let mut h = Workbench::open_managed(&source, Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let a = h.operation_evidence("card", "create").unwrap().remove(0);
    h.apply(Mutation {
        operation: "favorite",
        id: "card",
        revision: 1,
        action: Action::Favorite,
        proposed: None,
        text: "",
        flag: true,
    })
    .unwrap();
    let b = h.operation_evidence("card", "favorite").unwrap().remove(0);
    h.finish().unwrap();
    drop(h);
    let sql = rusqlite::Connection::open_with_flags(
        source.join("workbench.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut result = Vec::new();
    for (op, e) in [("create", a), ("favorite", b)] {
        let raw: Vec<u8> = sql
            .query_row("SELECT payload FROM operations WHERE id=?1", [op], |r| {
                r.get(0)
            })
            .unwrap();
        result.push((raw, e));
    }
    drop(sql);
    // All files were created by this fixture; no library path comes from user input.
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    assert!(!source.exists());
    (dir, result)
}
#[test]
fn actual_create_and_edit_replay_after_source_database_and_credentials_are_deleted() {
    let (dir, items) = fixture();
    for (commit, evidence) in items {
        let result = run(dir.path(), &commit, &evidence, false);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).starts_with("MATCH:"));
    }
}
#[test]
fn wrong_external_pin_and_self_consistent_different_content_are_refused() {
    let (dir, items) = fixture();
    let (commit, evidence) = &items[0];
    assert_eq!(
        run(dir.path(), commit, evidence, true).status.code(),
        Some(1)
    );
    let p = projection::derive(evidence).unwrap();
    let card = CardRecord::new(
        "card",
        "org.morrow.idea",
        1,
        "different result",
        p.card.body(),
    )
    .unwrap();
    let alternate = transaction::encode_commit_with_evidence(
        transaction::create_command("create", &card).unwrap(),
        &card,
        &[evidence.digest()],
    )
    .unwrap();
    let result = run(dir.path(), &alternate, evidence, false);
    assert_eq!(result.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("MATCH:"));
}
#[test]
fn internally_bound_but_incorrect_execution_observation_returns_mismatch() {
    let (dir, items) = fixture();
    let (_, evidence) = &items[0];
    let mut data = evidence.data().clone();
    let observation = &mut data.batch.as_mut().unwrap().observations[0];
    assert!(observation.fuel_remaining > 0);
    observation.fuel_remaining -= 1;
    let altered = task_evidence::encode(data).unwrap();
    let p = projection::derive(&altered).unwrap();
    let commit =
        transaction::encode_commit_with_evidence(p.command, &p.card, &[altered.digest()]).unwrap();
    let result = run(dir.path(), &commit, &altered, false);
    assert_eq!(
        result.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).starts_with("MISMATCH:"));
}

#[test]
fn actual_captured_edit_replays_offline_with_parent_chain() {
    use capnp::{
        message::{Builder, ReaderOptions},
        serialize,
    };
    use morrow_core::plugin_package::{Package, proto::TransformHandler};
    use morrow_workbench_host::capture_provenance::{EditorSnapshot, PasteEvent, PastePart};
    use morrow_workbench_plugin::{capture, capture_capnp as wire};
    fn request(format: &str, source: &str) -> Vec<u8> {
        let mut m = Builder::new_default();
        let mut r = m.init_root::<wire::request::Builder>();
        r.set_version(1);
        r.set_digest(&capture::digest());
        r.set_format(format);
        r.set_source(source);
        serialize::write_message_to_words(&m)
    }
    fn markdown(payload: &[u8]) -> String {
        let m = serialize::read_message(&mut std::io::Cursor::new(payload), ReaderOptions::new())
            .unwrap();
        m.get_root::<wire::response::Reader>()
            .unwrap()
            .get_markdown()
            .unwrap()
            .to_str()
            .unwrap()
            .into()
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture-source");
    std::fs::create_dir(&source).unwrap();
    let p = common::package();
    let mut manifest = p.manifest().clone();
    manifest.transform_handlers.push(TransformHandler {
        handler: "capture.convert".into(),
        input_type: "morrow.capture.request.v1".into(),
        output_type: "morrow.capture.response.v1".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    });
    let p = Package::build(manifest, p.module()).unwrap();
    let mut h = Workbench::open_managed(&source, Some(p)).unwrap();
    let mut draft = common::idea("card");
    h.create("initial", draft.clone()).unwrap();
    let scope = h.open_capture_scope("card", 1).unwrap();
    let (parent, payload) = h
        .capture_scoped(&scope, request("rtf", r"{\rtf1 hello\par world}"), "")
        .unwrap();
    let (ticket, payload) = h
        .capture_scoped(&scope, request("plain", &markdown(&payload)), &parent)
        .unwrap();
    let text = markdown(&payload);
    h.record_paste(
        &scope,
        PasteEvent {
            id: "adopt-chain".into(),
            field: "description".into(),
            before: draft.description.clone(),
            start_utf16: 0,
            end_utf16: draft.description.encode_utf16().count() as u32,
            parts: vec![PastePart {
                ticket,
                literal: String::new(),
                selection: "outputMarkdown".into(),
            }],
            after: text.clone(),
        },
    )
    .unwrap();
    draft.description = format!("{text}\n后续编辑");
    let snapshot = EditorSnapshot {
        title: draft.title.clone(),
        description: draft.description.clone(),
        hypothesis: draft.hypothesis.clone(),
        conclusion: draft.conclusion.clone(),
        todos: draft.todos.join("\n"),
        aliases: vec![],
    };
    h.apply_captured(
        Mutation {
            operation: "captured-edit",
            id: "card",
            revision: 1,
            action: Action::Edit,
            proposed: Some(draft),
            text: "",
            flag: false,
        },
        &scope,
        snapshot,
    )
    .unwrap();
    let evidence = h
        .operation_evidence("card", "captured-edit")
        .unwrap()
        .remove(0);
    assert_eq!(
        evidence.data().batch.as_ref().unwrap().observations.len(),
        3
    );
    h.finish().unwrap();
    drop(h);
    let sql = rusqlite::Connection::open_with_flags(
        source.join("workbench.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let commit: Vec<u8> = sql
        .query_row(
            "SELECT payload FROM operations WHERE id='captured-edit'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(sql);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "capture-source");
    std::fs::remove_dir_all(&source).unwrap();
    let result = run(dir.path(), &commit, &evidence, false);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).starts_with("MATCH:"));
    assert_eq!(
        run(dir.path(), &commit, &evidence, true).status.code(),
        Some(1)
    );
    let mut data = evidence.data().clone();
    data.batch.as_mut().unwrap().observations[0].fuel_remaining -= 1;
    let altered = task_evidence::encode(data).unwrap();
    let p = projection::derive(&altered).unwrap();
    let rebound =
        transaction::encode_commit_with_evidence(p.command, &p.card, &[altered.digest()]).unwrap();
    let result = run(dir.path(), &rebound, &altered, false);
    assert_eq!(
        result.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
