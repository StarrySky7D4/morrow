//! Unique live IO execution and durable reconciliation using a synthetic
//! backend only. No real network, file or credential effect is performed, and
//! no stored record is treated as a live grant.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
#[cfg(feature = "fault-injection")]
use morrow_core::io_evidence::max_container_bytes;
use morrow_core::{
    dispatch::HostRuntime,
    io_evidence::{Kind, Material},
    io_intent::{Command, ObservationSource, Phase, Record, Recovery},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::IoBinding,
    io_execution::{Broker, Error},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const ID: &str = "org.example.io.execution";
const OPERATION: &str = "operation-1";
const REQUEST: &[u8] = b"request";
const RESPONSE_LIMIT: u64 = 16;

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::HttpRequest])
}
fn request_sha256() -> [u8; 32] {
    Sha256::digest(REQUEST).into()
}
fn command(operation: &str) -> Command {
    Command {
        operation_id: operation.into(),
        subject: ID.into(),
        package_sha256: package().digest(),
        capability: IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: request_sha256(),
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: REQUEST.len() as u64,
        response_limit: RESPONSE_LIMIT,
    }
}
fn package() -> Package {
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["api.invoke".into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_resources = 4;
    budget.max_jobs = 4;
    budget.max_bytes = 64;
    budget.max_job_bytes = 32;
    budget.max_duration_ms = 100;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    Package::build(manifest, &wasm).unwrap()
}
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    package: Package,
}
impl Fixture {
    /// Builds an isolated host whose database lives at `<root>/db`. Catalog and
    /// registry side files are tagged so a crash child can build its own view of
    /// the same durable database without sharing live process state.
    fn at(root: std::path::PathBuf, tag: &str) -> Self {
        let package = package();
        let catalog = Catalog::open(&root.join(format!("{tag}-catalog"))).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&root.join(format!("{tag}-registry")), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(Store::open(&root.join("db"), EventBudget::default()).unwrap())
            .unwrap();
        Self {
            manager,
            host,
            package,
        }
    }
    fn new() -> (tempfile::TempDir, Self) {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Self::at(dir.path().to_path_buf(), "host");
        (dir, fixture)
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn bind(&self, instance: &ManagedInstance) -> IoBinding {
        self.bind_at(instance, 1, 40)
    }
    fn bind_at(&self, instance: &ManagedInstance, now: u64, expires: u64) -> IoBinding {
        self.manager
            .bind_io(
                &self.host,
                instance,
                self.package.digest(),
                self.manager.revision(),
                &caps(),
                expires,
                now,
            )
            .unwrap()
    }
    /// Durable Prepared history with the exact protected request original.
    fn seed(&mut self, command: &Command) {
        self.seed_history(command);
        let material = Material::encode(
            Kind::Request,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            REQUEST,
        )
        .unwrap();
        self.host
            .store_local_mut()
            .store_io_material(&command.subject, Kind::Request, &material, || Ok(()))
            .unwrap();
    }
    /// Durable Prepared history without any protected original.
    fn seed_history(&mut self, command: &Command) {
        self.host
            .store_local_mut()
            .append_io_intent_local_authorized(&Record::prepared(command.clone()).unwrap(), || {
                Ok(())
            })
            .unwrap();
        self.host
            .store_local_mut()
            .reserve_io_materials(command, || Ok(()))
            .unwrap();
    }
    fn phase(&self, operation: &str) -> Phase {
        self.host
            .store_local()
            .lookup_io_intent(ID, operation)
            .unwrap()
            .unwrap()
            .phase()
    }
}

#[test]
fn begin_dispatch_retains_the_original_and_closes_the_history() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    assert_eq!(broker.active(), 1);
    let mut calls = 0;
    let response = broker
        .dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |request| {
                calls += 1;
                assert_eq!(request, REQUEST);
                Ok(b"response".to_vec())
            },
            || 3,
        )
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(response, b"response");
    assert_eq!(broker.active(), 0);
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    let report = broker.recovery(&f.host, ID, OPERATION).unwrap();
    assert_eq!(report.recovery, Recovery::AlreadyObserved);
    assert!(report.request_retained && report.response_retained);
    let stored = f
        .host
        .store_local()
        .io_material(ID, OPERATION, Kind::Response)
        .unwrap()
        .unwrap();
    assert_eq!(stored.payload(), b"response");
    assert_eq!(
        stored.payload_sha256(),
        <[u8; 32]>::from(Sha256::digest(b"response"))
    );
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn duplicate_bindings_and_repeats_never_create_a_second_execution() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let first = f.bind(&instance);
    let second = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &first,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &second,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        ),
        Err(Error::Duplicate)
    ));
    let mut calls = 0;
    broker
        .dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| {
                calls += 1;
                Ok(b"response".to_vec())
            },
            || 3,
        )
        .unwrap();
    // A repeated submission finds no live execution and never a resend.
    assert!(matches!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| panic!("must not rerun"),
            || 4,
        ),
        Err(Error::NotFound)
    ));
    // The durable history itself refuses a fresh attempt.
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &first,
            IoCapability::HttpRequest,
            ID,
            &command,
            4,
        ),
        Err(Error::Dispatched)
    ));
    assert_eq!(calls, 1);
    assert_eq!(broker.active(), 0);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn interrupted_backend_stays_unknown_and_survives_a_restart_without_resend() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let mut calls = 0;
    assert!(matches!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| {
                calls += 1;
                Err(())
            },
            || 3,
        ),
        Err(Error::OutcomeUnknown)
    ));
    // The live execution is retired from dispatch but keeps the durable
    // OutcomeUnknown boundary; a repeat cannot rerun the backend.
    assert!(matches!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| panic!("must not rerun"),
            || 4,
        ),
        Err(Error::Dispatched)
    ));
    assert_eq!(calls, 1);
    let report = broker.recovery(&f.host, ID, OPERATION).unwrap();
    assert_eq!(report.recovery, Recovery::ReconcileOnly);
    assert!(report.request_retained && !report.response_retained);
    // Restart: a brand-new broker and binding still cannot resend.
    drop(broker);
    let broker = Broker::new();
    let second = f.bind_at(&instance, 5, 45);
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &second,
            IoCapability::HttpRequest,
            ID,
            &command,
            5,
        ),
        Err(Error::OutcomeUnknown)
    ));
    // Reconciliation closes the history without claiming a remote success.
    assert_eq!(
        broker
            .reconcile(&mut f.host, ID, OPERATION, Some([7; 32]))
            .unwrap(),
        Recovery::AlreadyObserved
    );
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    let report = broker.recovery(&f.host, ID, OPERATION).unwrap();
    assert_eq!(report.recovery, Recovery::AlreadyObserved);
    assert!(report.request_retained && !report.response_retained);
    assert_eq!(
        f.host
            .store_local()
            .io_material_reservation_usage()
            .unwrap(),
        (0, 0)
    );
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn retirement_and_stop_reject_dispatch_before_any_external_effect() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    instance.stop();
    // The live generation is checked before the boundary, so no external
    // effect and no durable dispatch happened.
    assert!(matches!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| panic!("must not run"),
            || 3,
        ),
        Err(Error::Denied)
    ));
    assert_eq!(f.phase(OPERATION), Phase::Prepared);
    assert_eq!(
        broker.recovery(&f.host, ID, OPERATION).unwrap().recovery,
        Recovery::AwaitFreshAuthorization
    );
    assert_eq!(broker.maintain(4), 1);
    assert_eq!(broker.active(), 0);
    // A fresh instance and binding may retry only because the durable history
    // is still exactly Prepared.
    let fresh = f.connect();
    let fresh_binding = f.bind(&fresh);
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &fresh,
            &fresh_binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            5,
        )
        .unwrap();
    let response = broker
        .dispatch(
            &f.manager,
            &mut f.host,
            &fresh,
            OPERATION,
            |_| Ok(b"response".to_vec()),
            || 6,
        )
        .unwrap();
    assert_eq!(response, b"response");
    instance.close(&mut f.host).unwrap();
    fresh.close(&mut f.host).unwrap();
}

#[test]
fn cancel_before_dispatch_is_terminal_and_releases_quota() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    broker.cancel(&mut f.host, ID, OPERATION).unwrap();
    assert_eq!(f.phase(OPERATION), Phase::CancelledBeforeDispatch);
    assert_eq!(
        f.host
            .store_local()
            .io_material_reservation_usage()
            .unwrap(),
        (0, 0)
    );
    assert_eq!(broker.active(), 0);
    assert!(matches!(broker.cancel(&mut f.host, ID, OPERATION), Ok(())));
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            3,
        ),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| panic!("must not run"),
            || 4,
        ),
        Err(Error::NotFound)
    ));
    f.host.store_local().integrity_check().unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn request_matching_precedes_every_admission() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let broker = Broker::new();
    // A history without the protected request original cannot start.
    let missing = command("missing-original");
    f.seed_history(&missing);
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &missing,
            2,
        ),
        Err(Error::EvidenceUnavailable)
    ));
    // A different request digest for the same operationId never matches.
    let stored = command("mismatch");
    f.seed(&stored);
    let mut altered = command("mismatch");
    altered.request_sha256 = [9; 32];
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &altered,
            2,
        ),
        Err(Error::Conflict)
    ));
    // A different subject or capability is refused before durable reads.
    let mut foreign = command("mismatch");
    foreign.subject = "plugin.stranger".into();
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &foreign,
            2,
        ),
        Err(Error::Conflict)
    ));
    let mut wrong_capability = command("mismatch");
    wrong_capability.capability = IoCapability::FileRead;
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::FileRead,
            ID,
            &wrong_capability,
            2,
        ),
        Err(Error::Denied)
    ));
    // The exact command still starts and the approved binding is required.
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &stored,
            2,
        )
        .unwrap();
    assert_eq!(broker.active(), 1);
    assert!(matches!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            "plugin.stranger",
            &stored,
            2,
        ),
        Err(Error::Conflict)
    ));
    instance.close(&mut f.host).unwrap();
}

#[test]
fn reconciliation_requires_an_unknown_history_and_closes_without_original() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    // A Prepared history is not reconcilable.
    assert!(matches!(
        broker.reconcile(&mut f.host, ID, OPERATION, Some([1; 32])),
        Err(Error::NotFound)
    ));
    // Crossing the boundary without retaining a response requires an explicit
    // reconciliation digest and releases the never-admitted quota.
    let stored = f
        .host
        .store_local()
        .lookup_io_intent(ID, OPERATION)
        .unwrap()
        .unwrap();
    f.host
        .store_local_mut()
        .reserve_io_intent_followup(&command, || Ok(()))
        .unwrap();
    f.host
        .store_local_mut()
        .append_io_intent_local_authorized(&stored.propose_dispatch_boundary().unwrap(), || Ok(()))
        .unwrap();
    assert!(matches!(
        broker.reconcile(&mut f.host, ID, OPERATION, None),
        Err(Error::EvidenceUnavailable)
    ));
    assert_eq!(
        broker
            .reconcile(&mut f.host, ID, OPERATION, Some([5; 32]))
            .unwrap(),
        Recovery::AlreadyObserved
    );
    f.host.store_local().integrity_check().unwrap();
    let stored = f
        .host
        .store_local()
        .lookup_io_intent(ID, OPERATION)
        .unwrap()
        .unwrap();
    assert_eq!(stored.phase(), Phase::Observed);
    assert_eq!(
        stored.data().observation_source,
        ObservationSource::Reconciliation as i32
    );
    assert_eq!(stored.data().observation_sha256.as_slice(), &[5u8; 32]);
    instance.close(&mut f.host).unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent passes an isolated database and crash point"]
fn io_execution_crash_child() {
    let root = std::path::PathBuf::from(
        std::env::var_os("MORROW_IO_EXECUTION_DIR").expect("child database directory"),
    );
    let mut f = Fixture::at(root, "child");
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let _ = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| panic!("the backend must never run after a boundary exit"),
        || 3,
    );
    panic!("fault injection boundary was not reached");
}

#[cfg(feature = "fault-injection")]
fn crash(root: &std::path::Path, point: &str) {
    let mut process = std::process::Command::new(std::env::current_exe().unwrap());
    process
        .args([
            "--exact",
            "io_execution_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_IO_EXECUTION_DIR", root)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut child = process.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert_eq!(status.code(), Some(86), "{point}");
                return;
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe {point}: {error}");
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout {point}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn send_boundary_process_exit_never_resends_and_reconciles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    {
        let mut f = Fixture::at(root.clone(), "host");
        let instance = f.connect();
        let binding = f.bind(&instance);
        let command = command(OPERATION);
        f.seed(&command);
        let broker = Broker::new();
        broker
            .begin(
                &f.manager,
                &mut f.host,
                &instance,
                &binding,
                IoCapability::HttpRequest,
                ID,
                &command,
                2,
            )
            .unwrap();
        assert_eq!(f.phase(OPERATION), Phase::Prepared);
        instance.close(&mut f.host).unwrap();
    }
    // The child commits the dispatch boundary and is killed immediately after;
    // its backend closure would panic if it ever ran.
    crash(&root, "io-intent-after-commit");
    let store = Store::open_existing(&root.join("db"), EventBudget::default()).unwrap();
    assert_eq!(
        store
            .lookup_io_intent(ID, OPERATION)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert!(matches!(
        store.io_material(ID, OPERATION, Kind::Response),
        Err(morrow_core::Error::EvidenceUnavailable)
    ));
    // The request original is retained and the response quota is still held.
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (1, max_container_bytes(RESPONSE_LIMIT).unwrap())
    );
    store.integrity_check().unwrap();
    drop(store);
    // A fresh broker can only reconcile; it can never resend.
    let mut host =
        HostRuntime::new(Store::open_existing(&root.join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let broker = Broker::new();
    let report = broker.recovery(&host, ID, OPERATION).unwrap();
    assert_eq!(report.recovery, Recovery::ReconcileOnly);
    assert!(report.request_retained && !report.response_retained);
    assert_eq!(
        broker
            .reconcile(&mut host, ID, OPERATION, Some([9; 32]))
            .unwrap(),
        Recovery::AlreadyObserved
    );
    host.store_local().integrity_check().unwrap();
}

#[test]
fn command_digest_must_match_the_actual_managed_package() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let mut wrong = command(OPERATION);
    wrong.package_sha256 = [99; 32];
    assert_ne!(wrong.package_sha256, f.package.digest());
    f.seed(&wrong);
    let broker = Broker::new();
    assert_eq!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &wrong,
            2
        ),
        Err(Error::Conflict)
    );
    assert_eq!(broker.active(), 0);
    assert_eq!(binding.usage().bytes, 0);
    assert_eq!(f.phase(OPERATION), Phase::Prepared);
}

#[test]
fn backend_stop_or_external_revocation_retains_facts_but_never_delivers_payload() {
    for external in [false, true] {
        let (_dir, mut f) = Fixture::new();
        let instance = f.connect();
        let revocation = f.host.revocation(instance.connection()).unwrap();
        let binding = f.bind(&instance);
        let command = command(OPERATION);
        f.seed(&command);
        let broker = Broker::new();
        broker
            .begin(
                &f.manager,
                &mut f.host,
                &instance,
                &binding,
                IoCapability::HttpRequest,
                ID,
                &command,
                2,
            )
            .unwrap();
        let mut now = 2;
        let result = broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| {
                if external {
                    revocation.revoke();
                } else {
                    instance.stop();
                }
                Ok(b"sensitive".to_vec())
            },
            || {
                now += 1;
                now
            },
        );
        assert_eq!(result, Err(Error::Denied));
        assert_eq!(f.phase(OPERATION), Phase::Observed);
        let original = f
            .host
            .store_local()
            .io_material(ID, OPERATION, Kind::Response)
            .unwrap()
            .unwrap();
        assert_eq!(original.payload(), b"sensitive");
        assert_eq!(broker.active(), 0);
        assert_eq!((binding.usage().resources, binding.usage().jobs), (0, 0));
        assert!(binding.usage().bytes > 0);
        assert_eq!(
            broker.recovery(&f.host, ID, OPERATION).unwrap().recovery,
            Recovery::AlreadyObserved
        );
        f.host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn fresh_clock_gates_admission_commit_effect_and_final_delivery() {
    for (times, expected_phase, calls) in [
        (vec![40], Phase::Prepared, 0),
        (vec![3, 40], Phase::Prepared, 0),
        (vec![3, 4, 40], Phase::OutcomeUnknown, 0),
        (vec![3, 4, 5, 40], Phase::Observed, 1),
    ] {
        let (_dir, mut f) = Fixture::new();
        let instance = f.connect();
        let binding = f.bind(&instance);
        let command = command(OPERATION);
        f.seed(&command);
        let broker = Broker::new();
        broker
            .begin(
                &f.manager,
                &mut f.host,
                &instance,
                &binding,
                IoCapability::HttpRequest,
                ID,
                &command,
                2,
            )
            .unwrap();
        let mut clock = times.into_iter();
        let mut actual_calls = 0;
        let result = broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| {
                actual_calls += 1;
                Ok(b"response".to_vec())
            },
            || clock.next().expect("unexpected extra clock sample"),
        );
        assert_eq!(result, Err(Error::Expired));
        assert_eq!(actual_calls, calls);
        assert_eq!(f.phase(OPERATION), expected_phase);
        assert!(clock.next().is_none());
        broker.maintain(40);
        assert_eq!((binding.usage().resources, binding.usage().jobs), (0, 0));
        f.host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn store_final_authorization_checks_current_revocation() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let revocation = f.host.revocation(instance.connection()).unwrap();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let mut samples = 0;
    let result = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| panic!("revocation before the boundary must prevent backend execution"),
        || {
            samples += 1;
            if samples == 2 {
                revocation.revoke();
            }
            2 + samples
        },
    );
    assert_eq!(result, Err(Error::Denied));
    assert_eq!(samples, 2);
    assert_eq!(f.phase(OPERATION), Phase::Prepared);
    assert!(broker.retire(OPERATION));
    assert_eq!((binding.usage().resources, binding.usage().jobs), (0, 0));
}

#[test]
fn retiring_inside_backend_holds_capacity_until_execution_returns() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let charged = binding.usage().bytes;
    let result = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| {
            assert!(broker.retire(OPERATION));
            assert_eq!(broker.active(), 0);
            // The backend is still running: its actual lease owns both slots.
            assert_eq!((binding.usage().resources, binding.usage().jobs), (1, 1));
            Ok(b"response".to_vec())
        },
        || 3,
    );
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    assert_eq!((binding.usage().resources, binding.usage().jobs), (0, 0));
    assert_eq!(binding.usage().bytes, charged);
    assert_eq!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            4
        ),
        Err(Error::Dispatched)
    );
}

#[test]
fn retirement_after_boundary_prevents_effect_and_requires_reconciliation() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let mut samples = 0;
    let result = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| panic!("retired execution must not start backend"),
        || {
            samples += 1;
            if samples == 3 {
                assert!(broker.retire(OPERATION));
                assert_eq!(binding.usage().jobs, 1);
            }
            2 + samples
        },
    );
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(
        broker.begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            6
        ),
        Err(Error::OutcomeUnknown)
    );
    assert_eq!(
        broker
            .reconcile(&mut f.host, ID, OPERATION, Some([8; 32]))
            .unwrap(),
        Recovery::AlreadyObserved
    );
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    f.host.store_local().integrity_check().unwrap();
}

#[test]
fn foreign_instance_cannot_claim_slot_or_invoke_clock_and_delivery_advances_time() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let foreign = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    assert_eq!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &foreign,
            OPERATION,
            |_| panic!("foreign effect"),
            || panic!("foreign clock")
        ),
        Err(Error::Denied)
    );
    let mut now = 2;
    let result = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| Ok(b"response".to_vec()),
        || {
            now += 1;
            now
        },
    );
    assert_eq!(result.unwrap(), b"response");
    assert_eq!(now, 6);
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 5),
        Err(morrow_plugin_runtime::io_binding::Error::Clock)
    );
}

#[test]
fn broker_administration_cannot_cancel_same_id_on_another_host() {
    let (_dir, mut f) = Fixture::new();
    let (_other_dir, mut other) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    other.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    assert_eq!(
        broker.cancel(&mut other.host, ID, OPERATION),
        Err(Error::Denied)
    );
    assert_eq!(
        broker.reconcile(&mut other.host, ID, OPERATION, Some([5; 32])),
        Err(Error::Denied)
    );
    assert_eq!(other.phase(OPERATION), Phase::Prepared);
    assert_eq!(broker.active(), 1);
    broker.cancel(&mut f.host, ID, OPERATION).unwrap();
    assert_eq!(f.phase(OPERATION), Phase::CancelledBeforeDispatch);
    assert_eq!(binding.usage().jobs, 0);
}

#[test]
fn delivery_clock_can_retire_execution_without_registry_deadlock() {
    let (_dir, mut f) = Fixture::new();
    let instance = f.connect();
    let binding = f.bind(&instance);
    let command = command(OPERATION);
    f.seed(&command);
    let broker = Broker::new();
    broker
        .begin(
            &f.manager,
            &mut f.host,
            &instance,
            &binding,
            IoCapability::HttpRequest,
            ID,
            &command,
            2,
        )
        .unwrap();
    let mut samples = 0;
    let mut calls = 0;
    let result = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| {
            calls += 1;
            Ok(b"response".to_vec())
        },
        || {
            samples += 1;
            if samples == 4 {
                assert!(broker.retire(OPERATION));
                assert_eq!(binding.usage().jobs, 1);
            }
            2 + samples
        },
    );
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(samples, 4);
    assert_eq!(calls, 1);
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    assert_eq!(broker.active(), 0);
    assert_eq!((binding.usage().jobs, binding.usage().resources), (0, 0));
    assert_eq!(
        f.host
            .store_local()
            .io_material(ID, OPERATION, Kind::Response)
            .unwrap()
            .unwrap()
            .payload(),
        b"response"
    );
}
