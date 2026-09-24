#![cfg(windows)]
mod common;
use morrow_core::plugin_package::{Package, proto::TransformHandler};
use morrow_workbench_host::Workbench;
use morrow_workbench_plugin::preferences::{self as p, Preferences, proto::Appearance};
use sha2::{Digest, Sha256};
fn package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.transform_handlers.push(TransformHandler {
        handler: "studio.preferences".into(),
        input_type: "morrow.studio.preferences.v1".into(),
        output_type: "morrow.studio.preferences.v1".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    });
    Package::build(manifest, original.module()).unwrap()
}
fn config(theme: &str) -> Vec<u8> {
    p::encode_wire(&Preferences {
        version: 1,
        appearance: Some(Appearance {
            theme: theme.into(),
            glass: "frosted".into(),
            background: "transparent".into(),
            opacity: 0.76,
            corner_radius: 20.,
            window_radius: 20.,
            component_opacity: 0.76,
            component_blur: 22.,
            material_version: 1,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap()
}
fn enabled(h: &mut Workbench, value: bool) {
    let s = h.plugin_status().unwrap();
    h.configure_plugin(s.revision, &s.digest, value).unwrap();
}
#[test]
fn committed_proposal_survives_reopen_without_reexecution_and_ack_is_bound() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let bytes = config("dark");
    let digest = Sha256::digest(&bytes);
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    h.submit_preferences("original", bytes.clone()).unwrap();
    assert!(
        h.submit_preferences("replacement", config("white"))
            .is_err()
    );
    assert!(h.acknowledge_preferences("other", &digest).is_err());
    assert!(h.acknowledge_preferences("original", &[0; 32]).is_err());
    drop(h);
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        h.pending_preferences().unwrap(),
        Some(("original".into(), bytes.clone()))
    );
    assert_eq!(h.read_preferences().unwrap(), Some(bytes.clone()));
    assert_eq!(
        h.operation_evidence("morrow-studio-preferences", "original")
            .unwrap()
            .len(),
        0
    );
    h.submit_preferences("original", bytes.clone()).unwrap();
    h.acknowledge_preferences("original", &digest).unwrap();
    h.acknowledge_preferences("original", &digest).unwrap();
    assert!(h.pending_preferences().unwrap().is_none());
    drop(h);
    let h = Workbench::open(&path, Some(package())).unwrap();
    assert!(h.pending_preferences().unwrap().is_none());
    assert_eq!(h.read_preferences().unwrap(), Some(bytes));
}
#[test]
fn unexecuted_noop_proposal_is_not_replayed_on_reopen_and_cannot_rebase() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let bytes = config("dark");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    h.save_preferences("baseline", bytes.clone()).unwrap();
    enabled(&mut h, false);
    // An unchanged value leaves no business receipt, but the proposal remains durable.
    h.submit_preferences("original", bytes.clone()).unwrap();
    assert!(
        h.operation_evidence("morrow-studio-preferences", "original")
            .is_err()
    );
    drop(h);
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        h.pending_preferences().unwrap(),
        Some(("original".into(), bytes.clone()))
    );
    assert_eq!(h.read_preferences().unwrap(), Some(bytes.clone()));
    enabled(&mut h, true);
    h.save_preferences("other-writer", config("white")).unwrap();
    assert!(h.submit_preferences("original", bytes.clone()).is_err());
    assert!(
        h.acknowledge_preferences("original", &Sha256::digest(&bytes))
            .is_err()
    );
    assert_eq!(h.read_preferences().unwrap(), Some(config("white")));
    assert_eq!(
        h.pending_preferences().unwrap(),
        Some(("original".into(), bytes))
    );
}
#[test]
fn invalid_proposal_leaves_no_journal_and_noop_ack_is_supported() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open(&dir.path().join("store.db"), Some(package())).unwrap();
    assert!(h.submit_preferences("invalid", vec![0; 16]).is_err());
    assert!(h.pending_preferences().unwrap().is_none());
    let bytes = config("dark");
    h.save_preferences("baseline", bytes.clone()).unwrap();
    h.submit_preferences("same-value", bytes.clone()).unwrap();
    h.acknowledge_preferences("same-value", &Sha256::digest(&bytes))
        .unwrap();
    assert!(h.pending_preferences().unwrap().is_none());
}

#[test]
fn explicit_abandonment_reserves_original_id_across_slot_reuse_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    let dark = config("dark");
    h.save_preferences("baseline", dark.clone()).unwrap();
    enabled(&mut h, false);
    h.submit_preferences("abandoned", dark.clone()).unwrap();
    enabled(&mut h, true);
    h.save_preferences("other-writer", config("white")).unwrap();
    assert!(
        h.abandon_preferences("wrong", &Sha256::digest(&dark))
            .is_err()
    );
    assert!(h.abandon_preferences("abandoned", &[0; 32]).is_err());
    // Exercise the actual private request lane, not only the direct helper.
    let mut message = capnp::message::Builder::new_default();
    {
        let mut request =
            message.init_root::<morrow_workbench_host::host_capnp::request::Builder>();
        request.set_version(1);
        request.set_digest(&morrow_workbench_host::protocol::digest());
        request.set_action(morrow_workbench_host::host_capnp::Action::AbandonPreferences);
        request.set_operation("abandoned");
        request.set_sha256(&Sha256::digest(&dark));
    }
    let reply = morrow_workbench_host::protocol::respond(
        &mut h,
        &capnp::serialize::write_message_to_words(&message),
    )
    .unwrap();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut reply.as_slice(), Default::default())
            .unwrap();
    let response = message
        .get_root::<morrow_workbench_host::host_capnp::response::Reader>()
        .unwrap();
    assert!(response.get_error().unwrap().is_empty());
    h.abandon_preferences("abandoned", &Sha256::digest(&dark))
        .unwrap();
    assert!(h.pending_preferences().unwrap().is_none());
    assert!(
        h.acknowledge_preferences("abandoned", &Sha256::digest(&dark))
            .is_err()
    );
    assert_eq!(h.read_preferences().unwrap(), Some(config("white")));
    // Reuse the bounded slot, then issue a delayed call for the abandoned ID.
    h.submit_preferences("new-proposal", dark.clone()).unwrap();
    assert!(
        h.abandon_preferences("new-proposal", &Sha256::digest(&dark))
            .is_err()
    );
    assert!(
        h.abandon_preferences("abandoned", &Sha256::digest(&dark))
            .is_err()
    );
    h.acknowledge_preferences("new-proposal", &Sha256::digest(&dark))
        .unwrap();
    drop(h);
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    assert!(h.submit_preferences("abandoned", dark).is_err());
    assert!(h.pending_preferences().unwrap().is_none());
    assert_eq!(h.read_preferences().unwrap(), Some(config("dark")));
}

#[test]
fn private_settings_save_without_plugin_has_no_guest_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let dark = config("dark");
    let mut h = Workbench::open(&path, None).unwrap();
    assert!(!h.writable(), "content commands still require the plugin");
    assert!(h.save_preferences("guest-route", dark.clone()).is_err());
    assert_eq!(h.submit_preferences("local", dark.clone()).unwrap(), dark);
    assert_eq!(h.read_preferences().unwrap(), Some(dark.clone()));
    assert!(h.pending_preferences().unwrap().is_some());
    assert!(
        h.operation_evidence("morrow-studio-preferences", "local")
            .unwrap()
            .is_empty()
    );
    h.acknowledge_preferences("local", &Sha256::digest(&dark))
        .unwrap();
    drop(h);
    let mut h = Workbench::open(&path, None).unwrap();
    assert_eq!(h.read_preferences().unwrap(), Some(dark.clone()));
    assert!(h.pending_preferences().unwrap().is_none());
    assert!(h.save_preferences("guest-route", dark).is_err());
}

#[test]
fn disabled_plugin_keeps_private_settings_writable_and_content_gated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    enabled(&mut h, false);
    let dark = config("dark");
    assert!(!h.writable());
    assert!(h.save_preferences("guest-route", dark.clone()).is_err());
    assert!(h.create("card", common::idea("card")).is_err());
    h.submit_preferences("local", dark.clone()).unwrap();
    assert!(
        h.operation_evidence("morrow-studio-preferences", "local")
            .unwrap()
            .is_empty()
    );
    h.acknowledge_preferences("local", &Sha256::digest(&dark))
        .unwrap();
    drop(h);
    let h = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(h.read_preferences().unwrap(), Some(dark));
    assert!(!h.writable());
}

#[test]
fn private_historical_retry_never_overwrites_later_value_or_accepts_changed_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut h = Workbench::open(&path, None).unwrap();
    let dark = config("dark");
    let white = config("white");
    h.submit_preferences("first", dark.clone()).unwrap();
    h.acknowledge_preferences("first", &Sha256::digest(&dark))
        .unwrap();
    h.submit_preferences("second", white.clone()).unwrap();
    h.acknowledge_preferences("second", &Sha256::digest(&white))
        .unwrap();
    assert_eq!(h.submit_preferences("first", dark.clone()).unwrap(), dark);
    assert_eq!(h.read_preferences().unwrap(), Some(white.clone()));
    assert!(h.submit_preferences("first", white.clone()).is_err());
    assert_eq!(h.read_preferences().unwrap(), Some(white.clone()));
    drop(h);
    let mut h = Workbench::open(&path, None).unwrap();
    assert_eq!(
        h.pending_preferences().unwrap(),
        Some(("first".into(), dark.clone()))
    );
    assert_eq!(h.submit_preferences("first", dark.clone()).unwrap(), dark);
    assert_eq!(h.read_preferences().unwrap(), Some(white));
}

#[test]
fn private_settings_reject_cross_card_operation_collision_before_journaling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut h = Workbench::open(&path, Some(package())).unwrap();
    h.create("occupied", common::idea("card")).unwrap();
    enabled(&mut h, false);
    assert!(h.submit_preferences("occupied", config("dark")).is_err());
    assert!(h.pending_preferences().unwrap().is_none());
    assert!(h.read_preferences().unwrap().is_none());
}
