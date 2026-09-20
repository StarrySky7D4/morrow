#![cfg(target_os = "windows")]
mod common;
use morrow_core::{
    plugin_package::{Package, proto::TransformHandler},
    ui::{Document, Event, EventKind},
};
use morrow_workbench_host::Workbench;
fn package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.package_version = "0.1.9-test.25".into();
    manifest.transform_handlers.extend([
        TransformHandler {
            handler: "ui.form".into(),
            input_type: "text.utf8".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 32,
            max_output_bytes: 65536,
        },
        TransformHandler {
            handler: "ui.edit".into(),
            input_type: "morrow.ui.event.v1".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        },
    ]);
    Package::build(manifest, original.module()).unwrap()
}
fn event(generation: u64, revision: u64, serial: u64, text: &str) -> Vec<u8> {
    Event {
        view: "workbench-tools".into(),
        generation,
        revision,
        serial,
        node: "text".into(),
        action: "text.edit".into(),
        kind: EventKind::EditText,
        text: text.into(),
        checked: false,
    }
    .encode()
    .unwrap()
}
#[test]
fn disabled_plugin_stays_disabled_after_managed_restart_until_explicit_reenable() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    let status = host.plugin_status().unwrap();
    assert!(status.enabled && status.approved && host.writable());
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(!host.writable());
    assert!(!host.plugin_status().unwrap().enabled);
    assert!(host.create("denied", common::idea("denied")).is_err());
    assert_eq!(host.read("card").unwrap().revision, 1);
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    assert!(!host.writable());
    let status = host.plugin_status().unwrap();
    assert!(!status.enabled);
    assert!(
        host.create("still-denied", common::idea("still-denied"))
            .is_err()
    );
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert!(host.writable());
    host.create("after-enable", common::idea("new-card"))
        .unwrap();
    assert_eq!(host.read("card").unwrap().revision, 1);
}
#[test]
fn stale_configuration_does_not_revoke_instance_or_close_its_active_view() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    let first = host.ui_open("seed").unwrap();
    assert!(first.failure.is_none(), "{:?}", first.failure);
    let status = host.plugin_status().unwrap();
    assert!(
        host.configure_plugin(status.revision + 1, &status.digest, false)
            .is_err()
    );
    assert!(
        host.configure_plugin(status.revision, &[0; 32], false)
            .is_err()
    );
    assert!(host.writable());
    assert_eq!(host.plugin_status().unwrap().revision, status.revision);
    let update = host
        .ui_event(
            first.generation,
            &event(first.generation, first.revision, 1, "works"),
        )
        .unwrap();
    assert!(update.failure.is_none(), "{:?}", update.failure);
    assert_eq!(update.serial, 1);
    host.create("still-working", common::idea("card")).unwrap();
}
#[test]
fn online_ui_generations_and_admission_are_independent_of_business_cards() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    assert!(host.page("", 100).unwrap().0.is_empty());
    let first = host.ui_open("abc").unwrap();
    assert!(first.failure.is_none(), "{:?}", first.failure);
    let document = Document::decode(first.document.as_ref().unwrap()).unwrap();
    assert_eq!(
        document
            .nodes()
            .iter()
            .find(|n| n.id == "preview")
            .unwrap()
            .text,
        "ABC"
    );
    assert!(host.ui_open("duplicate").is_err());
    let invalid = host.ui_event(first.generation, &[1, 2]).unwrap();
    assert!(invalid.failure.is_some());
    assert_eq!((invalid.revision, invalid.serial), (1, 0));
    let update = host
        .ui_event(first.generation, &event(first.generation, 1, 1, "hello"))
        .unwrap();
    assert!(update.failure.is_none());
    assert_eq!((update.revision, update.serial), (2, 1));
    let document = Document::decode(update.document.as_ref().unwrap()).unwrap();
    assert_eq!(
        document
            .nodes()
            .iter()
            .find(|n| n.id == "preview")
            .unwrap()
            .text,
        "HELLO"
    );
    let duplicate = host
        .ui_event(first.generation, &event(first.generation, 1, 1, "hello"))
        .unwrap();
    assert!(duplicate.failure.is_some());
    assert_eq!((duplicate.revision, duplicate.serial), (2, 1));
    host.ui_close(first.generation).unwrap();
    let second = host.ui_open("new").unwrap();
    assert!(second.failure.is_none());
    assert!(second.generation > first.generation);
    host.ui_close(first.generation).unwrap();
    assert!(
        host.ui_event(first.generation, &event(first.generation, 1, 1, "old"))
            .is_err()
    );
    let second_update = host
        .ui_event(
            second.generation,
            &event(second.generation, 1, 1, "new value"),
        )
        .unwrap();
    assert!(second_update.failure.is_none());
    assert!(host.page("", 100).unwrap().0.is_empty());
}
#[test]
fn disabling_invalidates_ui_and_reenabled_view_rejects_old_generation() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    let old = host.ui_open("old").unwrap();
    assert!(old.failure.is_none());
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(
        host.ui_event(old.generation, &event(old.generation, 1, 1, "stale"))
            .is_err()
    );
    assert!(host.ui_open("disabled").is_err());
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    let fresh = host.ui_open("fresh").unwrap();
    assert!(fresh.failure.is_none());
    assert!(fresh.generation > old.generation);
    host.ui_close(old.generation).unwrap();
    assert!(
        host.ui_event(fresh.generation, &event(fresh.generation, 1, 1, "retained"))
            .unwrap()
            .failure
            .is_none()
    );
    assert!(host.page("", 100).unwrap().0.is_empty());
}

#[test]
fn upgrading_disables_new_bundle_and_downgrade_preserves_read_only_content() {
    let root = tempfile::tempdir().unwrap();
    let mut old = Workbench::open_managed(root.path(), Some(common::package())).unwrap();
    old.create("seed", common::idea("card")).unwrap();
    old.finish().unwrap();
    drop(old);
    let mut current = Workbench::open_managed(root.path(), Some(package())).unwrap();
    let status = current.plugin_status().unwrap();
    assert!(!status.enabled);
    assert!(!current.writable());
    assert_eq!(current.read("card").unwrap().revision, 1);
    current
        .configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert!(current.writable());
    drop(current);
    let old = Workbench::open_managed(root.path(), Some(common::package())).unwrap();
    assert!(!old.writable());
    assert!(old.maintenance_warning().is_some());
    assert!(!old.plugin_status().unwrap().available);
    assert_eq!(old.read("card").unwrap().revision, 1);
}
#[test]
fn corrupted_plugin_registry_is_preserved_while_library_stays_readable() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    host.finish().unwrap();
    drop(host);
    let path = root.path().join("plugin-manager/state/selection.morrow");
    std::fs::write(&path, b"corrupt-registry").unwrap();
    let host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    assert!(!host.writable());
    assert!(host.maintenance_warning().is_some());
    assert!(!host.plugin_status().unwrap().available);
    assert_eq!(host.read("card").unwrap().revision, 1);
    assert_eq!(std::fs::read(path).unwrap(), b"corrupt-registry");
}
#[test]
fn enabled_without_write_approval_reports_read_only_and_refuses_mutation() {
    use morrow_core::{
        lifecycle::GrantKind,
        plugin_package::{catalog::Catalog, registry::Registry},
    };
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    host.finish().unwrap();
    drop(host);
    let catalog = Catalog::open(&root.path().join("plugin-manager/packages")).unwrap();
    let mut registry = Registry::open(&root.path().join("plugin-manager/state"), catalog).unwrap();
    let p = package();
    registry
        .approve(
            "org.morrow.workbench",
            p.digest(),
            [GrantKind::ReadContent].into_iter().collect(),
            registry.revision(),
        )
        .unwrap();
    drop(registry);
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    let status = host.plugin_status().unwrap();
    assert!(status.enabled && status.available);
    assert!(!status.approved);
    assert!(!host.writable());
    assert!(host.create("denied", common::idea("new")).is_err());
    assert_eq!(host.read("card").unwrap().revision, 1);
}

#[test]
fn managed_library_restore_keeps_original_plugin_policy_root() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    let archive = root.path().join("snapshot");
    host.backup_snapshot(&archive).unwrap();
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    host.finish().unwrap();
    drop(host);
    let target = root.path().join("recovered");
    morrow_workbench_host::restore_snapshot(&archive, &target).unwrap();
    morrow_workbench_host::activate_library(root.path(), &target).unwrap();
    let host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    assert!(!host.plugin_status().unwrap().enabled);
    assert!(!host.writable());
    assert_eq!(host.read("card").unwrap().revision, 1);
    assert!(!target.join("plugin-manager").exists());
    assert!(
        root.path()
            .join("plugin-manager/state/selection.morrow")
            .exists()
    );
}

#[test]
fn corrupt_bundle_child_process_reads_existing_cards_and_refuses_mutation() {
    use capnp::{
        message::{Builder, ReaderOptions},
        serialize,
    };
    use morrow_workbench_host::{host_capnp as wire, protocol};
    use morrow_workbench_plugin::{Action, Request, codec};
    use std::{
        io::{Read, Write},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let root = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(root.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    host.finish().unwrap();
    drop(host);
    let bad = root.path().join("damaged.morrowplugin");
    std::fs::write(&bad, b"not a plugin archive").unwrap();
    let mut requests = Vec::new();
    for action in [
        wire::Action::Page,
        wire::Action::Read,
        wire::Action::Query,
        wire::Action::Mutate,
    ] {
        let mut message = Builder::new_default();
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(1);
        r.set_digest(&protocol::digest());
        r.set_action(action);
        r.set_limit(100);
        r.set_id(if action == wire::Action::Mutate {
            "denied"
        } else {
            "card"
        });
        if matches!(action, wire::Action::Query | wire::Action::Mutate) {
            let request = Request {
                action: if action == wire::Action::Query {
                    Action::Query
                } else {
                    Action::Create
                },
                current: Default::default(),
                proposed: common::idea("denied"),
                text: String::new(),
                flag: false,
                now_ms: 1,
                ideas: vec![],
                section: "概览".into(),
                filter: "全部".into(),
                sort: "最近添加".into(),
            };
            r.set_payload(&codec::encode_request(&request).unwrap());
            r.set_operation("denied-create");
        }
        let bytes = serialize::write_message_to_words(&message);
        requests.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        requests.extend_from_slice(&bytes);
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_morrow-workbench-host"))
        .args([
            std::ffi::OsStr::new("--managed"),
            root.path().as_os_str(),
            bad.as_os_str(),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&requests).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("corrupt bundle host did not terminate");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "{stderr}");
    assert!(stderr.contains("只读"), "{stderr}");
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let mut bytes = stdout.as_slice();
    for index in 0..4 {
        let mut size = [0; 4];
        bytes.read_exact(&mut size).unwrap();
        let size = u32::from_le_bytes(size) as usize;
        assert!(size <= 128 * 1024 && bytes.len() >= size);
        let mut frame = std::io::Cursor::new(&bytes[..size]);
        let message = serialize::read_message(&mut frame, ReaderOptions::new()).unwrap();
        assert_eq!(frame.position() as usize, size);
        bytes = &bytes[size..];
        let response = message.get_root::<wire::response::Reader>().unwrap();
        assert!(response.get_read_only());
        if index == 2 || index == 3 {
            assert!(!response.get_error().unwrap().to_str().unwrap().is_empty());
        } else {
            assert!(
                response.get_error().unwrap().to_str().unwrap().is_empty(),
                "response {index}: {}",
                response.get_error().unwrap().to_str().unwrap()
            );
            if index == 1 {
                assert_eq!(
                    codec::decode_response(response.get_payload().unwrap())
                        .unwrap()
                        .idea
                        .id,
                    "card"
                );
                assert_eq!(response.get_revision(), 1);
            } else {
                let ids = response.get_ids().unwrap();
                assert_eq!(ids.len(), 1);
                assert_eq!(ids.get(0).unwrap().to_str().unwrap(), "card");
            }
        }
    }
    assert!(bytes.is_empty());
    assert_eq!(std::fs::read(&bad).unwrap(), b"not a plugin archive");
    let host = Workbench::open_managed(root.path(), None).unwrap();
    assert!(host.read("denied").is_err());
    assert_eq!(host.read("card").unwrap().revision, 1);
}
