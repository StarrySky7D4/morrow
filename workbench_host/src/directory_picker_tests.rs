//! Negative admission only: no native dialog and no guest are invoked.
use super::*;
use crate::io_tasks::directory::DirectoryObjectStart;

fn metadata(submission: [u8; 32], timeout_ms: u32) -> DirectoryObjectStart {
    DirectoryObjectStart {
        submission,
        package_id: "org.example.uninstalled-picker".into(),
        digest: [7; 32],
        revision: 0,
        handler: "directory.list".into(),
        max_entries: 16,
        max_metadata_bytes: 16384,
        timeout_ms,
    }
}

#[test]
fn picker_requires_valid_original_admission_before_creating_an_owner() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        temp.path()
            .starts_with(std::path::Path::new("D:/build_cache"))
    );
    let mut host = Workbench::open_managed(temp.path(), None).unwrap();
    for (submission, timeout) in [
        ([0; 32], 1000),
        ([1; 32], 0),
        ([2; 32], 30001),
        ([3; 32], 1000),
    ] {
        // Last case has valid shape but no installed or approved package.
        assert!(
            host.start_native_directory_picker(metadata(submission, timeout))
                .is_err()
        );
        assert!(host.io_status().key.is_none());
        assert!(host.http_submission().is_none());
    }
    host.finish().unwrap();
}

#[test]
fn capnp_picker_lane_never_accepts_a_callers_path_or_changes_old_ordinals() {
    assert_eq!(wire::Action::DirectoryStart as u16, 156);
    assert_eq!(wire::Action::DirectoryFinish as u16, 160);
    assert_eq!(wire::Action::DirectoryPick as u16, 161);
    assert!(is_scheduler_action(wire::Action::DirectoryPick));
    let temp = tempfile::tempdir().unwrap();
    assert!(
        temp.path()
            .starts_with(std::path::Path::new("D:/build_cache"))
    );
    let mut host = Workbench::open_managed(temp.path(), None).unwrap();
    for path in [vec![67, 58, 92], vec![0xd800], vec![65; 4097]] {
        let mut message = capnp::message::Builder::new_default();
        let mut request = message.init_root::<wire::request::Builder>();
        request.set_version(1);
        request.set_digest(&digest());
        request.set_action(wire::Action::DirectoryPick);
        let mut start = request.init_directory_start();
        start.set_submission(&[1; 32]);
        start.set_package_id("org.example.uninstalled-picker");
        start.set_package_digest(&[7; 32]);
        start.set_handler("directory.list");
        start.set_timeout_ms(1000);
        let mut units = start.init_selected_path_utf16(path.len() as u32);
        for (i, unit) in path.into_iter().enumerate() {
            units.set(i as u32, unit);
        }
        let frame = capnp::serialize::write_message_to_words(&message);
        let bytes = respond(&mut host, &frame).unwrap();
        let mut remaining = bytes.as_slice();
        let reply =
            capnp::serialize::read_message_from_flat_slice(&mut remaining, Default::default())
                .unwrap();
        let row = reply.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(
            row.get_error().unwrap().to_str().unwrap(),
            "native picker does not accept a caller pathname"
        );
        assert!(remaining.is_empty());
        assert!(host.io_status().key.is_none());
        assert!(host.http_submission().is_none());
    }
    host.finish().unwrap();
}
