#![cfg(target_os = "windows")]
mod common;
use common::{idea, package};
use morrow_audit::{keys::Key, session::key_path};
use morrow_core::store::{EventBudget, Store};
use morrow_workbench_host::Workbench;
use std::path::Path;
fn inspect(db: &Path) -> Store {
    let key = Key::load(&key_path(db).unwrap()).unwrap();
    Store::open_read_only_audited(db, key.trust()).unwrap()
}
#[test]
fn continuous_edits_cross_old_capacity_and_close_seals_the_tail() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    let original_key = std::fs::read(key_path(&db).unwrap()).unwrap();
    for i in 0..1100 {
        host.create(&format!("op-{i}"), idea(&format!("card-{i}")))
            .unwrap();
    }
    assert!(inspect(&db).pending_usage().unwrap().0 < 64);
    assert!(inspect(&db).last_sealed_segment().unwrap().is_some());
    assert!(host.maintenance_warning().is_none());
    host.finish().unwrap();
    assert_eq!(inspect(&db).pending_usage().unwrap(), (0, 0));
    drop(host);
    let readonly = Workbench::open(&db, None).unwrap();
    assert!(!readonly.writable());
    assert_eq!(readonly.read("card-1099").unwrap().revision, 1);
    assert_eq!(std::fs::read(key_path(&db).unwrap()).unwrap(), original_key);
}
#[test]
fn restart_recovers_unsealed_commits_and_missing_key_never_creates_identity() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let key = key_path(&db).unwrap();
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    host.create("saved", idea("saved")).unwrap();
    assert_eq!(inspect(&db).pending_usage().unwrap().0, 1);
    assert!(Workbench::open(&db, None).is_err());
    drop(host); // Neither correctness nor durability depends on Drop sealing.
    let host = Workbench::open(&db, None).unwrap();
    assert_eq!(host.read("saved").unwrap().idea.id, "saved");
    assert_eq!(inspect(&db).pending_usage().unwrap(), (0, 0));
    drop(host);
    let before = std::fs::read(&db).unwrap();
    std::fs::rename(&key, d.path().join("retained-key")).unwrap();
    assert!(
        Workbench::open(&db, None)
            .err()
            .unwrap()
            .to_string()
            .contains("保护密钥缺失")
    );
    assert!(!key.exists());
    assert_eq!(std::fs::read(&db).unwrap(), before);
    std::fs::rename(d.path().join("retained-key"), &key).unwrap();
    assert_eq!(
        Workbench::open(&db, None)
            .unwrap()
            .read("saved")
            .unwrap()
            .revision,
        1
    );
}
#[test]
fn old_unbound_workbench_keeps_its_original_content() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let original = idea("legacy");
    let card = morrow_core::content::CardRecord::new(
        "legacy",
        "org.morrow.idea",
        1,
        &original.title,
        morrow_workbench_plugin::persistence::encode(&original, None).unwrap(),
    )
    .unwrap();
    store.create_local("legacy-op", &card).unwrap();
    drop(store);
    let host = Workbench::open(&db, None).unwrap();
    assert_eq!(
        host.read("legacy").unwrap().idea.description,
        original.description
    );
    assert_eq!(inspect(&db).pending_usage().unwrap(), (0, 0));
}
#[test]
#[cfg(feature = "fault-injection")]
fn audit_child() {
    let Ok(root) = std::env::var("MORROW_HOST_AUDIT_TEST_ROOT") else {
        return;
    };
    let db = Path::new(&root).join("workbench.db");
    let mut host = Workbench::open(&db, Some(package())).unwrap();
    match std::env::var("MORROW_HOST_AUDIT_TEST_MODE")
        .unwrap()
        .as_str()
    {
        "commit" => {
            host.create("crash-op", idea("crash-card")).unwrap();
            panic!("Crash hook missed");
        }
        "seal" => panic!("Crash hook missed"),
        "failure" => {
            assert!(host.maintenance_warning().is_some());
            assert!(!host.writable());
            assert_eq!(host.read("saved").unwrap().revision, 1);
            assert!(host.create("refused", idea("refused")).is_err());
            assert!(host.read("refused").is_err());
            assert!(host.finish().is_err());
            let mut request = capnp::message::Builder::new_default();
            let mut r = request.init_root::<morrow_workbench_host::host_capnp::request::Builder>();
            r.set_version(1);
            r.set_digest(&morrow_workbench_host::protocol::digest());
            r.set_action(morrow_workbench_host::host_capnp::Action::Read);
            r.set_id("saved");
            let response = morrow_workbench_host::protocol::respond(
                &mut host,
                &capnp::serialize::write_message_to_words(&request),
            )
            .unwrap();
            let message =
                capnp::serialize::read_message(&mut &response[..], Default::default()).unwrap();
            let r = message
                .get_root::<morrow_workbench_host::host_capnp::response::Reader>()
                .unwrap();
            assert_eq!(r.get_revision(), 1);
            assert_eq!(r.get_error().unwrap().to_str().unwrap(), "");
            assert!(r.get_read_only());
            assert!(
                !r.get_maintenance_warning()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .is_empty()
            );
            // A refresh while the actual failure remains must not advertise restored writes.
            host.refresh_plugin_state().unwrap();
            assert!(!host.writable());
            assert!(host.maintenance_warning().is_some());
            // This test runs as its own Windows child process, with this one selected test.
            // Windows environment mutation is safe; no other process's injection is changed.
            unsafe { std::env::remove_var("MORROW_WORKBENCH_FAIL_SEAL") };
            host.refresh_plugin_state().unwrap();
            // finish() already closed this root even though sealing failed. Clearing
            // the storage fault must not implicitly revive a terminated plugin session.
            assert!(!host.writable());
            assert!(host.maintenance_warning().is_none());
            let status = host.plugin_status().unwrap();
            host.configure_plugin(status.revision, &status.digest, true)
                .unwrap();
            assert!(host.writable());
            assert_eq!(host.read("saved").unwrap().revision, 1);
            assert!(host.read("refused").is_err());
            assert_eq!(inspect(&db).pending_usage().unwrap(), (0, 0));
            let recovered = morrow_workbench_host::protocol::respond(
                &mut host,
                &capnp::serialize::write_message_to_words(&request),
            )
            .unwrap();
            let message =
                capnp::serialize::read_message(&mut &recovered[..], Default::default()).unwrap();
            let r = message
                .get_root::<morrow_workbench_host::host_capnp::response::Reader>()
                .unwrap();
            assert!(!r.get_read_only());
            assert_eq!(r.get_revision(), 1);
            assert_eq!(r.get_maintenance_warning().unwrap().to_str().unwrap(), "");
        }
        _ => panic!("unknown mode"),
    }
}
#[test]
#[cfg(feature = "fault-injection")]
fn crashes_and_maintenance_failures_preserve_committed_results() {
    for (mode, env, value, expected_exit) in [
        ("commit", "MORROW_TEST_CRASH_AT", "after-commit", 86),
        ("seal", "MORROW_AUDIT_CRASH_AT", "sealer-after-batch", 86),
        ("failure", "MORROW_WORKBENCH_FAIL_SEAL", "1", 0),
    ] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("workbench.db");
        let mut host = Workbench::open(&db, Some(package())).unwrap();
        host.create("saved", idea("saved")).unwrap();
        drop(host);
        let original_key = std::fs::read(key_path(&db).unwrap()).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "audit_child", "--nocapture"])
            .env("MORROW_HOST_AUDIT_TEST_ROOT", d.path())
            .env("MORROW_HOST_AUDIT_TEST_MODE", mode)
            .env(env, value)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(expected_exit), "{mode}");
        if mode == "failure" {
            assert!(inspect(&db).card("refused").unwrap().is_none());
            // The failure child explicitly refreshes after removing its injected seal fault.
            assert_eq!(inspect(&db).pending_usage().unwrap().0, 0);
        }
        let recovered = Workbench::open(&db, None).unwrap();
        assert_eq!(recovered.read("saved").unwrap().revision, 1);
        if mode == "commit" {
            assert_eq!(recovered.read("crash-card").unwrap().revision, 1);
        }
        assert_eq!(inspect(&db).pending_usage().unwrap(), (0, 0));
        assert_eq!(std::fs::read(key_path(&db).unwrap()).unwrap(), original_key);
    }
}
