//! Independent native broker faults with actual Manager, HostRuntime and SQLite.
//! Host-supplied local memory only; these tests qualify no HTTP/cloud provider.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
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

const ID: &str = "org.example.channel.faults";
const WAIT: Duration = Duration::from_secs(5);
const EXPIRES: u64 = 10_001;
fn budget() -> Budget {
    Budget {
        max_channels: 4,
        max_frame_bytes: 32_768,
        max_bytes: 1_048_576,
        max_messages: 128,
        max_requests: 4096,
        max_duration_ms: 10_000,
    }
}
fn source(kind: Kind, duplex: bool) -> Source {
    Source {
        kind,
        duplex,
        checkpoint_scope: None,
    }
}
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    digest: [u8; 32],
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self::with_store_budget(EventBudget::default())
    }
    fn with_store_budget(store_budget: EventBudget) -> Self {
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
                handler: "channel.exercise".into(),
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
            vec!["channel.exercise".into()],
            vec![Kind::ByteStream, Kind::Events],
        ));
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), store_budget).unwrap()).unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        Self {
            dir,
            manager,
            host,
            instance,
            digest,
        }
    }
    fn bind(&self, kind: Kind, duplex: bool, budget: Budget) -> ChannelBroker {
        self.manager
            .bind_channel(
                &self.host,
                &self.instance,
                self.digest,
                self.manager.revision(),
                source(kind, duplex),
                budget,
                EXPIRES,
                1,
            )
            .unwrap()
    }
    fn call(&mut self, broker: &ChannelBroker, action: Action, serial: u64, now: u64) -> Response {
        broker
            .dispatch(
                &self.manager,
                &mut self.host,
                &self.instance,
                &request(broker, action, serial),
                now,
            )
            .unwrap()
    }
}
fn request(broker: &ChannelBroker, action: Action, serial: u64) -> Request {
    let endpoint = broker.endpoint();
    let mut call_id = [1; 32];
    call_id[..8].copy_from_slice(&serial.to_le_bytes());
    Request {
        call_id,
        reference: endpoint.reference,
        source_epoch: endpoint.source_epoch,
        action,
    }
}
fn receive(f: &mut Fixture, broker: &ChannelBroker, last_acked: u64, serial: &mut u64) -> Frame {
    let deadline = Instant::now() + WAIT;
    loop {
        *serial += 1;
        let response = f.call(
            broker,
            Action::Receive {
                last_acked,
                credit_bytes: 32_768,
            },
            *serial,
            2,
        );
        match response.status {
            Status::Frame => return response.frame.unwrap(),
            Status::Idle => {
                assert!(Instant::now() < deadline, "producer did not publish");
                thread::sleep(Duration::from_millis(1));
            }
            status => panic!("unexpected receive status: {status:?}"),
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
fn joined(broker: &ChannelBroker) -> Status {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = broker.reap();
        if broker.snapshot().resource_reclaimed {
            assert_eq!(
                broker.snapshot().cleanup_proof,
                CleanupProof::Joined,
                "successful native spawn requires actual join proof"
            );
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "actual producer join was not observed"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn cross_manager_host_instance_reference_and_epoch_are_denied_before_accounting() {
    let mut f = Fixture::new();
    let mut foreign = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let valid = request(
        &broker,
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32_768,
        },
        1,
    );
    assert_eq!(
        broker.dispatch(&foreign.manager, &mut f.host, &f.instance, &valid, 2),
        Err(Error::Denied)
    );
    assert_eq!(
        broker.dispatch(&f.manager, &mut foreign.host, &f.instance, &valid, 2),
        Err(Error::Denied)
    );
    let other = f.manager.connect(ID, &mut f.host).unwrap();
    assert_eq!(
        broker.dispatch(&f.manager, &mut f.host, &other, &valid, 2),
        Err(Error::Denied)
    );
    for epoch in [false, true] {
        let mut altered = valid.clone();
        if epoch {
            altered.source_epoch[0] ^= 1;
        } else {
            altered.reference[0] ^= 1;
        }
        assert_eq!(
            broker.dispatch(&f.manager, &mut f.host, &f.instance, &altered, 2),
            Err(Error::Denied)
        );
    }
    assert_eq!(broker.snapshot().usage.requests, 0);
    assert!(
        f.manager
            .bind_channel(
                &f.host,
                &f.instance,
                [9; 32],
                f.manager.revision(),
                source(Kind::ByteStream, false),
                budget(),
                EXPIRES,
                1
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_channel(
                &f.host,
                &f.instance,
                f.digest,
                f.manager.revision() + 1,
                source(Kind::ByteStream, false),
                budget(),
                EXPIRES,
                1
            )
            .is_err()
    );
    assert_eq!(f.call(&broker, valid.action, 2, 2).status, Status::Idle);
}

#[test]
fn invalid_wire_consumes_bounded_request_budget_without_delivering_or_invoking_a_source() {
    let mut f = Fixture::new();
    let mut bounded = budget();
    bounded.max_requests = 4;
    let broker = f.bind(Kind::ByteStream, false, bounded);
    let valid = request(&broker, Action::Query, 1).encode().unwrap();
    for malformed in [
        vec![],
        vec![0; 8],
        valid[..valid.len() - 1].to_vec(),
        [valid.clone(), vec![0; 8]].concat(),
    ] {
        assert_eq!(
            broker.exchange(&f.manager, &mut f.host, &f.instance, &malformed, 2),
            Err(Error::Invalid)
        );
    }
    let usage = broker.snapshot().usage;
    assert_eq!(usage.requests, 4);
    assert_eq!(usage.messages, 0);
    assert_eq!(usage.bytes, 0);
    assert_eq!(
        f.call(
            &broker,
            Action::Receive {
                last_acked: 0,
                credit_bytes: 32_768
            },
            2,
            2
        )
        .status,
        Status::Limit
    );
    assert_eq!(broker.snapshot().usage, usage);
    // No source thread was ever spawned. Cleanup remains available after failed attempts exhaust budget.
    let closed = f.call(&broker, Action::Close, 3, 2);
    assert_eq!(closed.status, Status::Closed);
    assert!(closed.resource_reclaimed);
    assert_eq!(broker.snapshot().cleanup_proof, CleanupProof::NoProducer);
}

#[test]
fn credit_and_exact_ack_hold_the_single_inflight_frame_until_consumed() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let (published, barrier) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer.push(vec![7; 32], vec![]).unwrap();
            published.send(()).unwrap();
            held.recv_timeout(WAIT).unwrap();
            producer.finish().unwrap();
        })
        .unwrap();
    barrier.recv_timeout(WAIT).unwrap();
    assert_eq!(
        f.call(
            &broker,
            Action::Receive {
                last_acked: 0,
                credit_bytes: 31
            },
            1,
            2
        )
        .status,
        Status::Limit
    );
    let mut serial = 1;
    let frame = receive(&mut f, &broker, 0, &mut serial);
    let again = receive(&mut f, &broker, 0, &mut serial);
    assert_eq!(frame, again);
    let mut wrong_hash = frame.digest().unwrap();
    wrong_hash[0] ^= 1;
    for action in [
        Action::Ack {
            sequence: frame.sequence + 1,
            frame_sha256: frame.digest().unwrap(),
            cursor: vec![],
        },
        Action::Ack {
            sequence: frame.sequence,
            frame_sha256: wrong_hash,
            cursor: vec![],
        },
        Action::Ack {
            sequence: frame.sequence,
            frame_sha256: frame.digest().unwrap(),
            cursor: vec![1],
        },
    ] {
        serial += 1;
        assert_eq!(f.call(&broker, action, serial, 2).status, Status::Invalid);
    }
    assert_eq!(broker.snapshot().last_acked, 0);
    serial += 1;
    assert_eq!(
        f.call(&broker, ack(&frame), serial, 2).status,
        Status::Acked
    );
    assert_eq!(broker.snapshot().last_acked, 1);
    serial += 1;
    assert_eq!(
        f.call(&broker, ack(&frame), serial, 2).status,
        Status::Invalid
    );
    assert_eq!(broker.snapshot().usage.bytes, 32);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Closed);
    f.host.store_local().integrity_check().unwrap();
}

#[test]
fn producer_backpressure_unblocks_only_after_exact_consumption_ack() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let (steps, observations) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer.push(vec![1; 32], vec![]).unwrap();
            steps.send(1).unwrap();
            producer.push(vec![2; 32], vec![]).unwrap();
            steps.send(2).unwrap();
            producer.finish().unwrap();
        })
        .unwrap();
    assert_eq!(observations.recv_timeout(WAIT).unwrap(), 1);
    let mut serial = 0;
    let first = receive(&mut f, &broker, 0, &mut serial);
    assert_eq!(observations.try_recv(), Err(mpsc::TryRecvError::Empty));
    serial += 1;
    assert_eq!(
        f.call(&broker, ack(&first), serial, 2).status,
        Status::Acked
    );
    assert_eq!(observations.recv_timeout(WAIT).unwrap(), 2);
    let second = receive(&mut f, &broker, 1, &mut serial);
    assert_eq!(
        (second.sequence, second.bytes.as_slice()),
        (2, &[2; 32][..])
    );
    serial += 1;
    assert_eq!(
        f.call(&broker, ack(&second), serial, 2).status,
        Status::Acked
    );
    assert_eq!(joined(&broker), Status::Closed);
}

#[test]
fn eof_does_not_discard_an_unacknowledged_final_frame_or_claim_early_close() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    broker
        .spawn(|producer| {
            producer.push(vec![3; 64], vec![]).unwrap();
            producer.finish().unwrap();
        })
        .unwrap();
    let mut serial = 0;
    let final_frame = receive(&mut f, &broker, 0, &mut serial);
    assert_eq!(joined(&broker), Status::Ready);
    serial += 1;
    let query = f.call(&broker, Action::Query, serial, 2);
    assert_eq!(query.status, Status::Ready);
    assert!(
        !query.resource_reclaimed,
        "wire reclamation applies only to a terminal response"
    );
    serial += 1;
    assert_eq!(
        f.call(&broker, ack(&final_frame), serial, 2).status,
        Status::Acked
    );
    serial += 1;
    let closed = f.call(&broker, Action::Query, serial, 2);
    assert_eq!(closed.status, Status::Closed);
    assert!(closed.resource_reclaimed);
}

#[test]
fn cancel_immediately_closes_business_gate_while_actual_thread_join_remains_unconfirmed() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, true, budget());
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    f.instance.stop();
    assert_eq!(
        f.call(
            &broker,
            Action::Send {
                sequence: 1,
                bytes: vec![1]
            },
            1,
            2
        )
        .status,
        Status::Revoked
    );
    let closing = f.call(&broker, Action::Close, 2, 2);
    assert_eq!(closing.status, Status::ClosingUnconfirmed);
    assert!(!closing.resource_reclaimed);
    assert_eq!(broker.snapshot().usage.messages, 0);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Revoked);
    let closed = f.call(&broker, Action::Query, 3, 2);
    assert_eq!(closed.status, Status::Revoked);
    assert!(closed.resource_reclaimed);
}

#[test]
fn durable_manager_disable_revokes_pending_data_before_delivery_and_new_binding() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer.push(vec![9; 32], vec![]).unwrap();
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    f.manager
        .set_enabled(ID, f.digest, false, f.manager.revision())
        .unwrap();
    let revoked = f.call(
        &broker,
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32_768,
        },
        1,
        2,
    );
    assert_eq!(revoked.status, Status::Revoked);
    assert!(revoked.frame.is_none());
    assert!(
        f.manager
            .bind_channel(
                &f.host,
                &f.instance,
                f.digest,
                f.manager.revision(),
                source(Kind::ByteStream, false),
                budget(),
                EXPIRES,
                2
            )
            .is_err()
    );
    assert_eq!(broker.cleanup(), Status::ClosingUnconfirmed);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Revoked);
    f.host.store_local().integrity_check().unwrap();
}

#[test]
fn expiry_stops_delivery_and_source_before_actual_thread_reclamation() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (result, output) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            result.send(producer.push(vec![1], vec![])).unwrap();
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    assert_eq!(
        f.call(
            &broker,
            Action::Receive {
                last_acked: 0,
                credit_bytes: 32_768
            },
            1,
            EXPIRES
        )
        .status,
        Status::Expired
    );
    assert_eq!(broker.cleanup(), Status::ClosingUnconfirmed);
    release.send(()).unwrap();
    assert_eq!(output.recv_timeout(WAIT).unwrap(), Err(Error::Closed));
    assert_eq!(joined(&broker), Status::Expired);
    let expired = f.call(&broker, Action::Query, 2, EXPIRES);
    assert_eq!(expired.status, Status::Expired);
    assert!(expired.resource_reclaimed);
}

#[test]
fn panic_and_uncertain_producer_exit_stay_unknown_after_actual_join_and_no_replay() {
    for panic in [false, true] {
        let mut f = Fixture::new();
        let broker = f.bind(Kind::ByteStream, true, budget());
        broker
            .spawn(move |producer| {
                if panic {
                    panic!("intentional native source fault");
                }
                drop(producer);
            })
            .unwrap();
        assert_eq!(joined(&broker), Status::Unknown);
        let unknown = f.call(&broker, Action::Query, 1, 2);
        assert_eq!(unknown.status, Status::Unknown);
        assert!(unknown.resource_reclaimed);
        assert_eq!(
            f.call(
                &broker,
                Action::Send {
                    sequence: 1,
                    bytes: vec![8]
                },
                2,
                2
            )
            .status,
            Status::Unknown
        );
        assert_eq!(broker.snapshot().usage.messages, 0);
        assert!(broker.spawn(|_| panic!("must not replay source")).is_err());
    }
}

#[test]
fn send_admission_does_not_claim_peer_observation_and_duplicate_send_is_not_replayed() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, true, budget());
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (receipt, receipt_rx) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            receipt.send(producer.receive_sent().unwrap()).unwrap();
            producer.finish().unwrap();
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let action = Action::Send {
        sequence: 1,
        bytes: vec![4; 32],
    };
    let accepted = f.call(&broker, action.clone(), 1, 2);
    assert_eq!(accepted.status, Status::Accepted);
    assert_eq!(accepted.accepted_sequence, 1);
    assert_eq!(accepted.last_acked, 0);
    assert!(!accepted.resource_reclaimed);
    assert_eq!(receipt_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(f.call(&broker, action, 1, 2).status, Status::Invalid);
    assert_eq!(
        f.call(
            &broker,
            Action::Send {
                sequence: 2,
                bytes: vec![5]
            },
            2,
            2
        )
        .status,
        Status::Limit
    );
    assert_eq!(broker.snapshot().usage.messages, 1);
    release.send(()).unwrap();
    assert_eq!(
        receipt_rx.recv_timeout(WAIT).unwrap(),
        Some((1, vec![4; 32]))
    );
    assert_eq!(joined(&broker), Status::Closed);
}

#[test]
fn ack_and_cleanup_do_not_refund_total_bytes_messages_channels_or_requests() {
    let mut f = Fixture::new();
    let mut small = budget();
    small.max_frame_bytes = 32;
    small.max_bytes = 64;
    small.max_messages = 2;
    small.max_channels = 1;
    let broker = f.bind(Kind::ByteStream, false, small);
    let (limit, limit_rx) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer.push(vec![1; 32], vec![]).unwrap();
            producer.push(vec![2; 32], vec![]).unwrap();
            limit.send(producer.push(vec![3; 32], vec![])).unwrap();
            producer.finish().unwrap();
        })
        .unwrap();
    let mut serial = 0;
    for last_acked in 0..2 {
        let frame = receive(&mut f, &broker, last_acked, &mut serial);
        serial += 1;
        assert_eq!(
            f.call(&broker, ack(&frame), serial, 2).status,
            Status::Acked
        );
    }
    assert_eq!(limit_rx.recv_timeout(WAIT).unwrap(), Err(Error::Limit));
    assert_eq!(joined(&broker), Status::Closed);
    let usage = broker.snapshot().usage;
    assert_eq!((usage.channels, usage.messages, usage.bytes), (1, 2, 64));
    assert_eq!(broker.cleanup(), Status::Closed);
    assert_eq!(broker.snapshot().usage, usage);
    assert!(
        f.manager
            .bind_channel(
                &f.host,
                &f.instance,
                f.digest,
                f.manager.revision(),
                source(Kind::ByteStream, false),
                small,
                EXPIRES,
                2
            )
            .is_err()
    );
}

#[test]
fn request_budget_exhaustion_preserves_the_cleanup_route() {
    let mut f = Fixture::new();
    let mut small = budget();
    small.max_requests = 2;
    let broker = f.bind(Kind::ByteStream, false, small);
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    for serial in 1..=2 {
        assert_eq!(
            f.call(
                &broker,
                Action::Receive {
                    last_acked: 0,
                    credit_bytes: 32_768
                },
                serial,
                2
            )
            .status,
            Status::Idle
        );
    }
    assert_eq!(
        f.call(
            &broker,
            Action::Receive {
                last_acked: 0,
                credit_bytes: 32_768
            },
            3,
            2
        )
        .status,
        Status::Limit
    );
    assert_eq!(broker.snapshot().usage.requests, 2);
    assert_eq!(
        f.call(&broker, Action::Close, 4, 2).status,
        Status::ClosingUnconfirmed
    );
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Closed);
    let query = f.call(&broker, Action::Query, 5, 2);
    assert_eq!(query.status, Status::Closed);
    assert!(query.resource_reclaimed);
}

#[test]
fn rebinding_cannot_extend_deadline_expand_budget_or_reuse_ref_epoch() {
    let f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let next = f
        .manager
        .bind_channel(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            source(Kind::Events, false),
            budget(),
            EXPIRES,
            2,
        )
        .unwrap();
    assert_ne!(next.endpoint().reference, broker.endpoint().reference);
    assert_ne!(next.endpoint().source_epoch, broker.endpoint().source_epoch);
    let mut changed = budget();
    changed.max_messages += 1;
    for (budget, expires, now) in [
        (changed, EXPIRES, 2),
        (budget(), EXPIRES + 1, 2),
        (budget(), EXPIRES, 1),
    ] {
        assert!(
            f.manager
                .bind_channel(
                    &f.host,
                    &f.instance,
                    f.digest,
                    f.manager.revision(),
                    source(Kind::ByteStream, false),
                    budget,
                    expires,
                    now
                )
                .is_err()
        );
    }
    assert_eq!(broker.snapshot().usage.channels, 2);
    assert!(f.dir.path().join("db").exists());
}

#[test]
fn backwards_logical_clock_keeps_uncertainty_instead_of_delivering_pending_bytes() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer.push(vec![1], vec![]).unwrap();
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    assert_eq!(f.call(&broker, Action::Query, 1, 4).status, Status::Ready);
    let unknown = f.call(
        &broker,
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32_768,
        },
        2,
        3,
    );
    assert_eq!(unknown.status, Status::Unknown);
    assert!(unknown.frame.is_none());
    assert!(!unknown.resource_reclaimed);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Unknown);
}

#[test]
fn cleanup_only_owner_retains_the_real_join_after_broker_and_manager_are_dropped() {
    let f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let cleanup = broker.cleanup_handle();
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    drop(broker);
    drop(f.manager);
    assert_eq!(cleanup.cleanup(), Status::ClosingUnconfirmed);
    assert!(!cleanup.resource_reclaimed());
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::Pending);
    assert_eq!(cleanup.outstanding_resources(), 1);
    release.send(()).unwrap();
    let deadline = Instant::now() + WAIT;
    while !cleanup.resource_reclaimed() {
        cleanup.reap();
        assert!(
            Instant::now() < deadline,
            "retained cleanup owner did not actually join"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(cleanup.outstanding_resources(), 0);
    assert_eq!(cleanup.terminal_cause(), Some(Status::Closed));
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::Joined);
}

#[test]
fn event_cursor_advances_only_after_exact_ack_and_survives_real_sqlite_reopen() {
    let mut f = Fixture::new();
    let subscription = [0x54; 32];
    let broker = f
        .manager
        .bind_channel(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            Source {
                kind: Kind::Events,
                duplex: false,
                checkpoint_scope: Some(subscription),
            },
            budget(),
            EXPIRES,
            1,
        )
        .unwrap();
    let endpoint = broker.endpoint();
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer
                .push(b"host-local-event".to_vec(), b"cursor-1".to_vec())
                .unwrap();
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            producer.finish().unwrap();
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let mut serial = 0;
    let frame = receive(&mut f, &broker, 0, &mut serial);
    assert!(
        f.host
            .store_local()
            .channel_checkpoint(&subscription, &endpoint.source_epoch)
            .unwrap()
            .is_none()
    );
    serial += 1;
    let wrong = Action::Ack {
        sequence: frame.sequence,
        frame_sha256: frame.digest().unwrap(),
        cursor: b"cursor-2".to_vec(),
    };
    assert_eq!(f.call(&broker, wrong, serial, 2).status, Status::Invalid);
    assert!(
        f.host
            .store_local()
            .channel_checkpoint(&subscription, &endpoint.source_epoch)
            .unwrap()
            .is_none()
    );
    serial += 1;
    let original = request(&broker, ack(&frame), serial);
    let response = broker
        .dispatch(&f.manager, &mut f.host, &f.instance, &original, 2)
        .unwrap();
    assert_eq!(response.status, Status::Acked);
    assert!(!response.resource_reclaimed);
    let checkpoint = f
        .host
        .store_local()
        .channel_checkpoint(&subscription, &endpoint.source_epoch)
        .unwrap()
        .unwrap();
    assert_eq!((checkpoint.sequence, checkpoint.revision), (1, 1));
    assert_eq!(checkpoint.cursor, b"cursor-1");
    assert_eq!(checkpoint.frame_sha256, frame.digest().unwrap());
    let receipt = f
        .host
        .store_local()
        .channel_ack_receipt(&subscription, &endpoint.source_epoch, 1)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.checkpoint, checkpoint);
    assert_eq!(receipt.frame_wire, frame.encode().unwrap());
    assert_eq!(receipt.request_wire, original.encode().unwrap());
    assert_eq!(receipt.response_wire, response.encode().unwrap());
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Closed);
    f.host.store_local().integrity_check().unwrap();
    // A fresh Store connection reads durable state; it does not grant the new broker an old epoch.
    let reopened = Store::open(&f.dir.path().join("db"), Default::default()).unwrap();
    assert_eq!(
        reopened
            .channel_checkpoint(&subscription, &endpoint.source_epoch)
            .unwrap(),
        Some(checkpoint)
    );
    assert!(
        reopened
            .channel_checkpoint(&subscription, &[0x55; 32])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reopened
            .channel_ack_receipt(&subscription, &endpoint.source_epoch, 1)
            .unwrap(),
        Some(receipt)
    );
    reopened.integrity_check().unwrap();
}

#[test]
fn a_never_spawned_source_is_reclaimed_without_inventing_a_thread_join() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let cleanup = broker.cleanup_handle();
    assert_eq!(f.call(&broker, Action::Query, 1, 2).status, Status::Ready);
    assert!(!cleanup.resource_reclaimed());
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::Pending);
    assert_eq!(cleanup.outstanding_resources(), 1);
    let closed = f.call(&broker, Action::Close, 2, 2);
    assert_eq!(closed.status, Status::Closed);
    assert!(closed.resource_reclaimed);
    assert!(cleanup.resource_reclaimed());
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::NoProducer);
    assert_eq!(cleanup.outstanding_resources(), 0);
    assert!(
        broker
            .spawn(|_| panic!("closed unstarted source must not run"))
            .is_err()
    );
    assert_eq!(broker.snapshot().usage.messages, 0);
}

#[test]
fn real_journal_capacity_rejection_is_unknown_and_never_redelivers_the_consumed_frame() {
    let mut f = Fixture::with_store_budget(EventBudget {
        max_count: 1024,
        max_bytes: 1,
    });
    let subscription = [0x61; 32];
    let broker = f
        .manager
        .bind_channel(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            Source {
                kind: Kind::Events,
                duplex: false,
                checkpoint_scope: Some(subscription),
            },
            budget(),
            EXPIRES,
            1,
        )
        .unwrap();
    let epoch = broker.endpoint().source_epoch;
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            producer
                .push(b"actually-consumed".to_vec(), b"cursor-1".to_vec())
                .unwrap();
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let mut serial = 0;
    let frame = receive(&mut f, &broker, 0, &mut serial);
    serial += 1;
    assert_eq!(
        broker.dispatch(
            &f.manager,
            &mut f.host,
            &f.instance,
            &request(&broker, ack(&frame), serial),
            2
        ),
        Err(Error::Unknown)
    );
    assert_eq!(broker.snapshot().terminal_cause, Some(Status::Unknown));
    assert_eq!(broker.snapshot().last_acked, 0);
    assert!(
        f.host
            .store_local()
            .channel_checkpoint(&subscription, &epoch)
            .unwrap()
            .is_none()
    );
    assert!(
        f.host
            .store_local()
            .channel_ack_receipt(&subscription, &epoch, 1)
            .unwrap()
            .is_none()
    );
    serial += 1;
    let unknown = f.call(
        &broker,
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32_768,
        },
        serial,
        2,
    );
    assert_eq!(unknown.status, Status::Unknown);
    assert!(unknown.frame.is_none());
    assert!(!unknown.resource_reclaimed);
    assert_eq!(broker.cleanup(), Status::ClosingUnconfirmed);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Unknown);
    serial += 1;
    let reclaimed = f.call(&broker, Action::Query, serial, 2);
    assert_eq!(reclaimed.status, Status::Unknown);
    assert!(reclaimed.resource_reclaimed);
    assert!(
        broker
            .spawn(|_| panic!("ambiguous consumed event was replayed"))
            .is_err()
    );
    f.host.store_local().integrity_check().unwrap();
    let reopened = Store::open(&f.dir.path().join("db"), Default::default()).unwrap();
    assert!(
        reopened
            .channel_checkpoint(&subscription, &epoch)
            .unwrap()
            .is_none()
    );
    reopened.integrity_check().unwrap();
}

#[test]
fn managed_channel_invocation_checks_handler_types_and_input_limit_before_execution() {
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    for (handler, input_type, output_type, size) in [
        ("other.handler", "bytes", "bytes", 65),
        ("channel.exercise", "text", "bytes", 65),
        ("channel.exercise", "bytes", "text", 65),
        ("channel.exercise", "bytes", "bytes", 66),
    ] {
        let task = morrow_core::task::Invocation::new_transform(
            "channel-negative-shape",
            morrow_core::task::Transform {
                handler: handler.into(),
                input_type: input_type.into(),
                output_type: output_type.into(),
                input: vec![0; size],
            },
        )
        .unwrap();
        let result = broker.run_invocation(&f.manager, &mut f.host, &f.instance, &task, || 2);
        assert_eq!(
            result.execution.outcome,
            Err(morrow_plugin_runtime::Fault::TaskProtocol)
        );
        assert_eq!(result.execution.host_calls, 0);
        assert!(result.output.is_none());
        assert!(result.failure.is_none());
        assert!(result.response.is_none());
    }
    assert_eq!(broker.snapshot().usage.requests, 0);
    assert_eq!(broker.snapshot().usage.messages, 0);
}

#[cfg(all(feature = "fault-injection", target_os = "windows"))]
#[test]
fn actual_os_thread_creation_failure_uses_no_producer_proof_and_never_runs_or_replays_source() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let mut f = Fixture::new();
    let broker = f.bind(Kind::ByteStream, false, budget());
    let cleanup = broker.cleanup_handle();
    let called = Arc::new(AtomicBool::new(false));
    let source_called = called.clone();
    // The fault-only seam calls the real std::thread::Builder / Windows creation API.
    // An impossible reserve request must fail creation; no real resources are exhausted.
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        broker.spawn_with_stack_size(usize::MAX, move |producer| {
            source_called.store(true, Ordering::SeqCst);
            drop(producer);
        })
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(reason) => panic!(
            "real std::thread::Builder panicked instead of returning the Windows creation error: {:?}",
            reason
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| reason.downcast_ref::<&str>().copied())
        ),
    };
    assert_eq!(result, Err(Error::Unknown));
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(cleanup.cleanup(), Status::Unknown);
    assert!(cleanup.resource_reclaimed());
    assert_eq!(cleanup.cleanup_proof(), CleanupProof::NoProducer);
    assert_eq!(cleanup.outstanding_resources(), 0);
    let query = f.call(&broker, Action::Query, 1, 2);
    assert_eq!(query.status, Status::Unknown);
    assert!(query.resource_reclaimed);
    assert_eq!(
        broker.snapshot().producer_outcome,
        morrow_plugin_runtime::channel::ProducerOutcome::Unknown
    );
    assert!(
        broker
            .spawn(|_| panic!("failed creation was replayed"))
            .is_err()
    );
    assert_eq!(broker.snapshot().usage.messages, 0);
}

#[test]
fn monotonic_wall_deadline_expires_even_when_the_logical_clock_is_frozen() {
    let mut f = Fixture::new();
    let mut bounded = budget();
    bounded.max_duration_ms = 20;
    let broker = f
        .manager
        .bind_channel(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            source(Kind::ByteStream, false),
            bounded,
            21,
            1,
        )
        .unwrap();
    let (started, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    broker
        .spawn(move |producer| {
            started.send(()).unwrap();
            let _ = held.recv_timeout(WAIT);
            drop(producer);
        })
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    thread::sleep(Duration::from_millis(35));
    let expired = f.call(
        &broker,
        Action::Receive {
            last_acked: 0,
            credit_bytes: 32_768,
        },
        1,
        2,
    );
    assert_eq!(expired.status, Status::Expired);
    assert!(expired.frame.is_none());
    assert!(!expired.resource_reclaimed);
    assert_eq!(broker.cleanup(), Status::ClosingUnconfirmed);
    release.send(()).unwrap();
    assert_eq!(joined(&broker), Status::Expired);
    assert_eq!(broker.snapshot().usage.messages, 0);
}
