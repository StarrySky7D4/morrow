#![cfg(target_os = "windows")]
mod common;
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{Package, proto::TransformHandler};
use morrow_workbench_host::{Workbench, host_capnp as wire, protocol};
use morrow_workbench_plugin::{Action, Request, capture, capture_capnp as capture_wire, codec};
use sha2::{Digest, Sha256};
fn package() -> Package {
    let p = common::package();
    let mut m = p.manifest().clone();
    m.transform_handlers.push(TransformHandler {
        handler: "capture.convert".into(),
        input_type: "morrow.capture.request.v1".into(),
        output_type: "morrow.capture.response.v1".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    });
    Package::build(m, p.module()).unwrap()
}
fn call(
    h: &mut Workbench,
    action: wire::Action,
    fill: impl FnOnce(wire::request::Builder<'_>),
) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    fill(r);
    protocol::respond(h, &serialize::write_message_to_words(&m)).unwrap()
}
fn reader(bytes: &[u8]) -> capnp::message::Reader<capnp::serialize::OwnedSegments> {
    serialize::read_message(&mut std::io::Cursor::new(bytes), Default::default()).unwrap()
}
fn error(bytes: &[u8]) -> String {
    reader(bytes)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_error()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
fn scope(h: &mut Workbench) -> String {
    let bytes = call(h, wire::Action::OpenCaptureScope, |mut r| {
        r.set_id("card");
        r.set_revision(0)
    });
    assert!(error(&bytes).is_empty(), "{}", error(&bytes));
    reader(&bytes)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_capture_scope()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
fn upload(h: &mut Workbench, operation: &str, bytes: &[u8]) -> String {
    let reply = call(h, wire::Action::BeginCaptureUpload, |mut r| {
        r.set_operation(operation);
        r.set_total_length(bytes.len() as u64);
        r.set_sha256(&Sha256::digest(bytes));
    });
    assert!(error(&reply).is_empty(), "{}", error(&reply));
    let token = reader(&reply)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_transfer()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    for (i, piece) in bytes.chunks(32768).enumerate() {
        let reply = call(h, wire::Action::AppendCaptureUpload, |mut r| {
            r.set_transfer(&token);
            r.set_offset((i * 32768) as u64);
            r.set_payload(piece);
        });
        assert!(error(&reply).is_empty(), "{}", error(&reply));
    }
    token
}
fn capture(h: &mut Workbench, scope: &str) -> String {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<capture_wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&capture::digest());
    r.set_format("plain");
    r.set_source("converted **text**");
    let raw = serialize::write_message_to_words(&m);
    let reply = call(h, wire::Action::Capture, |mut r| {
        r.set_capture_scope(scope);
        r.set_payload(&raw);
    });
    assert!(error(&reply).is_empty(), "{}", error(&reply));
    reader(&reply)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_capture_ticket()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
fn paste(scope: &str, ticket: &str, large: bool) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut u = m.init_root::<wire::paste_upload::Builder>();
    u.set_scope(scope);
    let mut e = u.init_event();
    e.set_id("paste-1");
    e.set_field("description");
    let before = if large {
        "😀".repeat(19500)
    } else {
        String::new()
    };
    e.set_before(&before);
    e.set_start_utf16(0);
    e.set_end_utf16(before.encode_utf16().count() as u32);
    e.set_after("converted **text**");
    let mut parts = e.init_parts(1);
    let mut p = parts.reborrow().get(0);
    p.set_ticket(ticket);
    p.set_selection("outputMarkdown");
    serialize::write_message_to_words(&m)
}
fn save(scope: &str) -> Vec<u8> {
    let idea = common::idea("card");
    let mut m = Builder::new_default();
    let mut s = m.init_root::<wire::captured_save::Builder>();
    s.set_scope(scope);
    s.set_operation("create");
    s.set_target("card");
    s.set_revision(0);
    let req = Request {
        action: Action::Create,
        current: Default::default(),
        proposed: idea.clone(),
        text: String::new(),
        flag: false,
        now_ms: 0,
        ideas: vec![],
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    };
    s.set_payload(&codec::encode_request(&req).unwrap());
    let mut snapshot = s.init_snapshot();
    snapshot.set_title(&idea.title);
    snapshot.set_description(&idea.description);
    serialize::write_message_to_words(&m)
}
#[test]
fn captured_editor_metadata_streams_then_commits_and_retries_without_live_scope() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let scope = scope(&mut h);
    let ticket = capture(&mut h, &scope);
    let metadata = paste(&scope, &ticket, true);
    assert!(metadata.len() > 32768);
    let token = upload(&mut h, "paste-1", &metadata);
    let reply = call(&mut h, wire::Action::FinishPaste, |mut r| {
        r.set_transfer(&token)
    });
    assert!(error(&reply).is_empty(), "{}", error(&reply));
    let raw = save(&scope);
    let token = upload(&mut h, "create", &raw);
    let reply = call(&mut h, wire::Action::FinishCapturedSave, |mut r| {
        r.set_transfer(&token)
    });
    assert!(error(&reply).is_empty(), "{}", error(&reply));
    assert_eq!(h.read("card").unwrap().revision, 1);
    let original = h.operation_evidence("card", "create").unwrap().remove(0);
    assert_eq!(
        original.data().batch.as_ref().unwrap().observations.len(),
        2
    );
    let token = upload(&mut h, "create", &raw);
    let reply = call(&mut h, wire::Action::FinishCapturedSave, |mut r| {
        r.set_transfer(&token)
    });
    assert!(error(&reply).is_empty(), "{}", error(&reply));
    assert_eq!(
        h.operation_evidence("card", "create").unwrap()[0].digest(),
        original.digest()
    );
}
#[test]
fn mismatched_upload_identity_and_trailing_metadata_fail_without_publishing_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let scope = scope(&mut h);
    let ticket = capture(&mut h, &scope);
    let mut bytes = paste(&scope, &ticket, false);
    let token = upload(&mut h, "different-event", &bytes);
    let reply = call(&mut h, wire::Action::FinishPaste, |mut r| {
        r.set_transfer(&token)
    });
    assert!(!error(&reply).is_empty());
    bytes.extend([0; 8]);
    let token = upload(&mut h, "paste-1", &bytes);
    let reply = call(&mut h, wire::Action::FinishPaste, |mut r| {
        r.set_transfer(&token)
    });
    assert!(!error(&reply).is_empty());
    assert!(h.read("card").is_err());
    let reply = call(&mut h, wire::Action::Capture, |mut r| {
        r.set_capture_parent("orphan")
    });
    assert!(!error(&reply).is_empty());
}
