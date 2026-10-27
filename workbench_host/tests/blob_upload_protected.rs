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
