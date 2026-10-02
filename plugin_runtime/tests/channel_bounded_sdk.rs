//! Fresh reusable Rust WasmClient qualification through the production broker.
//! Generic in-memory sources; no protected Linux/Windows product qualification.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner,
    channel::{ChannelBroker, CleanupProof, Error, ProducerOutcome, Source},
    manager::{ManagedInstance, Manager},
    package::TaskReport,
};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(5);
const SCOPE: [u8; 32] = [0x58; 32];
const REUSABLE_ID: &str = "org.example.channel.bounded-sdk-reusable-001";
const ONESHOT_ID: &str = "org.example.channel.bounded-sdk-oneshot-001";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn load(control: bool, standard: bool) -> Package {
    let prefix = match (control, standard) {
        (false, false) => "MORROW_BOUNDED_REUSABLE",
        (true, false) => "MORROW_BOUNDED_ONESHOT",
        (false, true) => "MORROW_BOUNDED_STANDARD_REUSABLE",
        (true, true) => "MORROW_BOUNDED_STANDARD_ONESHOT",
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
        let expected =
            std::env::var(&key).unwrap_or_else(|_| panic!("independent pin mandatory: {key}"));
        assert_eq!(hex(&Sha256::digest(bytes)), expected, "{key}");
    }
    let package = Package::decode(&archive).unwrap();
    assert_eq!(package.module(), wasm);
    assert_eq!(package.archive(), archive);
    let (id, version) = match (control, standard) {
        (false, false) => (REUSABLE_ID, "0.1.0-test58.1"),
        (true, false) => (ONESHOT_ID, "0.1.0-test58.2"),
        (false, true) => (
            "org.example.channel.standard-sdk-reusable-002",
            "0.1.0-test58.3",
        ),
        (true, true) => (
            "org.example.channel.standard-sdk-oneshot-002",
            "0.1.0-test58.4",
        ),
    };
    assert_eq!(package.manifest().package_id, id);
    assert_eq!(package.manifest().package_version, version);
    assert_eq!(package.capabilities().len(), 0);
    let declared = package.manifest().budget.as_ref().unwrap();
    assert_eq!(declared.fuel, 20_000_000);
    assert_eq!(declared.memory_bytes, 16 * 1024 * 1024);
    assert_eq!(declared.host_calls, 16);
    // The native callback shim cannot accidentally stand in for this Wasm guest.
    assert!(matches!(
        Runner::new_task(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let runner = Runner::new_channel_task(&wasm, Limits::default()).unwrap();
    let unbound = runner.run_task(
        &[1],
        &mut |_| panic!("unbound callback"),
        Cancellation::default(),
    );
    assert_eq!(unbound.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(unbound.report.host_calls, 0);
    eprintln!(
        "bounded package control={control} id={} wasm_sha256={} package_sha256={} limits=20000000/16777216/16",
        package.manifest().package_id,
        hex(&Sha256::digest(&wasm)),
        hex(&Sha256::digest(&archive))
    );
    package
}
fn payload(sequence: u64, size: usize) -> Vec<u8> {
    (0..size)
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
fn transcript(frames: u64, size: usize) -> [u8; 32] {
    let mut digest = Sha256::new();
    for sequence in 1..=frames {
        digest.update(payload(sequence, size));
    }
    digest.finalize().into()
}
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
}
impl Fixture {
    fn new(package: &Package, kind: Kind, frames: u64) -> Self {
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
        let actual = instance.package().limits();
        assert_eq!(actual.fuel, 20_000_000);
        assert_eq!(actual.memory_bytes, 16 * 1024 * 1024);
        assert_eq!(actual.host_calls, 16);
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
                    max_frame_bytes: 65536,
                    max_bytes: frames * 65536,
                    max_messages: frames,
                    max_requests: 3 * frames + 1,
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
            "channel-bounded-sdk-001",
            Transform {
                handler: "channel.bounded.consume".into(),
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
                "original source actually Joined proof missing"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn durable_acks(&self, kind: Kind, frames: u64, size: usize) {
        let endpoint = self.broker.endpoint();
        let reopened =
            Store::open_existing(&self.dir.path().join("db"), Default::default()).unwrap();
        if kind == Kind::Events {
            let checkpoint = reopened
                .channel_checkpoint(&SCOPE, &endpoint.source_epoch)
                .unwrap()
                .unwrap();
            assert_eq!(checkpoint.sequence, frames);
            assert_eq!(checkpoint.cursor, cursor(kind, frames));
            for sequence in 1..=frames {
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
                    bytes: payload(sequence, size),
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
                response.validate_for(&request).unwrap();
                assert_eq!(response.status, Status::Acked);
                assert_eq!(response.last_acked, sequence);
                assert_eq!(receipt.checkpoint.sequence, sequence);
                assert_eq!(receipt.checkpoint.frame_sha256, frame.digest().unwrap());
                eprintln!(
                    "bounded durable_ack seq={sequence} frame_sha256={} request_sha256={} cursor={} persisted=true",
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
fn validate_output(report: &TaskReport, kind: Kind, frames: u64, size: usize) {
    assert_eq!(
        report.execution.outcome,
        Ok(0),
        "fresh guest real Wasmi budget outcome"
    );
    assert_eq!(report.execution.host_calls, (3 * frames + 1) as u32);
    assert!(report.execution.fuel_remaining > 0);
    assert!(report.execution.fuel_remaining < 20_000_000);
    assert!(report.failure.is_none());
    assert!(report.response.is_none());
    let output = report.output.as_ref().unwrap();
    assert_eq!(output.type_id, "bytes");
    let bytes = &output.bytes;
    assert_eq!(bytes.len(), 64);
    assert_eq!(&bytes[..4], b"BSDK");
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
        2 | 3
    ));
    assert_eq!(
        u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        frames
    );
    assert_eq!(
        u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        frames * size as u64
    );
    assert_eq!(&bytes[32..], &transcript(frames, size));
}
fn run_case(
    package: &Package,
    control: bool,
    kind: Kind,
    frames: u64,
    size: usize,
    expect_fuel_fault: bool,
) {
    let mut fixture = Fixture::new(package, kind, frames);
    let (published, ready) = mpsc::channel();
    let (stopped, source_result) = mpsc::channel();
    fixture
        .broker
        .spawn(move |producer| {
            let result = (|| {
                for sequence in 1..=frames {
                    producer.push(payload(sequence, size), cursor(kind, sequence))?;
                    if sequence == 1 {
                        published.send(()).unwrap();
                    }
                }
                // Remain alive until the guest's explicit Close or original Control
                // revocation. Never fake EOF or actual join inside a Wasm callback.
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
        "bounded receipt control={control} kind={kind:?} frames={frames} frame_bytes={size} outcome={:?} host_calls={} fuel_remaining={} fuel_used={} output={} expected_payload_sha256={} source={source:?} terminal={:?} last_acked={} usage={:?} producer={:?} cleanup={:?} reclaimed={}",
        report.execution.outcome,
        report.execution.host_calls,
        report.execution.fuel_remaining,
        20_000_000 - report.execution.fuel_remaining,
        report
            .output
            .as_ref()
            .map_or_else(|| "none".into(), |output| hex(&output.bytes)),
        hex(&transcript(frames, size)),
        snapshot.terminal_cause,
        snapshot.last_acked,
        snapshot.usage,
        snapshot.producer_outcome,
        snapshot.cleanup_proof,
        snapshot.resource_reclaimed
    );
    assert_eq!(
        format!("{report:?}"),
        original,
        "cleanup cannot rewrite original report"
    );
    assert!(source.is_ok(), "original source must stop within its bound");
    assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
    assert_eq!(
        snapshot.producer_outcome,
        ProducerOutcome::Unknown,
        "explicit Close of a live source is not invented EOF"
    );
    assert!(snapshot.resource_reclaimed);
    assert!(
        fixture
            .broker
            .spawn(|_| panic!("never replay original source"))
            .is_err()
    );
    // ACKs may already be durable even when fuel prevents Close/completion.
    // Prove the original acknowledged prefix without turning it into success.
    if snapshot.last_acked != 0 {
        fixture.durable_acks(kind, snapshot.last_acked, size);
    }
    if expect_fuel_fault {
        assert_eq!(report.execution.outcome, Err(Fault::Limits));
        assert_eq!(report.execution.host_calls, 8);
        assert!(report.execution.fuel_remaining < 10000);
        assert!(report.output.is_none());
        assert_eq!(snapshot.last_acked, 2);
        assert_eq!(source.unwrap(), Err(Error::Denied));
        assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    } else if !control || report.execution.outcome == Ok(0) {
        validate_output(&report, kind, frames, size);
        assert_eq!(source.unwrap(), Err(Error::Closed));
        assert_eq!(snapshot.terminal_cause, Some(Status::Closed));
        assert_eq!(snapshot.last_acked, frames);
        assert_eq!(snapshot.usage.messages, frames);
        assert_eq!(snapshot.usage.bytes, frames * size as u64);
        assert_eq!(snapshot.usage.requests, 3 * frames + 1);
    } else {
        assert!(
            report.execution.outcome.is_err(),
            "control nonzero guest return is not a budget-failure receipt"
        );
        assert!(report.execution.host_calls <= 16);
        assert!(report.output.is_none());
        assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
    }
}
#[test]
#[ignore = "requires new source-pinned opt3 Rust WasmClient guest and tool-packed archive"]
fn reusable_small_frames_bytes_and_events_preserve_owned_responses_and_join() {
    let package = load(false, false);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, false, kind, 4, 31, false);
    }
}
#[test]
#[ignore = "requires new source-pinned opt3 Rust WasmClient guest and tool-packed archive"]
fn reusable_full_64kib_boundary_bytes_and_events_ack_original_frame_and_join() {
    let package = load(false, false);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, false, kind, 1, 65536, false);
    }
}
#[test]
#[ignore = "requires new source-pinned opt3 Rust WasmClient guest and tool-packed archive"]
fn reusable_repeated_full_64kib_frames_bytes_and_events_keep_default_limits() {
    let package = load(false, false);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, false, kind, 2, 65536, false);
    }
}
#[test]
#[ignore = "explicit real20M fuel-boundary negative control; four64KiB frames are not business success"]
fn reusable_four_full_frames_preserve_original_limits_fault_revoke_and_actual_join() {
    let package = load(false, false);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, false, kind, 4, 65536, true);
    }
}
#[test]
#[ignore = "fresh matched one-shot measurement only; never substitutes for reusable qualification"]
fn matched_fresh_oneshot_control_records_real_outcomes_without_claiming_old_case_failed() {
    let package = load(true, false);
    for (frames, size) in [(4, 31), (1, 65536), (2, 65536), (4, 65536)] {
        for kind in [Kind::ByteStream, Kind::Events] {
            run_case(&package, true, kind, frames, size, false);
        }
    }
}

#[test]
#[ignore = "new002 standard reusable guest; original5x32768 source, semantics and limits"]
fn standard_five_original_32kib_byte_frames_use_all_sixteen_calls_and_join() {
    let package = load(false, true);
    run_case(&package, false, Kind::ByteStream, 5, 32768, false);
}
#[test]
#[ignore = "new002 standard reusable guest; original5x32768 source, semantics and limits"]
fn standard_five_original_32kib_event_frames_persist_exact_acks_and_join() {
    let package = load(false, true);
    run_case(&package, false, Kind::Events, 5, 32768, false);
}
#[test]
#[ignore = "new002 matched one-shot standard measurement; record authentic pass or fault"]
fn matched_fresh_standard_oneshot_control_records_original_five_frame_outcomes() {
    let package = load(true, true);
    for kind in [Kind::ByteStream, Kind::Events] {
        run_case(&package, true, kind, 5, 32768, false);
    }
}
