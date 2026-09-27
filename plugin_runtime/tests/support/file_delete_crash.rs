//! Actual Windows effect interrupted in a separate process, isolated under TempDir.
use super::*;
use morrow_core::{
    file_effect::{DeleteOutcome, DeleteResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::Kind,
    io_intent::{Phase, Recovery},
};
use morrow_plugin_runtime::file_target::{SelectionScope, TargetBroker};
use std::{
    fs,
    os::windows::process::CommandExt,
    process::Command,
    time::{Duration, Instant},
};
const SUBJECT: &str = "plugin.file-delete-crash";
const OP: &str = "native-delete-crash";

#[test]
#[ignore = "child harness; parent supplies isolated root and native crash boundary"]
fn child() {
    let root = std::env::var_os("MORROW_FILE_DELETE_TEST_ROOT").expect("isolated root");
    let dir = tempfile::Builder::new()
        .prefix("child-")
        .tempdir_in(&root)
        .unwrap();
    let caps = BTreeSet::from([IoCapability::FileDelete]);
    let mut f = Fixture::with_module_at(dir, true, 2, 100_000, caps.clone(), guest());
    let instance = f.connect();
    let binding = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &caps,
            90,
            1,
        )
        .unwrap();
    let path = f._dir.path().join("victim.bin");
    fs::write(&path, b"original selected file").unwrap();
    fs::write(
        std::path::Path::new(&root).join("location.txt"),
        f._dir.path().to_str().unwrap(),
    )
    .unwrap();
    let mut broker = TargetBroker::new([0x81; 32]).unwrap();
    let selected = broker
        .select_existing(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &path,
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: [0x82; 32],
                disposition: Disposition::Delete,
            },
            || 2,
        )
        .unwrap();
    let request = RequestRecord::new(MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: [0x82; 32],
        target: Target {
            reference: selected.reference,
            relative_path: None,
        },
        disposition: Disposition::Delete,
        expected_identity: Some(selected.expected_identity),
        content_length: 0,
        content_sha256: None,
    })
    .unwrap();
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    let _ = broker.delete(&f.manager, &mut f.host, &instance, &request, || 4);
    panic!("expected native fault boundary to terminate the child");
}

#[test]
fn actual_native_crash_preserves_unknown_or_observed_without_replay() {
    for point in ["after-claim", "after-effect", "after-observe"] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "file_delete_crash::child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_FILE_DELETE_TEST_ROOT", root.path())
            .env("MORROW_FILE_DELETE_FAULT", point)
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native file delete child timeout at {point}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(86), "{point}");
        let location =
            std::path::PathBuf::from(fs::read_to_string(root.path().join("location.txt")).unwrap());
        let location = location.canonicalize().unwrap();
        assert!(location.starts_with(root.path().canonicalize().unwrap()));
        let victim = location.join("victim.bin");
        assert_eq!(victim.exists(), point == "after-claim", "{point}");
        if point == "after-claim" {
            assert_eq!(fs::read(&victim).unwrap(), b"original selected file");
        }
        let mut store = Store::open_existing(&location.join("db"), EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        let record = store.lookup_io_intent(SUBJECT, OP).unwrap().unwrap();
        let response = store.io_material(SUBJECT, OP, Kind::Response);
        if point == "after-observe" {
            assert_eq!(record.phase(), Phase::Observed);
            assert_eq!(record.recovery(), Recovery::AlreadyObserved);
            let response = response.unwrap().unwrap();
            assert_eq!(
                DeleteOutcome::decode(response.payload()).unwrap().result(),
                DeleteResult::Deleted
            );
        } else {
            assert_eq!(record.phase(), Phase::OutcomeUnknown);
            assert_eq!(record.recovery(), Recovery::ReconcileOnly);
            assert!(matches!(
                response,
                Err(morrow_core::Error::EvidenceUnavailable)
            ));
            assert!(matches!(
                store.claim_file_delete_local_authorized(&record, || Ok(())),
                Err(morrow_core::Error::RevisionConflict)
            ));
        }
        // A later directory entry at this name is unrelated to the old operation.
        fs::write(&victim, b"later file must survive").unwrap();
        assert!(
            store
                .claim_io_dispatch_local_authorized(&record, || Ok(()))
                .is_err()
        );
        assert_eq!(fs::read(&victim).unwrap(), b"later file must survive");
    }
}
