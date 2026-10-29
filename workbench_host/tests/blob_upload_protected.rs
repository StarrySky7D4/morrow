//! Real SDK guests through the original Windows protected Workbench owner.
//! Synthetic content libraries only. Not rendered approval UI or full SDK proof.
#![cfg(all(windows, feature = "blob-upload-guest-qualification"))]
#![deny(unsafe_code)]
use morrow_audit::session::{OpenMode, Session};
use morrow_blob_transfer_v1::{Action, Frame, Identity, Limits as FrameLimits};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::{
    plugin_package::{Package, proto},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Limits, blob_upload::Spec, blob_upload_worker::Error};
use morrow_workbench_host::{
    Workbench,
    io_tasks::{
        StoragePhase, TaskKey,
        blob_upload::{Observation, UploadResult, UploadStart},
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.protected-upload";
const FUEL: u64 = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;

// Exercise the same private binary dispatcher used by the native UI. This
// qualification is still not evidence that a rendered approval was clicked.
use morrow_workbench_host::{host_capnp as private, protocol};
fn private_call(
    w: &mut Workbench,
    action: private::Action,
    configure: impl FnOnce(private::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut row = message.init_root::<private::request::Builder>();
    row.set_version(1);
    row.set_digest(&protocol::digest());
    row.set_action(action);
    configure(row);
    protocol::respond(w, &capnp::serialize::write_message_to_words(&message)).unwrap()
}
fn private_reply<T>(bytes: &[u8], check: impl FnOnce(private::response::Reader<'_>) -> T) -> T {
    let mut remaining = bytes;
    let message = capnp::serialize::read_message_from_flat_slice(
        &mut remaining,
        capnp::message::ReaderOptions::new(),
    )
    .unwrap();
    assert!(remaining.is_empty());
    let reply = message.get_root::<private::response::Reader>().unwrap();
    assert_eq!(reply.get_version(), 1);
    assert_eq!(reply.get_digest().unwrap(), protocol::digest());
    check(reply)
}
fn private_success(bytes: &[u8]) {
    private_reply(bytes, |r| assert!(r.get_error().unwrap().is_empty()));
}
fn private_start(r: private::request::Builder<'_>, value: &UploadStart) {
    let mut row = r.init_blob_upload_start();
    row.set_submission(&value.submission);
    row.set_package_id(value.package_id.as_str());
    row.set_package_digest(&value.digest);
    row.set_registry_revision(value.revision);
    row.set_nomination_ref(&value.nomination_ref);
    row.set_owner_domain(&value.spec.owner_domain);
    row.set_transfer_epoch(&value.spec.identity.transfer_epoch);
    row.set_object_ref(&value.spec.identity.object_ref);
    row.set_operation_id(&value.spec.identity.operation_id);
    row.set_byte_length(value.spec.byte_length);
    row.set_whole_sha256(&value.spec.sha256);
    row.set_timeout_ms(value.lifetime.as_millis().try_into().unwrap());
    let input = value.input.transform().unwrap();
    row.set_handler(input.handler.as_str());
    row.set_input_type(input.input_type.as_str());
    row.set_output_type(input.output_type.as_str());
    row.set_input(&input.input);
}
fn private_route(cancel: bool) {
    let root = root(if cancel {
        "private-cancel"
    } else {
        "private-lost-start"
    });
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, false);
    let catalog = private_call(&mut w, private::Action::PluginCatalog, |_| {});
    private_success(&catalog);
    private_reply(&catalog, |r| {
        let entry = r
            .get_plugins()
            .unwrap()
            .iter()
            .find(|p| p.get_package_id().unwrap() == ID)
            .unwrap();
        assert!(entry.get_blob_upload_supported());
        assert!(!entry.get_blob_upload_approved());
        assert_eq!(entry.get_blob_upload_max_bytes(), 16 * 1024 * 1024);
        assert_eq!(entry.get_blob_upload_handlers().unwrap().len(), 2);
    });
    let s = spec(150017);
    let unapproved = options(&w, &p, &s, 11, false);
    let reply = private_call(&mut w, private::Action::BlobUploadStart, |r| {
        private_start(r, &unapproved)
    });
    private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    assert!(w.io_status().key.is_none());
    let rev = revision(&w);
    let approve = private_call(&mut w, private::Action::PluginApproveBlobUpload, |mut r| {
        r.set_id(ID);
        r.set_sha256(&p.digest());
        r.set_revision(rev);
        r.set_blob_upload_approved(true);
    });
    private_success(&approve);
    let start = options(&w, &p, &s, 11, false);
    let mut invalid = options(&w, &p, &s, 12, false);
    invalid.lifetime = Duration::ZERO;
    let refusal = private_call(&mut w, private::Action::BlobUploadStart, |r| {
        private_start(r, &invalid)
    });
    private_reply(&refusal, |r| assert!(!r.get_error().unwrap().is_empty()));
    assert!(w.io_status().key.is_none());
    // Simulated missing START reply: discover only via the original submission.
    let start_reply = private_call(&mut w, private::Action::BlobUploadStart, |r| {
        private_start(r, &start)
    });
    private_success(&start_reply);
    drop(start_reply);
    let status = private_call(&mut w, private::Action::IoStatus, |_| {});
    private_success(&status);
    let key: [u8; 32] = private_reply(&status, |r| {
        let state = r.get_io_state().unwrap();
        assert_eq!(state.get_submission().unwrap(), start.submission);
        state.get_key().unwrap().try_into().unwrap()
    });
    let duplicate = private_call(&mut w, private::Action::BlobUploadStart, |r| {
        private_start(r, &start)
    });
    private_reply(&duplicate, |r| assert!(!r.get_error().unwrap().is_empty()));
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        let reply = private_call(&mut w, private::Action::BlobUploadStatus, |mut r| {
            r.set_io_key(&key)
        });
        private_success(&reply);
        let ready = private_reply(&reply, |r| {
            assert_eq!(r.get_io_state().unwrap().get_key().unwrap(), key);
            assert_eq!(
                r.get_io_state().unwrap().get_submission().unwrap(),
                start.submission
            );
            assert_eq!(r.get_blob_upload_result().unwrap().get_kind(), 0);
            r.get_blob_upload_state().unwrap().get_ready()
        });
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "retain original owner on timeout"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let premature = private_call(&mut w, private::Action::IoAcknowledge, |mut r| {
        r.set_io_key(&key)
    });
    private_reply(&premature, |r| assert!(!r.get_error().unwrap().is_empty()));
    let wrong = private_call(&mut w, private::Action::BlobUploadRead, |mut r| {
        r.set_io_key(&[99; 32])
    });
    private_reply(&wrong, |r| assert!(!r.get_error().unwrap().is_empty()));
    if cancel {
        let reply = private_call(&mut w, private::Action::IoCancel, |mut r| {
            r.set_io_key(&key)
        });
        private_success(&reply);
    }
    let result = private_call(&mut w, private::Action::BlobUploadRead, |mut r| {
        r.set_io_key(&key)
    });
    private_success(&result);
    private_reply(&result, |r| {
        let result = r.get_blob_upload_result().unwrap();
        let state = r.get_blob_upload_state().unwrap();
        assert!(state.get_delivered());
        assert!(!state.get_ready());
        assert_eq!(state.get_recovery_required(), cancel);
        assert_eq!(result.get_unknown(), cancel);
        assert_eq!(result.get_kind(), if cancel { 2 } else { 1 });
        assert_eq!(result.get_phase(), if cancel { 0 } else { 2 });
        assert_eq!(result.get_error(), if cancel { 4 } else { 0 });
        assert!(result.get_has_execution());
        assert_eq!(result.get_execution_fault(), 0);
        assert_eq!(result.get_exit_code(), 0);
        assert_eq!(result.get_calls(), 7);
        assert!(r.get_io_state().unwrap().get_has_exit());
    });
    let twice = private_call(&mut w, private::Action::BlobUploadRead, |mut r| {
        r.set_io_key(&key)
    });
    private_reply(&twice, |r| assert!(!r.get_error().unwrap().is_empty()));
    if cancel {
        let denied = private_call(&mut w, private::Action::IoAcknowledge, |mut r| {
            r.set_io_key(&key)
        });
        private_reply(&denied, |r| assert!(!r.get_error().unwrap().is_empty()));
        let observation = private_call(&mut w, private::Action::BlobUploadInspect, |mut r| {
            r.set_io_key(&key)
        });
        private_success(&observation);
        private_reply(&observation, |r| {
            assert_eq!(r.get_blob_upload_state().unwrap().get_observed(), 2);
            assert!(r.get_blob_upload_state().unwrap().get_recovery_required());
        });
        let wrong = private_call(&mut w, private::Action::BlobUploadConfirm, |mut r| {
            r.set_io_key(&key);
            r.set_blob_upload_observation(1);
        });
        private_reply(&wrong, |r| assert!(!r.get_error().unwrap().is_empty()));
        let confirmation = private_call(&mut w, private::Action::BlobUploadConfirm, |mut r| {
            r.set_io_key(&key);
            r.set_blob_upload_observation(2);
        });
        private_success(&confirmation);
    }
    let closed = private_call(&mut w, private::Action::IoAcknowledge, |mut r| {
        r.set_io_key(&key)
    });
    private_success(&closed);
    private_reply(&closed, |r| {
        let io = r.get_io_state().unwrap();
        assert!(io.get_key().unwrap().is_empty());
        assert!(io.get_submission().unwrap().is_empty());
    });
    let replay = private_call(&mut w, private::Action::BlobUploadStart, |r| {
        private_start(r, &start)
    });
    private_reply(&replay, |r| assert!(!r.get_error().unwrap().is_empty()));
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(false));
    eprintln!(
        "PRIVATE_UPLOAD_ACCEPTED cancel={cancel} original_workers_joined=1 sealed=true replay=false"
    );
}
#[test]
fn private_upload_route_lost_start_keeps_original_submission() {
    private_route(false);
}
#[test]
fn private_upload_route_cancel_requires_original_history() {
    private_route(true);
}

fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn limits() -> Limits {
    Limits {
        fuel: FUEL,
        host_calls: 1024,
        ..Limits::default()
    }
}
fn root(name: &str) -> PathBuf {
    assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
    let root = PathBuf::from(
        std::env::var_os("MORROW_PROTECTED_UPLOAD_ROOT").expect("fresh evidence root required"),
    );
    assert!(root.is_absolute() && root.starts_with("D:\\build_cache"));
    let root = root.join(name);
    fs::create_dir(&root).expect("fresh case only; never replay historical owners");
    eprintln!("PROTECTED_UPLOAD_STORE {}", root.display());
    root
}
fn package(language: &str, fuel: u64) -> Package {
    let key = format!("MORROW_{language}_UPLOAD_WASM");
    let path = PathBuf::from(std::env::var_os(&key).unwrap());
    let pin = std::env::var(format!("{key}_SHA256")).unwrap();
    assert!(path.is_absolute());
    let bytes = fs::read(path).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), pin);
    let mut manifest = Package::manifest_for_blob_upload(
        ID,
        "1.0.0",
        &bytes,
        ["blob.upload", "blob.upload.release"]
            .into_iter()
            .map(|handler| proto::TransformHandler {
                handler: handler.into(),
                input_type: "upload.descriptor.v1".into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: 1024,
                max_output_bytes: 4096,
            })
            .collect(),
    );
    manifest.budget.as_mut().unwrap().fuel = fuel;
    manifest.budget.as_mut().unwrap().host_calls = 1024;
    eprintln!(
        "PROTECTED_UPLOAD_GUEST language={language} sha256={pin} bytes={}",
        bytes.len()
    );
    Package::build(manifest, &bytes).unwrap()
}
fn install(w: &mut Workbench, root: &Path, p: &Package, approved: bool) {
    let file = root.join("incoming.mplugin");
    fs::write(&file, p.archive()).unwrap();
    w.import_plugin(&file, &p.digest(), revision(w)).unwrap();
    w.configure_external(ID, &p.digest(), revision(w), &[], true)
        .unwrap();
    if approved {
        w.configure_external_blob_upload(ID, &p.digest(), revision(w), true)
            .unwrap();
    }
}
fn spec(length: usize) -> Spec {
    Spec {
        owner_domain: [72; 32],
        identity: Identity {
            transfer_epoch: [71; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: length as u64,
        sha256: Sha256::digest(vec![42; length]).into(),
        limits: FrameLimits::default(),
    }
}
fn options(w: &Workbench, p: &Package, s: &Spec, submission: u8, release: bool) -> UploadStart {
    UploadStart {
        submission: [submission; 32],
        package_id: ID.into(),
        digest: p.digest(),
        revision: revision(w),
        nomination_ref: s.identity.transfer_epoch,
        spec: s.clone(),
        lifetime: Duration::from_secs(120),
        limits: limits(),
        input: Invocation::new_transform(
            "protected-upload",
            Transform {
                handler: if release {
                    "blob.upload.release"
                } else {
                    "blob.upload"
                }
                .into(),
                input_type: "upload.descriptor.v1".into(),
                output_type: "upload.result.v1".into(),
                input: Frame {
                    identity: s.identity,
                    action: Action::Descriptor {
                        total_length: s.byte_length,
                        whole_sha256: s.sha256,
                    },
                }
                .encode()
                .unwrap(),
            },
        )
        .unwrap(),
    }
}
fn wait(w: &mut Workbench, key: TaskKey) {
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        let status = w.poll_blob_upload(key).unwrap();
        if status.ready {
            assert!(matches!(
                status.task.storage,
                StoragePhase::Reclaimed | StoragePhase::RecoveryRequired
            ));
            let exit = status.task.exit.unwrap();
            assert_eq!(exit.disconnect, Ok(()));
            assert_eq!(exit.maintenance, Ok(()));
            return;
        }
        assert!(
            Instant::now() < deadline,
            "retain original owner; no replacement or automatic retry"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn read(w: &mut Workbench, key: TaskKey) -> UploadResult {
    wait(w, key);
    let result = w.read_blob_upload(key).unwrap().unwrap();
    assert!(w.read_blob_upload(key).is_err(), "one result delivery only");
    eprintln!(
        "PROTECTED_UPLOAD_RESULT outcome={:?} unknown={} usage={:?}",
        result.outcome, result.unknown, result.usage
    );
    result
}
fn history(database: &Path, s: &Spec, released: Option<bool>) {
    let session = Session::open(database, Default::default(), OpenMode::Existing).unwrap();
    let store = session.runtime_ref().store_local();
    store.integrity_check().unwrap();
    let record = store.inspect_blob_stage_local(&s.command()).unwrap();
    match released {
        None => assert!(record.is_none()),
        Some(released) => {
            let record = record.unwrap();
            assert_eq!(record.is_released(), released);
            if !released {
                let mut bytes = Vec::new();
                store
                    .export_blob_local(&record.blob().id, &mut bytes)
                    .unwrap();
                assert_eq!(bytes.len() as u64, s.byte_length);
                assert_eq!(<[u8; 32]>::from(Sha256::digest(&bytes)), s.sha256);
                assert!(bytes.iter().all(|b| *b == 42));
            }
        }
    }
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
}
fn accepted(language: &str, length: usize) {
    let root = root(&format!("{language}-{length}"));
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package(language, FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(length);
    let start = options(&w, &p, &s, 1, false);
    let duplicate = options(&w, &p, &s, 1, false);
    let key = w.start_blob_upload(start).unwrap();
    assert!(
        Workbench::open(&database, None).is_err(),
        "original protected lease stays exclusive on worker"
    );
    let result = read(&mut w, key);
    assert_eq!(result.outcome, Ok(Phase::Staged));
    assert!(!result.unknown);
    assert_eq!(result.execution.unwrap().outcome, Ok(0));
    assert_eq!(result.usage.calls, length.div_ceil(61440) as u32 + 4);
    let journal = w.inspect_persisted_blob_upload(&[1; 32]).unwrap();
    assert_eq!(journal.record.revision(), 2);
    assert_eq!(journal.record.request().command, s.command());
    assert!(journal.record.acknowledged_at_unix_ms().is_none());
    assert_eq!(journal.current_stage, Observation::Staged);
    let original = journal.record.original_result().unwrap();
    assert_eq!(
        original.outcome,
        morrow_core::blob_upload_journal::Outcome::Staged
    );
    assert!(!original.unknown);
    assert_eq!(
        w.inspect_blob_upload_history(key).unwrap(),
        Observation::Staged
    );
    assert!(
        Workbench::open(&database, None).is_err(),
        "returned owner retains original lease"
    );
    w.acknowledge_io(key).unwrap();
    assert!(
        w.start_blob_upload(duplicate).is_err(),
        "submission cannot be reused after acknowledgement"
    );
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(false));
    let mut w = Workbench::open(&database, None).unwrap();
    let journal = w.inspect_persisted_blob_upload(&[1; 32]).unwrap();
    assert_eq!(journal.record.revision(), 3);
    assert_eq!(journal.record.original_result(), Some(original));
    assert_eq!(
        w.list_blob_upload_history(None, 16).unwrap(),
        vec![journal.record]
    );
    assert!(
        w.list_blob_upload_history(Some(&[1; 32]), 16)
            .unwrap()
            .is_empty()
    );
    let duplicate_after_reopen = options(&w, &p, &s, 1, false);
    assert!(
        w.start_blob_upload(duplicate_after_reopen).is_err(),
        "durable submission cannot be replayed after known-closed reopen"
    );
    assert!(
        w.poll_blob_upload(key).is_err(),
        "reopen does not reconstruct a previous task"
    );
    let release = options(&w, &p, &s, 2, true);
    let release = w.start_blob_upload(release).unwrap();
    let result = read(&mut w, release);
    assert_eq!(result.outcome, Ok(Phase::Released));
    assert!(!result.unknown);
    assert_eq!(result.usage.calls, 2);
    w.acknowledge_io(release).unwrap();
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(true));
    eprintln!(
        "PROTECTED_UPLOAD_ACCEPTED language={language} bytes={length} original_workers_joined=2 protected_reopen=true sealed=true"
    );
}
macro_rules! accepted_case {
    ($name:ident,$lang:literal,$length:expr) => {
        #[test]
        fn $name() {
            accepted($lang, $length)
        }
    };
}
accepted_case!(rust_multichunk, "RUST", 150017);
accepted_case!(c_multichunk, "C", 150017);
accepted_case!(cpp_multichunk, "CPP", 150017);
accepted_case!(rust_maximum, "RUST", 16 * 1024 * 1024);
accepted_case!(c_maximum, "C", 16 * 1024 * 1024);
accepted_case!(cpp_maximum, "CPP", 16 * 1024 * 1024);
#[test]
fn approval_and_task_identity_refuse_before_transferring_owner() {
    let root = root("admission");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, false);
    let s = spec(150017);
    let start = options(&w, &p, &s, 1, false);
    assert!(w.start_blob_upload(start).is_err());
    assert_eq!(w.io_status().storage, StoragePhase::Local);
    w.configure_external_blob_upload(ID, &p.digest(), revision(&w), true)
        .unwrap();
    let mut stale = options(&w, &p, &s, 1, false);
    stale.revision -= 1;
    assert!(w.start_blob_upload(stale).is_err());
    let mut foreign = options(&w, &p, &s, 1, false);
    foreign.digest = [99; 32];
    assert!(w.start_blob_upload(foreign).is_err());
    let start = options(&w, &p, &s, 1, false);
    let key = w.start_blob_upload(start).unwrap();
    assert!(
        w.poll_blob_upload(TaskKey::from_bytes(&[33; 32]).unwrap())
            .is_err()
    );
    wait(&mut w, key);
    assert!(w.acknowledge_io(key).is_err(), "unread result retained");
    assert_eq!(read(&mut w, key).outcome, Ok(Phase::Staged));
    w.acknowledge_io(key).unwrap();
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(false));
}
#[test]
fn low_budget_rolls_back_without_a_protected_receipt() {
    let root = root("low-budget");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", 100_000_000);
    install(&mut w, &root, &p, true);
    let s = spec(16 * 1024 * 1024);
    let start = options(&w, &p, &s, 1, false);
    let key = w.start_blob_upload(start).unwrap();
    let result = read(&mut w, key);
    assert_eq!(result.outcome, Err(Error::Limit));
    assert!(!result.unknown);
    assert_eq!(
        w.inspect_blob_upload_history(key).unwrap(),
        Observation::Absent
    );
    w.acknowledge_io(key).unwrap();
    w.finish().unwrap();
    drop(w);
    history(&database, &s, None);
}
#[test]
fn unread_success_veto_requires_original_history_and_explicit_confirmation() {
    for revoke in [false, true] {
        let root = root(if revoke {
            "unread-revocation"
        } else {
            "unread-cancel"
        });
        let database = root.join("original.db");
        let mut w = Workbench::open(&database, None).unwrap();
        let p = package("RUST", FUEL);
        install(&mut w, &root, &p, true);
        let s = spec(150017);
        let start = options(&w, &p, &s, 1, false);
        let key = w.start_blob_upload(start).unwrap();
        wait(&mut w, key);
        let original = w
            .inspect_persisted_blob_upload(&[1; 32])
            .unwrap()
            .record
            .original_result()
            .unwrap();
        assert!(!original.unknown);
        if revoke {
            w.configure_external_blob_upload(ID, &p.digest(), revision(&w), false)
                .unwrap();
        } else {
            w.cancel_io(key).unwrap();
        }
        let result = read(&mut w, key);
        assert!(result.unknown);
        assert!(matches!(
            result.outcome,
            Err(Error::Cancelled | Error::Denied)
        ));
        assert!(w.acknowledge_io(key).is_err());
        assert!(w.finish().is_err());
        assert_eq!(
            w.repair_io(key).unwrap().storage,
            StoragePhase::RecoveryRequired,
            "generic cleanup cannot clear unresolved upload history"
        );
        assert!(
            w.confirm_blob_upload_recovery(key, Observation::Staged)
                .is_err()
        );
        assert_eq!(
            w.inspect_blob_upload_history(key).unwrap(),
            Observation::Staged
        );
        assert!(
            w.confirm_blob_upload_recovery(key, Observation::Absent)
                .is_err()
        );
        w.confirm_blob_upload_recovery(key, Observation::Staged)
            .unwrap();
        assert!(!w.poll_blob_upload(key).unwrap().recovery_required);
        assert!(
            result.unknown,
            "confirmation never reclassifies the original Unknown result"
        );
        w.acknowledge_io(key).unwrap();
        let journal = w.inspect_persisted_blob_upload(&[1; 32]).unwrap();
        assert_eq!(
            journal.record.original_result(),
            Some(original),
            "delivery loss must not rewrite the joined native result"
        );
        assert_eq!(journal.record.revision(), 3);
        w.finish().unwrap();
        drop(w);
        history(&database, &s, Some(false));
    }
}
#[test]
fn product_gate_loss_preserves_owner_and_refuses_delivery_or_replacement() {
    let root = root("product-gate-loss");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(150017);
    let start = options(&w, &p, &s, 1, false);
    let key = w.start_blob_upload(start).unwrap();
    wait(&mut w, key);
    w.product_gate().revoke();
    assert!(w.poll_blob_upload(key).unwrap().recovery_required);
    assert!(w.read_blob_upload(key).is_err());
    assert!(w.inspect_blob_upload_history(key).is_err());
    assert!(w.acknowledge_io(key).is_err());
    assert!(w.finish().is_err());
    assert!(Workbench::open(&database, None).is_err());
    // This deterministic test's actual original worker was already joined above.
    // Never contact, adopt or replay any historical Unknown acceptance owner.
    drop(w);
    history(&database, &s, Some(false));
}

#[test]
fn persisted_history_private_route_is_bounded_and_never_revives_a_task() {
    let root = root("private-history");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 81, false);
    let key = w.start_blob_upload(start).unwrap();
    assert_eq!(read(&mut w, key).outcome, Ok(Phase::Staged));
    w.acknowledge_io(key).unwrap();
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open(&database, None).unwrap();
    let reply = private_call(&mut w, private::Action::BlobUploadHistoryList, |r| {
        r.init_blob_upload_history_query().set_limit(16);
    });
    private_success(&reply);
    private_reply(&reply, |r| {
        let rows = r.get_blob_upload_history().unwrap();
        assert_eq!(rows.len(), 1);
        let row = rows.get(0);
        assert_eq!(row.get_submission().unwrap(), [81; 32]);
        assert_eq!(row.get_revision(), 3);
        assert!(row.get_result_recorded() && row.get_acknowledged());
        assert_eq!(row.get_original_result().unwrap().get_phase(), 2);
        assert_eq!(r.get_blob_upload_current_stage(), 0);
        assert_eq!(r.get_blob_upload_history_cursor().unwrap(), [81; 32]);
        assert!(r.get_io_state().unwrap().get_key().unwrap().is_empty());
    });
    let reply = private_call(&mut w, private::Action::BlobUploadHistoryInspect, |r| {
        r.init_blob_upload_history_query().set_submission(&[81; 32]);
    });
    private_success(&reply);
    private_reply(&reply, |r| assert_eq!(r.get_blob_upload_current_stage(), 2));
    for limit in [0, 17, u16::MAX] {
        let reply = private_call(&mut w, private::Action::BlobUploadHistoryList, |r| {
            r.init_blob_upload_history_query().set_limit(limit);
        });
        private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    }
    let reply = private_call(
        &mut w,
        private::Action::BlobUploadHistoryInspect,
        |mut r| {
            r.set_io_key(key.as_bytes());
            r.init_blob_upload_history_query().set_submission(&[81; 32]);
        },
    );
    private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    assert!(w.poll_blob_upload(key).is_err());
    assert!(w.acknowledge_io(key).is_err());
    w.finish().unwrap();
    drop(w);
    let foreign = Workbench::open(&root.join("foreign.db"), None).unwrap();
    assert!(
        foreign
            .list_blob_upload_history(None, 16)
            .unwrap()
            .is_empty()
    );
    assert!(foreign.inspect_persisted_blob_upload(&[81; 32]).is_err());
    drop(foreign);
    history(&database, &s, Some(false));
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "requires one explicit metadata-failure point and a fresh process/store"]
fn original_upload_metadata_failure_keeps_dispatch_and_acknowledgement_boundaries() {
    let point = std::env::var("MORROW_UPLOAD_JOURNAL_FAILURE").unwrap();
    assert!(
        [
            "reserve-before",
            "reserve-after",
            "observe-before",
            "observe-after",
            "ack-before",
            "ack-after"
        ]
        .contains(&point.as_str())
    );
    let root = root(&format!("metadata-{point}"));
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 91, false);
    let attempt = w.start_blob_upload(start);
    if point.starts_with("reserve-") {
        assert!(attempt.is_err());
        assert!(
            w.io_status().key.is_none(),
            "reservation failure dispatches no worker"
        );
        let duplicate = options(&w, &p, &s, 91, false);
        assert!(w.start_blob_upload(duplicate).is_err());
        let rows = w.list_blob_upload_history(None, 16).unwrap();
        assert_eq!(rows.len(), usize::from(point.ends_with("after")));
        if let Some(row) = rows.first() {
            assert_eq!(row.revision(), 1);
            assert!(row.original_unknown());
        }
        w.finish().unwrap();
        drop(w);
        history(&database, &s, None);
        return;
    }
    let key = attempt.unwrap();
    assert_eq!(read(&mut w, key).outcome, Ok(Phase::Staged));
    assert!(w.acknowledge_io(key).is_err());
    assert!(w.poll_blob_upload(key).unwrap().recovery_required);
    assert!(w.finish().is_err());
    let before = w.inspect_persisted_blob_upload(&[91; 32]).unwrap().record;
    let repair = w.repair_io(key);
    if point.ends_with("after") {
        assert_eq!(repair.unwrap().storage, StoragePhase::Reclaimed);
        assert_eq!(
            w.inspect_persisted_blob_upload(&[91; 32]).unwrap().record,
            before,
            "repair only reads exact original metadata"
        );
        w.acknowledge_io(key).unwrap();
        assert_eq!(
            w.inspect_persisted_blob_upload(&[91; 32])
                .unwrap()
                .record
                .revision(),
            3
        );
        w.finish().unwrap();
    } else {
        assert!(repair.is_err());
        assert!(w.acknowledge_io(key).is_err());
        assert!(w.poll_blob_upload(key).unwrap().recovery_required);
        assert_eq!(
            w.inspect_persisted_blob_upload(&[91; 32]).unwrap().record,
            before,
            "missing metadata is not silently rewritten"
        );
        assert!(Workbench::open(&database, None).is_err());
    }
    // Synthetic test fixture only: the original worker was actually joined by
    // read(). A retained metadata debt is not qualified as a successful close.
    drop(w);
    history(&database, &s, Some(false));
}

#[test]
fn private_history_read_never_reclaims_the_retained_original_worker() {
    let root = root("history-read-no-reclaim");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 92, false);
    let key = w.start_blob_upload(start).unwrap();
    // Let the original worker run without polling or joining it. History may
    // not advance this owner, even when its result is ready to be reclaimed.
    thread::sleep(Duration::from_secs(1));
    for action in [
        private::Action::BlobUploadMetadataReview,
        private::Action::BlobUploadMetadataComplete,
    ] {
        let reply = private_call(&mut w, action, |mut r| {
            r.set_io_key(key.as_bytes());
            if action == private::Action::BlobUploadMetadataComplete {
                r.set_blob_upload_metadata_permit(&[1; 32]);
            }
        });
        private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    }
    let reply = private_call(&mut w, private::Action::BlobUploadHistoryList, |r| {
        r.init_blob_upload_history_query().set_limit(16);
    });
    private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    let reply = private_call(&mut w, private::Action::BlobUploadHistoryInspect, |r| {
        r.init_blob_upload_history_query().set_submission(&[92; 32]);
    });
    private_reply(&reply, |r| assert!(!r.get_error().unwrap().is_empty()));
    assert!(
        w.list_blob_upload_history(None, 16).is_err(),
        "history queries did not restore the retained owner"
    );
    assert_eq!(read(&mut w, key).outcome, Ok(Phase::Staged));
    assert_eq!(
        w.inspect_persisted_blob_upload(&[92; 32])
            .unwrap()
            .record
            .revision(),
        2
    );
    w.acknowledge_io(key).unwrap();
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(false));
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "requires explicit original metadata fault and a fresh synthetic store/process"]
fn original_upload_metadata_completion_is_single_use_and_owner_bound() {
    use morrow_workbench_host::io_tasks::blob_upload::MetadataPhase;
    let point = std::env::var("MORROW_UPLOAD_JOURNAL_FAILURE").unwrap();
    assert!(
        ["observe-before", "observe-after", "ack-before", "ack-after"].contains(&point.as_str())
    );
    let mode = std::env::var("MORROW_METADATA_COMPLETION_MODE").unwrap_or_else(|_| "normal".into());
    assert!(["normal", "cancel", "gate"].contains(&mode.as_str()));
    let fault = std::env::var("MORROW_UPLOAD_METADATA_COMPLETION_FAILURE").ok();
    assert!(
        fault
            .as_deref()
            .is_none_or(|v| ["before", "after"].contains(&v))
    );
    let root = root(&format!(
        "complete-{point}-{mode}-{}",
        fault.as_deref().unwrap_or("none")
    ));
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 95, false);
    let key = w.start_blob_upload(start).unwrap();
    wait(&mut w, key);
    if mode == "cancel" {
        assert!(point.starts_with("observe-"));
        w.cancel_io(key).unwrap();
    }
    let delivered = read(&mut w, key);
    assert_eq!(delivered.unknown, mode == "cancel");
    assert!(w.acknowledge_io(key).is_err());
    let before = w.inspect_persisted_blob_upload(&[95; 32]).unwrap().record;
    let response = private_call(
        &mut w,
        private::Action::BlobUploadMetadataReview,
        |mut r| r.set_io_key(key.as_bytes()),
    );
    private_success(&response);
    let permit: [u8; 32] = private_reply(&response, |r| {
        let review = r.get_blob_upload_metadata_review().unwrap();
        assert_eq!(review.get_key().unwrap(), key.as_bytes());
        assert_eq!(review.get_submission().unwrap(), &[95; 32]);
        assert_eq!(
            review.get_phase(),
            if point.starts_with("ack-") { 2 } else { 1 }
        );
        assert_eq!(review.get_write_required(), point.ends_with("before"));
        review.get_permit().unwrap().try_into().unwrap()
    });
    let review = w.review_blob_upload_metadata(key).unwrap();
    assert_eq!(
        review.permit, permit,
        "review is read-only and repeats the same proposal"
    );
    assert_eq!(
        w.inspect_persisted_blob_upload(&[95; 32]).unwrap().record,
        before
    );
    let wrong_key = TaskKey::from_bytes(&[97; 32]).unwrap();
    assert!(w.review_blob_upload_metadata(wrong_key).is_err());
    assert!(w.complete_blob_upload_metadata(wrong_key, permit).is_err());
    assert!(w.complete_blob_upload_metadata(key, [0; 32]).is_err());
    let mut other = Workbench::open(&root.join("foreign.db"), None).unwrap();
    assert!(other.review_blob_upload_metadata(key).is_err());
    assert!(other.complete_blob_upload_metadata(key, permit).is_err());
    other.finish().unwrap();
    drop(other);
    if mode == "gate" {
        w.product_gate().revoke();
    }
    let response = private_call(
        &mut w,
        private::Action::BlobUploadMetadataComplete,
        |mut r| {
            r.set_io_key(key.as_bytes());
            r.set_blob_upload_metadata_permit(&permit);
        },
    );
    let failed = mode == "gate" || (fault.is_some() && review.write_required);
    private_reply(&response, |r| {
        assert_eq!(!r.get_error().unwrap().is_empty(), failed);
        if !failed {
            assert_eq!(
                r.get_blob_upload_metadata_completed_permit().unwrap(),
                &permit
            );
        }
    });
    assert!(
        w.complete_blob_upload_metadata(key, permit).is_err(),
        "the same completion never writes twice"
    );
    assert!(
        w.review_blob_upload_metadata(key).is_err(),
        "no replacement completion permit"
    );
    if failed {
        if mode != "gate" {
            let after = w.inspect_persisted_blob_upload(&[95; 32]).unwrap().record;
            assert_eq!(
                after.revision(),
                before.revision() + u32::from(fault.as_deref() == Some("after"))
            );
            if fault.as_deref() == Some("before") {
                assert_eq!(after, before);
            }
        }
        assert!(w.finish().is_err());
        assert!(Workbench::open(&database, None).is_err());
        println!("metadata completion remains retained; no successful product close claimed");
    } else {
        if review.phase == MetadataPhase::Observation {
            assert_eq!(w.poll_blob_upload(key).unwrap().task.key, Some(key));
            assert!(
                w.read_blob_upload(key).is_err(),
                "no second original result delivery"
            );
            if mode == "cancel" {
                assert!(w.poll_blob_upload(key).unwrap().recovery_required);
                assert!(
                    w.acknowledge_io(key).is_err(),
                    "metadata does not approve an Unknown delivery"
                );
                w.inspect_blob_upload_history(key).unwrap();
                w.confirm_blob_upload_recovery(key, Observation::Staged)
                    .unwrap();
                assert!(delivered.unknown);
            }
            w.acknowledge_io(key).unwrap();
        } else {
            private_reply(&response, |r| {
                assert!(r.get_blob_upload_metadata_acknowledged());
                assert!(r.get_io_state().unwrap().get_key().unwrap().is_empty());
            });
        }
        assert!(w.io_status().key.is_none());
        let record = w.inspect_persisted_blob_upload(&[95; 32]).unwrap().record;
        assert_eq!(record.revision(), 3);
        assert_eq!(record.original_result(), Some(review.original));
        assert_eq!(record.observed_at_unix_ms(), Some(review.observed_at));
        if review.phase == MetadataPhase::Acknowledgement {
            assert_eq!(record.acknowledged_at_unix_ms(), review.acknowledgement_at);
        }
        assert_eq!(w.list_blob_upload_history(None, 16).unwrap().len(), 1);
        w.finish().unwrap();
    }
    // All negative cases above own synthetic fixtures whose workers joined;
    // retained metadata debt is not reported as a successful product closure.
    drop(w);
    history(&database, &s, Some(false));
    let mut reopened = Workbench::open(&database, None).unwrap();
    assert!(reopened.review_blob_upload_metadata(key).is_err());
    assert!(reopened.complete_blob_upload_metadata(key, permit).is_err());
    assert!(
        reopened.io_status().key.is_none(),
        "saved history cannot revive authority"
    );
    reopened.finish().unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "requires an explicit observe-after fault and a fresh synthetic store/process"]
fn original_metadata_read_reconciliation_invalidates_old_completion_review() {
    assert_eq!(
        std::env::var("MORROW_UPLOAD_JOURNAL_FAILURE").as_deref(),
        Ok("observe-after")
    );
    let root = root("metadata-review-invalidated");
    let database = root.join("original.db");
    let mut w = Workbench::open(&database, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 96, false);
    let key = w.start_blob_upload(start).unwrap();
    assert_eq!(read(&mut w, key).outcome, Ok(Phase::Staged));
    let review = w.review_blob_upload_metadata(key).unwrap();
    assert!(!review.write_required);
    let before = w.inspect_persisted_blob_upload(&[96; 32]).unwrap().record;
    w.repair_io(key).unwrap();
    assert!(
        w.complete_blob_upload_metadata(key, review.permit).is_err(),
        "a review made before explicit reconciliation must be invalidated"
    );
    assert_eq!(
        w.inspect_persisted_blob_upload(&[96; 32]).unwrap().record,
        before
    );
    w.acknowledge_io(key).unwrap();
    assert!(w.complete_blob_upload_metadata(key, review.permit).is_err());
    assert_eq!(
        w.inspect_persisted_blob_upload(&[96; 32])
            .unwrap()
            .record
            .revision(),
        3
    );
    w.finish().unwrap();
    drop(w);
    history(&database, &s, Some(false));
}
// Append only to the selected protected upload integration fixture.
#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "explicit fresh managed-library child with one abrupt commit boundary"]
fn protected_upload_history_commit_crash_child() {
    let root = PathBuf::from(std::env::var_os("MORROW_UPLOAD_CRASH_ROOT").unwrap());
    assert!(root.is_absolute() && root.starts_with("D:/build_cache") && root.is_dir());
    assert!(
        fs::read_dir(&root)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "expected.txt"),
        "fresh fixture only; no replay"
    );
    let point = std::env::var("MORROW_TEST_CRASH_AT").unwrap();
    assert!(["blob-stage-before-commit", "blob-stage-after-commit"].contains(&point.as_str()));
    let mut w = Workbench::open_managed(&root, None).unwrap();
    let p = package("RUST", FUEL);
    install(&mut w, &root, &p, true);
    let s = spec(41);
    let start = options(&w, &p, &s, 93, false);
    let key = w.start_blob_upload(start).unwrap();
    wait(&mut w, key);
    panic!("explicit abrupt commit boundary was not reached");
}

#[cfg(feature = "fault-injection")]
#[test]
fn protected_abrupt_commit_keeps_missing_original_result_uncertain_after_reopen() {
    for (point, staged) in [
        ("blob-stage-before-commit", false),
        ("blob-stage-after-commit", true),
    ] {
        let root = root(&format!("crash-history-{point}"));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "protected_upload_history_commit_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_UPLOAD_CRASH_ROOT", &root)
            .env("MORROW_TEST_CRASH_AT", point)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(86), "{point}: {child:?}");
        // The held original child actually exited. This is a fresh read owner,
        // never a resumed worker, restored TaskKey or historical live owner.
        let mut w = Workbench::open_managed(&root, None).unwrap();
        let rows = w.list_blob_upload_history(None, 16).unwrap();
        assert_eq!(rows.len(), 1);
        let original = rows[0].clone();
        assert_eq!(original.request().submission, [93; 32]);
        assert_eq!(original.revision(), 1);
        assert!(original.original_unknown());
        assert!(original.original_result().is_none());
        assert!(original.acknowledged_at_unix_ms().is_none());
        let actual = w.inspect_persisted_blob_upload(&[93; 32]).unwrap();
        assert_eq!(
            actual.current_stage,
            if staged {
                Observation::Staged
            } else {
                Observation::Absent
            }
        );
        assert_eq!(
            actual.record, original,
            "current blob presence cannot upgrade the original result"
        );
        assert!(w.io_status().key.is_none());
        w.finish().unwrap();
        drop(w);
        let database = morrow_audit::library::Registry::open(&root)
            .unwrap()
            .selected_database()
            .unwrap();
        history(&database, &spec(41), staged.then_some(false));
        eprintln!(
            "PROTECTED_UPLOAD_CRASH_HISTORY point={point} actual_original_exit=86 historical_revision=1 historical_unknown=true new_workers=0"
        );
    }
}
