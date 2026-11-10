//! Explicit mutation-budget qualification on the original Windows owner.
//! The 16 MiB body is sent as 274 independent bounded guest calls.
use super::*;
use morrow_core::{
    mutation::{self, Action, Effect, Phase as WirePhase, Request, Status},
    plugin_package::{
        MAX_MUTATION_BYTES, MAX_MUTATION_JOB_BYTES, MUTATION_BUDGET_FEATURE, MUTATION_FEATURE,
        proto::MutationBudget as DeclaredMutationBudget,
    },
};
use morrow_plugin_runtime::{
    io_binding::MutationBudget,
    io_jobs::{MutationBudgetEstimate, MutationGuestJobMode, MutationGuestLease, Poll},
};
use std::io::Read;

const CONTENT_BYTES: usize = 16 * 1024 * 1024;
const OPERATION: &str = "guest-max-create";
const APPROVED: MutationBudget = MutationBudget {
    max_resources: 0,
    max_job_bytes: MAX_MUTATION_JOB_BYTES,
    max_bytes: MAX_MUTATION_BYTES,
};

fn mutation_caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileCreate, IoCapability::FileDelete])
}

fn budget_wat() -> Vec<u8> {
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

fn manifest(wasm: &[u8]) -> morrow_core::plugin_package::proto::Manifest {
    let mut value = Package::manifest_for_task(ID, "1.0.0", wasm, vec![]);
    let mut declaration =
        io::declaration(mutation_caps().into_iter().collect(), vec![HANDLER.into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 4;
    budget.max_resources = 8;
    budget.max_job_bytes = 16 * 1024 * 1024;
    budget.max_bytes = 64 * 1024 * 1024;
    budget.max_duration_ms = 30_000;
    value.io_declaration = Some(declaration);
    value.required_features.push(io::FEATURE.into());
    value.required_features.push(MUTATION_FEATURE.into());
    value.required_features.push(MUTATION_BUDGET_FEATURE.into());
    value.mutation_schema_sha256 = mutation::schema_digest().to_vec();
    value.mutation_budget = Some(DeclaredMutationBudget {
        max_resources: 0,
        max_job_bytes: MAX_MUTATION_JOB_BYTES,
        max_bytes: MAX_MUTATION_BYTES,
    });
    value
}

struct BudgetFixture {
    dir: tempfile::TempDir,
    owner: Owner,
    digest: [u8; 32],
    approved: MutationBudget,
    capabilities: BTreeSet<IoCapability>,
}
impl BudgetFixture {
    fn new(wasm: Vec<u8>, approved: MutationBudget) -> Self {
        let declaration = manifest(&wasm);
        Self::with_manifest(wasm, approved, declaration, mutation_caps())
    }
    fn replacement(wasm: Vec<u8>, resources: u32) -> Self {
        let mut declaration = manifest(&wasm);
        declaration
            .io_declaration
            .as_mut()
            .unwrap()
            .requested_capabilities
            .push(IoCapability::FileReplace.number());
        declaration.mutation_budget.as_mut().unwrap().max_resources = 32;
        Self::with_manifest(
            wasm,
            MutationBudget {
                max_resources: resources,
                ..APPROVED
            },
            declaration,
            BTreeSet::from([IoCapability::FileReplace]),
        )
    }
    fn with_manifest(
        wasm: Vec<u8>,
        approved: MutationBudget,
        declaration: morrow_core::plugin_package::proto::Manifest,
        capabilities: BTreeSet<IoCapability>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = Package::build(declaration, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                capabilities.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        Self {
            dir,
            owner: Owner {
                host,
                manager,
                identity: Arc::new(()),
                gate: None,
                managed_gate: Arc::new(Mutex::new(None)),
            },
            digest: package.digest(),
            approved,
            capabilities,
        }
    }

    fn start(mut self) -> (tempfile::TempDir, IoWorker<Owner>, Instant) {
        let instance = self
            .owner
            .manager
            .connect(ID, &mut self.owner.host)
            .unwrap();
        // Body generation and hashing happen before this trusted millisecond clock starts.
        let started = Instant::now();
        let binding = self
            .owner
            .manager
            .bind_budgeted_mutation(
                &self.owner.host,
                &instance,
                self.digest,
                self.owner.manager.revision(),
                &self.capabilities,
                30_000,
                1,
                self.approved,
            )
            .unwrap();
        let clock = started;
        let worker = IoWorker::spawn_managed_owner(
            self.owner,
            instance,
            binding,
            move || 1 + clock.elapsed().as_millis() as u64,
            1,
            JobLimits::mutation(8, self.approved.max_job_bytes, self.approved.max_bytes).unwrap(),
        )
        .unwrap();
        (self.dir, worker, started)
    }
}

fn body() -> Vec<u8> {
    let mut result = Vec::with_capacity(CONTENT_BYTES);
    let mut counter = 0u64;
    while result.len() < CONTENT_BYTES {
        let mut hash = Sha256::new();
        hash.update(b"morrow-mutation-max-content-v1");
        hash.update(counter.to_le_bytes());
        result.extend_from_slice(&hash.finalize());
        counter += 1;
    }
    result.truncate(CONTENT_BYTES);
    result
}

fn request(serial: u64, lease: MutationGuestLease, operation_id: &str, action: Action) -> Request {
    let mut submission = [0; 32];
    submission[..8].copy_from_slice(&serial.to_le_bytes());
    submission[8] = 0xa5;
    Request {
        call_id: serial,
        reference: lease.reference(),
        submission,
        operation_id: operation_id.into(),
        deadline_ms: 30_000,
        action,
    }
}

fn read_at(
    handle: &mut MutationHandle,
    phase: &str,
    started: Instant,
) -> Result<MutationResponse, TargetError> {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(
            Instant::now() < until,
            "{phase} did not complete after {} ms",
            started.elapsed().as_millis()
        );
        thread::sleep(Duration::from_millis(1));
    }
    match handle.read() {
        Ok(Some(reply)) => reply,
        Ok(None) => panic!(
            "{phase} reply missing after {} ms",
            started.elapsed().as_millis()
        ),
        Err(error) => panic!(
            "{phase} read failed after {} ms: {error:?}",
            started.elapsed().as_millis()
        ),
    }
}

fn selected_at(handle: &mut MutationHandle, phase: &str, started: Instant) {
    assert!(
        matches!(
            read_at(handle, phase, started).unwrap(),
            MutationResponse::Selected { .. }
        ),
        "expected {phase} target selection"
    );
}

fn planned_at(handle: &mut MutationHandle, phase: &str, started: Instant) -> RequestRecord {
    match read_at(handle, phase, started).unwrap() {
        MutationResponse::Planned(plan) => plan,
        other => panic!("expected {phase} canonical plan: {other:?}"),
    }
}

fn call(
    worker: &IoWorker<Owner>,
    mode: MutationGuestJobMode,
    request: Request,
) -> mutation::Response {
    let action = request.action.kind();
    let call_started = Instant::now();
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(30))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while handle.poll() == Poll::Pending {
        assert!(Instant::now() < until, "max-content mutation call stalled");
        thread::sleep(Duration::from_millis(1));
    }
    let report = handle.read(131_072).unwrap().unwrap();
    assert_eq!(
        report.task.execution.outcome,
        Ok(0),
        "call_id={} action={action:?} elapsed_ms={}: {report:?}",
        request.call_id,
        call_started.elapsed().as_millis()
    );
    assert_eq!(report.calls, 1);
    let response = report
        .mutation_response
        .expect("correlated mutation response");
    assert_eq!(response.call_id, request.call_id);
    assert_eq!(response.reference, request.reference);
    assert_eq!(response.submission, request.submission);
    response
}

fn file_sha256(path: &std::path::Path) -> ([u8; 32], u64) {
    let mut file = fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    (hash.finalize().into(), size)
}

/// Runs the same maximum-body recipe against a WAT or a freshly built SDK guest.
pub(super) fn exercise_maximum_module(wasm: Vec<u8>) {
    let _serial = serial_effects();
    let body = body();
    let digest: [u8; 32] = Sha256::digest(&body).into();
    let (dir, mut worker, started) = BudgetFixture::new(wasm, APPROVED).start();
    let root = dir.path().join("maximum-create");
    fs::create_dir(&root).unwrap();
    let created = root.join("payload.bin");
    let (session, mut selection) = worker
        .select_mutation_create(
            root.clone(),
            RelativeFilePath::parse("payload.bin").unwrap(),
            scope(Disposition::Create),
            [201; 32],
        )
        .unwrap();
    selected_at(&mut selection, "select maximum target", started);
    let plan = planned_at(
        &mut worker
            .build_mutation_plan(
                session,
                OPERATION.into(),
                CONTENT_BYTES as u64,
                Some(digest),
            )
            .unwrap(),
        "build maximum plan",
        started,
    );
    let estimate = MutationBudgetEstimate::for_plan(&plan).unwrap();
    assert_eq!(estimate.submissions, 279);
    assert!(estimate.max_admission_bytes <= APPROVED.max_job_bytes);
    let lease = match read_at(
        &mut worker
            .issue_mutation_guest(session, plan, [202; 32])
            .unwrap(),
        "issue maximum guest lease",
        started,
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected guest approval: {other:?}"),
    };
    let baseline = worker.mutation_budget_usage().unwrap();
    let stage = MutationGuestJobMode::Stage(lease);
    let prepared = call(
        &worker,
        stage,
        request(
            1,
            lease,
            OPERATION,
            Action::Create {
                content_length: CONTENT_BYTES as u64,
                content_sha256: digest,
            },
        ),
    );
    assert_eq!(
        (prepared.status, prepared.phase),
        (Status::Completed, WirePhase::Prepared)
    );
    for (index, chunk) in body.chunks(MAX_MUTATION_CHUNK).enumerate() {
        let staged = call(
            &worker,
            stage,
            request(
                index as u64 + 2,
                lease,
                OPERATION,
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
    let commit_serial = 2 + body.chunks(MAX_MUTATION_CHUNK).count() as u64;
    assert!(
        call(
            &worker,
            stage,
            request(commit_serial, lease, OPERATION, Action::Commit)
        )
        .durable_content
    );
    assert!(!created.exists(), "commit must not perform the OS effect");
    let permit = match read_at(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
        "authorize maximum execution",
        started,
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected delivered execution permit: {other:?}"),
    };
    let effect = call(
        &worker,
        MutationGuestJobMode::Execute(permit),
        request(commit_serial + 1, lease, OPERATION, Action::Execute),
    );
    assert_eq!(
        (effect.status, effect.phase, effect.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    let observed = call(
        &worker,
        stage,
        request(commit_serial + 2, lease, OPERATION, Action::Query),
    );
    assert_eq!(
        (observed.status, observed.phase, observed.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(
        call(
            &worker,
            stage,
            request(commit_serial + 3, lease, OPERATION, Action::Release)
        )
        .status,
        Status::Completed
    );
    let used = worker.mutation_budget_usage().unwrap();
    assert!(used.worker_bytes >= baseline.worker_bytes);
    assert!(used.instance_bytes >= baseline.instance_bytes);
    assert!(
        used.worker_bytes - baseline.worker_bytes <= estimate.worker_bytes,
        "worker estimate underestimated: {baseline:?} -> {used:?}, {estimate:?}"
    );
    assert!(
        used.instance_bytes - baseline.instance_bytes <= estimate.instance_bytes,
        "instance estimate underestimated: {baseline:?} -> {used:?}, {estimate:?}"
    );
    let flow_elapsed = started.elapsed();
    eprintln!(
        "16 MiB mutation complete in {} ms; worker={} instance={}",
        flow_elapsed.as_millis(),
        used.worker_bytes,
        used.instance_bytes
    );

    // Release did not refund either cumulative ledger. The same-size second
    // plan can be selected but IssueGuest must refuse it before durable Prepare.
    let second = root.join("second.bin");
    let (second_session, mut second_selection) = worker
        .select_mutation_create(
            root,
            RelativeFilePath::parse("second.bin").unwrap(),
            scope(Disposition::Create),
            [203; 32],
        )
        .unwrap();
    selected_at(&mut second_selection, "select second target", started);
    let second_plan = planned_at(
        &mut worker
            .build_mutation_plan(
                second_session,
                "guest-max-second".into(),
                CONTENT_BYTES as u64,
                Some(digest),
            )
            .unwrap(),
        "build second plan",
        started,
    );
    assert!(matches!(
        read_at(
            &mut worker
                .issue_mutation_guest(second_session, second_plan, [204; 32])
                .unwrap(),
            "reject second guest lease",
            started,
        ),
        Err(TargetError::Limit)
    ));
    assert!(!second.exists());

    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    assert!(
        exit.owner
            .host
            .store_local()
            .lookup_io_intent(SUBJECT, "guest-max-second")
            .unwrap()
            .is_none()
    );
    assert_eq!(file_sha256(&created), (digest, CONTENT_BYTES as u64));
}

#[test]
fn wat_guest_completes_exact_maximum_body_with_measured_budget() {
    exercise_maximum_module(budget_wat());
}

#[test]
fn budget_profile_and_maximum_plan_fail_before_durable_prepare() {
    let _serial = serial_effects();
    let wasm = budget_wat();
    let mut missing = manifest(&wasm);
    missing.mutation_budget = None;
    assert!(Package::build(missing, &wasm).is_err());
    let mut unversioned = manifest(&wasm);
    unversioned
        .required_features
        .retain(|value| value != MUTATION_BUDGET_FEATURE);
    assert!(Package::build(unversioned, &wasm).is_err());
    let mut excess = manifest(&wasm);
    excess.mutation_budget.as_mut().unwrap().max_bytes = MAX_MUTATION_BYTES + 1;
    assert!(Package::build(excess, &wasm).is_err());

    // Per-admission and cumulative shortfalls are distinct host approvals.
    // Neither may issue a guest lease for a recipe known not to fit.
    for (index, approved) in [
        MutationBudget {
            max_resources: 0,
            max_job_bytes: CONTENT_BYTES as u64,
            max_bytes: MAX_MUTATION_BYTES,
        },
        MutationBudget {
            max_resources: 0,
            max_job_bytes: MAX_MUTATION_JOB_BYTES,
            max_bytes: 64 * 1024 * 1024,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let (dir, mut worker, started) = BudgetFixture::new(wasm.clone(), approved).start();
        let root = dir.path().join("denied-maximum");
        fs::create_dir(&root).unwrap();
        let path = root.join("payload.bin");
        let (session, mut selection) = worker
            .select_mutation_create(
                root,
                RelativeFilePath::parse("payload.bin").unwrap(),
                scope(Disposition::Create),
                [210 + index as u8; 32],
            )
            .unwrap();
        selected_at(&mut selection, "select denied target", started);
        let digest = [31; 32];
        assert!(matches!(
            worker.build_mutation_plan(
                session,
                "guest-max-plus-one".into(),
                CONTENT_BYTES as u64 + 1,
                Some(digest),
            ),
            Err(OwnerCommandError::Limit)
        ));
        let plan = planned_at(
            &mut worker
                .build_mutation_plan(
                    session,
                    "guest-max-denied".into(),
                    CONTENT_BYTES as u64,
                    Some(digest),
                )
                .unwrap(),
            "build denied plan",
            started,
        );
        let before = worker.mutation_budget_usage().unwrap();
        assert!(matches!(
            read_at(
                &mut worker
                    .issue_mutation_guest(session, plan, [220 + index as u8; 32],)
                    .unwrap(),
                "reject denied guest lease",
                started,
            ),
            Err(TargetError::Limit)
        ));
        let after = worker.mutation_budget_usage().unwrap();
        assert!(after.worker_bytes >= before.worker_bytes);
        assert!(after.instance_bytes >= before.instance_bytes);
        assert!(!path.exists());
        let exit = reclaim(&mut worker);
        assert_eq!(exit.disconnect, Ok(()));
        assert!(
            exit.owner
                .host
                .store_local()
                .lookup_io_intent(SUBJECT, "guest-max-plus-one")
                .unwrap()
                .is_none()
        );
        assert!(
            exit.owner
                .host
                .store_local()
                .lookup_io_intent(SUBJECT, "guest-max-denied")
                .unwrap()
                .is_none()
        );
    }
}

/// A real guest, original managed owner and deep target identity. Scope is
/// temporary Windows test files; no host account or policy setup is involved.
pub(super) fn exercise_replacement_module(wasm: Vec<u8>) {
    let _serial = serial_effects();
    const ORIGINAL: &[u8] = b"original selected bytes\x00\xff";
    const OP: &str = "guest-conditional-replace";
    // Both omitted and ordinary approvals must fail before a deep native open.
    for resources in [0, 8] {
        let (dir, mut worker, started) =
            BudgetFixture::replacement(wasm.clone(), resources).start();
        let parent = dir.path().join("selected/a/b/c/d/e/f/g/h/i");
        fs::create_dir_all(&parent).unwrap();
        let target = parent.join("target.bin");
        fs::write(&target, ORIGINAL).unwrap();
        let (_, mut selection) = worker
            .select_mutation_existing(target.clone(), scope(Disposition::Replace), [231; 32])
            .unwrap();
        assert!(matches!(
            read_at(&mut selection, "deny unapproved resources", started),
            Err(TargetError::Admission(
                morrow_plugin_runtime::io_binding::Error::Limit
            ))
        ));
        assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
        let exit = reclaim(&mut worker);
        assert_eq!(exit.disconnect, Ok(()));
        assert!(
            exit.owner
                .host
                .store_local()
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .is_none()
        );
    }
    // Cancellation after durable content staging proves that no native replace
    // occurred before the separate Execute approval. Read only after releasing
    // the original exclusive selection, never bypass its sharing protection.
    {
        let bytes = [0u8, 255];
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        let (dir, mut worker, started) = BudgetFixture::replacement(wasm.clone(), 32).start();
        let parent = dir.path().join("cancel/a/b/c/d/e/f/g/h/i");
        fs::create_dir_all(&parent).unwrap();
        let target = parent.join("original.bin");
        fs::write(&target, ORIGINAL).unwrap();
        let (session, mut selected) = worker
            .select_mutation_existing(target.clone(), scope(Disposition::Replace), [234; 32])
            .unwrap();
        selected_at(&mut selected, "select cancellable replacement", started);
        let plan = planned_at(
            &mut worker
                .build_mutation_plan(session, OP.into(), bytes.len() as u64, Some(digest))
                .unwrap(),
            "build cancellable replacement",
            started,
        );
        let lease = match read_at(
            &mut worker
                .issue_mutation_guest(session, plan, [235; 32])
                .unwrap(),
            "issue cancellable replacement",
            started,
        )
        .unwrap()
        {
            MutationResponse::GuestApproved(lease) => lease,
            other => panic!("guest lease: {other:?}"),
        };
        let stage = MutationGuestJobMode::Stage(lease);
        assert_eq!(
            call(
                &worker,
                stage,
                request(
                    1,
                    lease,
                    OP,
                    Action::Replace {
                        content_length: bytes.len() as u64,
                        content_sha256: digest
                    }
                )
            )
            .phase,
            WirePhase::Prepared
        );
        assert_eq!(
            call(
                &worker,
                stage,
                request(
                    2,
                    lease,
                    OP,
                    Action::Chunk {
                        offset: 0,
                        bytes: bytes.to_vec()
                    }
                )
            )
            .staged_bytes,
            2
        );
        assert!(call(&worker, stage, request(3, lease, OP, Action::Commit)).durable_content);
        assert_eq!(
            call(&worker, stage, request(4, lease, OP, Action::CancelPlan)).phase,
            WirePhase::CancelledBeforeDispatch
        );
        assert_eq!(
            call(&worker, stage, request(5, lease, OP, Action::Release)).status,
            Status::Completed
        );
        assert_eq!(fs::read(&target).unwrap(), ORIGINAL);
        let exit = reclaim(&mut worker);
        assert_eq!(exit.disconnect, Ok(()));
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .unwrap()
                .phase(),
            Phase::CancelledBeforeDispatch
        );
        exit.owner.host.store_local().integrity_check().unwrap();
    }
    for bytes in [Vec::new(), vec![0, 255, 1, 0, 128], body()] {
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        let (dir, mut worker, started) = BudgetFixture::replacement(wasm.clone(), 32).start();
        let parent = dir.path().join("selected/a/b/c/d/e/f/g/h/i");
        fs::create_dir_all(&parent).unwrap();
        let target = parent.join("target.bin");
        fs::write(&target, ORIGINAL).unwrap();
        let alias = parent.join("original-link.bin");
        fs::hard_link(&target, &alias).unwrap();
        let (session, mut selection) = worker
            .select_mutation_existing(target.clone(), scope(Disposition::Replace), [232; 32])
            .unwrap();
        selected_at(&mut selection, "select deep original target", started);
        let plan = planned_at(
            &mut worker
                .build_mutation_plan(session, OP.into(), bytes.len() as u64, Some(digest))
                .unwrap(),
            "build replacement",
            started,
        );
        let estimate = MutationBudgetEstimate::for_plan(&plan).unwrap();
        let lease = match read_at(
            &mut worker
                .issue_mutation_guest(session, plan, [233; 32])
                .unwrap(),
            "issue replacement guest",
            started,
        )
        .unwrap()
        {
            MutationResponse::GuestApproved(lease) => lease,
            other => panic!("guest lease: {other:?}"),
        };
        let stage = MutationGuestJobMode::Stage(lease);
        let baseline = worker.mutation_budget_usage().unwrap();
        // A Create payload cannot act on a reviewed Replace plan, even with
        // identical bytes. The original owner rejects it before persistence.
        let wrong = request(
            900,
            lease,
            OP,
            Action::Create {
                content_length: bytes.len() as u64,
                content_sha256: digest,
            },
        );
        let mut job = worker
            .submit_mutation_guest_frame(wrong.encode().unwrap(), stage, Duration::from_secs(10))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while job.poll() == Poll::Pending {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            job.read(131072).unwrap().unwrap().task.execution.outcome,
            Err(morrow_plugin_runtime::Fault::TaskProtocol)
        );
        assert_eq!(
            fs::read(&target).unwrap_err().raw_os_error(),
            Some(32),
            "original selection remains exclusively held"
        );
        let prepared = call(
            &worker,
            stage,
            request(
                1,
                lease,
                OP,
                Action::Replace {
                    content_length: bytes.len() as u64,
                    content_sha256: digest,
                },
            ),
        );
        assert_eq!(
            (prepared.kind, prepared.status, prepared.phase),
            (
                mutation::Kind::PrepareReplace,
                Status::Completed,
                WirePhase::Prepared
            )
        );
        for (i, chunk) in bytes.chunks(MAX_MUTATION_CHUNK).enumerate() {
            let response = call(
                &worker,
                stage,
                request(
                    i as u64 + 2,
                    lease,
                    OP,
                    Action::Chunk {
                        offset: (i * MAX_MUTATION_CHUNK) as u64,
                        bytes: chunk.to_vec(),
                    },
                ),
            );
            assert_eq!(response.status, Status::Completed);
        }
        let serial = 2 + bytes.chunks(MAX_MUTATION_CHUNK).count() as u64;
        let committed = call(&worker, stage, request(serial, lease, OP, Action::Commit));
        assert!(committed.durable_content);
        assert_eq!(committed.staged_bytes, bytes.len() as u64);
        assert_eq!(
            fs::read(&target).unwrap_err().raw_os_error(),
            Some(32),
            "staging must retain the locked original selection"
        );
        let execute = request(serial + 1, lease, OP, Action::Execute);
        assert!(
            worker
                .submit_mutation_guest_frame(
                    execute.encode().unwrap(),
                    stage,
                    Duration::from_secs(10)
                )
                .is_err()
        );
        let permit = match read_at(
            &mut worker
                .authorize_mutation_guest_execution(session, lease.plan_sha256())
                .unwrap(),
            "authorize original replacement",
            started,
        )
        .unwrap()
        {
            MutationResponse::GuestExecutionAuthorized(permit) => permit,
            other => panic!("execution permit: {other:?}"),
        };
        let mode = MutationGuestJobMode::Execute(permit);
        let effect = call(&worker, mode, execute.clone());
        assert_eq!(
            (effect.status, effect.phase, effect.effect),
            (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
        );
        assert_eq!(file_sha256(&target), (digest, bytes.len() as u64));
        assert_eq!(
            fs::read(&alias).unwrap(),
            ORIGINAL,
            "an existing hard link retains the original file"
        );
        // Explicit retry reads the cached original observation and cannot apply
        // the payload again after a later local edit of this synthetic file.
        fs::write(&target, b"later independent edit").unwrap();
        let mut retry = execute;
        retry.call_id = 901;
        assert_eq!(call(&worker, mode, retry).effect, Effect::OsSucceeded);
        assert_eq!(fs::read(&target).unwrap(), b"later independent edit");
        let history = call(
            &worker,
            stage,
            request(serial + 2, lease, OP, Action::Query),
        );
        assert_eq!(
            (history.phase, history.effect),
            (WirePhase::Observed, Effect::OsSucceeded)
        );
        assert_eq!(
            call(
                &worker,
                stage,
                request(serial + 3, lease, OP, Action::Release)
            )
            .status,
            Status::Completed
        );
        let used = worker.mutation_budget_usage().unwrap();
        assert!(
            used.worker_bytes > baseline.worker_bytes
                && used.instance_bytes > baseline.instance_bytes
        );
        // Additional rejected call and explicit retry are outside the standard
        // recipe estimate, so do not use that estimate as measured total usage.
        assert!(estimate.max_admission_bytes <= APPROVED.max_job_bytes);
        let exit = reclaim(&mut worker);
        assert_eq!(exit.disconnect, Ok(()));
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .unwrap()
                .phase(),
            Phase::Observed
        );
        exit.owner.host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn wat_guest_replaces_deep_original_identity_only_under_explicit_resource_approval() {
    exercise_replacement_module(budget_wat());
}
