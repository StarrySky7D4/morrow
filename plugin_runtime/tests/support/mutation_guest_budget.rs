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
}
impl BudgetFixture {
    fn new(wasm: Vec<u8>, approved: MutationBudget) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = Package::build(manifest(&wasm), &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), mutation_caps(), manager.revision())
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
                &mutation_caps(),
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
            max_job_bytes: CONTENT_BYTES as u64,
            max_bytes: MAX_MUTATION_BYTES,
        },
        MutationBudget {
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
