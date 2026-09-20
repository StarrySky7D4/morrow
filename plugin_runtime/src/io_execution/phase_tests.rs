use super::*;
use crate::Limits;
use morrow_core::{
    content::CardRecord,
    io_intent::Record,
    plugin_package::{Package, catalog::Catalog, io, registry::Registry},
    store::{EventBudget, Store},
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

fn ready() -> (
    tempfile::TempDir,
    Fixture,
    ManagedInstance,
    IoBinding,
    Broker,
) {
    let (dir, mut f) = Fixture::new();
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
    (dir, f, instance, binding, broker)
}
fn claim(broker: &Broker, f: &mut Fixture, instance: &ManagedInstance) -> DispatchTicket {
    broker
        .claim_dispatch_checked(
            &mut f.host,
            instance,
            OPERATION,
            &|binding, host, instance| {
                binding
                    .validate_identity(&f.manager, host, instance)
                    .map_err(Error::from)
            },
            &mut |live| live.check_liveness(3),
        )
        .unwrap()
}
fn complete(
    broker: &Broker,
    f: &mut Fixture,
    instance: &ManagedInstance,
    observation: DispatchObservation,
) -> Result<Vec<u8>> {
    broker.complete_dispatch_checked(
        observation,
        &mut f.host,
        instance,
        &|binding, host, instance| {
            binding
                .validate_identity(&f.manager, host, instance)
                .map_err(Error::from)
        },
        &mut |live| live.check_liveness(5),
    )
}

#[test]
fn owned_ticket_dispatch_runs_on_thread_with_host_usable_in_main() {
    use std::sync::mpsc;
    use std::time::Duration;

    let (_dir, mut f, instance, binding, broker) = ready();
    let ticket = claim(&broker, &mut f, &instance);

    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let original_host = f.host.binding();
    let handle = std::thread::spawn(move || {
        ticket
            .execute(
                |request| {
                    assert_eq!(request, REQUEST);
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(b"response".to_vec())
                },
                &mut |live| live.check_liveness(4),
            )
            .unwrap()
    });

    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("backend did not start");

    // While backend waits, main uses the host on the original binding.
    let card = CardRecord::new("during-wait", "note", 1, "title", vec![1]).unwrap();
    f.host
        .store_local_mut()
        .create_local("create-during-wait", &card)
        .unwrap();
    assert_eq!(f.host.binding(), original_host);
    assert_eq!(binding.usage().jobs, 1);
    assert_eq!(
        f.host
            .store_local()
            .card("during-wait")
            .unwrap()
            .unwrap()
            .body(),
        [1]
    );

    release_tx.send(()).unwrap();
    let _observation = handle.join().expect("backend panicked");
    assert_eq!(binding.usage().jobs, 1);

    let response = complete(&broker, &mut f, &instance, _observation).unwrap();
    assert_eq!(response, b"response");
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    assert_eq!(binding.usage().jobs, 0);
    let path = _dir.path().join("db");
    drop((instance, f));
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(reopened.card("during-wait").unwrap().unwrap().body(), [1]);
}

#[test]
fn revoked_instance_denies_completion_but_retains_observation() {
    let (_dir, mut f, instance, binding, broker) = ready();
    let ticket = claim(&broker, &mut f, &instance);

    let observation = ticket
        .execute(
            |req| {
                assert_eq!(req, REQUEST);
                Ok(b"response".to_vec())
            },
            &mut |live: &Live| live.check_liveness(4),
        )
        .unwrap();

    // Ready-side holds the job until completion.
    assert_eq!(binding.usage().jobs, 1);

    instance.stop();

    match complete(&broker, &mut f, &instance, observation) {
        Err(Error::Denied) => {}
        other => panic!("expected Error::Denied, got: {:?}", other.as_ref().err()),
    }
    assert_eq!(f.phase(OPERATION), Phase::Observed);

    let evidence = f
        .host
        .store_local()
        .io_material(ID, OPERATION, morrow_core::io_evidence::Kind::Response)
        .unwrap()
        .unwrap();
    assert_eq!(evidence.payload(), b"response");

    assert_eq!(binding.usage().jobs, 0);
}

#[test]
fn abandoned_dispatch_claim_cannot_authorize_resend() {
    let (_tmp, mut f, instance, binding, broker) = ready();
    let ticket = claim(&broker, &mut f, &instance);
    drop(ticket);

    assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
    assert_eq!(broker.active(), 1);

    let res = broker.dispatch(
        &f.manager,
        &mut f.host,
        &instance,
        OPERATION,
        |_| panic!("no resend"),
        || panic!("no clock"),
    );
    assert!(matches!(res, Err(Error::Dispatched)));

    assert!(broker.retire(OPERATION));
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
}

#[test]
fn foreign_completion_targets_cannot_write_or_sample_a_clock() {
    for target in 0..3 {
        let (_dir, mut f, instance, binding, broker) = ready();
        let observation = claim(&broker, &mut f, &instance)
            .execute(|_| Ok(b"response".to_vec()), &mut |live| {
                live.check_liveness(4)
            })
            .unwrap();
        let (_foreign_dir, mut foreign) = Fixture::new();
        let other_instance = f.connect();
        let other_broker = Broker::new();
        let destination = if target == 0 { &other_broker } else { &broker };
        let host = if target == 1 {
            &mut foreign.host
        } else {
            &mut f.host
        };
        let instance = if target == 2 {
            &other_instance
        } else {
            &instance
        };
        let result = destination.complete_dispatch_checked(
            observation,
            host,
            instance,
            &|_, _, _| panic!("foreign completion must not check live approval"),
            &mut |_| panic!("foreign completion must not sample clock"),
        );
        assert_eq!(result, Err(Error::Denied));
        assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
        assert!(matches!(
            f.host
                .store_local()
                .io_material(ID, OPERATION, Kind::Response),
            Err(morrow_core::Error::EvidenceUnavailable)
        ));
        assert!(
            foreign
                .host
                .store_local()
                .lookup_io_intent(ID, OPERATION)
                .unwrap()
                .is_none()
        );
        assert_eq!(broker.active(), 1);
        assert!(broker.retire(OPERATION));
        assert_eq!(binding.usage().jobs, 0);
    }
}

#[test]
fn retired_unread_observation_holds_quota_until_original_owner_completion() {
    let (_dir, mut f, instance, binding, broker) = ready();
    let observation = claim(&broker, &mut f, &instance)
        .execute(|_| Ok(b"response".to_vec()), &mut |live| {
            live.check_liveness(4)
        })
        .unwrap();
    assert!(broker.retire(OPERATION));
    assert_eq!(binding.usage().jobs, 1);
    assert_eq!(broker.active(), 0);
    assert_eq!(
        complete(&broker, &mut f, &instance, observation),
        Err(Error::Cancelled)
    );
    assert_eq!(f.phase(OPERATION), Phase::Observed);
    assert_eq!(binding.usage().jobs, 0);
}

#[test]
fn revoked_after_claim_never_enters_backend_but_keeps_durable_unknown() {
    let (_dir, mut f, instance, binding, broker) = ready();
    let ticket = claim(&broker, &mut f, &instance);
    instance.stop();
    assert!(matches!(
        ticket.execute(|_| panic!("revoked before effect"), &mut |live| live
            .check_liveness(4)),
        Err(Error::Denied)
    ));
    assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
    assert!(broker.retire(OPERATION));
    assert_eq!(binding.usage().jobs, 0);
}

#[test]
fn lost_observation_cannot_authorize_a_second_execution() {
    let (_dir, mut f, instance, binding, broker) = ready();
    let mut effects = 0;
    let observation = claim(&broker, &mut f, &instance)
        .execute(
            |_| {
                effects += 1;
                Ok(b"response".to_vec())
            },
            &mut |live| live.check_liveness(4),
        )
        .unwrap();
    drop(observation);
    assert_eq!(effects, 1);
    assert_eq!(f.phase(OPERATION), Phase::OutcomeUnknown);
    assert!(matches!(
        f.host
            .store_local()
            .io_material(ID, OPERATION, Kind::Response),
        Err(morrow_core::Error::EvidenceUnavailable)
    ));
    assert_eq!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &instance,
            OPERATION,
            |_| panic!("lost reply cannot resend"),
            || panic!("duplicate cannot sample clock")
        ),
        Err(Error::Dispatched)
    );
    assert!(broker.retire(OPERATION));
    assert_eq!(binding.usage().jobs, 0);
}
