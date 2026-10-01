//! Actual compiled SDK guests, real native source threads, Manager and SQLite.
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
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.channel.sdk-native";
const WAIT: Duration = Duration::from_secs(10);
const FRAMES: u64 = 5;
const FRAME_BYTES: usize = 32_768;

fn guests() -> Vec<(&'static str, Vec<u8>, Package)> {
    [
        ("Rust", "MORROW_RUST_CHANNEL_WASM", "MORROW_RUST_CHANNEL_PACKAGE"),
        ("C", "MORROW_SDK_CHANNEL_GUEST_C", "MORROW_SDK_CHANNEL_PACKAGE_C"),
        ("C++", "MORROW_SDK_CHANNEL_GUEST_CPP", "MORROW_SDK_CHANNEL_PACKAGE_CPP"),
    ]
    .into_iter()
    .map(|(language, key, package_key)| {
        let path = std::env::var_os(key)
            .unwrap_or_else(|| panic!("compiled {language} channel guest is mandatory: {key}"));
        let bytes = std::fs::read(PathBuf::from(path)).unwrap();
        let package_path = std::env::var_os(package_key).unwrap_or_else(|| panic!(
            "original compiled and packed {language} .mplugin is mandatory: {package_key}"));
        let raw_archive = std::fs::read(PathBuf::from(package_path)).unwrap();
        let package = Package::decode(&raw_archive).expect("original tool-packed channel archive admitted");
        assert_eq!(package.module(), bytes, "{language}: archive module differs from independently pinned Wasm");
        assert_eq!(package.archive(), raw_archive, "{language}: preserve original archive bytes");
        assert_eq!(package.manifest().package_id, ID);
        assert!(package.channel_declaration().is_some());
        eprintln!(
            "channel SDK original package {language}: wasm_sha256={:x} archive_sha256={:x} original_module_equal=true original_archive_equal=true",
            Sha256::digest(&bytes), Sha256::digest(&raw_archive)
        );
        (language, bytes, package)
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
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
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
#[ignore = "requires all three compiled channel SDK guests; root runs under a bounded Windows Job"]
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
#[ignore = "requires all three compiled channel SDK guests; root runs under a bounded Windows Job"]
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
#[ignore = "requires all three compiled channel SDK guests; root runs under a bounded Windows Job"]
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
