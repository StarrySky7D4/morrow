//! Actual Wasm -> original native owner/Store calls. WAT fixtures are not the
//! separately built Rust/C/C++ SDK guests or the protected product owner.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_blob_transfer_v1::{Action, Frame, Identity, Limits as FrameLimits};
use morrow_blob_upload_request_v1::{ActionRef, Phase, Request};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    blob_upload::Spec,
    blob_upload_binding::{
        ApprovalRequest, MAX_NATIVE_WORK_BYTES, UploadApproval, UploadRevocation,
    },
    blob_upload_worker::{Error, Exit, Poll, UploadWorker},
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

const NOMINATION: [u8; 32] = [71; 32];
enum Prepare {
    Normal,
    Fail,
    Panic,
    Revoke(UploadRevocation),
    Wait(SyncSender<()>, Receiver<()>),
    ReplaceStore,
}
struct Owner {
    dir: tempfile::TempDir,
    host: HostRuntime,
    manager: Manager,
    prepare: Prepare,
    prepared: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    retained_store: Option<Store>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        match &self.prepare {
            Prepare::Normal => {}
            Prepare::Fail => return Err(JobError::Unavailable),
            Prepare::Panic => panic!("injected prepare panic"),
            Prepare::Revoke(revoke) => revoke.revoke(),
            Prepare::Wait(entered, release) => {
                entered.send(()).map_err(|_| JobError::Unavailable)?;
                release
                    .recv_timeout(Duration::from_secs(30))
                    .map_err(|_| JobError::Unavailable)?;
            }
            Prepare::ReplaceStore => {
                let replacement =
                    Store::open(&self.dir.path().join("replacement"), Default::default()).unwrap();
                self.retained_store =
                    Some(std::mem::replace(self.host.store_local_mut(), replacement));
            }
        }
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.finished.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}
fn spec(bytes: &[u8]) -> Spec {
    Spec {
        owner_domain: [72; 32],
        identity: Identity {
            transfer_epoch: [1; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).into(),
        limits: FrameLimits::default(),
    }
}
fn frames(s: &Spec, bytes: &[u8]) -> Vec<Vec<u8>> {
    let frame = |action| {
        Frame {
            identity: s.identity,
            action,
        }
        .encode()
        .unwrap()
    };
    let mut result = vec![frame(Action::Descriptor {
        total_length: s.byte_length,
        whole_sha256: s.sha256,
    })];
    for (n, payload) in bytes.chunks(60 * 1024).enumerate() {
        result.push(frame(Action::Chunk {
            sequence: n as u64 + 1,
            offset: (n * 60 * 1024) as u64,
            payload: payload.to_vec(),
            chunk_sha256: Sha256::digest(payload).into(),
        }));
    }
    result.push(frame(Action::End {
        total_length: s.byte_length,
        whole_sha256: s.sha256,
    }));
    result
}
fn request(index: usize, action: ActionRef<'_>) -> Request {
    let mut nonce = [9; 32];
    nonce[..8].copy_from_slice(&(index as u64).to_le_bytes());
    Request::new(NOMINATION, nonce, action).unwrap()
}
fn calls(frames: &[Vec<u8>]) -> Vec<Request> {
    let mut requests = vec![request(0, ActionRef::Open)];
    for frame in frames {
        requests.push(request(requests.len(), ActionRef::Push(frame)));
    }
    requests
}
fn module(requests: &[Request], trap_after: bool) -> Vec<u8> {
    let mut data = String::new();
    let mut run = String::new();
    let mut offset = 131072;
    for request in requests {
        let bytes = request.wire();
        let hex: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
        data.push_str(&format!("(data (i32.const {offset}) \"{hex}\")\n"));
        run.push_str(&format!("i32.const {offset} i32.const {} i32.const 65536 i32.const 4096 call $upload local.tee $n i32.const 1 i32.lt_s if unreachable end\n", bytes.len()));
        offset += bytes.len();
    }
    let finish = if trap_after {
        "unreachable"
    } else {
        "i32.const 65536 local.get $n call $complete drop"
    };
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
      (import "morrow_blob_upload_v1" "call" (func $upload (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 16) {data}
      (func (export "morrow_run") (result i32) (local $n i32)
       i32.const 0 i32.const 131072 call $read drop {run} {finish} i32.const 0))"#
    ))
    .unwrap()
}
fn input() -> Invocation {
    Invocation::new_transform(
        "upload-original",
        Transform {
            handler: "blob.upload".into(),
            input_type: "upload.input.v1".into(),
            output_type: "upload.result.v1".into(),
            input: NOMINATION.to_vec(),
        },
    )
    .unwrap()
}
fn fixture(
    s: &Spec,
    requests: &[Request],
    trap_after: bool,
) -> (Owner, ManagedInstance, UploadApproval) {
    let dir = tempfile::tempdir().unwrap();
    let bytes = module(requests, trap_after);
    let mut manifest = Package::manifest_for_blob_upload(
        "org.example.upload-worker",
        "1.0.0",
        &bytes,
        vec![proto::TransformHandler {
            handler: "blob.upload".into(),
            input_type: "upload.input.v1".into(),
            output_type: "upload.result.v1".into(),
            max_input_bytes: 1024,
            max_output_bytes: 4096,
        }],
    );
    manifest.budget.as_mut().unwrap().host_calls = 1024;
    let package = Package::build(manifest, &bytes).unwrap();
    let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&dir.path().join("registry"), catalog).unwrap(),
        Limits {
            host_calls: 1024,
            ..Limits::default()
        },
    );
    manager.select(&package, 0).unwrap();
    let id = &package.manifest().package_id;
    manager
        .approve_blob_upload(id, package.digest(), true, manager.revision())
        .unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("original"), Default::default()).unwrap())
            .unwrap();
    let instance = manager.connect(id, &mut host).unwrap();
    let approval = UploadApproval::issue(
        &manager,
        &host,
        &instance,
        ApprovalRequest {
            expected_revision: manager.revision(),
            nomination_ref: NOMINATION,
            spec: s.clone(),
            lifetime: Duration::from_secs(30),
        },
    )
    .unwrap();
    (
        Owner {
            dir,
            host,
            manager,
            prepare: Prepare::Normal,
            prepared: Arc::new(AtomicUsize::new(0)),
            finished: Arc::new(AtomicUsize::new(0)),
            retained_store: None,
        },
        instance,
        approval,
    )
}
fn join(mut worker: UploadWorker<Owner>) -> Exit<Owner> {
    let end = Instant::now() + Duration::from_secs(30);
    while worker.poll() == Poll::Running {
        assert!(Instant::now() < end, "original worker failed to return");
        std::thread::sleep(Duration::from_millis(5));
    }
    let exit = worker.try_join().unwrap().unwrap();
    assert_eq!(worker.poll(), Poll::Consumed);
    assert!(matches!(worker.try_join(), Err(Error::Consumed)));
    exit
}
fn run(s: &Spec, calls: &[Request], trap_after: bool) -> Exit<Owner> {
    let (owner, instance, approval) = fixture(s, calls, trap_after);
    join(UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap())
}
fn clean(exit: &Exit<Owner>) {
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert!(exit.instance.is_none());
    assert_eq!(exit.owner.prepared.load(Ordering::SeqCst), 1);
    assert_eq!(exit.owner.finished.load(Ordering::SeqCst), 1);
    exit.owner.host.store_local().integrity_check().unwrap();
}
#[test]
fn actual_guest_commits_empty_and_multichunk_bytes_on_original_owner() {
    for bytes in [vec![], vec![43; 150_017]] {
        let s = spec(&bytes);
        let mut requests = calls(&frames(&s, &bytes));
        requests.push(request(requests.len(), ActionRef::Inspect));
        let exit = run(&s, &requests, false);
        clean(&exit);
        assert!(!exit.unknown);
        let result = exit.result.as_ref().unwrap();
        assert_eq!(result.phase, Phase::Staged);
        assert_eq!(result.execution.outcome, Ok(0));
        assert_eq!(result.execution.host_calls as usize, requests.len());
        assert_eq!(exit.usage.calls as usize, requests.len());
        assert!(exit.usage.native_work_bytes > s.byte_length);
        let record = exit
            .owner
            .host
            .store_local()
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .unwrap();
        let mut actual = vec![];
        exit.owner
            .host
            .store_local()
            .export_blob_local(&record.blob().id, &mut actual)
            .unwrap();
        assert_eq!(actual, bytes);
        assert_eq!(record.staged_at_unix_ms(), 7);
        let Owner { dir, host, .. } = exit.owner;
        drop(host);
        let reopened =
            Store::open_existing(&dir.path().join("original"), Default::default()).unwrap();
        assert_eq!(
            reopened
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .unwrap(),
            record
        );
    }
}
#[test]
fn duplicate_chunk_ack_does_not_duplicate_original_bytes() {
    let bytes = vec![19; 150_017];
    let s = spec(&bytes);
    let mut body = frames(&s, &bytes);
    body.insert(2, body[1].clone());
    let exit = run(&s, &calls(&body), false);
    clean(&exit);
    assert!(exit.result.is_ok() && !exit.unknown);
    let record = exit
        .owner
        .host
        .store_local()
        .inspect_blob_stage_local(&s.command())
        .unwrap()
        .unwrap();
    let mut actual = vec![];
    exit.owner
        .host
        .store_local()
        .export_blob_local(&record.blob().id, &mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
}
#[test]
fn missing_end_and_duplicate_nonce_roll_back_without_publishing_a_receipt() {
    let bytes = vec![28; 90_000];
    let s = spec(&bytes);
    let body = frames(&s, &bytes);
    let mut reused = calls(&body);
    reused[2] = Request::new(NOMINATION, reused[1].nonce(), reused[2].action()).unwrap();
    for requests in [calls(&body[..body.len() - 1]), reused] {
        let exit = run(&s, &requests, false);
        clean(&exit);
        assert!(exit.result.is_err() && !exit.unknown);
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
}
#[test]
fn wrong_nomination_and_foreign_frame_never_publish_bytes() {
    let s = spec(b"payload");
    let body = frames(&s, b"payload");
    let mut wrong = calls(&body);
    wrong[0] = Request::new([99; 32], wrong[0].nonce(), ActionRef::Open).unwrap();
    let mut other = s.clone();
    other.identity.object_ref = [77; 32];
    for requests in [wrong, calls(&frames(&other, b"payload"))] {
        let exit = run(&s, &requests, false);
        clean(&exit);
        assert!(exit.result.is_err() && !exit.unknown);
        assert!(
            exit.owner
                .host
                .store_local()
                .list_blobs_local("", 128)
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn later_push_or_guest_trap_cannot_turn_a_committed_end_into_rollback() {
    let s = spec(b"original");
    let body = frames(&s, b"original");
    let normal = calls(&body);
    let mut extra = normal.clone();
    extra.push(request(extra.len(), ActionRef::Push(&body[1])));
    for (requests, trap) in [(extra, false), (normal, true)] {
        let exit = run(&s, &requests, trap);
        clean(&exit);
        assert!(exit.result.is_err() && exit.unknown);
        let original = exit
            .owner
            .host
            .store_local()
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .unwrap();
        assert_eq!(original.staged_at_unix_ms(), 7);
        assert!(!original.is_released());
    }
}
#[test]
fn release_is_terminal_and_original_history_survives_a_later_refused_push() {
    let s = spec(b"original");
    let body = frames(&s, b"original");
    for extra in [false, true] {
        let mut requests = calls(&body);
        requests.push(request(requests.len(), ActionRef::Release));
        requests.push(request(requests.len(), ActionRef::Inspect));
        if extra {
            requests.push(request(requests.len(), ActionRef::Push(&body[0])));
        }
        let exit = run(&s, &requests, false);
        clean(&exit);
        if extra {
            assert!(exit.result.is_err() && exit.unknown);
        } else {
            assert_eq!(exit.result.as_ref().unwrap().phase, Phase::Released);
            assert!(!exit.unknown);
        }
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .unwrap()
                .is_released()
        );
    }
}
#[test]
fn absent_inspection_does_not_reopen_a_new_upload() {
    let s = spec(b"payload");
    let body = frames(&s, b"payload");
    let requests = vec![
        request(0, ActionRef::Open),
        request(1, ActionRef::Inspect),
        request(2, ActionRef::Push(&body[0])),
    ];
    let exit = run(&s, &requests, false);
    clean(&exit);
    assert!(exit.result.is_err() && !exit.unknown);
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
fn prepare_failure_panic_and_revocation_keep_owner_for_confirmed_cleanup() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    for kind in 0..3 {
        let (mut owner, instance, approval) = fixture(&s, &requests, false);
        owner.prepare = match kind {
            0 => Prepare::Fail,
            1 => Prepare::Panic,
            _ => Prepare::Revoke(approval.revocation()),
        };
        let exit = join(UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap());
        clean(&exit);
        assert!(exit.result.is_err() && !exit.unknown);
        assert_eq!(exit.usage.calls, 0);
        assert!(
            exit.owner
                .host
                .store_local()
                .list_blobs_local("", 128)
                .unwrap()
                .is_empty()
        );
    }
}
#[test]
fn cancellation_before_guest_start_is_checked_on_original_worker() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    let (mut owner, instance, approval) = fixture(&s, &requests, false);
    let (entered_tx, entered) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    owner.prepare = Prepare::Wait(entered_tx, release_rx);
    let worker = UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap();
    entered.recv_timeout(Duration::from_secs(30)).unwrap();
    worker.cancel();
    release.send(()).unwrap();
    let exit = join(worker);
    clean(&exit);
    assert!(matches!(exit.result, Err(Error::Cancelled)));
    assert!(!exit.unknown);
    assert_eq!(exit.usage.calls, 0);
}
#[test]
fn changed_store_is_not_sealed_and_original_unresolved_instance_is_returned() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    let (mut owner, instance, approval) = fixture(&s, &requests, false);
    owner.prepare = Prepare::ReplaceStore;
    let mut exit = join(UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap());
    assert!(exit.result.is_err() && !exit.unknown);
    assert!(exit.disconnect.is_err() && exit.maintenance.is_err());
    assert_eq!(exit.owner.finished.load(Ordering::SeqCst), 0);
    assert_eq!(exit.usage.calls, 0);
    let original = exit.owner.retained_store.take().unwrap();
    assert!(
        original
            .inspect_blob_stage_local(&s.command())
            .unwrap()
            .is_none()
    );
    *exit.owner.host.store_local_mut() = original;
    exit.instance
        .take()
        .unwrap()
        .close(&mut exit.owner.host)
        .unwrap();
}
#[test]
fn unread_commit_is_not_success_after_cancellation_or_explicit_revocation() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    for revoke in [false, true] {
        let (owner, instance, approval) = fixture(&s, &requests, false);
        let revocation = approval.revocation();
        let worker = UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap();
        let end = Instant::now() + Duration::from_secs(30);
        while worker.poll() == Poll::Running {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(5));
        }
        if revoke {
            revocation.revoke();
        } else {
            worker.cancel();
        }
        let exit = join(worker);
        clean(&exit);
        assert!(matches!(exit.result, Err(Error::Cancelled | Error::Denied)));
        assert!(exit.unknown);
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_some()
        );
    }
}
#[test]
fn native_verification_budget_is_separate_and_not_refunded() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    let (owner, instance, approval) = fixture(&s, &requests, false);
    approval
        .charge_native_work(MAX_NATIVE_WORK_BYTES - 1)
        .unwrap();
    let exit = join(UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap());
    clean(&exit);
    assert!(matches!(exit.result, Err(Error::Limit)));
    assert!(!exit.unknown);
    assert_eq!(exit.usage.native_work_bytes, MAX_NATIVE_WORK_BYTES - 1);
    assert_eq!(exit.usage.calls, 1);
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
fn original_delivery_control_survives_join_without_reopening_authority() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    for revoke in [false, true] {
        let (owner, instance, approval) = fixture(&s, &requests, false);
        let revocation = approval.revocation();
        let worker = UploadWorker::spawn_owned(owner, instance, approval, input(), 7).unwrap();
        let control = worker.control();
        let exit = join(worker);
        clean(&exit);
        assert_eq!(exit.result.as_ref().unwrap().phase, Phase::Staged);
        assert!(!exit.unknown);
        assert_eq!(control.delivery_error(), None);
        if revoke {
            revocation.revoke();
        } else {
            control.cancel();
        }
        assert!(matches!(
            control.delivery_error(),
            Some(Error::Cancelled | Error::Denied)
        ));
        assert!(
            exit.owner
                .host
                .store_local()
                .inspect_blob_stage_local(&s.command())
                .unwrap()
                .is_some()
        );
    }
}
#[test]
fn mismatched_handler_refuses_before_transferring_the_original_owner() {
    let s = spec(b"payload");
    let requests = calls(&frames(&s, b"payload"));
    let (owner, instance, approval) = fixture(&s, &requests, false);
    let wrong = Invocation::new_transform(
        "wrong",
        Transform {
            handler: "other".into(),
            ..input().transform().unwrap().clone()
        },
    )
    .unwrap();
    let mut failure = UploadWorker::spawn_owned(owner, instance, approval, wrong, 7)
        .err()
        .unwrap();
    assert_eq!(failure.error, Error::Protocol);
    assert_eq!(failure.owner.prepared.load(Ordering::SeqCst), 0);
    failure
        .approval
        .validate_owner(
            &failure.owner.manager,
            &failure.owner.host,
            &failure.instance,
        )
        .unwrap();
    failure.instance.close(&mut failure.owner.host).unwrap();
}
