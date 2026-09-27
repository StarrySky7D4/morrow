//! Real process exits during a nonempty guest Create on its original owner.
//! Every selected path and database lives below the parent test's TempDir.
use super::*;
use morrow_core::{
    file_effect::{CreateOutcome, CreateResult},
    io_evidence::Kind,
    io_intent::Recovery,
    mutation::{self, Action, Phase as WirePhase, Request as WireRequest, Status},
    store::EventBudget,
};
use morrow_plugin_runtime::io_jobs::{MutationGuestJobMode, MutationGuestLease, Poll};
use std::{
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

const ROOT_ENV: &str = "MORROW_MUTATION_GUEST_CRASH_ROOT";
const POINT_ENV: &str = "MORROW_MUTATION_GUEST_CRASH_POINT";
const WASM_ENV: &str = "MORROW_MUTATION_CRASH_WASM_PATH";
const OPERATION: &str = "guest-crash-nonempty-create";
const LEAF: &str = "created.bin";
const TAIL_BYTES: usize = 137;
const BODY_BYTES: usize = 3 * MAX_MUTATION_CHUNK + TAIL_BYTES;

const CORE_POINTS: &[&str] = &[
    "file-content-after-bytes",
    "file-content-after-receipt",
    "file-content-before-commit",
    "file-content-after-commit",
];
const NATIVE_POINTS: &[&str] = &[
    "after-claim",
    "after-temp",
    "after-write-chunk",
    "after-write",
    "after-flush",
    "after-publish",
    "after-effect",
    "after-observe",
];

fn body() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(BODY_BYTES);
    let mut counter = 0u64;
    while bytes.len() < BODY_BYTES {
        let mut hash = Sha256::new();
        hash.update(b"morrow-guest-crash-distinct-blocks-v1");
        hash.update(counter.to_le_bytes());
        bytes.extend_from_slice(&hash.finalize());
        counter += 1;
    }
    bytes.truncate(BODY_BYTES);
    bytes
}

fn wat_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_mutation_v1" "call" (func $mutation (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32)
            (local $n i32) (local $m i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $mutation local.set $m
            i32.const 131072 local.get $m call $done drop
            i32.const 0))"#,
    )
    .unwrap()
}

fn wasm() -> Vec<u8> {
    match std::env::var_os(WASM_ENV) {
        Some(path) => fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "explicit mutation crash Wasm {}: {error}",
                Path::new(&path).display()
            )
        }),
        None => wat_module(),
    }
}

fn fixture_in(root: &Path, wasm: &[u8]) -> Fixture {
    let dir = tempfile::Builder::new()
        .prefix("guest-create-crash-")
        .tempdir_in(root)
        .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec![HANDLER.into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 4;
    budget.max_resources = 8;
    budget.max_job_bytes = 4 * 1024 * 1024;
    budget.max_bytes = 32 * 1024 * 1024;
    budget.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    manifest
        .required_features
        .push(morrow_core::plugin_package::MUTATION_FEATURE.into());
    manifest.mutation_schema_sha256 = mutation::schema_digest().to_vec();
    let package = Package::build(manifest, wasm).unwrap();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve_io(ID, package.digest(), caps(), manager.revision())
        .unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
    let host =
        HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    Fixture {
        dir,
        owner: Owner {
            host,
            manager,
            identity: Arc::new(()),
            gate: None,
            managed_gate: Arc::new(Mutex::new(None)),
        },
        clock: Arc::new(AtomicU64::new(1)),
        package_digest: package.digest(),
        mutation: true,
    }
}

fn request(call_id: u64, lease: MutationGuestLease, action: Action) -> WireRequest {
    let mut submission = [0; 32];
    submission[..8].copy_from_slice(&call_id.to_le_bytes());
    submission[8] = 0x9a;
    WireRequest {
        call_id,
        reference: lease.reference(),
        submission,
        operation_id: OPERATION.into(),
        deadline_ms: 10_000,
        action,
    }
}

fn call(
    worker: &IoWorker<Owner>,
    mode: MutationGuestJobMode,
    request: &WireRequest,
) -> mutation::Response {
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == Poll::Pending {
        assert!(Instant::now() < until, "guest mutation call stalled");
        thread::sleep(Duration::from_millis(1));
    }
    let report = handle.read(131_072).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Ok(0), "{report:?}");
    assert_eq!(report.calls, 1);
    let reply = report.mutation_response.expect("typed mutation response");
    assert_eq!(reply.call_id, request.call_id);
    assert_eq!(reply.reference, request.reference);
    assert_eq!(reply.submission, request.submission);
    reply
}

#[test]
#[ignore = "child harness; parent supplies a real crash boundary and isolated root"]
fn child() {
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("parent TempDir root"));
    let point = std::env::var(POINT_ENV).expect("parent crash boundary");
    assert!(CORE_POINTS.contains(&point.as_str()) || NATIVE_POINTS.contains(&point.as_str()));
    let wasm = wasm();
    fs::write(root.join("wasm.sha256"), Sha256::digest(&wasm)).unwrap();
    let fixture = fixture_in(&root, &wasm);
    let location = fixture.dir.path().to_path_buf();
    fs::write(root.join("location.txt"), location.to_str().unwrap()).unwrap();
    let (dir, _, worker) = fixture.start();
    let selected_root = dir.path().join("selected-root");
    fs::create_dir(&selected_root).unwrap();
    let bytes = body();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let (session, mut selected_handle) = worker
        .select_mutation_create(
            selected_root,
            RelativeFilePath::parse(LEAF).unwrap(),
            scope(Disposition::Create),
            [0xc1; 32],
        )
        .unwrap();
    selected(&mut selected_handle);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, OPERATION.into(), bytes.len() as u64, Some(digest))
            .unwrap(),
    );
    fs::write(root.join("request.bin"), plan.container()).unwrap();
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [0xc2; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected delivered guest approval: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    let prepare = request(
        1,
        lease,
        Action::Create {
            content_length: bytes.len() as u64,
            content_sha256: digest,
        },
    );
    let prepared = call(&worker, stage, &prepare);
    assert_eq!(
        (prepared.status, prepared.phase),
        (Status::Completed, WirePhase::Prepared)
    );
    for (index, chunk) in bytes.chunks(MAX_MUTATION_CHUNK).enumerate() {
        let staged = call(
            &worker,
            stage,
            &request(
                index as u64 + 2,
                lease,
                Action::Chunk {
                    offset: (index * MAX_MUTATION_CHUNK) as u64,
                    bytes: chunk.to_vec(),
                },
            ),
        );
        assert_eq!(staged.status, Status::Completed);
        assert_eq!(
            staged.staged_bytes,
            ((index * MAX_MUTATION_CHUNK) + chunk.len()) as u64
        );
    }
    fs::write(root.join("staged.marker"), b"four guest chunks completed").unwrap();
    let commit = request(6, lease, Action::Commit);
    let committed = call(&worker, stage, &commit);
    assert_eq!(
        (committed.status, committed.phase),
        (Status::Completed, WirePhase::Prepared)
    );
    assert!(committed.durable_content);
    let permit = match read(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected delivered execution permit: {other:?}"),
    };
    assert!(!dir.path().join("selected-root").join(LEAF).exists());
    fs::write(
        root.join("permitted.marker"),
        b"separate host permit delivered",
    )
    .unwrap();
    let execute = request(7, lease, Action::Execute);
    let mut handle = worker
        .submit_mutation_guest_frame(
            execute.encode().unwrap(),
            MutationGuestJobMode::Execute(permit),
            Duration::from_secs(10),
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == Poll::Pending {
        assert!(
            Instant::now() < until,
            "fault boundary not reached: {point}"
        );
        thread::sleep(Duration::from_millis(1));
    }
    panic!(
        "fault boundary {point} did not exit: {:?}",
        handle.read(131_072)
    );
}

fn crash_child(root: &Path, point: &str) -> Result<(), String> {
    let mut command = Command::new(std::env::current_exe().unwrap());
    let log_path = root.join("child.log");
    let log = fs::File::create(&log_path).unwrap();
    command
        .args([
            "--exact",
            "mutation_guest_crash::child",
            "--ignored",
            "--nocapture",
        ])
        .env(ROOT_ENV, root)
        .env(POINT_ENV, point)
        .env_remove("MORROW_TEST_CRASH_AT")
        .env_remove("MORROW_FILE_CREATE_FAULT")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .creation_flags(0x0800_0000);
    if CORE_POINTS.contains(&point) {
        command.env("MORROW_TEST_CRASH_AT", point);
    } else {
        command.env("MORROW_FILE_CREATE_FAULT", point);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("{point}: spawn crash child: {error}"))?;
    let pid = child.id();
    let until = Instant::now() + Duration::from_secs(40);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.code() == Some(86) => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!(
                    "{point}: child PID {pid} missed real fault ({status}): {}",
                    child_diagnostic(&log_path)
                ));
            }
            Ok(None) => {}
            Err(error) => {
                let stopped = terminate_child(&mut child);
                return Err(format!(
                    "{point}: observe child PID {pid}: {error}; {stopped}; {}",
                    child_diagnostic(&log_path)
                ));
            }
        };
        if Instant::now() >= until {
            let stopped = terminate_child(&mut child);
            return Err(format!(
                "{point}: child PID {pid} timed out; {stopped}; {}",
                child_diagnostic(&log_path)
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_child(child: &mut Child) -> String {
    let pid = child.id();
    let kill = child.kill();
    let until = Instant::now() + Duration::from_secs(5);
    let mut last_error = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return format!("child PID {pid} exited {status} after kill={kill:?}");
            }
            Ok(None) => {}
            Err(error) => last_error = Some(error),
        }
        if Instant::now() >= until {
            return format!(
                "child PID {pid} exit unconfirmed after kill={kill:?}, last_wait={last_error:?}"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn child_diagnostic(path: &Path) -> String {
    let mut diagnostic = Vec::new();
    match fs::File::open(path).and_then(|file| file.take(8192).read_to_end(&mut diagnostic)) {
        Ok(_) => String::from_utf8_lossy(&diagnostic).into_owned(),
        Err(error) => format!("child log unavailable: {error}"),
    }
}

fn hash_file(path: &Path) -> ([u8; 32], u64) {
    let mut file = fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    let mut len = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        len += count as u64;
    }
    (hasher.finalize().into(), len)
}

#[test]
fn guest_create_real_process_crash_matrix_preserves_original_history() {
    let _serial = serial_effects();
    let expected_wasm = wasm();
    let expected_wasm_sha256: [u8; 32] = Sha256::digest(&expected_wasm).into();
    let bytes = body();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    for point in CORE_POINTS.iter().chain(NATIVE_POINTS) {
        let root = tempfile::tempdir().unwrap();
        let began = Instant::now();
        if let Err(error) = crash_child(root.path(), point) {
            let retained = root.keep();
            panic!(
                "{error}; retained crash artifacts at {}",
                retained.display()
            );
        }
        assert!(root.path().join("staged.marker").exists(), "{point}");
        assert_eq!(
            fs::read(root.path().join("wasm.sha256")).unwrap(),
            expected_wasm_sha256,
            "{point}: wrong Wasm artifact"
        );
        let location = PathBuf::from(fs::read_to_string(root.path().join("location.txt")).unwrap())
            .canonicalize()
            .unwrap();
        assert!(location.starts_with(root.path().canonicalize().unwrap()));
        let plan =
            RequestRecord::decode(&fs::read(root.path().join("request.bin")).unwrap()).unwrap();
        assert_eq!(plan.request().operation_id, OPERATION);
        assert_eq!(plan.request().content_length, BODY_BYTES as u64);
        assert_eq!(plan.request().content_sha256, Some(digest));
        let command = plan.command().unwrap();
        let target = location.join("selected-root").join(LEAF);
        let temporary = location.join("selected-root").join(format!(
            ".morrow-create-{}.tmp",
            command
                .request_sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ));
        let core_boundary = CORE_POINTS.contains(point);
        assert_eq!(
            root.path().join("permitted.marker").exists(),
            !core_boundary,
            "{point}: permit stage"
        );
        let published = matches!(*point, "after-publish" | "after-effect" | "after-observe");
        assert_eq!(target.exists(), published, "{point}: destination state");
        if published {
            assert_eq!(hash_file(&target), (digest, BODY_BYTES as u64), "{point}");
        }
        let temp_expected = matches!(
            *point,
            "after-temp" | "after-write-chunk" | "after-write" | "after-flush"
        );
        assert_eq!(temporary.exists(), temp_expected, "{point}: temp state");
        if *point == "after-temp" {
            assert_eq!(fs::metadata(&temporary).unwrap().len(), 0);
        } else if *point == "after-write-chunk" {
            assert_eq!(fs::metadata(&temporary).unwrap().len(), 64 * 1024);
            let first_chunk_sha256: [u8; 32] = Sha256::digest(&bytes[..64 * 1024]).into();
            assert_eq!(hash_file(&temporary).0, first_chunk_sha256);
        } else if matches!(*point, "after-write" | "after-flush") {
            assert_eq!(hash_file(&temporary), (digest, BODY_BYTES as u64));
        }

        let mut store = Store::open_existing(&location.join("db"), EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        let record = store.lookup_io_intent(SUBJECT, OPERATION).unwrap().unwrap();
        let content = store
            .file_mutation_content_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .unwrap();
        let committed = !core_boundary || *point == "file-content-after-commit";
        assert_eq!(content.is_some(), committed, "{point}: content receipt");
        assert_eq!(
            store.file_mutation_content_usage().unwrap().0,
            u64::from(committed)
        );
        if let Some(content) = content.as_ref() {
            assert_eq!(content.request_sha256(), command.request_sha256, "{point}");
            assert_eq!(content.content_sha256(), digest, "{point}");
            assert_eq!(content.content(), bytes, "{point}");
        }
        let receipt = store
            .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .unwrap();
        assert_eq!(receipt.is_some(), committed, "{point}: audit receipt");
        if let Some(receipt) = receipt {
            assert_eq!(receipt.operation_id(), OPERATION, "{point}");
            assert_eq!(receipt.subject(), SUBJECT, "{point}");
            assert_eq!(receipt.request_sha256(), command.request_sha256, "{point}");
            assert_eq!(receipt.content_sha256(), digest, "{point}");
            assert_eq!(receipt.content_length(), BODY_BYTES as u64, "{point}");
            assert_eq!(
                receipt.source(),
                morrow_core::file_content_receipt::Source::LiveStaging,
                "{point}"
            );
            let content = content.as_ref().unwrap();
            let container_sha256: [u8; 32] = Sha256::digest(content.container()).into();
            assert_eq!(
                receipt.content_container_sha256(),
                container_sha256,
                "{point}"
            );
        }
        if core_boundary {
            assert_eq!(record.phase(), Phase::Prepared, "{point}");
            assert_eq!(
                record.recovery(),
                Recovery::AwaitFreshAuthorization,
                "{point}"
            );
            assert!(matches!(
                store.io_material(SUBJECT, OPERATION, Kind::Response),
                Err(morrow_core::Error::EvidenceUnavailable)
            ));
        } else if *point == "after-observe" {
            assert_eq!(record.phase(), Phase::Observed, "{point}");
            assert_eq!(record.recovery(), Recovery::AlreadyObserved, "{point}");
            let response = store
                .io_material(SUBJECT, OPERATION, Kind::Response)
                .unwrap()
                .unwrap();
            let outcome = CreateOutcome::decode(response.payload()).unwrap();
            assert_eq!(outcome.result(), CreateResult::Created);
        } else {
            assert_eq!(record.phase(), Phase::OutcomeUnknown, "{point}");
            assert_eq!(record.recovery(), Recovery::ReconcileOnly, "{point}");
            assert!(matches!(
                store.io_material(SUBJECT, OPERATION, Kind::Response),
                Err(morrow_core::Error::EvidenceUnavailable)
            ));
        }
        if !core_boundary {
            // Replace the target name with an unrelated later entry, even
            // when the original operation published first. The spent claim
            // must not run again against that new entry.
            if published {
                fs::remove_file(&target).unwrap();
            }
            fs::write(&target, b"later unrelated file").unwrap();
            let after = store.lookup_io_intent(SUBJECT, OPERATION).unwrap().unwrap();
            assert_eq!(after.container(), record.container(), "{point}");
            if record.phase() == Phase::OutcomeUnknown {
                assert!(matches!(
                    store.claim_file_create_local_authorized(&record, || Ok(())),
                    Err(morrow_core::Error::RevisionConflict)
                ));
            } else {
                assert!(
                    store
                        .claim_file_create_local_authorized(&record, || Ok(()))
                        .is_err()
                );
            }
            assert!(
                store
                    .claim_io_dispatch_local_authorized(&record, || Ok(()))
                    .is_err()
            );
            assert_eq!(fs::read(&target).unwrap(), b"later unrelated file");
        }
        eprintln!(
            "guest create crash {point}: phase={:?} committed={committed} published={published} elapsed_ms={} wasm_sha256={}",
            record.phase(),
            began.elapsed().as_millis(),
            expected_wasm_sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        eprintln!("PASS mutation crash boundary: {point}");
    }
}
