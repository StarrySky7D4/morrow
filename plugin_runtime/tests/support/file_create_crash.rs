//! Real process exits around a Create effect; every path lives under one TempDir.
use super::*;
use morrow_core::{
    file_effect::{CreateOutcome, CreateResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::Kind,
    io_intent::{Phase, Recovery},
};
use morrow_plugin_runtime::file_target::{SelectionScope, TargetBroker};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const SUBJECT: &str = "plugin.file-create-crash";
const OP: &str = "native-create-crash";
const CONTENT: &[u8] = b"create crash original content";

#[test]
#[ignore = "child harness; parent supplies isolated root and native crash boundary"]
fn child() {
    let root = std::env::var_os("MORROW_FILE_CREATE_TEST_ROOT").expect("isolated root");
    let dir = tempfile::Builder::new()
        .prefix("child-")
        .tempdir_in(&root)
        .unwrap();
    let caps = BTreeSet::from([IoCapability::FileCreate]);
    let mut f = Fixture::with_module_at(dir, true, 3, 100_000, caps.clone(), guest());
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
    let selected_root = f._dir.path().join("selected-root");
    fs::create_dir(&selected_root).unwrap();
    fs::create_dir(selected_root.join("parent")).unwrap();
    fs::write(
        Path::new(&root).join("location.txt"),
        f._dir.path().to_str().unwrap(),
    )
    .unwrap();
    let relative = RelativeFilePath::parse("parent/created.bin").unwrap();
    let mut broker = TargetBroker::new([0xb1; 32]).unwrap();
    let selected = broker
        .select_create(
            &f.manager,
            &f.host,
            &instance,
            &binding,
            &selected_root,
            &relative,
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: [0xb2; 32],
                disposition: Disposition::Create,
            },
            || 2,
        )
        .unwrap();
    let request = RequestRecord::new(MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: f.package.digest(),
        approval_sha256: [0xb2; 32],
        target: Target {
            reference: selected.reference,
            relative_path: Some(relative),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: CONTENT.len() as u64,
        content_sha256: Some(Sha256::digest(CONTENT).into()),
    })
    .unwrap();
    fs::write(Path::new(&root).join("request.bin"), request.container()).unwrap();
    broker
        .prepare_request(&f.manager, &mut f.host, &instance, &request, || 3)
        .unwrap();
    broker
        .stage_content(&f.manager, &mut f.host, &instance, &request, CONTENT, || 4)
        .unwrap();
    let _ = broker.create(&f.manager, &mut f.host, &instance, &request, || 5);
    panic!("expected native create fault boundary to terminate child");
}

#[test]
fn actual_create_crash_preserves_unknown_or_observed_without_replay() {
    let _serial = super::file_delete::serial_effects();
    for point in [
        "after-claim",
        "after-temp",
        "after-write",
        "after-flush",
        "after-publish",
        "after-effect",
        "after-observe",
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "file_create_crash::child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_FILE_CREATE_TEST_ROOT", root.path())
            .env("MORROW_FILE_CREATE_FAULT", point)
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
                panic!("native file create child timeout at {point}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(86), "{point}");
        let location =
            std::path::PathBuf::from(fs::read_to_string(root.path().join("location.txt")).unwrap())
                .canonicalize()
                .unwrap();
        assert!(location.starts_with(root.path().canonicalize().unwrap()));
        let request =
            RequestRecord::decode(&fs::read(root.path().join("request.bin")).unwrap()).unwrap();
        let digest = request.command().unwrap().request_sha256;
        let temporary = location.join("selected-root").join("parent").join(format!(
            ".morrow-create-{}.tmp",
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let final_path = location
            .join("selected-root")
            .join("parent")
            .join("created.bin");
        let published = matches!(point, "after-publish" | "after-effect" | "after-observe");
        assert_eq!(final_path.exists(), published, "{point}");
        if published {
            assert_eq!(fs::read(&final_path).unwrap(), CONTENT);
        }
        let temp_expected = matches!(point, "after-temp" | "after-write" | "after-flush");
        assert_eq!(temporary.exists(), temp_expected, "{point}");
        if matches!(point, "after-flush") {
            assert_eq!(fs::read(&temporary).unwrap(), CONTENT);
        }
        let mut store = Store::open_existing(&location.join("db"), EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        let record = store.lookup_io_intent(SUBJECT, OP).unwrap().unwrap();
        let response = store.io_material(SUBJECT, OP, Kind::Response);
        if point == "after-observe" {
            assert_eq!(record.phase(), Phase::Observed);
            assert_eq!(record.recovery(), Recovery::AlreadyObserved);
            assert_eq!(
                CreateOutcome::decode(response.unwrap().unwrap().payload())
                    .unwrap()
                    .result(),
                CreateResult::Created
            );
        } else {
            assert_eq!(record.phase(), Phase::OutcomeUnknown);
            assert_eq!(record.recovery(), Recovery::ReconcileOnly);
            assert!(matches!(
                response,
                Err(morrow_core::Error::EvidenceUnavailable)
            ));
            assert!(matches!(
                store.claim_file_create_local_authorized(&record, || Ok(())),
                Err(morrow_core::Error::RevisionConflict)
            ));
        }
        if !published {
            fs::write(&final_path, b"later unrelated entry").unwrap();
        }
        assert!(
            store
                .claim_io_dispatch_local_authorized(&record, || Ok(()))
                .is_err()
        );
        if !published {
            assert_eq!(fs::read(&final_path).unwrap(), b"later unrelated entry");
        }
    }
}
