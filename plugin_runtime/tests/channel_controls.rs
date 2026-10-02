//! Native original-Control regressions; previous SDK014 evidence is unchanged.
//! Newly compiled WAT faults and correlated TaskReports use real native sources.
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
    task::{FailureCode, Invocation, PluginFailure, Transform},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    channel::{ChannelBroker, CleanupProof, Error, ProducerOutcome, Source},
    manager::{ManagedInstance, Manager},
    package::TaskReport,
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
        Self::with_program("i32.const 0", Limits::default(), 1, 10_000, false)
    }
    fn with_program(
        body: &str,
        limits: Limits,
        max_channels: u32,
        duration_ms: u64,
        duplex: bool,
    ) -> Self {
        Self::with_module(body, "", limits, max_channels, duration_ms, duplex)
    }
    fn with_module(
        body: &str,
        data: &str,
        limits: Limits,
        max_channels: u32,
        duration_ms: u64,
        duplex: bool,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(limits.fuel, 20_000_000);
        assert_eq!(limits.memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.host_calls, 16);
        let read_import = if body.contains("call $read") {
            r#"(import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))"#
        } else {
            ""
        };
        let complete_import = if body.contains("call $complete") {
            r#"(import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))"#
        } else {
            ""
        };
        let wasm = wat::parse_str(format!(
            r#"(module
            (import "morrow_channel_v1" "call" (func $channel (param i32 i32 i32 i32) (result i32)))
            {read_import}
            {complete_import}
            (memory (export "memory") 4)
            {data}
            (func (export "morrow_run") (result i32) {body}))"#,
        ))
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
        let mut manager = Manager::new(registry, limits);
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let original_limits = instance.package().limits();
        assert_eq!(original_limits.fuel, 20_000_000);
        assert_eq!(original_limits.memory_bytes, 16 * 1024 * 1024);
        assert_eq!(original_limits.host_calls, 16);
        let broker = manager
            .bind_channel(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                Source {
                    kind: Kind::Events,
                    duplex,
                    checkpoint_scope: Some(SCOPE),
                },
                Budget {
                    max_channels,
                    max_frame_bytes: 32768,
                    max_bytes: 1048576,
                    max_messages: 128,
                    max_requests: 4096,
                    max_duration_ms: duration_ms,
                },
                duration_ms + 1,
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
    fn bind_peer(&self, instance: &ManagedInstance) -> ChannelBroker {
        let endpoint = self.broker.endpoint();
        self.manager
            .bind_channel(
                &self.host,
                instance,
                instance.package().package().digest(),
                self.manager.revision(),
                Source {
                    kind: Kind::Events,
                    duplex: true,
                    checkpoint_scope: Some(SCOPE),
                },
                endpoint.budget,
                endpoint.budget.max_duration_ms + 1,
                1,
            )
            .unwrap()
    }
    fn invocation(&self) -> Invocation {
        let endpoint = self.broker.endpoint();
        let mut input = vec![0];
        input.extend_from_slice(&endpoint.reference);
        input.extend_from_slice(&endpoint.source_epoch);
        assert_eq!(input.len(), 65);
        Invocation::new_transform(
            "channel-original-control",
            Transform {
                handler: "control.test".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input,
            },
        )
        .unwrap()
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
        joined(&self.broker);
    }
}
fn joined(broker: &ChannelBroker) {
    let owner = broker.cleanup_handle();
    let deadline = Instant::now() + WAIT;
    loop {
        let _ = owner.try_reap();
        if let Some(snapshot) = broker.try_snapshot() {
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
fn request_for(broker: &ChannelBroker, action: Action, serial: u8) -> Request {
    let endpoint = broker.endpoint();
    Request {
        call_id: [serial; 32],
        reference: endpoint.reference,
        source_epoch: endpoint.source_epoch,
        action,
    }
}
fn waiting_source(broker: &ChannelBroker) -> mpsc::Receiver<Result<(u64, Vec<u8>), Error>> {
    let (started, ready) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer
                .push(b"waiting-native-source".to_vec(), b"cursor-1".to_vec())
                .unwrap();
            started.send(()).unwrap();
            // Actual source-thread wait, not a host callback or simulated join.
            finished.send(producer.wait_sent()).unwrap();
        })
        .unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    result
}
fn failing_guest(tail: &str) -> String {
    // Read the authentic invocation and count one real channel host call. The
    // one-byte request is rejected once without send, ACK, receipt or retry.
    format!(
        "i32.const 0 i32.const 131072 call $read drop \
         i32.const 0 i32.const 1 i32.const 131072 i32.const 131072 call $channel drop \
         {tail}"
    )
}
fn finish_failed_source(
    fixture: &Fixture,
    report: &TaskReport,
    stopped: mpsc::Receiver<Result<(u64, Vec<u8>), Error>>,
) -> Result<Result<(u64, Vec<u8>), Error>, mpsc::RecvTimeoutError> {
    let original_report = format!("{report:?}");
    let source_result = stopped.recv_timeout(WAIT);
    if source_result.is_err() {
        // A red baseline must still stop and actually join its original thread.
        fixture.instance.request_stop();
    }
    fixture.joined();
    assert_eq!(
        fixture.broker.cleanup_handle().try_cleanup(),
        Status::Revoked
    );
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("failed invocation must never replay its source"))
            .is_err()
    );
    assert_eq!(format!("{report:?}"), original_report);
    source_result
}
fn failed_invocation_stops_original_source(tail: &str, expected: Fault) {
    let mut fixture =
        Fixture::with_program(&failing_guest(tail), Limits::default(), 2, 10_000, true);
    let sibling = fixture.bind_peer(&fixture.instance);
    let foreign = fixture.manager.connect(ID, &mut fixture.host).unwrap();
    let foreign_broker = fixture.bind_peer(&foreign);
    let stopped = waiting_source(&fixture.broker);
    let frame = fixture.frame();
    let input = fixture.invocation();
    let started = Instant::now();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let source_result = finish_failed_source(&fixture, &report, stopped);
    assert_eq!(report.execution.outcome, Err(expected));
    assert_eq!(report.execution.host_calls, 1);
    assert!(report.execution.fuel_remaining < fixture.instance.package().limits().fuel);
    assert!(report.output.is_none());
    assert!(report.response.is_none());
    assert!(report.failure.is_none());
    assert_eq!(source_result, Ok(Err(Error::Denied)));
    assert!(
        started.elapsed() < WAIT,
        "Original Control must stop and actually join before its ten-second deadline"
    );
    let snapshot = fixture.broker.try_snapshot().unwrap();
    assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(snapshot.last_acked, 0);
    assert_eq!(snapshot.accepted_sequence, 0);
    assert_eq!(snapshot.usage.messages, 1);
    assert_eq!(snapshot.usage.requests, 2);
    fixture.assert_no_durable_ack(&frame.source_epoch);

    let receive = Action::Receive {
        last_acked: 0,
        credit_bytes: 32_768,
    };
    let sibling_request = request_for(&sibling, receive.clone(), 8);
    let sibling_denied = sibling
        .dispatch(
            &fixture.manager,
            &mut fixture.host,
            &fixture.instance,
            &sibling_request,
            2,
        )
        .unwrap();
    assert_eq!(sibling_denied.status, Status::Revoked);
    assert!(sibling_denied.frame.is_none());
    assert_eq!(sibling.cleanup_handle().try_cleanup(), Status::Revoked);
    assert_eq!(
        sibling.try_snapshot().unwrap().cleanup_proof,
        CleanupProof::NoProducer
    );
    let foreign_query = request_for(&foreign_broker, Action::Query, 9);
    let foreign_ready = foreign_broker
        .dispatch(
            &fixture.manager,
            &mut fixture.host,
            &foreign,
            &foreign_query,
            2,
        )
        .unwrap();
    assert_eq!(foreign_ready.status, Status::Ready);
    assert_eq!(foreign_broker.try_snapshot().unwrap().terminal_cause, None);
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("failed invocation must never replay its source"))
            .is_err()
    );
}

#[test]
fn native_fuel_limits_report_revokes_original_control_and_joins_actual_wait_sent_source() {
    failed_invocation_stops_original_source(
        "(loop $forever br $forever) i32.const 0",
        Fault::Limits,
    );
}

#[test]
fn native_unreachable_trap_report_revokes_original_control_and_joins_actual_wait_sent_source() {
    failed_invocation_stops_original_source("unreachable", Fault::Trap);
}

#[test]
fn native_missing_completion_report_revokes_original_control_and_joins_actual_wait_sent_source() {
    failed_invocation_stops_original_source("i32.const 0", Fault::TaskProtocol);
}

fn fixed_invocation() -> Invocation {
    Invocation::new_transform(
        "channel-original-completion",
        Transform {
            handler: "control.test".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![0; 65],
        },
    )
    .unwrap()
}
fn completion_fixture(completion: &[u8]) -> Fixture {
    let data: String = completion
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect();
    Fixture::with_module(
        &failing_guest(&format!(
            "i32.const 131072 i32.const {} call $complete drop i32.const 0",
            completion.len()
        )),
        &format!(r#"(data (i32.const 131072) "{data}")"#),
        Limits::default(),
        1,
        10_000,
        true,
    )
}

#[test]
fn correlated_plugin_failure_stops_original_source_after_preserving_owned_ok_execution_report() {
    let input = fixed_invocation();
    let failure = PluginFailure {
        code: FailureCode::Failed,
        message: "real plugin reported failure".into(),
    };
    let completion = input.failure_completion(&failure).unwrap();
    let mut fixture = completion_fixture(&completion);
    let stopped = waiting_source(&fixture.broker);
    let frame = fixture.frame();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let source_result = finish_failed_source(&fixture, &report, stopped);
    assert_eq!(source_result, Ok(Err(Error::Denied)));
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 1);
    assert_eq!(report.failure, Some(failure));
    assert!(report.response.is_none());
    assert!(report.output.is_none());
    let snapshot = fixture.broker.try_snapshot().unwrap();
    assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(snapshot.last_acked, 0);
    fixture.assert_no_durable_ack(&frame.source_epoch);
}

#[test]
fn completed_oversized_output_is_rejected_before_original_control_stop_without_rewriting_report() {
    let input = fixed_invocation();
    let completion = input.output_completion(&[0x52; 65]).unwrap();
    let mut fixture = completion_fixture(&completion);
    let stopped = waiting_source(&fixture.broker);
    let frame = fixture.frame();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let source_result = finish_failed_source(&fixture, &report, stopped);
    assert_eq!(source_result, Ok(Err(Error::Denied)));
    assert_eq!(report.execution.outcome, Err(Fault::TaskProtocol));
    assert_eq!(report.execution.host_calls, 1);
    assert!(report.failure.is_none());
    assert!(report.response.is_none());
    assert!(report.output.is_none());
    let snapshot = fixture.broker.try_snapshot().unwrap();
    assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(snapshot.last_acked, 0);
    fixture.assert_no_durable_ack(&frame.source_epoch);
}

#[test]
fn successful_correlated_output_keeps_original_control_live_until_explicit_host_stop() {
    let input = fixed_invocation();
    let completion = input.output_completion(b"valid-output").unwrap();
    let mut fixture = completion_fixture(&completion);
    let stopped = waiting_source(&fixture.broker);
    let frame = fixture.frame();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let still_ready = fixture.call(Action::Query, 30);
    let before_stop = fixture.broker.try_snapshot().unwrap();
    fixture.instance.request_stop();
    let source_result = stopped.recv_timeout(WAIT);
    fixture.joined();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 1);
    assert_eq!(report.output.unwrap().bytes, b"valid-output");
    assert!(report.failure.is_none());
    assert!(report.response.is_none());
    assert_eq!(still_ready.status, Status::Ready);
    assert_eq!(before_stop.terminal_cause, None);
    assert_eq!(before_stop.cleanup_proof, CleanupProof::Pending);
    assert_eq!(source_result, Ok(Err(Error::Denied)));
    fixture.assert_no_durable_ack(&frame.source_epoch);
}

#[test]
fn authentic_invalid_registration_early_report_stops_original_wait_sent_source_without_execution() {
    let mut fixture = Fixture::new();
    let stopped = waiting_source(&fixture.broker);
    let frame = fixture.frame();
    let input = Invocation::new_transform(
        "channel-original-invalid-registration",
        Transform {
            handler: "not.registered".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![0; 65],
        },
    )
    .unwrap();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let source_result = finish_failed_source(&fixture, &report, stopped);
    assert_eq!(source_result, Ok(Err(Error::Denied)));
    assert_eq!(report.execution.outcome, Err(Fault::TaskProtocol));
    assert_eq!(report.execution.host_calls, 0);
    assert_eq!(
        report.execution.fuel_remaining,
        fixture.instance.package().limits().fuel
    );
    assert!(report.failure.is_none());
    assert!(report.response.is_none());
    assert!(report.output.is_none());
    let snapshot = fixture.broker.try_snapshot().unwrap();
    assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(snapshot.last_acked, 0);
    fixture.assert_no_durable_ack(&frame.source_epoch);
}

#[test]
fn unauthenticated_owner_failures_never_revoke_original_or_foreign_controls() {
    let mut fixture = Fixture::with_program("i32.const 0", Limits::default(), 2, 10_000, true);
    let foreign_instance = fixture.manager.connect(ID, &mut fixture.host).unwrap();
    let foreign_broker = fixture.bind_peer(&foreign_instance);
    let mut other = Fixture::new();
    let input = fixture.invocation();
    let invalid = Invocation::new_transform(
        "channel-foreign-invalid-registration",
        Transform {
            handler: "not.registered".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![0; 65],
        },
    )
    .unwrap();
    for (input, expected) in [
        (&input, Fault::PackageBinding),
        (&invalid, Fault::TaskProtocol),
    ] {
        let reports = [
            fixture.broker.run_invocation(
                &other.manager,
                &mut fixture.host,
                &fixture.instance,
                input,
                || 2,
            ),
            fixture.broker.run_invocation(
                &fixture.manager,
                &mut other.host,
                &fixture.instance,
                input,
                || 2,
            ),
            fixture.broker.run_invocation(
                &fixture.manager,
                &mut fixture.host,
                &foreign_instance,
                input,
                || 2,
            ),
        ];
        for report in reports {
            assert_eq!(report.execution.outcome, Err(expected.clone()));
            assert_eq!(report.execution.host_calls, 0);
            assert!(report.output.is_none());
            assert!(report.response.is_none());
            assert!(report.failure.is_none());
        }
        assert_eq!(fixture.call(Action::Query, 20).status, Status::Ready);
        assert_eq!(other.call(Action::Query, 21).status, Status::Ready);
        let request = request_for(&foreign_broker, Action::Query, 22);
        let ready = foreign_broker
            .dispatch(
                &fixture.manager,
                &mut fixture.host,
                &foreign_instance,
                &request,
                2,
            )
            .unwrap();
        assert_eq!(ready.status, Status::Ready);
        assert_eq!(fixture.broker.try_snapshot().unwrap().terminal_cause, None);
        assert_eq!(other.broker.try_snapshot().unwrap().terminal_cause, None);
        assert_eq!(foreign_broker.try_snapshot().unwrap().terminal_cause, None);
    }
}

#[test]
fn cleanup_first_after_atomic_stop_establishes_revoked_under_original_queue_lock() {
    let mut fixture = Fixture::new();
    let release_source = fixture.held_source();
    let frame = fixture.frame();
    let cleanup = fixture.broker.cleanup_handle();
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
        let started = Instant::now();
        fixture.instance.request_stop();
        assert_eq!(cleanup.try_cleanup(), Status::ClosingUnconfirmed);
        assert!(fixture.broker.try_snapshot().is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        release_lock.send(()).unwrap();
        holder.join().unwrap();
    });
    // No dispatch, gate or reap precedes this cleanup-first cause decision.
    assert_eq!(cleanup.try_cleanup(), Status::ClosingUnconfirmed);
    let pending = fixture.broker.try_snapshot().unwrap();
    let cause = pending.terminal_cause;
    assert!(!pending.resource_reclaimed);
    assert_eq!(pending.cleanup_proof, CleanupProof::Pending);
    assert_eq!(pending.last_acked, 0);
    release_source.send(()).unwrap();
    fixture.joined();
    assert_eq!(cause, Some(Status::Revoked));
    assert_eq!(cleanup.terminal_cause(), Some(Status::Revoked));
    fixture.assert_no_durable_ack(&frame.source_epoch);
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("cleanup must never replay its original source"))
            .is_err()
    );
}

#[test]
fn cleanup_first_after_monotonic_expiry_establishes_expired_under_original_queue_lock() {
    let mut fixture = Fixture::with_program("i32.const 0", Limits::default(), 1, 500, false);
    let release_source = fixture.held_source();
    let frame = fixture.frame();
    let cleanup = fixture.broker.cleanup_handle();
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
        thread::sleep(Duration::from_millis(650));
        let started = Instant::now();
        assert_eq!(cleanup.try_cleanup(), Status::ClosingUnconfirmed);
        assert!(fixture.broker.try_snapshot().is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        release_lock.send(()).unwrap();
        holder.join().unwrap();
    });
    // The logical clock remains at 2; only the actual original grant elapsed.
    assert_eq!(cleanup.try_cleanup(), Status::ClosingUnconfirmed);
    let pending = fixture.broker.try_snapshot().unwrap();
    let cause = pending.terminal_cause;
    assert!(!pending.resource_reclaimed);
    assert_eq!(pending.cleanup_proof, CleanupProof::Pending);
    assert_eq!(pending.last_acked, 0);
    release_source.send(()).unwrap();
    fixture.joined();
    assert_eq!(cause, Some(Status::Expired));
    assert_eq!(cleanup.terminal_cause(), Some(Status::Expired));
    fixture.assert_no_durable_ack(&frame.source_epoch);
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("expiry must never replay its original source"))
            .is_err()
    );
}

#[test]
fn cleanup_without_created_producer_proves_absence_and_preserves_control_or_deadline_cause() {
    for expired in [false, true] {
        let fixture = Fixture::with_program(
            "i32.const 0",
            Limits::default(),
            1,
            if expired { 100 } else { 10_000 },
            false,
        );
        if expired {
            thread::sleep(Duration::from_millis(150));
        } else {
            fixture.instance.request_stop();
        }
        // Blocking cleanup is also cause-first without a preceding dispatch.
        let cleanup = fixture.broker.cleanup_handle();
        let status = cleanup.cleanup();
        assert_eq!(
            status,
            if expired {
                Status::Expired
            } else {
                Status::Revoked
            }
        );
        let snapshot = fixture.broker.try_snapshot().unwrap();
        assert!(snapshot.resource_reclaimed);
        assert_eq!(snapshot.cleanup_proof, CleanupProof::NoProducer);
        assert_eq!(snapshot.producer_outcome, ProducerOutcome::Pending);
        assert_eq!(snapshot.last_acked, 0);
        assert_eq!(snapshot.usage.messages, 0);
        fixture.assert_no_durable_ack(&fixture.broker.endpoint().source_epoch);
    }
}

#[test]
fn real_producer_panic_after_revoked_keeps_original_cause_and_actual_join_proof() {
    let mut fixture = Fixture::new();
    let (published, ready) = mpsc::channel();
    let (release, held) = mpsc::channel();
    fixture
        .broker
        .spawn(move |producer| {
            producer
                .push(b"panic-native-source".to_vec(), b"cursor-1".to_vec())
                .unwrap();
            published.send(()).unwrap();
            held.recv_timeout(WAIT).unwrap();
            panic!("actual original producer panic after established revocation");
        })
        .unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let frame = fixture.frame();
    fixture.instance.request_stop();
    let cleanup = fixture.broker.cleanup_handle();
    assert_eq!(cleanup.try_reap(), Status::ClosingUnconfirmed);
    assert_eq!(cleanup.terminal_cause(), Some(Status::Revoked));
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::Pending);
    release.send(()).unwrap();
    fixture.joined();
    let snapshot = fixture.broker.try_snapshot().unwrap();
    assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(snapshot.last_acked, 0);
    fixture.assert_no_durable_ack(&frame.source_epoch);
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
    assert_eq!(
        fixture.broker.cleanup_handle().try_cleanup(),
        Status::ClosingUnconfirmed
    );
    assert_eq!(
        fixture.broker.try_snapshot().unwrap().terminal_cause,
        Some(Status::Unknown)
    );
    release_source.send(()).unwrap();
    fixture.joined();
    assert_eq!(
        fixture.broker.try_snapshot().unwrap().terminal_cause,
        Some(Status::Unknown)
    );
    assert_eq!(
        fixture.broker.cleanup_handle().try_cleanup(),
        Status::Unknown
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
