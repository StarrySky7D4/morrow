#![cfg(target_os = "windows")]
mod common;
use morrow_audit::session::{OpenMode, Session};
use morrow_core::{
    content::CardRecord,
    lifecycle::GrantKind,
    task::Invocation,
    task_evidence::{self, Evidence, proto::TaskEvidence},
    transaction,
};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_host::{Mutation, Workbench, projection};
use morrow_workbench_plugin::{Action, Idea, codec, persistence};
use sha2::{Digest, Sha256};
use std::path::Path;
fn mutation<'a>(
    op: &'a str,
    rev: u64,
    action: Action,
    text: &'a str,
    flag: bool,
    proposed: Option<Idea>,
) -> Mutation<'a> {
    Mutation {
        operation: op,
        id: "card",
        revision: rev,
        action,
        proposed,
        text,
        flag,
    }
}
fn original(h: &Workbench, op: &str) -> Evidence {
    let mut e = h.operation_evidence("card", op).unwrap();
    assert_eq!(e.len(), 1);
    e.remove(0)
}
fn commit(path: &Path, op: &str) -> transaction::proto::Commit {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let raw: Vec<u8> = sql
        .query_row("SELECT payload FROM operations WHERE id=?1", [op], |r| {
            r.get(0)
        })
        .unwrap();
    transaction::decode_commit(&raw).unwrap().0
}
fn stored_card(path: &Path) -> CardRecord {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let raw: Vec<u8> = sql
        .query_row("SELECT payload FROM cards WHERE id='card'", [], |r| {
            r.get(0)
        })
        .unwrap();
    morrow_core::envelope::decode(&raw).unwrap()
}
fn checked(h: &Workbench, path: &Path, op: &str) -> Evidence {
    let e = original(h, op);
    assert_eq!(e.data().schema_version, 2);
    let batch = e.data().batch.as_ref().unwrap();
    assert_eq!(batch.intent_type, "morrow.workbench.content-projection.v1");
    assert_eq!(batch.observations.len(), 1);
    let c = commit(path, op);
    let p = projection::verify_commit(&c, &e).unwrap();
    assert_eq!(p.command, c.command);
    assert_eq!(Sha256::digest(p.card.encode()).as_slice(), c.content_sha256);
    assert_eq!(p.card.encode(), stored_card(path).encode());
    e
}
fn configure(h: &mut Workbench, on: bool) {
    let s = h.plugin_status().unwrap();
    h.configure_plugin(s.revision, &s.digest, on).unwrap();
}
fn varint(mut n: usize) -> Vec<u8> {
    let mut b = vec![];
    while n >= 128 {
        b.push(n as u8 | 128);
        n >>= 7;
    }
    b.push(n as u8);
    b
}
fn unknown(mut raw: Vec<u8>, payload: &[u8]) -> Vec<u8> {
    raw.extend([0xa2, 0x06]);
    raw.extend(varint(payload.len()));
    raw.extend(payload);
    raw
}
fn seed(path: &Path, card: &CardRecord, evidence: &[Evidence]) {
    let mut s = Session::open(path, Default::default(), OpenMode::Initialize).unwrap();
    let h = s.runtime();
    let mut c = h.connect().unwrap();
    h.grant(&mut c, GrantKind::CreateContent, "card", 100, 0)
        .unwrap();
    h.create_content_with_evidence(&c, "create", card, evidence, || 1)
        .unwrap();
    h.disconnect(&c).unwrap();
    s.flush(16).unwrap();
}
#[test]
fn every_writable_action_derives_exact_stored_command_and_result_then_replays_without_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    let mut idea = common::idea("card");
    idea.todos = vec!["one".into(), "two".into()];
    h.create("create", idea.clone()).unwrap();
    let mut evidence = vec![checked(&h, &path, "create")];
    let mut rev = 1;
    for (op, action, text, flag) in [
        ("favorite", Action::Favorite, "", true),
        ("todo", Action::Todo, "one", true),
        ("stage", Action::Stage, "已整理", false),
        ("project", Action::ToProject, "", false),
        ("done", Action::Stage, "已完成", false),
    ] {
        rev = h
            .apply(mutation(op, rev, action, text, flag, None))
            .unwrap()
            .revision;
        let e = checked(&h, &path, op);
        assert_eq!(
            projection::derive(&e).unwrap().original_request.action,
            action
        );
        evidence.push(e);
    }
    idea = h.read("card").unwrap().idea;
    idea.description = "汉".repeat(6000);
    rev = h
        .apply(mutation("edit", rev, Action::Edit, "", false, Some(idea)))
        .unwrap()
        .revision;
    let edited = checked(&h, &path, "edit");
    let projected = projection::derive(&edited).unwrap();
    assert_eq!(projected.card.summary().preview_text, "汉".repeat(5461));
    assert!(projected.original_request.current.description.is_empty());
    assert!(projected.original_request.current.hypothesis.is_empty());
    assert!(projected.original_request.current.conclusion.is_empty());
    evidence.push(edited);
    rev = h
        .apply(mutation("delete", rev, Action::Delete, "", false, None))
        .unwrap()
        .revision;
    evidence.push(checked(&h, &path, "delete"));
    h.apply(mutation("restore", rev, Action::Restore, "", false, None))
        .unwrap();
    evidence.push(checked(&h, &path, "restore"));
    let commits: Vec<_> = [
        "create", "favorite", "todo", "stage", "project", "done", "edit", "delete", "restore",
    ]
    .iter()
    .map(|op| commit(&path, op))
    .collect();
    h.finish().unwrap();
    drop(h);
    dir.close().unwrap();
    assert!(!path.exists());
    for (e, c) in evidence.iter().zip(commits) {
        let batch = e.data().batch.as_ref().unwrap();
        assert!(
            replay::replay_batch(e, Limits::default(), batch.total_fuel)
                .unwrap()
                .matches
        );
        assert_eq!(projection::verify_commit(&c, e).unwrap().command, c.command);
    }
}
#[test]
fn attachment_digest_and_metadata_are_part_of_exact_projection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    let bytes = b"actual synthetic attachment\0\xff";
    let a = h
        .import(
            "card",
            "sample.bin",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut draft = common::idea("card");
    draft.assets.push(a.clone());
    h.create("create", draft).unwrap();
    let e = checked(&h, &path, "create");
    let derived = projection::derive(&e).unwrap();
    let attachments = derived.card.attachments();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].id, a.id);
    assert_eq!(attachments[0].display_name, a.name);
    assert_eq!(attachments[0].byte_length, bytes.len() as u64);
    assert_eq!(
        attachments[0].sha256,
        <[u8; 32]>::from(Sha256::digest(bytes))
    );
    h.apply(mutation("favorite", 1, Action::Favorite, "", true, None))
        .unwrap();
    let edited = checked(&h, &path, "favorite");
    assert_eq!(
        projection::derive(&edited).unwrap().card.attachments(),
        attachments
    );
    let mut proposed = h.read("card").unwrap().idea;
    proposed.assets.clear();
    h.apply(mutation(
        "remove",
        2,
        Action::Edit,
        "",
        false,
        Some(proposed),
    ))
    .unwrap();
    assert!(
        projection::derive(&checked(&h, &path, "remove"))
            .unwrap()
            .card
            .attachments()
            .is_empty()
    );
}
#[test]
fn near_eight_mib_unknown_outer_prior_survives_actual_guest_edit_and_exact_projection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let idea = common::idea("card");
    let body = unknown(
        persistence::encode(&idea, None).unwrap(),
        b"unknown inner properties",
    );
    let card = CardRecord::new("card", "org.morrow.idea", 1, &idea.title, body).unwrap();
    let mut noise = vec![0; 8 * 1024 * 1024 - 8192];
    let mut x = 0x91234567u32;
    for b in &mut noise {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *b = x as u8;
    }
    let bytes = unknown(card.encode(), &noise);
    let prior = CardRecord::decode(&bytes).unwrap();
    seed(&path, &prior, &[]);
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    let mut proposed = idea;
    proposed.description = "edited after large prior".into();
    h.apply(mutation("edit", 1, Action::Edit, "", false, Some(proposed)))
        .unwrap();
    let e = checked(&h, &path, "edit");
    assert!(e.data().batch.as_ref().unwrap().intent.len() > 7 * 1024 * 1024);
    let facts: projection::proto::HostProjection =
        prost::Message::decode(e.data().batch.as_ref().unwrap().intent.as_slice()).unwrap();
    assert_eq!(facts.prior, prior.encode());
    let result = projection::derive(&e).unwrap().card;
    assert!(result.encode().windows(noise.len()).any(|w| w == noise));
    assert!(
        result
            .body()
            .windows(24)
            .any(|w| w == b"unknown inner properties")
    );
    assert_eq!(result.summary().revision, 2);
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    assert!(h.writable());
    h.finish().unwrap();
}
#[test]
fn old_projection_retry_is_historical_and_never_restores_live_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    let draft = common::idea("card");
    h.create("create", draft.clone()).unwrap();
    let first = checked(&h, &path, "create");
    let mut proposed = draft.clone();
    proposed.description = "original edit".into();
    h.apply(mutation(
        "edit",
        1,
        Action::Edit,
        "",
        false,
        Some(proposed.clone()),
    ))
    .unwrap();
    let edit = checked(&h, &path, "edit");
    h.apply(mutation("favorite", 2, Action::Favorite, "", true, None))
        .unwrap();
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    assert_eq!(h.create("create", draft).unwrap().revision, 1);
    assert_eq!(
        h.apply(mutation(
            "edit",
            1,
            Action::Edit,
            "",
            false,
            Some(proposed.clone())
        ))
        .unwrap()
        .revision,
        2
    );
    assert_eq!(h.read("card").unwrap().revision, 3);
    assert!(h.read("card").unwrap().idea.favorite);
    assert_eq!(original(&h, "create").container(), first.container());
    assert_eq!(original(&h, "edit").container(), edit.container());
    assert!(
        h.apply(mutation(
            "different",
            1,
            Action::Edit,
            "",
            false,
            Some(proposed.clone())
        ))
        .is_err()
    );
    configure(&mut h, false);
    assert!(
        h.apply(mutation("edit", 1, Action::Edit, "", false, Some(proposed)))
            .is_err()
    );
    assert!(projection::derive(&edit).is_ok());
    assert_eq!(h.read("card").unwrap().revision, 3);
}
#[test]
fn actual_legacy_single_observation_retry_remains_supported_without_projection_claim() {
    let source = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&source.path().join("db"), Some(common::package())).unwrap();
    let draft = common::idea("card");
    h.create("create", draft.clone()).unwrap();
    let batch = original(&h, "create");
    let card = projection::derive(&batch).unwrap().card;
    let o = &batch.data().batch.as_ref().unwrap().observations[0];
    let legacy = task_evidence::encode(TaskEvidence {
        schema_version: 1,
        package_archive: batch.data().package_archive.clone(),
        invocation: o.invocation.clone(),
        budget: o.budget,
        backend: o.backend.clone(),
        completion: o.completion.clone(),
        fault: o.fault,
        exit_code: o.exit_code,
        observed_host_calls: o.observed_host_calls,
        fuel_remaining: o.fuel_remaining,
        batch: None,
    })
    .unwrap();
    assert!(replay::replay(&legacy, Limits::default()).unwrap().matches);
    assert!(projection::derive(&legacy).is_err());
    h.finish().unwrap();
    drop(h);
    let dest = tempfile::tempdir().unwrap();
    let path = dest.path().join("db");
    seed(&path, &card, std::slice::from_ref(&legacy));
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    assert_eq!(h.create("create", draft).unwrap().revision, 1);
    assert_eq!(original(&h, "create").container(), legacy.container());
    h.apply(mutation("favorite", 1, Action::Favorite, "", true, None))
        .unwrap();
    assert_eq!(original(&h, "favorite").data().schema_version, 2);
    configure(&mut h, false);
    assert!(h.create("create", common::idea("card")).is_err());
}

fn changed_intent(e: &Evidence, intent: Vec<u8>) -> Evidence {
    let mut data = e.data().clone();
    data.batch.as_mut().unwrap().intent = intent;
    task_evidence::encode(data).unwrap()
}
#[test]
fn contradictory_host_facts_and_modified_commit_or_result_hash_fail_verification() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let created = original(&h, "create");
    let prior = projection::derive(&created).unwrap().card;
    h.apply(mutation("favorite", 1, Action::Favorite, "", true, None))
        .unwrap();
    let e = original(&h, "favorite");
    let original_commit = commit(&path, "favorite");
    let req = projection::derive(&e).unwrap().original_request;
    let wrong_time =
        projection::prepare_intent("favorite", "card", Some(&prior), &[], req.now_ms + 1, None)
            .unwrap();
    assert!(projection::derive(&changed_intent(&e, wrong_time)).is_err());
    let wrong_prior = CardRecord::new(
        "card",
        "org.morrow.idea",
        1,
        "different prior",
        prior.body(),
    )
    .unwrap();
    let wrong = projection::prepare_intent(
        "favorite",
        "card",
        Some(&wrong_prior),
        &[],
        req.now_ms,
        None,
    )
    .unwrap();
    assert!(projection::derive(&changed_intent(&e, wrong)).is_err());
    for case in 0..7 {
        let mut c = original_commit.clone();
        match case {
            0 => c.command[0] ^= 1,
            1 => c.command_sha256[0] ^= 1,
            2 => c.content_sha256[0] ^= 1,
            3 => c.revision += 1,
            4 => c.operation_id = "other".into(),
            5 => c.task_evidence_sha256[0][0] ^= 1,
            _ => c.card_id = "other".into(),
        };
        assert!(
            projection::verify_commit(&c, &e).is_err(),
            "modified commit case {case}"
        );
    }
    // A newly pinned, internally valid Output is not the original observed output.
    let mut d = e.data().clone();
    let o = &mut d.batch.as_mut().unwrap().observations[0];
    let i = Invocation::decode(&o.invocation).unwrap();
    let mut output =
        codec::decode_response(&i.verify_output(&o.completion).unwrap().bytes).unwrap();
    output.idea.title = "substituted output".into();
    o.completion = i
        .output_completion(&codec::encode_response(&output).unwrap())
        .unwrap();
    let altered = task_evidence::encode(d).unwrap();
    assert!(projection::verify_commit(&original_commit, &altered).is_err());
    assert!(
        !replay::replay_batch(
            &altered,
            Limits::default(),
            altered.data().batch.as_ref().unwrap().total_fuel
        )
        .unwrap()
        .matches
    );
}
#[test]
fn restore_projection_requires_original_prior_clock_and_unexpired_host_undo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    h.apply(mutation("delete", 1, Action::Delete, "", false, None))
        .unwrap();
    let deleted = projection::derive(&original(&h, "delete")).unwrap().card;
    h.apply(mutation("restore", 2, Action::Restore, "", false, None))
        .unwrap();
    let e = checked(&h, &path, "restore");
    let request = projection::derive(&e).unwrap().original_request;
    assert!(request.current.deleted);
    for undo in [
        None,
        Some(projection::Undo {
            revision: 1,
            deadline: request.now_ms + 8000,
        }),
        Some(projection::Undo {
            revision: 2,
            deadline: request.now_ms,
        }),
    ] {
        let intent = projection::prepare_intent(
            "restore",
            "card",
            Some(&deleted),
            &[],
            request.now_ms,
            undo,
        );
        match intent {
            Ok(intent) => assert!(projection::derive(&changed_intent(&e, intent)).is_err()),
            Err(_) => assert!(undo.is_some()),
        }
    }
    // Rebind a syntactically valid completion to the exact 8-second boundary; this is deliberate tampering, not fresh execution.
    let mut d = e.data().clone();
    let b = d.batch.as_mut().unwrap();
    let o = &mut b.observations[0];
    let old = Invocation::decode(&o.invocation).unwrap();
    let output = old.verify_output(&o.completion).unwrap();
    let mut t = old.transform().unwrap().clone();
    let mut late = request;
    late.now_ms = late.current.deleted_at + 8000;
    t.input = codec::encode_request(&late).unwrap();
    let new = Invocation::new_transform("late-restore", t).unwrap();
    o.invocation = new.bytes().to_vec();
    o.completion = new.output_completion(&output.bytes).unwrap();
    b.intent = projection::prepare_intent(
        "restore",
        "card",
        Some(&deleted),
        &[],
        late.now_ms,
        Some(projection::Undo {
            revision: 2,
            deadline: late.now_ms + 1,
        }),
    )
    .unwrap();
    let changed = task_evidence::encode(d).unwrap();
    assert!(projection::derive(&changed).is_err());
    h.apply(mutation("delete-again", 3, Action::Delete, "", false, None))
        .unwrap();
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    assert!(
        h.apply(mutation("new-restore", 4, Action::Restore, "", false, None))
            .is_err()
    );
    assert_eq!(
        h.apply(mutation("restore", 2, Action::Restore, "", false, None))
            .unwrap()
            .revision,
        3
    );
    assert!(h.read("card").unwrap().idea.deleted);
    assert_eq!(h.read("card").unwrap().revision, 4);
}

#[test]
fn attachment_fact_substitution_and_duplicate_projection_fields_cannot_match_original_commit() {
    use prost::Message;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(common::package())).unwrap();
    let bytes = b"attachment";
    let asset = h
        .import(
            "card",
            "sample.bin",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut draft = common::idea("card");
    draft.assets.push(asset);
    h.create("create", draft).unwrap();
    let e = original(&h, "create");
    let original = commit(&path, "create");
    let intent = &e.data().batch.as_ref().unwrap().intent;
    let facts = projection::proto::HostProjection::decode(intent.as_slice()).unwrap();
    for case in 0..6 {
        let mut changed = facts.clone();
        match case {
            0 => changed.attachments[0].sha256[0] ^= 1,
            1 => changed.attachments[0].byte_length += 1,
            2 => changed.attachments[0].display_name = "wrong.bin".into(),
            3 => changed.target_id = "other".into(),
            4 => changed.schema_version = 2,
            _ => changed.attachments.push(changed.attachments[0].clone()),
        };
        let altered = changed_intent(&e, changed.encode_to_vec());
        assert!(
            projection::verify_commit(&original, &altered).is_err(),
            "case {case}"
        );
        if case != 0 {
            assert!(
                projection::derive(&altered).is_err(),
                "internal contradiction {case}"
            );
        }
    }
    let mut duplicate = intent.clone();
    duplicate.extend([8, 1]);
    assert!(projection::derive(&changed_intent(&e, duplicate)).is_err());
    // Unknown optional metadata is retained and does not redefine projection v1; its different evidence digest cannot replace the original reference.
    let extended = changed_intent(&e, unknown(intent.clone(), b"future optional metadata"));
    assert_eq!(
        projection::derive(&extended).unwrap().command,
        projection::derive(&e).unwrap().command
    );
    assert!(projection::verify_commit(&original, &extended).is_err());
}
