//! Actual managed guest IO history reads over ordinary synthetic Stores only.
//! One existing broker call produces a synthetic HTTP 503; every history query
//! is read-only with no additional effect, public network or protected owner.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{HttpOutcome, HttpSubmission, OperationOutcome, Request, Response, Status},
    io_evidence::{Kind, Material},
    io_intent::{Command, ObservationSource, Record},
    plugin_package::{Package, catalog::Catalog, io::{self, IoCapability}, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    io_binding::IoBinding,
    io_history::OperationHistoryGrant,
    io_jobs::{BrokerRouter, HostOwner, IoWorker, JobError, JobHandle, JobLimits, JobReport,
        Poll, RouteContext, Router, RouterFault},
    manager::Manager,
};
use morrow_plugin_sdk::io as sdk_io;
use std::{
    collections::BTreeSet,
    sync::{Arc, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}},
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.io.history.guest";
const OP: &str = "original-http-operation";
const WAIT: Duration = Duration::from_secs(10);
const EXPIRES: u64 = 20_000;
#[derive(Clone, Copy)]
enum History { Missing, Prepared, Cancelled, Unknown, Observed(u16), Reconciled, Foreign }
struct Owner {
    host: HostRuntime,
    prepare: Arc<AtomicUsize>,
    finish: Arc<AtomicUsize>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime { &self.host }
    fn runtime_mut(&mut self) -> &mut HostRuntime { &mut self.host }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.prepare.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.finish.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
struct Running {
    manager: Manager,
    worker: IoWorker<Owner>,
    observer: IoBinding,
    grant: OperationHistoryGrant,
    clock: Arc<AtomicU64>,
    prepare: Arc<AtomicUsize>,
    finish: Arc<AtomicUsize>,
    command: Command,
    stored: Option<Vec<u8>>,
    stored_subject: String,
    events: usize,
    material_usage: (u64, u64),
    followup_usage: (u64, u64),
    produced_response: Option<Vec<u8>>,
    // Owner, guest, registry and worker handles must release before TempDir.
    _dir: tempfile::TempDir,
}
fn original_request() -> Request {
    Request::encode_http_submit(1, &HttpSubmission {
        operation_id: OP.as_bytes().to_vec(), deadline_ms: 50,
        endpoint: b"synthetic-endpoint".to_vec(), method: "POST".into(),
        relative_target: "/history".into(), headers: vec![],
        body: b"retained-original-request-secret".to_vec(), credential: vec![],
    }).unwrap()
}
fn query(call_id: u64, operation: &str) -> sdk_io::Request {
    // The existing SDK encoder and schema are unmodified.
    sdk_io::Request::new(call_id, sdk_io::Action::QueryOperation {
        operation_id: operation.as_bytes().to_vec(),
    }).unwrap()
}
fn query_cost(q: &sdk_io::Request) -> u64 {
    2 * q.bytes().len() as u64 + morrow_core::io::MAX_OPERATION_RESPONSE_BYTES as u64
}
impl Running {
    fn new(history: History) -> Self { Self::limits(history, 4096, 16384, 16384, false, false) }
    fn limits(history: History, per_job: u64, worker_total: u64, declared_total: u64,
              double_call: bool, invalid_completion: bool) -> Self {
        Self::configured(history, per_job, worker_total, declared_total, double_call, invalid_completion, None)
    }
    fn configured(history: History, per_job: u64, worker_total: u64, declared_total: u64,
                  double_call: bool, invalid_completion: bool, live: Option<Arc<AtomicBool>>) -> Self {
        Self::budgeted(history, per_job, worker_total, declared_total, double_call, invalid_completion, live, 0)
    }
    fn budgeted(history: History, per_job: u64, worker_total: u64, declared_total: u64,
                double_call: bool, invalid_completion: bool, live: Option<Arc<AtomicBool>>,
                binding_precharge: u64) -> Self {
        assert!(worker_total <= declared_total, "worker ceiling must respect the package declaration");
        let dir = tempfile::tempdir().unwrap();
        let middle = if double_call {
            "drop drop i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $io"
        } else { "" };
        let completion = if invalid_completion {
            "drop drop i32.const 0 i32.const 1 call $done"
        } else { "call $done" };
        let wasm = wat::parse_str(format!(r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) (local $n i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
            call $io {middle} {completion} drop i32.const 0))"#)).unwrap();
        let capabilities = BTreeSet::from([IoCapability::HttpRequest]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(vec![IoCapability::HttpRequest], vec!["io.invoke".into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 1;
        budget.max_resources = 1;
        budget.max_job_bytes = per_job;
        budget.max_bytes = declared_total;
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager.approve_io(ID, digest, capabilities.clone(), manager.revision()).unwrap();
        manager.set_enabled(ID, digest, true, manager.revision()).unwrap();
        let original = original_request();
        let command = Command {
            operation_id: OP.into(), subject: ID.into(), package_sha256: digest,
            capability: IoCapability::HttpRequest, protocol_sha256: morrow_core::io::schema_digest(),
            request_sha256: original.digest(), approval_sha256: [3; 32], target_sha256: [4; 32],
            request_bytes: original.bytes().len() as u64, response_limit: 1024,
        };
        let path = dir.path().join("synthetic.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut historical = command.clone();
        if matches!(history, History::Foreign) { historical.subject = "another-plugin".into(); }
        if !matches!(history, History::Missing) {
            let prepared = Record::prepared(historical.clone()).unwrap();
            store.append_io_intent_local_authorized(&prepared, || Ok(())).unwrap();
            if matches!(history, History::Cancelled) {
                store.append_io_intent_local_authorized(
                    &prepared.propose_cancel_before_dispatch().unwrap(), || Ok(())).unwrap();
            } else if matches!(history, History::Unknown | History::Observed(_) | History::Reconciled) {
                store.reserve_io_materials(&historical, || Ok(())).unwrap();
                let request_material = Material::encode(Kind::Request, OP, ID,
                    original.digest(), original.bytes()).unwrap();
                store.store_io_material(ID, Kind::Request, &request_material, || Ok(())).unwrap();
                store.reserve_io_intent_followup(&historical, || Ok(())).unwrap();
                // Synthetic historical append, never an executable dispatch claim.
                let unknown = prepared.propose_dispatch_boundary().unwrap();
                store.append_io_intent_local_authorized(&unknown, || Ok(())).unwrap();
                if let History::Observed(http_status) = history {
                    let reply = Response::encode_http(&original, &HttpOutcome {
                        status: Status::Completed, http_status, headers: vec![],
                        body: b"retained-original-response-secret".to_vec(),
                    }).unwrap();
                    let material = Material::encode(Kind::Response, OP, ID, original.digest(), &reply).unwrap();
                    store.store_io_material(ID, Kind::Response, &material, || Ok(())).unwrap();
                    let observed = unknown.propose_observation(material.payload_sha256(), ObservationSource::OriginalResponse).unwrap();
                    store.append_io_intent_local_authorized(&observed, || Ok(())).unwrap();
                } else if matches!(history, History::Reconciled) {
                    store.release_io_material_reconciliation(ID, OP, Kind::Response).unwrap();
                    let observed = unknown.propose_observation([8; 32], ObservationSource::Reconciliation).unwrap();
                    store.append_io_intent_local_authorized(&observed, || Ok(())).unwrap();
                }
            }
        }
        // Actual close/reopen: Unknown must survive without being resent or observed.
        drop(store);
        let store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let stored_subject = historical.subject.clone();
        let stored = store.lookup_io_intent(&stored_subject, OP).unwrap().map(|r| r.container().to_vec());
        let events = store.pending(0, 100).unwrap().len();
        let material_usage = store.io_material_reservation_usage().unwrap();
        let followup_usage = store.io_intent_reservation_usage().unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager.bind_io(&host, &instance, digest, manager.revision(), &capabilities, EXPIRES, 1).unwrap();
        let observer = manager.bind_io(&host, &instance, digest, manager.revision(), &capabilities, EXPIRES, 1).unwrap();
        if binding_precharge != 0 {
            // Consume only shared-context accounting through the existing public
            // admission API; dropping this zero-resource lease refunds no bytes.
            drop(binding.admit(&manager, &host, &instance, IoCapability::HttpRequest,
                0, binding_precharge, 1).unwrap());
            assert_eq!(observer.usage().resources, 0);
            assert_eq!(observer.usage().jobs, 0);
            assert_eq!(observer.usage().bytes, binding_precharge);
        }
        let grant = OperationHistoryGrant::issue(&manager, &host, &instance, &binding, command.clone(), 1).unwrap();
        let grant = match live {
            Some(live) => grant.with_live_guard(move || live.load(Ordering::SeqCst)).unwrap(),
            None => grant,
        };
        let clock = Arc::new(AtomicU64::new(2));
        let ticks = clock.clone();
        let prepare = Arc::new(AtomicUsize::new(0));
        let finish = Arc::new(AtomicUsize::new(0));
        let owner = Owner { host, prepare: prepare.clone(), finish: finish.clone() };
        let worker = IoWorker::spawn_managed_owned(&manager, owner, instance, binding,
            move || ticks.load(Ordering::SeqCst), 1, JobLimits::new(1, per_job, worker_total).unwrap()).unwrap();
        Self { manager, worker, observer, grant, clock, prepare, finish, command,
            stored, stored_subject, events, material_usage, followup_usage, produced_response: None, _dir: dir }
    }
    fn submit(&self, q: &sdk_io::Request) -> JobHandle {
        self.worker.submit_operation_history(q.bytes().to_vec(), self.grant.clone(), WAIT).unwrap()
    }
    fn finish(&mut self) -> HostRuntime {
        self.worker.stop();
        let end = Instant::now() + WAIT;
        loop {
            if let Some(exit) = self.worker.try_reclaim().unwrap() {
                assert_eq!(exit.result, Ok(()));
                assert_eq!(exit.maintenance, Ok(()));
                assert_eq!(exit.disconnect, Ok(()));
                assert!(exit.instance.is_none());
                assert_eq!(self.finish.load(Ordering::SeqCst), 1);
                assert_eq!(self.observer.usage().resources, 0);
                assert_eq!(self.observer.usage().jobs, 0);
                let host = exit.owner.host;
                let store = host.store_local();
                store.integrity_check().unwrap();
                if let Some(reply) = &self.produced_response {
                    // The first real brokered job legitimately produced this history.
                    // Subsequent read-only queries must preserve its exact originals.
                    let original = original_request();
                    let material = Material::encode(Kind::Response, OP, ID, original.digest(), reply).unwrap();
                    let expected = Record::prepared(self.command.clone()).unwrap()
                        .propose_dispatch_boundary().unwrap()
                        .propose_observation(material.payload_sha256(), ObservationSource::OriginalResponse).unwrap();
                    assert_eq!(store.lookup_io_intent(ID, OP).unwrap().unwrap().container(), expected.container());
                    assert_eq!(store.io_material(ID, OP, Kind::Request).unwrap().unwrap().payload(), original.bytes());
                    assert_eq!(store.io_material(ID, OP, Kind::Response).unwrap().unwrap().payload(), reply);
                    assert_eq!(store.pending(0, 100).unwrap().len(), self.events + 3);
                    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
                    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
                } else {
                    assert_eq!(store.lookup_io_intent(&self.stored_subject, OP).unwrap().map(|r| r.container().to_vec()), self.stored);
                    assert_eq!(store.pending(0, 100).unwrap().len(), self.events);
                    assert_eq!(store.io_material_reservation_usage().unwrap(), self.material_usage);
                    assert_eq!(store.io_intent_reservation_usage().unwrap(), self.followup_usage);
                }
                return host;
            }
            assert!(Instant::now() < end, "actual worker join not completed");
            thread::sleep(Duration::from_millis(1));
        }
    }
}
fn ready(job: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end, "guest did not reach Ready");
                thread::sleep(Duration::from_millis(1));
            }
            state => panic!("unexpected state {state:?}"),
        }
    }
}
fn consume(job: &mut JobHandle) -> JobReport { ready(job); job.read(131072).unwrap().unwrap() }
fn clean_denial(report: &JobReport) {
    assert!(report.task.execution.outcome.is_err());
    assert!(report.operation_response.is_none());
    assert!(report.operation_frame.is_none());
    assert!(report.http_response.is_none());
    assert!(report.response.is_none());
    assert!(!report.unknown, "a read-only query cannot create a new unknown effect");
}
fn assert_reply(q: &sdk_io::Request, report: &JobReport, status: Status, http_status: u16) {
    assert_eq!(report.task.execution.outcome, Ok(0));
    assert_eq!(report.calls, 1);
    assert_eq!(report.bytes, query_cost(q));
    assert!(report.task.execution.host_calls > 0, "must execute actual guest imports");
    assert!(!report.cancelled);
    assert!(!report.unknown);
    assert!(report.http_response.is_none());
    let frame = report.operation_frame.as_ref().unwrap();
    assert!(frame.len() <= morrow_core::io::MAX_OPERATION_RESPONSE_BYTES);
    assert_eq!(report.payload_bytes(), frame.len());
    let core = Request::decode(q.bytes()).unwrap();
    let expected = OperationOutcome { status, http_status };
    assert_eq!(Response::decode_operation(&core, frame).unwrap(), expected);
    assert_eq!(report.operation_response, Some(expected));
    let old = sdk_io::Response::decode(q, frame).unwrap();
    assert_eq!(old.status as u16, status as u16);
    assert_eq!(old.http_status, http_status);
    assert!(old.bytes.is_empty());
    assert!(old.headers.is_empty());
    assert!(old.reference.is_empty());
    assert_eq!(old.offset, 0);
    assert!(old.eof);
    let wrong_call = query(q.call_id() + 1, OP);
    assert!(sdk_io::Response::decode(&wrong_call, frame).is_err());
    assert!(Response::decode_operation(&Request::decode(wrong_call.bytes()).unwrap(), frame).is_err());
    let wrong_hash = query(q.call_id(), "another-operation");
    assert!(sdk_io::Response::decode(&wrong_hash, frame).is_err());
    assert!(Response::decode_operation(&Request::decode(wrong_hash.bytes()).unwrap(), frame).is_err());
}
#[test]
fn original_response_is_bodyless_http_completion_including_server_error() {
    for code in [201, 503] {
        let mut run = Running::new(History::Observed(code));
        let q = query(7, OP);
        let mut job = run.submit(&q);
        let report = consume(&mut job);
        assert_reply(&q, &report, Status::Completed, code);
        assert_eq!(run.worker.bytes(), query_cost(&q));
        assert_eq!(run.observer.usage().bytes, query_cost(&q));
        assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
        drop(run.finish());
    }
}
#[test]
fn prepared_cancelled_and_reconciliation_are_historical_facts_not_fake_completion() {
    for (history, status) in [(History::Prepared, Status::Pending),
        (History::Cancelled, Status::Cancelled), (History::Reconciled, Status::EvidenceUnavailable)] {
        let mut run = Running::new(history);
        let q = query(8, OP);
        let report = consume(&mut run.submit(&q));
        assert_reply(&q, &report, status, 0);
        assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
        drop(run.finish());
    }
}
#[test]
fn reopened_unknown_remains_unknown_across_two_new_calls_without_claim_or_replay() {
    let mut run = Running::new(History::Unknown);
    for call in [9, 10] {
        let q = query(call, OP);
        let report = consume(&mut run.submit(&q));
        assert_reply(&q, &report, Status::OutcomeUnknown, 0);
        assert_eq!(run.observer.usage().jobs, 0);
        assert_eq!(run.observer.usage().resources, 0);
    }
    assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
    drop(run.finish());
}
#[test]
fn missing_and_foreign_subject_are_the_same_bodyless_status() {
    for history in [History::Missing, History::Foreign] {
        let mut run = Running::new(history);
        let q = query(11, OP);
        assert_reply(&q, &consume(&mut run.submit(&q)), Status::NotFound, 0);
        assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
        drop(run.finish());
    }
}
#[test]
fn wrong_operation_and_nonquery_never_reach_historical_route() {
    let mut run = Running::new(History::Observed(200));
    let wrong = query(12, "another-operation");
    clean_denial(&consume(&mut run.submit(&wrong)));
    let original = original_request();
    let mut job = run.worker.submit_operation_history(original.bytes().to_vec(), run.grant.clone(), WAIT).unwrap();
    let denied = consume(&mut job);
    clean_denial(&denied);
    assert_eq!(denied.calls, 0);
    assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
    drop(run.finish());
}
#[test]
fn foreign_owner_grant_rejects_before_admission_without_using_job_or_byte_quota() {
    let mut a = Running::new(History::Missing);
    let mut b = Running::new(History::Observed(200));
    let before = a.observer.usage();
    assert_eq!(a.worker.submit_operation_history(query(13, OP).bytes().to_vec(), b.grant.clone(), WAIT).err(), Some(JobError::InvalidOptions));
    assert_eq!(a.observer.usage(), before);
    assert_eq!(a.worker.bytes(), 0);
    assert_eq!(a.worker.pending(), 0);
    assert_eq!(a.prepare.load(Ordering::SeqCst), 0);
    drop(a.finish());
    drop(b.finish());
}
#[test]
fn ready_revocation_suppresses_completed_and_missing_without_refunding_either_budget() {
    for history in [History::Observed(200), History::Missing] {
        let mut run = Running::new(history);
        let q = query(14, OP);
        let mut job = run.submit(&q);
        ready(&mut job);
        assert_eq!(run.observer.usage().jobs, 1);
        run.grant.revoke();
        let report = job.read(131072).unwrap().unwrap();
        clean_denial(&report);
        assert!(report.cancelled);
        assert_eq!(report.bytes, query_cost(&q));
        assert_eq!(run.observer.usage().bytes, query_cost(&q));
        assert_eq!(run.worker.bytes(), query_cost(&q));
        assert_eq!(run.observer.usage().jobs, 0);
        drop(run.finish());
    }
}
#[test]
fn ready_expiration_suppresses_completed_and_missing_on_fresh_original_clock() {
    for history in [History::Observed(200), History::Missing] {
        let mut run = Running::new(history);
        let q = query(15, OP);
        let mut job = run.submit(&q);
        ready(&mut job);
        run.clock.store(EXPIRES, Ordering::SeqCst);
        let report = job.read(131072).unwrap().unwrap();
        clean_denial(&report);
        assert_eq!(report.task.execution.outcome, Err(Fault::Deadline));
        assert_eq!(run.worker.bytes(), query_cost(&q));
        drop(run.finish());
    }
}
#[test]
fn ready_cancel_suppresses_completed_and_missing_without_new_unknown_effect() {
    for history in [History::Observed(200), History::Missing] {
        let mut run = Running::new(history);
        let q = query(16, OP);
        let mut job = run.submit(&q);
        ready(&mut job);
        job.cancel();
        let report = job.read(131072).unwrap().unwrap();
        clean_denial(&report);
        assert_eq!(report.task.execution.outcome, Err(Fault::Cancelled));
        assert_eq!(run.observer.usage().bytes, query_cost(&q));
        drop(run.finish());
    }
}
#[test]
fn ready_registry_generation_change_suppresses_retained_history() {
    let mut run = Running::new(History::Observed(200));
    let mut job = run.submit(&query(17, OP));
    ready(&mut job);
    run.manager.set_enabled(ID, run.command.package_sha256, false, run.manager.revision()).unwrap();
    clean_denial(&job.read(131072).unwrap().unwrap());
    drop(run.finish());
}
#[test]
fn request_and_reply_are_reserved_before_lookup_with_both_original_budget_caps() {
    let q = query(18, OP);
    let cost = query_cost(&q);
    // Per-job cap, previously consumed worker total, and independently consumed
    // IO-context total. All trusted worker ceilings obey the package declaration.
    // The last case leaves the worker room for a full query while shared IO
    // accounting is one byte short after a released, zero-resource admission.
    for (per_job, worker_total, declared_total, first_query, binding_precharge) in [
        (cost - 1, 16384, 16384, false, 0),
        (cost, 2 * cost - 1, 16384, true, 0),
        (cost, 2 * cost - 1, 2 * cost - 1, false, cost),
    ] {
        let mut run = Running::budgeted(History::Unknown, per_job, worker_total,
            declared_total, false, false, None, binding_precharge);
        let previously_charged = if first_query {
            let first = consume(&mut run.submit(&q));
            assert_reply(&q, &first, Status::OutcomeUnknown, 0);
            assert_eq!(first.bytes, cost);
            cost
        } else { 0 };
        let report = consume(&mut run.submit(&q));
        clean_denial(&report);
        assert_eq!(report.task.execution.outcome, Err(Fault::Limits));
        assert_eq!(report.calls, 0);
        assert_eq!(report.bytes, q.bytes().len() as u64);
        assert_eq!(run.observer.usage().jobs, 0);
        assert_eq!(run.observer.usage().resources, 0);
        assert_eq!(run.observer.usage().bytes, binding_precharge + previously_charged + q.bytes().len() as u64);
        assert_eq!(run.worker.bytes(), previously_charged + q.bytes().len() as u64);
        assert_eq!(run.prepare.load(Ordering::SeqCst), 0);
        drop(run.finish());
    }
}
#[test]
fn ready_live_guard_failure_is_permanent_for_completed_and_missing() {
    for history in [History::Observed(200), History::Missing] {
        let live = Arc::new(AtomicBool::new(true));
        let mut run = Running::configured(history, 4096, 16384, 16384, false, false, Some(live.clone()));
        let q = query(22, OP);
        let mut job = run.submit(&q);
        ready(&mut job);
        live.store(false, Ordering::SeqCst);
        clean_denial(&job.read(131072).unwrap().unwrap());
        live.store(true, Ordering::SeqCst);
        assert_eq!(run.worker.submit_operation_history(q.bytes().to_vec(), run.grant.clone(), WAIT).err(), Some(JobError::Closed));
        assert_eq!(run.observer.usage().bytes, query_cost(&q));
        assert_eq!(run.worker.bytes(), query_cost(&q));
        drop(run.finish());
    }
}
#[test]
fn invalid_guest_completion_cannot_invent_a_history_reply_or_unknown_effect() {
    let mut run = Running::limits(History::Observed(200), 4096, 16384, 16384, false, true);
    let q = query(19, OP);
    let report = consume(&mut run.submit(&q));
    clean_denial(&report);
    assert_eq!(report.task.execution.outcome, Err(Fault::TaskProtocol));
    assert_eq!(report.calls, 1);
    assert_eq!(run.observer.usage().bytes, query_cost(&q));
    drop(run.finish());
}
#[test]
fn second_guest_io_call_is_denied_by_unchanged_call_limit_without_replaying_unknown() {
    let mut run = Running::limits(History::Unknown, 4096, 16384, 16384, true, false);
    let q = query(20, OP);
    let report = consume(&mut run.submit(&q));
    clean_denial(&report);
    assert_eq!(report.task.execution.outcome, Err(Fault::Limits));
    assert_eq!(report.calls, 1);
    assert_eq!(report.bytes, query_cost(&q));
    drop(run.finish());
}
struct NeverRoute(Arc<AtomicUsize>);
impl Router for NeverRoute {
    fn route(&mut self, _: u32, _: &[u8]) -> Result<Vec<u8>, RouterFault> {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic!("history query reached raw adapter")
    }
}
impl BrokerRouter for NeverRoute {
    fn route(&mut self, _: &mut RouteContext<'_>, _: u32, _: &Request) -> Result<Vec<u8>, RouterFault> {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic!("history query reached broker adapter")
    }
}
#[test]
fn ordinary_raw_and_brokered_jobs_still_reject_query_without_adapter_dispatch() {
    let mut run = Running::new(History::Observed(200));
    let calls = Arc::new(AtomicUsize::new(0));
    for brokered in [false, true] {
        let input = query(21, OP).bytes().to_vec();
        let mut job = if brokered {
            run.worker.submit_brokered(input, Box::new(NeverRoute(calls.clone())), WAIT).unwrap()
        } else { run.worker.submit(input, Box::new(NeverRoute(calls.clone())), WAIT).unwrap() };
        let report = consume(&mut job);
        assert_eq!(report.task.execution.outcome, Err(Fault::TaskProtocol));
        assert!(report.operation_response.is_none());
        assert!(report.operation_frame.is_none());
        assert_eq!(report.calls, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    // Existing effect-mode maintenance behavior is deliberately preserved.
    assert_eq!(run.prepare.load(Ordering::SeqCst), 2);
    drop(run.finish());
}

struct ActualHttp {
    command: Command,
    backend_calls: Arc<AtomicUsize>,
    reply: Vec<u8>,
}
impl BrokerRouter for ActualHttp {
    fn route(&mut self, context: &mut RouteContext<'_>, call: u32, request: &Request) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        let exact = request.bytes().to_vec();
        let reply = self.reply.clone();
        let calls = self.backend_calls.clone();
        context.dispatch(&self.command, move |received| {
            assert_eq!(received, exact);
            calls.fetch_add(1, Ordering::SeqCst);
            // Ordinary synthetic closure: no endpoint, socket or credential.
            Ok(reply)
        }).map_err(|_| RouterFault::Unknown)
    }
}
#[test]
fn actual_broker_http_producer_is_read_by_new_guest_query_without_second_effect() {
    // Grant issued before the original submit on the same managed host/instance.
    let mut run = Running::new(History::Missing);
    let original = original_request();
    let outcome = HttpOutcome {
        status: Status::Completed, http_status: 503, headers: vec![],
        body: b"real-synthetic-backend-result-not-for-history-reader".to_vec(),
    };
    let reply = Response::encode_http(&original, &outcome).unwrap();
    let backend_calls = Arc::new(AtomicUsize::new(0));
    let script = ActualHttp { command: run.command.clone(), backend_calls: backend_calls.clone(), reply: reply.clone() };
    let mut submitted = run.worker.submit_brokered(original.bytes().to_vec(), Box::new(script), WAIT).unwrap();
    let first = consume(&mut submitted);
    assert_eq!(first.task.execution.outcome, Ok(0));
    assert_eq!(first.http_response, Some(outcome));
    assert!(!first.unknown);
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.prepare.load(Ordering::SeqCst), 1);
    run.produced_response = Some(reply);
    let before_query = run.worker.bytes();
    let q = query(23, OP);
    assert_ne!(q.call_id(), original.call_id());
    assert_ne!(q.digest(), original.digest());
    let report = consume(&mut run.submit(&q));
    assert_reply(&q, &report, Status::Completed, 503);
    assert_eq!(backend_calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.prepare.load(Ordering::SeqCst), 1);
    assert_eq!(run.worker.bytes(), before_query + query_cost(&q));
    assert_eq!(run.observer.usage().bytes, before_query + query_cost(&q));
    drop(run.finish());
}
