#![cfg(target_os = "windows")]
mod common;
use common::{idea, package};
use morrow_core::plugin_package::{Package, proto::Capability};
use morrow_workbench_host::{Mutation, Workbench, host_capnp as wire, protocol};
use std::fs;
fn request(action: wire::Action, revision: u64, digest: &[u8], enable: bool) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    r.set_revision(revision);
    r.set_sha256(digest);
    r.set_limit(u32::from(enable));
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn failed_disable_stops_connection_and_reports_read_only_until_explicit_reenable() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open(&dir.path().join("workbench.db"), Some(package())).unwrap();
    host.create("seed", idea("saved")).unwrap();
    let before = host.plugin_status();
    assert!(host.writable());
    let path = dir.path().join("plugin-manager/state/selection.morrow");
    let saved = dir.path().join("plugin-manager/state/saved");
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, &saved).unwrap();
    fs::create_dir(&path).unwrap();
    let response = protocol::respond(
        &mut host,
        &request(
            wire::Action::PluginConfigure,
            before.revision,
            &before.digest,
            false,
        ),
    )
    .unwrap();
    let message = capnp::serialize::read_message(&mut &response[..], Default::default()).unwrap();
    let r = message.get_root::<wire::response::Reader>().unwrap();
    assert!(!r.get_error().unwrap().to_str().unwrap().is_empty());
    assert!(r.get_read_only());
    assert!(!host.writable());
    assert!(host.create("blocked", idea("blocked")).is_err());
    assert_eq!(host.read("saved").unwrap().revision, 1);
    assert!(host.read("blocked").is_err());
    let state = protocol::respond(
        &mut host,
        &request(wire::Action::PluginState, 0, &[], false),
    )
    .unwrap();
    let message = capnp::serialize::read_message(&mut &state[..], Default::default()).unwrap();
    let r = message.get_root::<wire::response::Reader>().unwrap();
    // Persisted intent remains enabled, while actual write capability is truthfully false.
    assert!(r.get_plugin_enabled());
    assert!(r.get_plugin_approved());
    assert!(r.get_read_only());
    assert_eq!(r.get_revision(), before.revision);
    fs::remove_dir(&path).unwrap();
    fs::rename(saved, &path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    host.refresh_plugin_state().unwrap();
    assert!(!host.writable()); // No implicit reconnect.
    host.configure_plugin(before.revision, &before.digest, true)
        .unwrap();
    assert!(host.writable());
    host.create("after-reenable", idea("after-reenable"))
        .unwrap();
    assert_eq!(host.read("saved").unwrap().revision, 1);
    host.finish().unwrap();
}
#[test]
fn incomplete_approval_is_read_only_for_every_persistent_write_route() {
    let dir = tempfile::tempdir().unwrap();
    let original = package();
    let mut manifest = original.manifest().clone();
    manifest.requested_capabilities = vec![Capability::CreateContent as i32];
    let partial = Package::build(manifest, original.module()).unwrap();
    let mut host = Workbench::open(&dir.path().join("workbench.db"), Some(partial)).unwrap();
    assert!(!host.writable());
    assert!(!host.plugin_status().approved);
    assert!(
        host.create("blocked", idea("blocked"))
            .err()
            .unwrap()
            .to_string()
            .contains("只读")
    );
    assert!(
        host.apply(Mutation {
            operation: "edit",
            id: "missing",
            revision: 1,
            action: morrow_workbench_plugin::Action::Edit,
            proposed: Some(idea("missing")),
            text: "",
            flag: false
        })
        .err()
        .unwrap()
        .to_string()
        .contains("只读")
    );
    assert!(
        host.save_preferences("preferences", vec![])
            .err()
            .unwrap()
            .to_string()
            .contains("只读")
    );
    assert!(
        host.import(
            "blocked",
            "name",
            "file",
            &mut std::io::Cursor::new(Vec::<u8>::new()),
            0
        )
        .err()
        .unwrap()
        .to_string()
        .contains("只读")
    );
    assert!(host.read("blocked").is_err());
    assert!(host.read_preferences().unwrap().is_none());
    let response = protocol::respond(
        &mut host,
        &request(wire::Action::PluginState, 0, &[], false),
    )
    .unwrap();
    let message = capnp::serialize::read_message(&mut &response[..], Default::default()).unwrap();
    assert!(
        message
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_read_only()
    );
    host.finish().unwrap();
}
