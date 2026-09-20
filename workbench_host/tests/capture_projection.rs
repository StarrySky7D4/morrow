#![cfg(target_os = "windows")]
mod common;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::{
    plugin_package::{Package, proto::TransformHandler},
    task::Invocation,
    task_evidence::{self, Evidence},
    transaction,
};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_host::{
    Mutation, Workbench,
    capture_provenance::{AttachmentAlias, EditorSnapshot, PasteEvent, PastePart},
    projection,
};
use morrow_workbench_plugin::{Action, Idea, capture, capture_capnp as wire};
use std::path::Path;
fn package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.transform_handlers.push(TransformHandler {
        handler: "capture.convert".into(),
        input_type: "morrow.capture.request.v1".into(),
        output_type: "morrow.capture.response.v1".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    });
    Package::build(manifest, original.module()).unwrap()
}
fn request(format: &str, source: &str) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut b = m.init_root::<wire::request::Builder>();
    b.set_version(1);
    b.set_digest(&capture::digest());
    b.set_format(format);
    b.set_source(source);
    serialize::write_message_to_words(&m)
}
fn markdown(payload: &[u8]) -> String {
    let m =
        serialize::read_message(&mut std::io::Cursor::new(payload), ReaderOptions::new()).unwrap();
    let r = m.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(r.get_version(), 1);
    assert_eq!(r.get_digest().unwrap(), capture::digest());
    r.get_markdown().unwrap().to_str().unwrap().into()
}
fn snapshot(draft: &Idea) -> EditorSnapshot {
    EditorSnapshot {
        title: draft.title.clone(),
        description: draft.description.clone(),
        hypothesis: draft.hypothesis.clone(),
        conclusion: draft.conclusion.clone(),
        todos: draft.todos.join("\n"),
        aliases: vec![],
    }
}
fn part(ticket: &str) -> PastePart {
    PastePart {
        ticket: ticket.into(),
        literal: String::new(),
        selection: "outputMarkdown".into(),
    }
}
fn literal(s: &str) -> PastePart {
    PastePart {
        ticket: String::new(),
        literal: s.into(),
        selection: String::new(),
    }
}
fn paste(
    id: &str,
    before: &str,
    start: u32,
    end: u32,
    parts: Vec<PastePart>,
    after: &str,
) -> PasteEvent {
    PasteEvent {
        id: id.into(),
        field: "description".into(),
        before: before.into(),
        start_utf16: start,
        end_utf16: end,
        parts,
        after: after.into(),
    }
}
fn original(h: &Workbench, op: &str) -> Evidence {
    let mut items = h.operation_evidence("card", op).unwrap();
    assert_eq!(items.len(), 1);
    items.remove(0)
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
fn verify(e: &Evidence, path: &Path, op: &str, observations: usize) {
    let b = e.data().batch.as_ref().unwrap();
    assert_eq!(b.observations.len(), observations);
    assert_eq!(
        Invocation::decode(&b.observations.last().unwrap().invocation)
            .unwrap()
            .transform()
            .unwrap()
            .handler,
        "workbench.command"
    );
    assert!(b.observations[..observations - 1].iter().all(|o| {
        Invocation::decode(&o.invocation)
            .unwrap()
            .transform()
            .unwrap()
            .handler
            == "capture.convert"
    }));
    assert_eq!(
        projection::verify_commit(&commit(path, op), e)
            .unwrap()
            .card
            .summary()
            .id,
        "card"
    );
    assert!(
        replay::replay_batch(e, Limits::default(), b.total_fuel)
            .unwrap()
            .matches
    );
}
fn configure(h: &mut Workbench, enabled: bool) {
    let s = h.plugin_status().unwrap();
    h.configure_plugin(s.revision, &s.digest, enabled).unwrap();
}
#[test]
fn actual_capture_mixed_parts_utf16_selection_and_later_manual_rewrite_are_distinct_facts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (ticket, payload) = h
        .capture_scoped(&scope, request("plain", "pasted"), "")
        .unwrap();
    assert_eq!(markdown(&payload), "pasted");
    h.record_paste(
        &scope,
        paste(
            "paste",
            "A😀中B",
            1,
            3,
            vec![literal("["), part(&ticket), literal("]")],
            "A[pasted]中B",
        ),
    )
    .unwrap();
    let mut draft = common::idea("card");
    draft.description = "用户随后重写为完全不同的正文。".into();
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    let e = original(&h, "create");
    verify(&e, &path, "create", 2);
    assert_eq!(projection::derive(&e).unwrap().card.summary().revision, 1);
    assert_eq!(h.read("card").unwrap().idea.description, draft.description);
    assert!(
        h.create_captured("new", draft.clone(), &scope, snapshot(&draft))
            .is_err()
    );
    assert_eq!(
        h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
            .unwrap()
            .revision,
        1
    );
}
#[test]
fn seventeen_distinct_pastes_form_one_batch_and_replay_after_source_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let mut text = String::new();
    for i in 0..17 {
        let value = format!("paste-{i};");
        let (ticket, payload) = h
            .capture_scoped(&scope, request("plain", &value), "")
            .unwrap();
        assert_eq!(markdown(&payload), value);
        let next = format!("{text}{value}");
        let end = text.encode_utf16().count() as u32;
        h.record_paste(
            &scope,
            paste(
                &format!("event-{i}"),
                &text,
                end,
                end,
                vec![part(&ticket)],
                &next,
            ),
        )
        .unwrap();
        text = next;
    }
    let mut draft = common::idea("card");
    draft.description = text;
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    let e = original(&h, "create");
    verify(&e, &path, "create", 18);
    let c = commit(&path, "create");
    h.finish().unwrap();
    drop(h);
    dir.close().unwrap();
    assert!(!path.exists());
    assert!(
        replay::replay_batch(
            &e,
            Limits::default(),
            e.data().batch.as_ref().unwrap().total_fuel
        )
        .unwrap()
        .matches
    );
    projection::verify_commit(&c, &e).unwrap();
}
#[test]
fn selected_child_keeps_parent_once_and_discards_unused_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (parent, payload) = h
        .capture_scoped(&scope, request("plain", "a\tb\n1\t2"), "")
        .unwrap();
    let first = markdown(&payload);
    assert!(first.contains("| --- | --- |"));
    let (child, payload) = h
        .capture_scoped(&scope, request("plain", &first), &parent)
        .unwrap();
    let selected = markdown(&payload);
    let _unused = h
        .capture_scoped(&scope, request("plain", "not selected"), "")
        .unwrap();
    let after = format!("{selected}\n{selected}");
    h.record_paste(
        &scope,
        paste(
            "twice",
            "",
            0,
            0,
            vec![part(&child), literal("\n"), part(&child)],
            &after,
        ),
    )
    .unwrap();
    let mut draft = common::idea("card");
    draft.description = after;
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    let e = original(&h, "create");
    verify(&e, &path, "create", 3);
}
#[test]
fn cross_scope_target_host_and_parent_substitution_are_rejected_without_poisoning_owner() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    let a = h.open_capture_scope("card", 0).unwrap();
    let b = h.open_capture_scope("other", 0).unwrap();
    let same = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h.capture_scoped(&a, request("plain", "owned"), "").unwrap();
    for foreign in [&b, &same] {
        assert!(
            h.record_paste(
                foreign,
                paste("cross", "", 0, 0, vec![part(&ticket)], "owned")
            )
            .is_err()
        );
        assert!(
            h.capture_scoped(foreign, request("plain", "owned"), &ticket)
                .is_err()
        );
    }
    assert!(
        h.capture_scoped(&a, request("plain", "different input"), &ticket)
            .is_err()
    );
    let foreign_dir = tempfile::tempdir().unwrap();
    let mut foreign = Workbench::open(&foreign_dir.path().join("db"), Some(package())).unwrap();
    assert!(
        foreign
            .capture_scoped(&a, request("plain", "owned"), "")
            .is_err()
    );
    h.record_paste(&a, paste("valid", "", 0, 0, vec![part(&ticket)], "owned"))
        .unwrap();
    let mut draft = common::idea("card");
    draft.description = "owned".into();
    assert!(
        h.create_captured(
            "wrong",
            common::idea("other"),
            &a,
            snapshot(&common::idea("other"))
        )
        .is_err()
    );
    h.create_captured("create", draft.clone(), &a, snapshot(&draft))
        .unwrap();
}
#[test]
fn invalid_surrogate_ranges_fields_after_text_and_repeated_event_id_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h.capture_scoped(&scope, request("plain", "x"), "").unwrap();
    for (start, end) in [(2, 2), (3, 1), (0, 99)] {
        assert!(
            h.record_paste(
                &scope,
                paste("bad", "A😀B", start, end, vec![part(&ticket)], "AxB")
            )
            .is_err()
        );
    }
    assert!(
        h.record_paste(
            &scope,
            paste("bad-after", "A😀B", 1, 3, vec![part(&ticket)], "wrong")
        )
        .is_err()
    );
    let mut wrong = paste("bad-field", "", 0, 0, vec![part(&ticket)], "x");
    wrong.field = "arbitrary".into();
    assert!(h.record_paste(&scope, wrong).is_err());
    h.record_paste(&scope, paste("once", "", 0, 0, vec![part(&ticket)], "x"))
        .unwrap();
    assert!(
        h.record_paste(&scope, paste("once", "x", 1, 1, vec![part(&ticket)], "xx"))
            .is_err()
    );
    let mut draft = common::idea("card");
    draft.description = "x".into();
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
}
#[test]
fn editor_cancel_disable_reenable_and_reopen_never_revive_old_tickets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let closed = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h
        .capture_scoped(&closed, request("plain", "old"), "")
        .unwrap();
    h.close_capture_scope(&closed).unwrap();
    assert!(
        h.record_paste(&closed, paste("late", "", 0, 0, vec![part(&ticket)], "old"))
            .is_err()
    );
    let stopped = h.open_capture_scope("card", 0).unwrap();
    h.capture_scoped(&stopped, request("plain", "old"), "")
        .unwrap();
    configure(&mut h, false);
    configure(&mut h, true);
    assert!(
        h.capture_scoped(&stopped, request("plain", "old"), "")
            .is_err()
    );
    let old = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h.capture_scoped(&old, request("plain", "old"), "").unwrap();
    h.finish().unwrap();
    drop(h);
    let original_package = package();
    let mut manifest = original_package.manifest().clone();
    manifest.package_version = "0.1.9-test.99".into();
    let upgraded = Package::build(manifest, original_package.module()).unwrap();
    let mut h = Workbench::open(&path, Some(upgraded)).unwrap();
    assert!(!h.writable());
    configure(&mut h, true);
    let fresh = h.open_capture_scope("card", 0).unwrap();
    assert!(
        h.record_paste(&fresh, paste("late", "", 0, 0, vec![part(&ticket)], "old"))
            .is_err()
    );
    assert!(h.capture_scoped(&old, request("plain", "old"), "").is_err());
    assert!(h.operation_evidence("card", "cancelled").is_err());
}

#[test]
fn edit_capture_history_retries_after_reopen_without_covering_newer_content_or_reviving_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    h.create("create", common::idea("card")).unwrap();
    let scope = h.open_capture_scope("card", 1).unwrap();
    let (ticket, _) = h
        .capture_scoped(&scope, request("plain", "inserted"), "")
        .unwrap();
    let before = h.read("card").unwrap().idea.description;
    h.record_paste(
        &scope,
        paste(
            "replace",
            &before,
            0,
            before.encode_utf16().count() as u32,
            vec![part(&ticket)],
            "inserted",
        ),
    )
    .unwrap();
    let mut draft = h.read("card").unwrap().idea;
    draft.description = "inserted and then rewritten".into();
    let edit = |proposed: Idea| Mutation {
        operation: "edit",
        id: "card",
        revision: 1,
        action: Action::Edit,
        proposed: Some(proposed),
        text: "",
        flag: false,
    };
    h.apply_captured(edit(draft.clone()), &scope, snapshot(&draft))
        .unwrap();
    let e = original(&h, "edit");
    verify(&e, &path, "edit", 2);
    h.apply(Mutation {
        operation: "favorite",
        id: "card",
        revision: 2,
        action: Action::Favorite,
        proposed: None,
        text: "",
        flag: true,
    })
    .unwrap();
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        h.apply_captured(edit(draft.clone()), &scope, snapshot(&draft))
            .unwrap()
            .revision,
        2
    );
    assert_eq!(h.read("card").unwrap().revision, 3);
    assert!(h.read("card").unwrap().idea.favorite);
    assert_eq!(original(&h, "edit").container(), e.container());
    let mut changed = draft.clone();
    changed.description.push('!');
    assert!(
        h.apply_captured(edit(changed.clone()), &scope, snapshot(&changed))
            .is_err()
    );
    let mut new = edit(draft.clone());
    new.operation = "new";
    assert!(h.apply_captured(new, &scope, snapshot(&draft)).is_err());
    configure(&mut h, false);
    assert!(
        h.apply_captured(edit(draft.clone()), &scope, snapshot(&draft))
            .is_err()
    );
    assert!(projection::derive(&e).is_ok());
}
#[test]
fn missing_reordered_duplicated_capture_observations_or_changed_application_are_rejected() {
    use prost::Message;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (a, _) = h
        .capture_scoped(&scope, request("plain", "first"), "")
        .unwrap();
    let (b, _) = h
        .capture_scoped(&scope, request("plain", "second"), "")
        .unwrap();
    h.record_paste(
        &scope,
        paste(
            "both",
            "",
            0,
            0,
            vec![part(&a), literal("/"), part(&b)],
            "first/second",
        ),
    )
    .unwrap();
    let mut draft = common::idea("card");
    draft.description = "first/second".into();
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    let e = original(&h, "create");
    let c = commit(&path, "create");
    verify(&e, &path, "create", 3);
    for case in 0..4 {
        let mut d = e.data().clone();
        let o = &mut d.batch.as_mut().unwrap().observations;
        match case {
            0 => {
                o.remove(0);
            }
            1 => o.swap(0, 1),
            2 => o.insert(0, o[0].clone()),
            _ => {
                o[0] = o[1].clone();
            }
        }
        let altered = task_evidence::encode(d).unwrap();
        assert!(
            projection::derive(&altered).is_err(),
            "observation substitution {case}"
        );
        assert!(projection::verify_commit(&c, &altered).is_err());
    }
    for case in 0..5 {
        let mut d = e.data().clone();
        let batch = d.batch.as_mut().unwrap();
        let mut facts =
            projection::proto::ContentProjectionV2::decode(batch.intent.as_slice()).unwrap();
        match case {
            0 => facts.applications[0].after = "other".into(),
            1 => facts.applications[0].parts[0].observation = Some(99),
            2 => facts.applications[0].parts.swap(0, 2),
            3 => facts.snapshot.as_mut().unwrap().description = "unrelated final draft".into(),
            _ => facts.captures[0].parent = Some(1),
        }
        batch.intent = facts.encode_to_vec();
        let altered = task_evidence::encode(d).unwrap();
        assert!(
            projection::derive(&altered).is_err(),
            "application substitution {case}"
        );
    }
}
fn tree_request(format: &str, nodes: &[capture::Node]) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut b = m.init_root::<wire::request::Builder>();
    b.set_version(1);
    b.set_digest(&capture::digest());
    b.set_format(format);
    let mut list = b.init_nodes(nodes.len() as u32);
    for (i, n) in nodes.iter().enumerate() {
        let mut node = list.reborrow().get(i as u32);
        node.set_parent(n.parent as u16);
        node.set_tag(n.tag.as_str());
        node.set_text(n.text.as_str());
        node.set_href(n.href.as_str());
        node.set_src(n.src.as_str());
        node.set_formula(n.formula.as_str());
    }
    serialize::write_message_to_words(&m)
}
fn node(parent: usize, tag: &str, text: &str) -> capture::Node {
    capture::Node {
        parent,
        tag: tag.into(),
        text: text.into(),
        ..Default::default()
    }
}
#[test]
fn actual_html_rtf_spreadsheet_and_plain_selection_convert_only_inert_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let html = tree_request(
        "html",
        &[
            node(0, "root", ""),
            node(0, "strong", ""),
            node(1, "text", "hello"),
            node(0, "script", ""),
            node(3, "text", "never execute"),
        ],
    );
    let mut cell = node(2, "Cell", "");
    cell.formula = "=SUM(1,41)".into();
    let sheet = tree_request(
        "spreadsheet",
        &[
            node(0, "root", ""),
            node(0, "Table", ""),
            node(1, "Row", ""),
            cell,
            node(3, "Data", "42"),
        ],
    );
    let mut parts = vec![];
    let mut rendered = vec![];
    for (input, expected) in [
        (html, "**hello**"),
        (request("rtf", r"{\rtf1\ansi \u20013?\u25991?}"), "中文"),
        (sheet, "42 (公式: =SUM(1,41))"),
    ] {
        let (ticket, payload) = h.capture_scoped(&scope, input, "").unwrap();
        let text = markdown(&payload);
        assert!(text.contains(expected), "{text:?}");
        assert!(!text.contains("never execute"));
        if !parts.is_empty() {
            parts.push(literal("\n"));
        }
        parts.push(part(&ticket));
        rendered.push(text);
    }
    let source = "a\tb\n1\t2";
    let (ticket, payload) = h
        .capture_scoped(&scope, request("plain", source), "")
        .unwrap();
    assert_ne!(markdown(&payload), source);
    parts.push(literal("\n"));
    parts.push(PastePart {
        ticket,
        literal: String::new(),
        selection: "inputPlainText".into(),
    });
    rendered.push(source.into());
    let after = rendered.join("\n");
    h.record_paste(&scope, paste("formats", "", 0, 0, parts, &after))
        .unwrap();
    let mut draft = common::idea("card");
    draft.description = after;
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    verify(&original(&h, "create"), &path, "create", 5);
}
#[test]
fn scope_capacity_and_failed_conversion_leave_existing_valid_editor_usable() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
    let scopes: Vec<_> = (0..8)
        .map(|i| h.open_capture_scope(&format!("draft-{i}"), 0).unwrap())
        .collect();
    assert!(h.open_capture_scope("overflow", 0).is_err());
    h.close_capture_scope(&scopes[0]).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h
        .capture_scoped(&scope, request("plain", "valid"), "")
        .unwrap();
    assert!(
        h.capture_scoped(&scope, request("unsupported", "bad"), "")
            .is_err()
    );
    assert!(h.writable());
    h.record_paste(
        &scope,
        paste("valid", "", 0, 0, vec![part(&ticket)], "valid"),
    )
    .unwrap();
    let mut draft = common::idea("card");
    draft.description = "valid".into();
    h.create_captured("create", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
}

#[test]
fn pasted_attachment_alias_projects_to_selected_persistent_blob_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let content = b"synthetic attachment bytes";
    let asset = h
        .import(
            "card",
            "pasted file.bin",
            "file",
            &mut &content[..],
            content.len() as u64,
        )
        .unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let mut image = node(0, "img", "");
    image.src = "attachment:pasted%20file.bin".into();
    let input = tree_request("html", &[node(0, "root", ""), image]);
    let (ticket, payload) = h.capture_scoped(&scope, input, "").unwrap();
    let rendered = markdown(&payload);
    assert!(rendered.contains("attachment:pasted%20file.bin"));
    h.record_paste(
        &scope,
        paste("image", "", 0, 0, vec![part(&ticket)], &rendered),
    )
    .unwrap();
    let mut draft = common::idea("card");
    draft.assets.push(asset.clone());
    draft.description = rendered.replace(
        "attachment:pasted%20file.bin",
        &format!("attachment:{}", asset.id),
    );
    let mut endpoint = snapshot(&draft);
    endpoint.description = rendered;
    endpoint.aliases.push(AttachmentAlias {
        id: asset.id.clone(),
        location: "synthetic/pasted file.bin".into(),
        name: asset.name.clone(),
    });
    let mut wrong = endpoint.clone();
    wrong.aliases[0].id = "not-selected".into();
    assert!(
        h.create_captured("wrong-alias", draft.clone(), &scope, wrong)
            .is_err()
    );
    h.create_captured("create", draft.clone(), &scope, endpoint)
        .unwrap();
    verify(&original(&h, "create"), &path, "create", 2);
    let mut exported = vec![];
    h.export("card", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, content);
}

#[test]
fn failed_sql_statement_and_actual_commit_unknown_freeze_original_scope_until_retry() {
    for deferred in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut h = Workbench::open(&path, Some(package())).unwrap();
        let scope = h.open_capture_scope("card", 0).unwrap();
        let (ticket, payload) = h
            .capture_scoped(&scope, request("plain", "retained conversion"), "")
            .unwrap();
        h.record_paste(
            &scope,
            paste(
                "paste",
                "",
                0,
                0,
                vec![part(&ticket)],
                "retained conversion",
            ),
        )
        .unwrap();
        let mut draft = common::idea("card");
        draft.description = "retained conversion".into();
        let sql = rusqlite::Connection::open(&path).unwrap();
        if deferred {
            sql.execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY); CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER review_capture_failure AFTER INSERT ON outbox BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99); END;").unwrap();
        } else {
            sql.execute_batch("CREATE TRIGGER review_capture_failure BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'synthetic statement failure'); END;").unwrap();
        }
        let failure = h
            .create_captured("save", draft.clone(), &scope, snapshot(&draft))
            .err()
            .expect("synthetic storage failure");
        if deferred {
            assert!(
                failure.to_string().contains("CommitUnknown"),
                "expected actual deferred-FK commit failure, got {failure}"
            );
        }
        for table in [
            "cards",
            "operations",
            "outbox",
            "task_evidence",
            "operation_evidence",
            "evidence_chunks",
            "task_evidence_chunks",
        ] {
            let count: i64 = sql
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0, "partial content/evidence in {table}");
        }
        assert!(h.writable());
        assert!(h.operation_evidence("card", "save").is_err());
        assert!(
            h.create_captured("different-op", draft.clone(), &scope, snapshot(&draft))
                .is_err()
        );
        let mut changed = draft.clone();
        changed.description = "new intent".into();
        assert!(
            h.create_captured("save", changed.clone(), &scope, snapshot(&changed))
                .is_err()
        );
        assert!(
            h.capture_scoped(&scope, request("plain", "late"), "")
                .is_err()
        );
        assert!(
            h.record_paste(
                &scope,
                paste("late", "", 0, 0, vec![part(&ticket)], "retained conversion")
            )
            .is_err()
        );
        sql.execute_batch("DROP TRIGGER review_capture_failure;")
            .unwrap();
        if deferred {
            sql.execute_batch("DROP TABLE review_deferred; DROP TABLE review_parent;")
                .unwrap();
        }
        drop(sql);
        assert_eq!(
            h.create_captured("save", draft.clone(), &scope, snapshot(&draft))
                .unwrap()
                .revision,
            1
        );
        let e = original(&h, "save");
        verify(&e, &path, "save", 2);
        let observation = &e.data().batch.as_ref().unwrap().observations[0];
        let task = Invocation::decode(&observation.invocation).unwrap();
        assert_eq!(
            task.transform().unwrap().input,
            request("plain", "retained conversion")
        );
        assert_eq!(
            task.verify_output(&observation.completion).unwrap().bytes,
            payload
        );
        assert_eq!(
            h.create_captured("save", draft.clone(), &scope, snapshot(&draft))
                .unwrap()
                .revision,
            1
        );
        assert_eq!(original(&h, "save").container(), e.container());
        h.finish().unwrap();
        drop(h);
        let mut h = Workbench::open(&path, Some(package())).unwrap();
        assert_eq!(
            h.create_captured("save", draft.clone(), &scope, snapshot(&draft))
                .unwrap()
                .revision,
            1
        );
        assert_eq!(original(&h, "save").container(), e.container());
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn captured_commit_crash_child() {
    let Ok(root) = std::env::var("MORROW_CAPTURE_CRASH_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mut h = Workbench::open(&root.join("db"), Some(package())).unwrap();
    let scope = h.open_capture_scope("card", 0).unwrap();
    let (ticket, _) = h
        .capture_scoped(&scope, request("plain", "lost reply"), "")
        .unwrap();
    h.record_paste(
        &scope,
        paste("paste", "", 0, 0, vec![part(&ticket)], "lost reply"),
    )
    .unwrap();
    std::fs::write(root.join("scope.txt"), &scope).unwrap();
    let mut draft = common::idea("card");
    draft.description = "lost reply".into();
    let _ = h
        .create_captured("save", draft.clone(), &scope, snapshot(&draft))
        .unwrap();
    panic!("expected injected process termination");
}
#[cfg(feature = "fault-injection")]
#[test]
fn after_commit_process_loss_retries_original_capture_batch_without_live_scope() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "captured_commit_crash_child", "--nocapture"])
        .env("MORROW_CAPTURE_CRASH_ROOT", dir.path())
        .env("MORROW_TEST_CRASH_AT", "after-commit")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe owned child: {e}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned child timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(86));
    let path = dir.path().join("db");
    let scope = std::fs::read_to_string(dir.path().join("scope.txt")).unwrap();
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let before = original(&h, "save");
    verify(&before, &path, "save", 2);
    let mut draft = common::idea("card");
    draft.description = "lost reply".into();
    assert_eq!(
        h.create_captured("save", draft.clone(), &scope, snapshot(&draft))
            .unwrap()
            .revision,
        1
    );
    assert_eq!(original(&h, "save").container(), before.container());
    assert_eq!(original(&h, "save").digest(), before.digest());
    assert!(
        h.create_captured("new", draft.clone(), &scope, snapshot(&draft))
            .is_err()
    );
    configure(&mut h, false);
    assert!(
        h.create_captured("save", draft.clone(), &scope, snapshot(&draft))
            .is_err()
    );
}
