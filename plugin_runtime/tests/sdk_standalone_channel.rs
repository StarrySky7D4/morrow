//! Original standalone channel archives, real native source threads, Manager and SQLite.
//! No mocked channel callbacks. Local memory qualifies neither HTTP nor cloud.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    channel::{Budget, Kind, Status},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner,
    channel::{ChannelBroker, CleanupProof, Source},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(10);
const FRAMES: u64 = 5;
const FRAME_BYTES: usize = 32_768;

fn guests() -> Vec<(&'static str, Vec<u8>, Package)> {
    [("Rust", "RUST"), ("C", "C"), ("C++", "CPP")]
        .into_iter()
        .map(|(language, suffix)| {
            let prefix = format!("MORROW_STANDALONE_CHANNEL_{suffix}");
            let input = |name| std::env::var(format!("{prefix}_{name}")).unwrap();
            let archive = std::fs::read(input("PACKAGE")).unwrap();
            let wasm = std::fs::read(input("WASM")).unwrap();
            for (name, bytes) in [("PACKAGE_SHA256", &archive), ("WASM_SHA256", &wasm)] {
                let sha = sha2::Sha256::digest(bytes);
                assert_eq!(format!("{sha:x}"), input(name), "independent {language} {name}");
            }
            let package = Package::decode(&archive).expect("original standalone archive");
            assert_eq!(package.archive(), archive);
            assert_eq!(package.module(), wasm);
            assert_eq!(package.manifest().package_id, input("ID"));
            assert_eq!(package.manifest().package_version, "0.1.0");
            assert!(package.capabilities().is_empty());
            let budget = package.manifest().budget.as_ref().unwrap();
            assert_eq!(budget.fuel, 20_000_000);
            assert_eq!(budget.memory_bytes, 16 * 1024 * 1024);
            assert_eq!(budget.host_calls, 16);
            assert!(package.channel_declaration().is_some());
            eprintln!("standalone_original language={language} id={} archive_sha256={} wasm_sha256={} archive_unchanged=true", package.manifest().package_id, input("PACKAGE_SHA256"), input("WASM_SHA256"));
            (language, wasm, package)
        })
        .collect()
}
fn payload(sequence: u64) -> Vec<u8> {
    (0..FRAME_BYTES)
        .map(|i| ((i + sequence as usize * 17) % 251) as u8)
        .collect()
}
fn digest() -> [u8; 32] {
    let mut hash = Sha256::new();
    for sequence in 1..=FRAMES {
        hash.update(payload(sequence));
    }
    hash.finalize().into()
}
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    broker: ChannelBroker,
}
impl Fixture {
    fn new(package: &Package, kind: Kind, duplex: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Execute the original tool-packed manifest and module; no qualification-only rebuild.
        let declared = package.channel_declaration().unwrap();
        let budget = Budget::from_proto(declared.budget.as_ref().unwrap()).unwrap();
        assert_eq!(budget.max_frame_bytes as usize, FRAME_BYTES);
        let execution = package.manifest().budget.as_ref().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let limits = Limits {
            host_calls: execution.host_calls,
            fuel: execution.fuel,
            memory_bytes: usize::try_from(execution.memory_bytes).unwrap(),
        };
        let mut manager = Manager::new(registry, limits);
        manager.select(package, manager.revision()).unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
        let broker = manager
            .bind_channel(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                Source {
                    kind,
                    duplex,
                    checkpoint_scope: (kind == Kind::Events).then_some([0x74; 32]),
                },
                budget,
                1 + budget.max_duration_ms,
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
    fn input(&self, mode: u8) -> Invocation {
        let endpoint = self.broker.endpoint();
        let mut bytes = vec![mode];
        bytes.extend_from_slice(&endpoint.reference);
        bytes.extend_from_slice(&endpoint.source_epoch);
        assert_eq!(bytes.len(), 65);
        Invocation::new_transform(
            "channel-sdk-native",
            Transform {
                handler: "channel.exercise".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input: bytes,
            },
        )
        .unwrap()
    }
    fn summary(&mut self, input: &Invocation) -> Vec<u8> {
        let result =
            self.broker
                .run_invocation(&self.manager, &mut self.host, &self.instance, input, || 2);
        assert_eq!(
            result.execution.outcome,
            Ok(0),
            "real compiled guest report"
        );
        assert!((1..=256).contains(&result.execution.host_calls));
        assert!(result.failure.is_none());
        assert!(
            result.response.is_none(),
            "channel output is not a core receipt"
        );
        let output = result
            .output
            .expect("old Task ABI completion was correlated by the dedicated managed route");
        assert_eq!(output.type_id, "bytes");
        assert_eq!(output.bytes.len(), 64);
        output.bytes
    }
    fn join(&self) -> Status {
        let deadline = Instant::now() + WAIT;
        loop {
            let status = self.broker.reap();
            if self.broker.snapshot().resource_reclaimed {
                assert_eq!(self.broker.snapshot().cleanup_proof, CleanupProof::Joined);
                return status;
            }
            assert!(Instant::now() < deadline, "native source did not join");
            thread::sleep(Duration::from_millis(1));
        }
    }
}
fn check_summary(language: &str, mode: u8, summary: &[u8]) {
    assert_eq!(summary.len(), 64, "{language}");
    assert_eq!(&summary[..4], b"CHV1", "{language}");
    assert_eq!(
        u32::from_le_bytes(summary[4..8].try_into().unwrap()),
        u32::from(mode)
    );
    let status = u32::from_le_bytes(summary[8..12].try_into().unwrap());
    assert!(
        matches!(status, 5 | 6),
        "{language}: bounded success must report Closed or actual ClosingUnconfirmed, got {status}"
    );
    let count = u64::from_le_bytes(summary[16..24].try_into().unwrap());
    let bytes = u64::from_le_bytes(summary[24..32].try_into().unwrap());
    assert_eq!(
        count, FRAMES,
        "{language}: natural native scheduling must carry all frames"
    );
    assert_eq!(bytes, FRAMES * FRAME_BYTES as u64, "{language}");
    assert!(bytes > morrow_core::task::MAX_VALUE_BYTES as u64);
    assert_eq!(
        &summary[32..],
        &digest(),
        "{language}: digest covers complete native payload"
    );
}

#[test]
#[ignore = "requires three independently pinned original standalone channel archives"]
fn compiled_consumers_and_event_subscriptions_digest_more_than_the_old_task_value_limit() {
    for (language, _wasm, package) in guests() {
        for (mode, kind) in [(0, Kind::ByteStream), (2, Kind::Events)] {
            let mut f = Fixture::new(&package, kind, false);
            let (published, observed) = mpsc::channel();
            f.broker
                .spawn(move |producer| {
                    for sequence in 1..=FRAMES {
                        let cursor = if kind == Kind::Events {
                            sequence.to_le_bytes().to_vec()
                        } else {
                            vec![]
                        };
                        producer.push(payload(sequence), cursor).unwrap();
                        if sequence == 1 {
                            published.send(()).unwrap();
                        }
                    }
                    producer.finish().unwrap();
                })
                .unwrap();
            observed.recv_timeout(WAIT).unwrap();
            let input = f.input(mode);
            let summary = f.summary(&input);
            let joined = f.join();
            check_summary(language, mode, &summary);
            assert_eq!(joined, Status::Closed);
            assert_eq!(f.broker.snapshot().last_acked, FRAMES);
            assert_eq!(f.broker.snapshot().usage.bytes, FRAMES * FRAME_BYTES as u64);
            if kind == Kind::Events {
                let endpoint = f.broker.endpoint();
                let checkpoint = f
                    .host
                    .store_local()
                    .channel_checkpoint(&[0x74; 32], &endpoint.source_epoch)
                    .unwrap()
                    .unwrap();
                assert_eq!(checkpoint.sequence, FRAMES);
                assert_eq!(checkpoint.cursor, FRAMES.to_le_bytes());
                let reopened = Store::open(&f.dir.path().join("db"), Default::default()).unwrap();
                assert_eq!(
                    reopened
                        .channel_checkpoint(&[0x74; 32], &endpoint.source_epoch)
                        .unwrap(),
                    Some(checkpoint)
                );
                reopened.integrity_check().unwrap();
            }
            f.host.store_local().integrity_check().unwrap();
        }
    }
}

#[test]
#[ignore = "requires three independently pinned original standalone channel archives"]
fn compiled_producers_require_actual_peer_observation_beyond_send_admission() {
    for (language, _wasm, package) in guests() {
        let mut f = Fixture::new(&package, Kind::ByteStream, true);
        let (started, observed) = mpsc::channel();
        let (receipt, delivered) = mpsc::channel();
        f.broker
            .spawn(move |producer| {
                started.send(()).unwrap();
                let deadline = Instant::now() + WAIT;
                let mut count = 0u64;
                let mut bytes = Vec::new();
                loop {
                    match producer.receive_sent() {
                        Ok(Some((sequence, frame))) => {
                            assert_eq!(sequence, count + 1);
                            assert_eq!(frame, payload(sequence));
                            count += 1;
                            bytes.extend(frame);
                            if count == FRAMES {
                                break;
                            }
                        }
                        Ok(None) => {
                            if Instant::now() >= deadline {
                                break;
                            }
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => break,
                    }
                }
                receipt.send((count, bytes)).unwrap();
                let _ = producer.finish();
            })
            .unwrap();
        observed.recv_timeout(WAIT).unwrap();
        let input = f.input(1);
        let summary = f.summary(&input);
        let joined = f.join();
        let (count, peer_bytes) = delivered.recv_timeout(WAIT).unwrap();
        check_summary(language, 1, &summary);
        assert_eq!(joined, Status::Closed);
        assert_eq!(
            count, FRAMES,
            "{language}: accepted send is not peer receipt"
        );
        assert_eq!(peer_bytes.len(), FRAMES as usize * FRAME_BYTES);
        assert_eq!(<[u8; 32]>::from(Sha256::digest(peer_bytes)), digest());
        f.host.store_local().integrity_check().unwrap();
    }
}

#[test]
#[ignore = "requires three independently pinned original standalone channel archives"]
fn compiled_guests_keep_old_runner_rejection_and_unknown_without_source_replay() {
    for (language, wasm, package) in guests() {
        assert!(
            matches!(
                Runner::new_task(&wasm, Limits::default()),
                Err(Fault::UnsupportedAbi)
            ),
            "{language}"
        );
        let runner = Runner::new_channel_task(&wasm, Limits::default()).unwrap();
        let unbound = runner.run_task(
            &[1],
            &mut |_| panic!("unbound channel callback ran"),
            Cancellation::default(),
        );
        assert_eq!(unbound.report.outcome, Err(Fault::UnsupportedAbi));
        assert_eq!(unbound.report.host_calls, 0);
        let mut f = Fixture::new(&package, Kind::ByteStream, false);
        f.broker.spawn(|producer| drop(producer)).unwrap();
        assert_eq!(f.join(), Status::Unknown);
        let input = f.input(0);
        let summary = f.summary(&input);
        assert_eq!(&summary[..4], b"CHV1");
        assert_eq!(u32::from_le_bytes(summary[8..12].try_into().unwrap()), 11);
        assert_eq!(u32::from_le_bytes(summary[12..16].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(summary[16..24].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(summary[24..32].try_into().unwrap()), 0);
        assert!(
            f.broker
                .spawn(|_| panic!("unknown source replayed"))
                .is_err()
        );
    }
}
