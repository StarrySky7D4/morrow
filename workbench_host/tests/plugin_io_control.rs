#![cfg(target_os = "windows")]
//! Trusted host category controls. These tests create new metadata fixtures from
//! an existing task module; frozen compatibility archives remain untouched.
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{
    Package, catalog,
    io::{self, IoCapability},
};
use morrow_workbench_host::{Workbench, host_capnp as wire, plugin_catalog::PluginEntry, protocol};
use std::path::{Path, PathBuf};

const NAMES: [&str; 10] = [
    "file-read",
    "file-list",
    "file-create",
    "file-replace",
    "file-delete",
    "http-request",
    "http-listen",
    "http-publish",
    "credential-use",
    "websocket-connect",
];
fn fixture(stem: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(format!("sdk/compat/guest-v1-rc1/{stem}.mplugin"))
}
fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn entry(w: &Workbench, id: &str) -> PluginEntry {
    let mut page = w.catalog_page("", None).unwrap();
    loop {
        if let Some(index) = page.entries.iter().position(|e| e.id == id) {
            return page.entries.remove(index);
        }
        assert!(!page.cursor.is_empty());
        page = w.catalog_page(&page.cursor, Some(page.revision)).unwrap();
    }
}
fn install(w: &mut Workbench, dir: &Path, caps: Vec<IoCapability>) -> PluginEntry {
    let base = catalog::read_file(&fixture("rust-task")).unwrap();
    let mut m = base.manifest().clone();
    m.package_id = "org.example.host-io-control".into();
    m.required_features.push(io::FEATURE.into());
    m.io_declaration = Some(io::declaration(caps, vec!["api.invoke".into()]));
    let package = Package::build(m, base.module()).unwrap();
    let path = dir.join("incoming.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(w))
        .unwrap();
    entry(w, &package.manifest().package_id)
}
fn names() -> Vec<String> {
    NAMES.iter().map(|v| (*v).into()).collect()
}
fn all() -> Vec<IoCapability> {
    (1..=10)
        .map(|n| IoCapability::from_number(n).unwrap())
        .collect()
}
fn approve(w: &mut Workbench, e: &PluginEntry, values: &[String]) {
    w.configure_external_io(&e.id, &e.digest, revision(w), values)
        .unwrap();
}
#[test]
fn independent_approval_persists_without_enabling_or_changing_content_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = install(&mut w, dir.path(), all());
    assert_eq!(e.declared_io, names());
    assert!(e.approved_io.is_empty());
    approve(&mut w, &e, &names());
    let current = entry(&w, &e.id);
    assert!(!current.enabled);
    assert!(current.approved.is_empty());
    assert_eq!(current.approved_io, names());
    w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, true)
        .unwrap();
    approve(&mut w, &e, &["http-request".into()]);
    let current = entry(&w, &e.id);
    assert!(current.enabled);
    assert_eq!(current.approved, e.declared);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let current = entry(&w, &e.id);
    assert!(current.enabled);
    assert_eq!(current.approved, e.declared);
    assert_eq!(current.approved_io, ["http-request"]);
    approve(&mut w, &e, &[]);
    assert!(entry(&w, &e.id).approved_io.is_empty());
    assert!(entry(&w, &e.id).enabled);
    assert_eq!(entry(&w, &e.id).approved, e.declared);
}
#[test]
fn malformed_stale_and_undeclared_decisions_leave_selection_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = install(&mut w, dir.path(), vec![IoCapability::HttpRequest]);
    approve(&mut w, &e, &["http-request".into()]);
    let r = revision(&w);
    for values in [
        vec!["HttpRequest".into()],
        vec!["http-request".into(); 2],
        vec!["file-read".into()],
        vec!["http-request".into(); 11],
        vec!["rename".into()],
    ] {
        assert!(
            w.configure_external_io(&e.id, &e.digest, r, &values)
                .is_err()
        );
        assert_eq!(revision(&w), r);
        assert_eq!(entry(&w, &e.id).approved_io, ["http-request"]);
    }
    for digest in [vec![0; 32], vec![], vec![0; 33]] {
        assert!(w.configure_external_io(&e.id, &digest, r, &[]).is_err());
    }
    assert!(
        w.configure_external_io(&e.id, &e.digest, r - 1, &[])
            .is_err()
    );
    assert!(
        w.configure_external_io("org.morrow.workbench", &e.digest, r, &[])
            .is_err()
    );
    assert_eq!(revision(&w), r);
    assert_eq!(entry(&w, &e.id).approved_io, ["http-request"]);
}
#[test]
fn unavailable_package_keeps_visible_approval_and_allows_only_empty_clear() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = install(&mut w, dir.path(), all());
    approve(&mut w, &e, &["http-request".into()]);
    let name: String = e.digest.iter().map(|b| format!("{b:02x}")).collect();
    let path = dir
        .path()
        .join(format!("plugin-manager/packages/{name}.mplugin"));
    std::fs::remove_file(&path).unwrap();
    let missing = entry(&w, &e.id);
    assert!(!missing.available);
    assert!(missing.declared_io.is_empty());
    assert_eq!(missing.approved_io, ["http-request"]);
    let r = revision(&w);
    assert!(
        w.configure_external_io(&e.id, &e.digest, r, &["file-read".into()])
            .is_err()
    );
    assert_eq!(revision(&w), r);
    approve(&mut w, &e, &[]);
    assert!(entry(&w, &e.id).approved_io.is_empty());
    assert!(!path.exists());
    w.finish().unwrap();
    drop(w);
    let w = Workbench::open_managed(dir.path(), None).unwrap();
    // Existing startup policy rejects any missing selected package. Clearing a
    // ceiling does not silently relax that complete-registry validation.
    assert!(w.catalog_page("", None).is_err());
    drop(w);
    std::fs::copy(dir.path().join("incoming.mplugin"), &path).unwrap();
    let w = Workbench::open_managed(dir.path(), None).unwrap();
    assert!(entry(&w, &e.id).approved_io.is_empty());
}
fn wire_request(
    action: wire::Action,
    e: &PluginEntry,
    revision: u64,
    values: &[String],
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    r.set_id(e.id.as_str());
    r.set_sha256(&e.digest);
    r.set_revision(revision);
    let mut approved = r.init_approved_io_capabilities(values.len() as u32);
    for (i, value) in values.iter().enumerate() {
        approved.set(i as u32, value.as_str());
    }
    serialize::write_message_to_words(&message)
}
#[test]
fn private_wire_approves_distinct_io_vector_and_returns_both_catalog_namespaces() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = install(&mut w, dir.path(), all());
    let request = wire_request(wire::Action::PluginApproveIo, &e, revision(&w), &names());
    let bytes = protocol::respond(&mut w, &request).unwrap();
    let msg = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    assert!(
        msg.get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    let request = wire_request(wire::Action::PluginCatalog, &e, revision(&w), &[]);
    let bytes = protocol::respond(&mut w, &request).unwrap();
    let msg = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    let response = msg.get_root::<wire::response::Reader>().unwrap();
    let row = response.get_plugins().unwrap().get(0);
    assert_eq!(
        row.get_declared_io()
            .unwrap()
            .iter()
            .map(|s| s.unwrap().to_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        names()
    );
    assert_eq!(row.get_approved_io().unwrap().len(), 10);
    assert_eq!(row.get_declared().unwrap().len(), 7);
    assert_eq!(row.get_approved().unwrap().len(), 0);
    assert!(!row.get_enabled());
    let r = revision(&w);
    let request = wire_request(
        wire::Action::PluginApproveIo,
        &e,
        r,
        &vec!["http-request".into(); 11],
    );
    let bytes = protocol::respond(&mut w, &request).unwrap();
    let msg = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    assert!(
        !msg.get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    assert_eq!(revision(&w), r);
}
#[test]
fn invalid_io_expansion_does_not_revoke_an_actual_non_io_ui_session() {
    use morrow_core::ui::{Event, EventKind};
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let path = fixture("rust-ui");
    let p = w.inspect_plugin(&path).unwrap();
    let e = &p.entries[0];
    w.import_plugin(&path, &e.digest, p.revision).unwrap();
    w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, true)
        .unwrap();
    let reply = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "seed")
        .unwrap();
    assert!(
        w.configure_external_io(&e.id, &e.digest, revision(&w), &["http-request".into()])
            .is_err()
    );
    let event = Event {
        view: reply.view.clone(),
        generation: reply.generation,
        revision: reply.revision,
        serial: reply.serial + 1,
        node: "title".into(),
        action: "title.edit".into(),
        kind: EventKind::EditText,
        text: "still alive".into(),
        checked: false,
    }
    .encode()
    .unwrap();
    assert!(
        w.external_ui_event(&e.id, reply.generation, &event)
            .unwrap()
            .failure
            .is_none()
    );
}
