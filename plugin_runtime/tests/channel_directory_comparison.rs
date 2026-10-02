//! One fresh contract-equivalent Directory example comparison, not old binaries.
//! Baseline operation semantics are Receive/digest/transcript/ACK, then Close.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    channel::{ChannelBroker, CleanupProof, Error, ProducerOutcome, Source},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(5);
const SCOPE: [u8; 32] = [0x59; 32];
const FRAMES: u64 = 5;
const FRAME_BYTES: usize = 32768;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn payload(sequence: u64) -> Vec<u8> {
    (0..FRAME_BYTES)
        .map(|offset| ((offset + sequence as usize * 17) % 251) as u8)
        .collect()
}
fn cursor(kind: Kind, sequence: u64) -> Vec<u8> {
    if kind == Kind::Events {
        sequence.to_le_bytes().to_vec()
    } else {
        Vec::new()
    }
}
fn payload_digest() -> [u8; 32] {
    let mut digest = Sha256::new();
    for sequence in 1..=FRAMES {
        digest.update(payload(sequence));
    }
    digest.finalize().into()
}
fn load(control: bool) -> Package {
    let prefix = if control {
        "MORROW_DIRECTORY_COMPARISON_ONESHOT"
    } else {
        "MORROW_DIRECTORY_COMPARISON_REUSABLE"
    };
    let read = |suffix| {
        let key = format!("{prefix}_{suffix}");
        let path =
            std::env::var_os(&key).unwrap_or_else(|| panic!("mandatory fresh artifact: {key}"));
        std::fs::read(PathBuf::from(path)).unwrap()
    };
    let wasm = read("WASM");
    let archive = read("PACKAGE");
    for (suffix, bytes) in [("WASM_SHA256", &wasm), ("PACKAGE_SHA256", &archive)] {
        let key = format!("{prefix}_{suffix}");
        assert_eq!(
            hex(&Sha256::digest(bytes)),
            std::env::var(&key).unwrap(),
            "independent pin {key}"
        );
    }
    let package = Package::decode(&archive).unwrap();
    assert_eq!(package.module(), wasm);
    assert_eq!(package.archive(), archive);
    let (id, version) = if control {
        (
            "org.example.channel.directory-sdk-oneshot-003",
            "0.1.0-test58.6",
        )
    } else {
        (
            "org.example.channel.directory-sdk-reusable-003",
            "0.1.0-test58.5",
        )
    };
    assert_eq!(package.manifest().package_id, id);
    assert_eq!(package.manifest().package_version, version);
    assert!(package.capabilities().is_empty());
    let budget = package.manifest().budget.as_ref().unwrap();
    assert_eq!(budget.fuel, 20_000_000);
    assert_eq!(budget.memory_bytes, 16 * 1024 * 1024);
    assert_eq!(budget.host_calls, 16);
    eprintln!(
        "directory comparison package control={control} id={id} wasm_sha256={} package_sha256={} fresh_build=true baseline_operations=receive/digest/transcript/ack/close",
        hex(&Sha256::digest(&wasm)),
        hex(&Sha256::digest(&archive))
    );
    package
}
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
}
impl Fixture {
    fn new(package: &Package, kind: Kind) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(package, manager.revision()).unwrap();
        let id = &package.manifest().package_id;
        manager
            .set_enabled(id, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager.connect(id, &mut host).unwrap();
        let limits = instance.package().limits();
        assert_eq!(limits.fuel, 20_000_000);
        assert_eq!(limits.memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.host_calls, 16);
        let broker = manager
            .bind_channel(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                Source {
                    kind,
                    duplex: true,
                    checkpoint_scope: (kind == Kind::Events).then_some(SCOPE),
                },
                Budget {
                    max_channels: 1,
                    max_frame_bytes: FRAME_BYTES as u32,
                    max_bytes: FRAMES * FRAME_BYTES as u64,
                    max_messages: FRAMES,
                    max_requests: 2 * FRAMES + 1,
                    max_duration_ms: 30_000,
                },
                30_001,
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
    fn invocation(&self) -> Invocation {
        let directory = self.broker.directory();
        let bytes = directory.encode().unwrap();
        assert_eq!(
            morrow_core::channel::Directory::decode(&bytes).unwrap(),
            directory
        );
        Invocation::new_transform(
            "channel-directory-comparison-003",
            Transform {
                handler: "channel.directory.consume".into(),
                input_type: "morrow.channel.directory.v1".into(),
                output_type: "bytes".into(),
                input: bytes,
            },
        )
        .unwrap()
    }
    fn join(&self) {
        let cleanup = self.broker.cleanup_handle();
        let deadline = Instant::now() + WAIT;
        loop {
            let _ = cleanup.try_reap();
            if let Some(snapshot) = self.broker.try_snapshot() {
                if snapshot.resource_reclaimed {
                    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "actual original producer join missing"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn verify_acks(&self, kind: Kind, acknowledged: u64) {
        let endpoint = self.broker.endpoint();
        let directory = self.broker.directory();
        let reopened =
            Store::open_existing(&self.dir.path().join("db"), Default::default()).unwrap();
        if kind == Kind::Events && acknowledged != 0 {
            let checkpoint = reopened
                .channel_checkpoint(&SCOPE, &endpoint.source_epoch)
                .unwrap()
                .unwrap();
            assert_eq!(checkpoint.sequence, acknowledged);
            assert_eq!(checkpoint.cursor, cursor(kind, acknowledged));
            for sequence in 1..=acknowledged {
                let receipt = reopened
                    .channel_ack_receipt(&SCOPE, &endpoint.source_epoch, sequence)
                    .unwrap()
                    .unwrap();
                let frame = Frame::decode(&receipt.frame_wire).unwrap();
                let request = Request::decode(&receipt.request_wire).unwrap();
                let response = Response::decode(&receipt.response_wire).unwrap();
                let expected = Frame {
                    sequence,
                    source_epoch: endpoint.source_epoch,
                    bytes: payload(sequence),
                    cursor: cursor(kind, sequence),
                };
                assert_eq!(frame, expected);
                assert_eq!(request.reference, endpoint.reference);
                assert_eq!(request.source_epoch, endpoint.source_epoch);
                assert_eq!(
                    request.action,
                    Action::Ack {
                        sequence,
                        frame_sha256: expected.digest().unwrap(),
                        cursor: expected.cursor
                    }
                );
                // Exact immutable example domain and Receive/ACK serial ordering.
                let mut identity = Sha256::new();
                for bytes in [
                    b"morrow.channel.directory.request.v1".as_slice(),
                    directory.scope_sha256.as_slice(),
                    endpoint.reference.as_slice(),
                    endpoint.source_epoch.as_slice(),
                    (2 * sequence).to_le_bytes().as_slice(),
                ] {
                    identity.update(bytes);
                }
                assert_eq!(request.call_id, <[u8; 32]>::from(identity.finalize()));
                response.validate_for(&request).unwrap();
                assert_eq!(response.status, Status::Acked);
                assert_eq!(response.last_acked, sequence);
                assert_eq!(receipt.checkpoint.sequence, sequence);
                assert_eq!(receipt.checkpoint.frame_sha256, frame.digest().unwrap());
                eprintln!(
                    "directory comparison durable_ack seq={sequence} frame_sha256={} request_sha256={} cursor={} original_domain=true persisted=true",
                    hex(&frame.digest().unwrap()),
                    hex(&request.digest().unwrap()),
                    hex(&frame.cursor)
                );
            }
        } else {
            assert!(
                reopened
                    .channel_checkpoint(&SCOPE, &endpoint.source_epoch)
                    .unwrap()
                    .is_none()
            );
        }
        reopened.integrity_check().unwrap();
        self.host.store_local().integrity_check().unwrap();
    }
}
fn run_case(package: &Package, control: bool, kind: Kind) {
    let mut fixture = Fixture::new(package, kind);
    let (published, ready) = mpsc::channel();
    let (stopped, source_result) = mpsc::channel();
    fixture
        .broker
        .spawn(move |producer| {
            let result = (|| {
                for sequence in 1..=FRAMES {
                    producer.push(payload(sequence), cursor(kind, sequence))?;
                    if sequence == 1 {
                        published.send(()).unwrap();
                    }
                }
                producer.wait_sent()
            })();
            stopped.send(result).unwrap();
        })
        .unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let input = fixture.invocation();
    let report = fixture.broker.run_invocation(
        &fixture.manager,
        &mut fixture.host,
        &fixture.instance,
        &input,
        || 2,
    );
    let original = format!("{report:?}");
    let source = source_result.recv_timeout(WAIT);
    if source.is_err() {
        fixture.instance.request_stop();
    }
    fixture.join();
    let snapshot = fixture.broker.snapshot();
    eprintln!(
        "directory comparison receipt control={control} kind={kind:?} frames={FRAMES} frame_bytes={FRAME_BYTES} outcome={:?} host_calls={} fuel_remaining={} fuel_used={} output={} expected_payload_sha256={} source={source:?} terminal={:?} last_acked={} usage={:?} producer={:?} cleanup={:?} reclaimed={}",
        report.execution.outcome,
        report.execution.host_calls,
        report.execution.fuel_remaining,
        20_000_000 - report.execution.fuel_remaining,
        report
            .output
            .as_ref()
            .map_or_else(|| "none".into(), |output| hex(&output.bytes)),
        hex(&payload_digest()),
        snapshot.terminal_cause,
        snapshot.last_acked,
        snapshot.usage,
        snapshot.producer_outcome,
        snapshot.cleanup_proof,
        snapshot.resource_reclaimed
    );
    assert_eq!(format!("{report:?}"), original);
    assert!(source.is_ok());
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert!(snapshot.resource_reclaimed);
    assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("never replay source"))
            .is_err()
    );
    fixture.verify_acks(kind, snapshot.last_acked);
    assert_eq!(
        report.execution.outcome,
        Ok(0),
        "fresh operation-equivalent guest qualification outcome"
    );
    assert_eq!(report.execution.host_calls, 11);
    assert!(report.execution.fuel_remaining > 0);
    assert!(report.failure.is_none());
    assert!(report.response.is_none());
    assert_eq!(source.unwrap(), Err(Error::Closed));
    assert_eq!(snapshot.terminal_cause, Some(Status::Closed));
    assert_eq!(snapshot.last_acked, FRAMES);
    assert_eq!(snapshot.usage.messages, FRAMES);
    assert_eq!(snapshot.usage.bytes, FRAMES * FRAME_BYTES as u64);
    assert_eq!(snapshot.usage.requests, 11);
    let output = report.output.unwrap();
    assert_eq!(output.type_id, "bytes");
    let bytes = output.bytes;
    assert_eq!(bytes.len(), 64);
    assert_eq!(&bytes[..4], b"CHV1");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        if kind == Kind::Events { 2 } else { 0 }
    );
    assert!(matches!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        5 | 6
    ));
    assert!(matches!(
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        0 | 1
    ));
    assert_eq!(
        u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        FRAMES
    );
    assert_eq!(
        u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        FRAMES * FRAME_BYTES as u64
    );
    assert_eq!(&bytes[32..], &payload_digest());
}
#[test]
#[ignore = "one new003 operation-equivalent reusable guest; five original32KiB byte frames"]
fn fresh_directory_reusable_original_five_byte_frames_ack_close_and_join() {
    run_case(&load(false), false, Kind::ByteStream);
}
#[test]
#[ignore = "one new003 operation-equivalent reusable guest; five original32KiB event frames"]
fn fresh_directory_reusable_original_five_event_frames_persist_acks_close_and_join() {
    run_case(&load(false), false, Kind::Events);
}
#[test]
#[ignore = "one fresh matched baseline one-shot build; records real outcomes without old-byte claims"]
fn fresh_directory_oneshot_comparison_records_both_original_five_frame_outcomes() {
    let package = load(true);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, true, kind);
    }
}
