//! New nonblocking control regressions; previous SDK014 evidence is unchanged.
//! Native queue contention uses its actual Mutex via a fault-injection-only
//! lock holder. SQLite contention uses a real second Store/IMMEDIATE transaction.
//! The final public Store guard test is layered original-Control evidence, not
//! a simulated runtime ACK transaction or an implicit SQLite busy retry.
#![cfg(all(
    feature = "packages",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
use morrow_core::{
    Error as CoreError,
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    channel::{ChannelBroker, CleanupProof, Error, Source},
    manager::{ManagedInstance, Manager},
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.channel.controls";
const WAIT: Duration = Duration::from_secs(5);
const SCOPE: [u8; 32] = [0x73; 32];
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let mut manifest = Package::manifest_for_transform(
            ID,
            "1.0.0",
            &wasm,
            vec![TransformHandler {
                handler: "control.test".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65,
                max_output_bytes: 64,
            }],
        );
        manifest
            .required_features
            .push(morrow_core::channel::FEATURE.into());
        manifest.channel_declaration = Some(morrow_core::channel::declaration(
            vec!["control.test".into()],
            vec![Kind::Events],
        ));
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let broker = manager
            .bind_channel(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                Source {
                    kind: Kind::Events,
                    duplex: false,
                    checkpoint_scope: Some(SCOPE),
                },
                Budget {
                    max_channels: 1,
                    max_frame_bytes: 32768,
                    max_bytes: 1048576,
                    max_messages: 128,
                    max_requests: 4096,
                    max_duration_ms: 10000,
                },
                10001,
                1,
            )
            .unwrap();
        Self {
            dir,
            manager,
            host,
            instance,
            broker,
        }
    }
    fn request(&self, action: Action, serial: u8) -> Request {
        let endpoint = self.broker.endpoint();
        Request {
            call_id: [serial; 32],
            reference: endpoint.reference,
            source_epoch: endpoint.source_epoch,
            action,
        }
    }
    fn call(&mut self, action: Action, serial: u8) -> Response {
        let request = self.request(action, serial);
        self.broker
            .dispatch(&self.manager, &mut self.host, &self.instance, &request, 2)
            .unwrap()
    }
    fn held_source(&self) -> mpsc::Sender<()> {
        let (published, ready) = mpsc::channel();
        let (release, hold) = mpsc::channel();
        self.broker
            .spawn(move |producer| {
                producer
                    .push(b"real-native-source".to_vec(), b"cursor-1".to_vec())
                    .unwrap();
                published.send(()).unwrap();
                let _ = hold.recv_timeout(WAIT);
                let _ = producer.finish();
            })
            .unwrap();
        ready.recv_timeout(WAIT).unwrap();
        release
    }
    fn frame(&mut self) -> Frame {
        let received = self.call(
            Action::Receive {
                last_acked: 0,
                credit_bytes: 32768,
            },
            1,
        );
        assert_eq!(received.status, Status::Frame);
        received.frame.unwrap()
    }
    fn assert_no_durable_ack(&self, epoch: &[u8; 32]) {
        assert!(
            self.host
                .store_local()
                .channel_checkpoint(&SCOPE, epoch)
                .unwrap()
                .is_none()
        );
        assert!(
            self.host
                .store_local()
                .channel_ack_receipt(&SCOPE, epoch, 1)
                .unwrap()
                .is_none()
        );
        let reopened =
            Store::open_existing(&self.dir.path().join("db"), EventBudget::default()).unwrap();
        assert!(
            reopened
                .channel_checkpoint(&SCOPE, epoch)
                .unwrap()
                .is_none()
        );
        assert!(
            reopened
                .channel_ack_receipt(&SCOPE, epoch, 1)
                .unwrap()
                .is_none()
        );
        reopened.integrity_check().unwrap();
    }
    fn joined(&self) {
        let owner = self.broker.cleanup_handle();
        let deadline = Instant::now() + WAIT;
        loop {
            let _ = owner.try_reap();
            if let Some(snapshot) = self.broker.try_snapshot() {
                if snapshot.resource_reclaimed {
                    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "Actual producer join was not proven"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}
fn ack(frame: &Frame) -> Action {
    Action::Ack {
        sequence: frame.sequence,
        frame_sha256: frame.digest().unwrap(),
        cursor: frame.cursor.clone(),
    }
}
fn material(frame: &Frame, request: &Request) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let response = Response {
        call_id: request.call_id,
        request_sha256: request.digest().unwrap(),
        reference: request.reference,
        source_epoch: request.source_epoch,
        status: Status::Acked,
        frame: None,
        last_acked: frame.sequence,
        accepted_sequence: 0,
        resource_reclaimed: false,
    };
    (
        frame.encode().unwrap(),
        request.encode().unwrap(),
        response.encode().unwrap(),
    )
}

#[test]
fn original_atomic_stop_and_nonblocking_cleanup_keep_real_queue_contention_pending_until_join() {
    let mut fixture = Fixture::new();
    let release_source = fixture.held_source();
    let cleanup = fixture.broker.cleanup_handle();
    assert!(!fixture.broker.try_snapshot().unwrap().resource_reclaimed);
    let (locked, acquired) = mpsc::channel();
    let (release_lock, held) = mpsc::channel();
    thread::scope(|scope| {
        let broker = &fixture.broker;
        let holder = scope.spawn(move || {
            broker
                .with_queue_lock_for_fault_test(|| {
                    locked.send(()).unwrap();
                    held.recv_timeout(WAIT).unwrap();
                })
                .unwrap()
        });
        acquired.recv_timeout(WAIT).unwrap();
        let stop = Instant::now();
        fixture.instance.request_stop();
        assert!(
            fixture.broker.try_snapshot().is_none(),
            "Real queue contention is not a reclaimed snapshot"
        );
        assert_eq!(cleanup.try_cleanup(), Status::ClosingUnconfirmed);
        assert_eq!(cleanup.try_reap(), Status::ClosingUnconfirmed);
        assert!(
            stop.elapsed() < Duration::from_secs(1),
            "Control waited for an actual held queue"
        );
        release_lock.send(()).unwrap();
        holder.join().unwrap();
    });
    let denied = fixture.call(
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32768,
        },
        2,
    );
    assert_eq!(denied.status, Status::Revoked);
    assert!(denied.frame.is_none());
    assert_eq!(cleanup.try_reap(), Status::ClosingUnconfirmed);
    let pending = fixture.broker.try_snapshot().unwrap();
    assert!(!pending.resource_reclaimed);
    assert_eq!(pending.cleanup_proof, CleanupProof::Pending);
    assert_eq!(pending.last_acked, 0);
    release_source.send(()).unwrap();
    fixture.joined();
    assert_eq!(
        fixture.broker.try_snapshot().unwrap().terminal_cause,
        Some(Status::Revoked)
    );
}

#[test]
fn actual_second_sqlite_transaction_makes_runtime_ack_unknown_without_retry_or_cursor_advance() {
    let mut fixture = Fixture::new();
    let release_source = fixture.held_source();
    let frame = fixture.frame();
    let request = fixture.request(ack(&frame), 2);
    let (frame_wire, request_wire, response_wire) = material(&frame, &request);
    let path = fixture.dir.path().join("db");
    let (locked, acquired) = mpsc::channel();
    let (release_lock, held) = mpsc::channel();
    let locker = thread::spawn(move || {
        let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let mut checks = 0;
        let result = store.commit_channel_ack_guarded(
            &[0x91; 32],
            None,
            &frame_wire,
            &request_wire,
            &response_wire,
            || {
                checks += 1;
                assert_eq!(checks, 1);
                // Public Store guard is after real BEGIN IMMEDIATE: this
                // blocks the actual second connection, not a fake SQLite result.
                locked.send(()).unwrap();
                held.recv_timeout(WAIT).unwrap();
                Err(CoreError::Invalid("explicit locker rollback"))
            },
        );
        assert!(matches!(
            result,
            Err(CoreError::Invalid("explicit locker rollback"))
        ));
        assert_eq!(checks, 1);
    });
    acquired.recv_timeout(WAIT).unwrap();
    assert_eq!(
        fixture.broker.dispatch(
            &fixture.manager,
            &mut fixture.host,
            &fixture.instance,
            &request,
            2
        ),
        Err(Error::Unknown)
    );
    let uncertain = fixture.broker.try_snapshot().unwrap();
    assert_eq!(uncertain.terminal_cause, Some(Status::Unknown));
    assert_eq!(uncertain.last_acked, 0);
    assert!(!uncertain.resource_reclaimed);
    fixture.instance.request_stop();
    release_lock.send(()).unwrap();
    locker.join().unwrap();
    fixture.assert_no_durable_ack(&frame.source_epoch);
    let denied = fixture.call(
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32768,
        },
        3,
    );
    assert!(denied.frame.is_none());
    assert_ne!(denied.status, Status::Frame);
    assert_eq!(
        fixture.broker.try_snapshot().unwrap().terminal_cause,
        Some(Status::Unknown)
    );
    assert_eq!(
        fixture.broker.cleanup_handle().try_reap(),
        Status::ClosingUnconfirmed
    );
    release_source.send(()).unwrap();
    fixture.joined();
    assert_eq!(
        fixture.broker.try_snapshot().unwrap().terminal_cause,
        Some(Status::Unknown)
    );
    fixture.assert_no_durable_ack(&frame.source_epoch);
}

#[test]
fn final_public_store_guard_observes_original_instance_stop_and_rolls_back_both_written_rows() {
    let mut fixture = Fixture::new();
    let release_source = fixture.held_source();
    let frame = fixture.frame();
    let request = fixture.request(ack(&frame), 2);
    let (frame_wire, request_wire, response_wire) = material(&frame, &request);
    let mut actual_store =
        Store::open_existing(&fixture.dir.path().join("db"), EventBudget::default()).unwrap();
    let mut checks = 0;
    let result = actual_store.commit_channel_ack_guarded(
        &SCOPE,
        None,
        &frame_wire,
        &request_wire,
        &response_wire,
        || {
            checks += 1;
            if checks == 2 {
                // The second public guard runs after both real SQL writes.
                // Stop and query the same original Control, Manager and host.
                fixture.instance.request_stop();
                let denied = fixture.call(
                    Action::Receive {
                        last_acked: 0,
                        credit_bytes: 32768,
                    },
                    3,
                );
                assert_eq!(denied.status, Status::Revoked);
                assert!(denied.frame.is_none());
                return Err(CoreError::Invalid("original instance revoked"));
            }
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(CoreError::Invalid("original instance revoked"))
    ));
    assert_eq!(checks, 2);
    drop(actual_store);
    fixture.assert_no_durable_ack(&frame.source_epoch);
    assert_eq!(fixture.broker.try_snapshot().unwrap().last_acked, 0);
    assert_eq!(
        fixture.broker.cleanup_handle().try_reap(),
        Status::ClosingUnconfirmed
    );
    release_source.send(()).unwrap();
    fixture.joined();
    fixture.assert_no_durable_ack(&frame.source_epoch);
}
