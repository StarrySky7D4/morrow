#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_blob_transfer_v1::{
    ActionRef, FrameRef, Identity, Limits, MAX_CHUNK_BYTES, Phase, digest,
};
use morrow_core::store::Store;
use morrow_plugin_runtime::blob_upload::{Error, FrameSource, Spec, stage_local};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn spec(bytes: &[u8]) -> Spec {
    Spec {
        owner_domain: [4; 32],
        identity: Identity {
            transfer_epoch: [1; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: bytes.len() as u64,
        sha256: digest(bytes),
        limits: Limits {
            max_payload_bytes: 1024 * 1024,
            max_wire_bytes: 2 * 1024 * 1024,
            max_request_bytes: 1024 * 1024,
            max_response_bytes: 1024 * 1024,
            max_requests: 64,
            max_receipts: 64,
        },
    }
}
fn wire(s: &Spec, action: ActionRef<'_>) -> Vec<u8> {
    FrameRef {
        identity: s.identity,
        action,
    }
    .encode()
    .unwrap()
}
fn frames(s: &Spec, bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![wire(
        s,
        ActionRef::Descriptor {
            total_length: s.byte_length,
            whole_sha256: s.sha256,
        },
    )];
    for (i, part) in bytes.chunks(MAX_CHUNK_BYTES).enumerate() {
        out.push(wire(
            s,
            ActionRef::Chunk {
                sequence: i as u64 + 1,
                offset: (i * MAX_CHUNK_BYTES) as u64,
                payload: part,
                chunk_sha256: digest(part),
            },
        ));
    }
    out.push(wire(
        s,
        ActionRef::End {
            total_length: s.byte_length,
            whole_sha256: s.sha256,
        },
    ));
    out
}
struct Frames {
    frames: VecDeque<Vec<u8>>,
    reads: Arc<AtomicUsize>,
}
impl Frames {
    fn new(frames: Vec<Vec<u8>>) -> Self {
        Self {
            frames: frames.into(),
            reads: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl FrameSource for Frames {
    fn read_frame(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let Some(next) = self.frames.pop_front() else {
            return Ok(0);
        };
        out[..next.len()].copy_from_slice(&next);
        Ok(next.len())
    }
}
struct Never;
impl FrameSource for Never {
    fn read_frame(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("replacement source must not run")
    }
}
fn fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
    (dir, store)
}

#[test]
fn streaming_capnp_chunks_and_live_duplicates_commit_exact_bytes_once() {
    let (_dir, mut store) = fixture();
    let bytes: Vec<u8> = (0..200_017).map(|i| (i % 253) as u8).collect();
    let s = spec(&bytes);
    let mut input = frames(&s, &bytes);
    input.insert(2, input[1].clone());
    let count = input.len();
    let mut source = Frames::new(input);
    let done = stage_local(&mut store, &mut source, &s, 10, || true).unwrap();
    assert!(!done.existing);
    assert_eq!(done.received.phase, Phase::VerifiedBytes);
    assert_eq!(done.received.received_bytes, bytes.len() as u64);
    assert_eq!(
        done.received.receipts,
        bytes.len().div_ceil(MAX_CHUNK_BYTES) as u64 + 1
    );
    assert_eq!(source.reads.load(Ordering::SeqCst), count + 1);
    let mut actual = Vec::new();
    store.export_blob_local(&done.blob.id, &mut actual).unwrap();
    assert_eq!(actual, bytes);
    assert_eq!(
        store
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .unwrap()
            .blob(),
        &done.blob
    );
    store.integrity_check().unwrap();
}
#[test]
fn empty_object_still_requires_descriptor_end_and_real_eof() {
    let (_dir, mut store) = fixture();
    let s = spec(b"");
    for bad in [vec![], vec![frames(&s, b"")[0].clone()]] {
        assert!(stage_local(&mut store, &mut Frames::new(bad), &s, 1, || true).is_err());
        assert!(
            store
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
    }
    let done = stage_local(&mut store, &mut Frames::new(frames(&s, b"")), &s, 2, || {
        true
    })
    .unwrap();
    assert_eq!(done.blob.byte_length, 0);
    assert_eq!(done.received.phase, Phase::VerifiedBytes);
}
#[test]
fn premature_end_trailing_frame_foreign_identity_and_old_chunk_never_commit() {
    let bytes = vec![5; MAX_CHUNK_BYTES + 9];
    let s = spec(&bytes);
    let good = frames(&s, &bytes);
    let mut cases = vec![
        good[..good.len() - 1].to_vec(),
        good[1..].to_vec(),
        vec![good[0].clone(), good[3].clone()],
    ];
    let mut trailing = good.clone();
    trailing.push(good[0].clone());
    cases.push(trailing);
    let mut old = good.clone();
    old.insert(3, good[1].clone());
    cases.push(old);
    let mut foreign = s.clone();
    foreign.identity.object_ref = [7; 32];
    cases.push(frames(&foreign, &bytes));
    let mut wrong = s.clone();
    wrong.sha256 = digest(b"different");
    cases.push(frames(&wrong, &bytes));
    let mut gap = good.clone();
    gap.swap(1, 2);
    cases.push(gap);
    cases.push(vec![vec![0; 8]]);
    for input in cases {
        let (_dir, mut store) = fixture();
        assert!(stage_local(&mut store, &mut Frames::new(input), &s, 10, || true).is_err());
        assert!(
            store
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
        assert!(store.list_blobs_local("", 128).unwrap().is_empty());
        store.integrity_check().unwrap();
    }
}
#[test]
fn source_failure_and_oversized_return_roll_back_the_original_transaction() {
    struct Broken {
        too_large: bool,
    }
    impl FrameSource for Broken {
        fn read_frame(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.too_large {
                Ok(out.len() + 1)
            } else {
                Err(io::Error::other("private detail must not escape"))
            }
        }
    }
    for too_large in [false, true] {
        let (_dir, mut store) = fixture();
        let s = spec(b"x");
        let error = stage_local(&mut store, &mut Broken { too_large }, &s, 1, || true).unwrap_err();
        assert!(!error.to_string().contains("private detail"));
        assert!(
            store
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
        assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    }
}
#[test]
fn source_is_not_consumed_before_veto_and_mid_source_veto_rolls_back() {
    let (_dir, mut store) = fixture();
    let s = spec(b"secret");
    assert_eq!(
        stage_local(&mut store, &mut Never, &s, 1, || false).unwrap_err(),
        Error::Vetoed
    );
    let mut source = Frames::new(frames(&s, b"secret"));
    let reads = source.reads.clone();
    assert_eq!(
        stage_local(&mut store, &mut source, &s, 1, || reads
            .load(Ordering::SeqCst)
            < 2)
        .unwrap_err(),
        Error::Vetoed
    );
    assert!(
        store
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .is_none()
    );
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
}
#[test]
fn every_veto_boundary_distinguishes_rollback_from_original_committed_receipt() {
    let mut saw_delivery_unknown = false;
    let mut reached_success = false;
    // Inject once at every encountered check boundary without depending on the
    // number or order of internal read/commit checks. Each uses a fresh Store.
    for veto in 1..=64 {
        let (_dir, mut store) = fixture();
        let s = spec(b"x");
        let mut checks = 0;
        let result = stage_local(
            &mut store,
            &mut Frames::new(frames(&s, b"x")),
            &s,
            7,
            || {
                checks += 1;
                checks != veto
            },
        );
        let original = store.inspect_blob_stage_local(&s.command()).unwrap();
        match result {
            Err(Error::Vetoed) => {
                assert!(original.is_none());
                assert!(store.list_blobs_local("", 128).unwrap().is_empty());
            }
            Err(Error::DeliveryUnknown) => {
                saw_delivery_unknown = true;
                assert_eq!(original.unwrap().staged_at_unix_ms(), 7);
                assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
            }
            Ok(staged) => {
                assert!(checks < veto);
                assert_eq!(original.unwrap().blob(), &staged.blob);
                reached_success = true;
                break;
            }
            Err(error) => panic!("unexpected result: {error:?}"),
        }
        store.integrity_check().unwrap();
    }
    assert!(saw_delivery_unknown && reached_success);
}
#[test]
fn original_receipt_reopens_without_source_replay_and_release_is_permanent() {
    let (dir, mut store) = fixture();
    let s = spec(b"payload");
    let first = stage_local(
        &mut store,
        &mut Frames::new(frames(&s, b"payload")),
        &s,
        2,
        || true,
    )
    .unwrap();
    drop(store);
    let mut store = Store::open_existing(&dir.path().join("db"), Default::default()).unwrap();
    let old = stage_local(&mut store, &mut Never, &s, -1, || true).unwrap();
    assert!(old.existing);
    assert_eq!(old.blob, first.blob);
    assert_eq!(old.received.requests, 0);
    store.release_blob_stage_local(&s.command(), 3).unwrap();
    assert_eq!(
        stage_local(&mut store, &mut Never, &s, 4, || true).unwrap_err(),
        Error::Storage(morrow_core::Error::OperationConflict)
    );
}
#[test]
fn wire_budget_exhaustion_does_not_publish_a_partial_blob() {
    let (_dir, mut store) = fixture();
    let mut s = spec(b"bytes");
    s.limits.max_requests = 1;
    assert_eq!(
        stage_local(
            &mut store,
            &mut Frames::new(frames(&s, b"bytes")),
            &s,
            1,
            || true
        )
        .unwrap_err(),
        Error::Protocol(morrow_blob_transfer_v1::Error::Limit)
    );
    assert!(
        store
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .is_none()
    );
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
}
#[test]
fn metadata_mismatch_precedes_replacement_read_and_debug_redacts_identities() {
    let (_dir, mut store) = fixture();
    let s = spec(b"a");
    stage_local(
        &mut store,
        &mut Frames::new(frames(&s, b"a")),
        &s,
        1,
        || true,
    )
    .unwrap();
    let mut changed = s.clone();
    changed.sha256 = digest(b"b");
    assert_eq!(
        stage_local(&mut store, &mut Never, &changed, 2, || true).unwrap_err(),
        Error::Storage(morrow_core::Error::OperationConflict)
    );
    assert!(!format!("{s:?}").contains("owner_domain"));
    let old = s.command();
    let mut changed = s.clone();
    changed.identity.transfer_epoch = [8; 32];
    assert_ne!(old, changed.command());
}

mod original_worker {
    use super::*;
    use morrow_core::{
        dispatch::HostRuntime,
        plugin_package::{
            Package,
            catalog::Catalog,
            io::{self as declared_io, IoCapability},
            registry::Registry,
        },
    };
    use morrow_plugin_runtime::{
        Limits as WasmLimits,
        io_jobs::{
            BlobCommandHandle, BlobCommandKind, BlobResponse, BlobUploadOwner, HostOwner, IoWorker,
            JobLimits, OwnerCommandError, OwnerCommandPoll, WorkerExit,
        },
        manager::Manager,
    };
    use std::{
        collections::BTreeSet,
        sync::{
            Condvar, Mutex,
            atomic::{AtomicBool, AtomicU64},
            mpsc,
        },
        thread,
        time::{Duration, Instant},
    };
    const ID: &str = "org.example.blob.owner.local";
    const WAIT: Duration = Duration::from_secs(10);
    struct Owner {
        host: HostRuntime,
        identity: Arc<()>,
        allowed: Arc<AtomicBool>,
        threads: Arc<Mutex<Vec<thread::ThreadId>>>,
    }
    impl HostOwner for Owner {
        fn runtime(&self) -> &HostRuntime {
            &self.host
        }
        fn runtime_mut(&mut self) -> &mut HostRuntime {
            &mut self.host
        }
    }
    impl BlobUploadOwner for Owner {
        fn validate_blob_command(&mut self, _: BlobCommandKind, spec: &Spec) -> Result<(), Error> {
            self.threads.lock().unwrap().push(thread::current().id());
            if self.allowed.load(Ordering::SeqCst) && spec.owner_domain == [4; 32] {
                Ok(())
            } else {
                Err(Error::Vetoed)
            }
        }
    }
    struct Run {
        _dir: tempfile::TempDir,
        manager: Manager,
        worker: IoWorker<Owner>,
        digest: [u8; 32],
        clock: Arc<AtomicU64>,
        allowed: Arc<AtomicBool>,
        identity: Arc<()>,
        threads: Arc<Mutex<Vec<thread::ThreadId>>>,
    }
    fn run(limits: JobLimits) -> Run {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let capabilities = BTreeSet::from([IoCapability::FileRead]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features.push(declared_io::FEATURE.into());
        manifest.io_declaration = Some(declared_io::declaration(
            capabilities.iter().copied().collect(),
            vec!["io.invoke".into()],
        ));
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, WasmLimits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                capabilities.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &capabilities,
                100,
                1,
            )
            .unwrap();
        let identity = Arc::new(());
        let allowed = Arc::new(AtomicBool::new(true));
        let threads = Arc::new(Mutex::new(Vec::new()));
        let owner = Owner {
            host,
            identity: identity.clone(),
            allowed: allowed.clone(),
            threads: threads.clone(),
        };
        let clock = Arc::new(AtomicU64::new(2));
        let sampled = clock.clone();
        let worker = IoWorker::spawn_managed_owned(
            &manager,
            owner,
            instance,
            binding,
            move || sampled.load(Ordering::SeqCst),
            2,
            limits,
        )
        .unwrap();
        Run {
            _dir: dir,
            manager,
            worker,
            digest: package.digest(),
            clock,
            allowed,
            identity,
            threads,
        }
    }
    fn result(
        handle: &mut BlobCommandHandle,
    ) -> Result<Result<BlobResponse, Error>, OwnerCommandError> {
        let end = Instant::now() + WAIT;
        loop {
            match handle.read() {
                Ok(None) => {
                    assert!(Instant::now() < end);
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(Some(value)) => return Ok(value),
                Err(error) => return Err(error),
            }
        }
    }
    fn reclaim(run: &mut Run) -> WorkerExit<Owner> {
        run.worker.stop();
        let end = Instant::now() + WAIT;
        loop {
            if let Some(exit) = run.worker.try_reclaim().unwrap() {
                return exit;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[derive(Default)]
    struct Gate {
        opened: Mutex<bool>,
        changed: Condvar,
    }
    impl Gate {
        fn open(&self) {
            *self.opened.lock().unwrap() = true;
            self.changed.notify_all();
        }
        fn wait(&self) {
            let guard = self.opened.lock().unwrap();
            let (guard, timeout) = self
                .changed
                .wait_timeout_while(guard, WAIT, |v| !*v)
                .unwrap();
            assert!(*guard && !timeout.timed_out());
        }
    }
    struct Blocked {
        frames: Frames,
        entered: Option<mpsc::Sender<thread::ThreadId>>,
        gate: Arc<Gate>,
    }
    impl FrameSource for Blocked {
        fn read_frame(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if let Some(tx) = self.entered.take() {
                tx.send(thread::current().id()).unwrap();
                self.gate.wait();
            }
            self.frames.read_frame(out)
        }
    }
    fn blocked(s: &Spec) -> (Blocked, Arc<Gate>, mpsc::Receiver<thread::ThreadId>) {
        let (tx, rx) = mpsc::channel();
        let gate = Arc::new(Gate::default());
        (
            Blocked {
                frames: Frames::new(frames(s, b"bytes")),
                entered: Some(tx),
                gate: gate.clone(),
            },
            gate,
            rx,
        )
    }
    #[test]
    fn exact_original_worker_and_store_stage_inspect_release_and_join() {
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        let (source, gate, entered) = blocked(&s);
        let mut handle = r
            .worker
            .stage_blob_frames_local(source, s.clone(), 1)
            .unwrap();
        let worker_thread = entered.recv_timeout(WAIT).unwrap();
        assert_ne!(worker_thread, thread::current().id());
        gate.open();
        let BlobResponse::Staged(staged) = result(&mut handle).unwrap().unwrap() else {
            panic!()
        };
        assert!(handle.is_started());
        assert!(matches!(handle.read(), Err(OwnerCommandError::Consumed)));
        let mut inspect = r.worker.inspect_blob_stage_local(s.clone()).unwrap();
        let BlobResponse::Inspected(Some(record)) = result(&mut inspect).unwrap().unwrap() else {
            panic!()
        };
        assert_eq!(record.blob(), &staged.blob);
        let mut release = r.worker.release_blob_stage_local(s.clone(), 2).unwrap();
        let BlobResponse::Released(record) = result(&mut release).unwrap().unwrap() else {
            panic!()
        };
        assert!(record.is_released());
        assert!(
            r.threads
                .lock()
                .unwrap()
                .iter()
                .all(|t| *t == worker_thread)
        );
        let exit = reclaim(&mut r);
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(exit.disconnect, Ok(()));
        assert!(Arc::ptr_eq(&r.identity, &exit.owner.identity));
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .unwrap()
                .is_released()
        );
        assert_eq!(r.worker.owner_command_usage(), (0, 0));
    }
    #[test]
    fn explicit_owner_denial_prevents_source_and_store_access() {
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        r.allowed.store(false, Ordering::SeqCst);
        let mut handle = r
            .worker
            .stage_blob_frames_local(Never, s.clone(), 1)
            .unwrap();
        assert!(matches!(result(&mut handle), Ok(Err(Error::Vetoed))));
        let exit = reclaim(&mut r);
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn queued_cancel_never_consumes_source_or_releases_capacity_early() {
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        let (source, gate, entered) = blocked(&s);
        let mut first = r
            .worker
            .stage_blob_frames_local(source, s.clone(), 1)
            .unwrap();
        entered.recv_timeout(WAIT).unwrap();
        let mut second = r
            .worker
            .stage_blob_frames_local(Never, s.clone(), 2)
            .unwrap();
        second.cancel();
        assert!(matches!(
            result(&mut second),
            Err(OwnerCommandError::Cancelled)
        ));
        assert_eq!(r.worker.owner_command_usage().0, 2);
        gate.open();
        result(&mut first).unwrap().unwrap();
        let exit = reclaim(&mut r);
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .list_blobs_local("", 128)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(r.worker.owner_command_usage(), (0, 0));
    }
    #[test]
    fn cancellation_after_start_returns_unknown_and_original_join_proves_rollback() {
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        let (source, gate, entered) = blocked(&s);
        let mut handle = r
            .worker
            .stage_blob_frames_local(source, s.clone(), 1)
            .unwrap();
        entered.recv_timeout(WAIT).unwrap();
        handle.cancel();
        assert!(matches!(
            result(&mut handle),
            Err(OwnerCommandError::Unknown)
        ));
        gate.open();
        let exit = reclaim(&mut r);
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
        assert!(
            exit.owner
                .host
                .store_local()
                .list_blobs_local("", 128)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn original_expiry_or_registry_revocation_vetoes_a_blocked_source_after_it_returns() {
        for expiry in [false, true] {
            let mut r = run(JobLimits::default());
            let s = spec(b"bytes");
            let (source, gate, entered) = blocked(&s);
            let mut handle = r
                .worker
                .stage_blob_frames_local(source, s.clone(), 1)
                .unwrap();
            entered.recv_timeout(WAIT).unwrap();
            if expiry {
                r.clock.store(100, Ordering::SeqCst);
            } else {
                r.manager
                    .set_enabled(ID, r.digest, false, r.manager.revision())
                    .unwrap();
            }
            assert!(matches!(
                result(&mut handle),
                Err(OwnerCommandError::Unknown)
            ));
            gate.open();
            let exit = reclaim(&mut r);
            assert!(
                exit.owner
                    .host
                    .store_local()
                    .inspect_blob_stage_local(&s.command())
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn cancelled_unread_commit_is_durable_and_original_history_survives() {
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        let mut handle = r
            .worker
            .stage_blob_frames_local(Frames::new(frames(&s, b"bytes")), s.clone(), 1)
            .unwrap();
        let end = Instant::now() + WAIT;
        while handle.poll() != OwnerCommandPoll::Ready {
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        }
        handle.cancel();
        assert!(matches!(
            result(&mut handle),
            Err(OwnerCommandError::Unknown)
        ));
        let mut inspect = r.worker.inspect_blob_stage_local(s.clone()).unwrap();
        assert!(matches!(
            result(&mut inspect),
            Ok(Ok(BlobResponse::Inspected(Some(_))))
        ));
        let exit = reclaim(&mut r);
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .list_blobs_local("", 128)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn shared_worker_budget_refuses_before_source_or_owner_callback() {
        let mut r = run(JobLimits::new(16, 1024 * 1024, 1024 * 1024).unwrap());
        let s = spec(b"bytes");
        assert!(matches!(
            r.worker.stage_blob_frames_local(Never, s, 1),
            Err(OwnerCommandError::Limit)
        ));
        assert_eq!(r.worker.owner_command_usage(), (0, 0));
        assert!(r.threads.lock().unwrap().is_empty());
        let mut tiny_wire = spec(b"bytes");
        tiny_wire.byte_length = 1024 * 1024;
        tiny_wire.limits.max_wire_bytes = 0;
        assert!(matches!(
            r.worker.stage_blob_frames_local(Never, tiny_wire, 1),
            Err(OwnerCommandError::Limit)
        ));
        assert!(r.threads.lock().unwrap().is_empty());
        let exit = reclaim(&mut r);
        assert_eq!(exit.result, Ok(()));
    }
    #[test]
    fn source_panic_keeps_original_owner_and_rolls_back_without_replay() {
        struct Panics;
        impl FrameSource for Panics {
            fn read_frame(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("injected original source panic")
            }
        }
        let mut r = run(JobLimits::default());
        let s = spec(b"bytes");
        let mut handle = r
            .worker
            .stage_blob_frames_local(Panics, s.clone(), 1)
            .unwrap();
        assert!(matches!(
            result(&mut handle),
            Err(OwnerCommandError::Unknown)
        ));
        let exit = reclaim(&mut r);
        assert!(exit.result.is_err());
        assert!(Arc::ptr_eq(&r.identity, &exit.owner.identity));
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_none()
        );
        exit.owner.host.store_local().integrity_check().unwrap();
    }
}
