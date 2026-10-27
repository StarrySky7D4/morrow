//! Actual private binary controls against fresh protected stores and pinned
//! three-language guests. This does not assert that a rendered user reviewed it.
use super::*;
use crate::{host_capnp as wire, protocol};

fn send(
    app: &mut Workbench,
    action: wire::Action,
    configure: impl FnOnce(wire::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    configure(r);
    protocol::respond(app, &capnp::serialize::write_message_to_words(&message)).unwrap()
}
fn read<T>(bytes: &[u8], inspect: impl FnOnce(wire::response::Reader<'_>) -> T) -> T {
    let mut bytes = bytes;
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut bytes, Default::default()).unwrap();
    assert!(bytes.is_empty());
    let r = message.get_root::<wire::response::Reader>().unwrap();
    inspect(r)
}
fn ok<T>(bytes: &[u8], inspect: impl FnOnce(wire::response::Reader<'_>) -> T) -> T {
    read(bytes, |r| {
        assert!(r.get_error().unwrap().is_empty(), "private reply error");
        inspect(r)
    })
}
fn command(app: &mut Workbench, action: wire::Action, key: PlanKey) -> Vec<u8> {
    send(app, action, |mut r| r.set_blob_plan_key(key.as_bytes()))
}
fn start_frame(s: &mut Setup, submission: &[u8], length: usize, timeout: u32) -> Vec<u8> {
    let digest = s.package.digest();
    let rev = revision(&s.app);
    send(&mut s.app, wire::Action::BlobPlanStart, |r| {
        let mut v = r.init_blob_plan_start();
        v.set_submission(submission);
        v.set_package_id(ID);
        v.set_package_digest(&digest);
        v.set_registry_revision(rev);
        v.set_handler("blob.plan");
        v.set_input_type(INPUT_TYPE);
        v.set_output_type(OUTPUT_TYPE);
        v.set_input(&if length == 0 {
            Options::new(150017, 73).unwrap().encode()
        } else {
            vec![0; length]
        });
        v.set_timeout_ms(timeout);
    })
}
fn start(s: &mut Setup) -> PlanKey {
    ok(&start_frame(s, &[1; 32], 0, 120000), |r| {
        let p = r.get_blob_plan_state().unwrap();
        assert!(p.get_found());
        assert!(!p.get_consumed());
        assert!(p.get_upload_key().unwrap().is_empty());
        assert_eq!(p.get_submission().unwrap(), &[1; 32]);
        PlanKey::from_bytes(p.get_key().unwrap()).unwrap()
    })
}
fn wait(s: &mut Setup, key: PlanKey) {
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        let ready = ok(
            &command(&mut s.app, wire::Action::BlobPlanStatus, key),
            |r| {
                let p = r.get_blob_plan_state().unwrap();
                assert_eq!(p.get_key().unwrap(), key.as_bytes());
                assert_eq!(p.get_error(), 0);
                if p.get_ready() {
                    assert!(p.get_original_worker_joined());
                }
                p.get_ready()
            },
        );
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "retain original planning owner; no replay"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn view(s: &mut Setup, key: PlanKey) -> PlanView {
    wait(s, key);
    ok(&command(&mut s.app, wire::Action::BlobPlanRead, key), |r| {
        let result = r.get_blob_plan_result().unwrap();
        assert_eq!(result.get_kind(), 1);
        assert_eq!(result.get_error(), 0);
        assert!(result.get_has_execution());
        assert_eq!(result.get_exit_code(), 0);
        assert_eq!(result.get_execution_fault(), 0);
        assert_eq!(result.get_host_calls(), 0);
        let v = result.get_view().unwrap();
        PlanView {
            package_id: v.get_package_id().unwrap().to_str().unwrap().into(),
            package_digest: v.get_package_digest().unwrap().try_into().unwrap(),
            registry_revision: v.get_registry_revision(),
            planner_handler: v.get_planner_handler().unwrap().to_str().unwrap().into(),
            upload_handler: v.get_upload_handler().unwrap().to_str().unwrap().into(),
            planning_input_sha256: v.get_planning_input_sha256().unwrap().try_into().unwrap(),
            label: v.get_label().unwrap().to_str().unwrap().into(),
            byte_length: v.get_byte_length(),
            whole_sha256: v.get_whole_sha256().unwrap().try_into().unwrap(),
            proposal_sha256: v.get_proposal_sha256().unwrap().try_into().unwrap(),
        }
    })
}
fn approve(s: &mut Setup, key: PlanKey, view: &PlanView) -> Vec<u8> {
    send(&mut s.app, wire::Action::BlobPlanApprove, |mut r| {
        r.set_blob_plan_key(key.as_bytes());
        let mut v = r.init_blob_plan_approval();
        v.set_package_digest(&view.package_digest);
        v.set_registry_revision(view.registry_revision);
        v.set_proposal_sha256(&view.proposal_sha256);
    })
}
fn discover(s: &mut Setup) -> Vec<u8> {
    send(&mut s.app, wire::Action::BlobPlanDiscover, |mut r| {
        r.set_blob_plan_submission(&[1; 32])
    })
}
fn accepted(language: &str) {
    let mut s = Setup::new(&format!("private-{language}"), language, true);
    let catalog = send(&mut s.app, wire::Action::PluginCatalog, |_| {});
    ok(&catalog, |r| {
        let p = r
            .get_plugins()
            .unwrap()
            .iter()
            .find(|p| p.get_package_id().unwrap().to_str().unwrap() == ID)
            .unwrap();
        let mappings = p.get_blob_planners().unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(
            mappings.get(0).get_handler().unwrap().to_str().unwrap(),
            "blob.plan"
        );
        assert_eq!(
            mappings
                .get(0)
                .get_upload_handler()
                .unwrap()
                .to_str()
                .unwrap(),
            "blob.upload"
        );
    });
    let key = start(&mut s);
    let v = view(&mut s, key);
    assert_eq!(v.package_digest, s.package.digest());
    assert_eq!(v.label, LABEL);
    assert_eq!(v.byte_length, 150017);
    assert_eq!(
        v.planning_input_sha256,
        <[u8; 32]>::from(Sha256::digest(Options::new(150017, 73).unwrap().encode()))
    );
    ok(&command(&mut s.app, wire::Action::BlobPlanRead, key), |r| {
        assert_eq!(r.get_blob_plan_result().unwrap().get_kind(), 0)
    });
    // The caller deliberately loses this reply, while native admission happens
    // exactly once. Discover the host-generated identity through Cap'n Proto.
    drop(approve(&mut s, key, &v));
    let (upload, submission) = ok(&discover(&mut s), |r| {
        let p = r.get_blob_plan_state().unwrap();
        assert!(p.get_consumed());
        assert!(!p.get_submission_failed());
        (
            TaskKey::from_bytes(p.get_upload_key().unwrap()).unwrap(),
            <[u8; 32]>::try_from(p.get_upload_submission().unwrap()).unwrap(),
        )
    });
    assert_ne!(submission, [0; 32]);
    assert_ne!(submission, [1; 32]);
    read(&approve(&mut s, key, &v), |r| {
        assert!(!r.get_error().unwrap().is_empty())
    });
    let spec = s
        .app
        .upload_plans
        .attempt
        .as_ref()
        .unwrap()
        .submitted_spec
        .clone()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        let ready = ok(
            &send(&mut s.app, wire::Action::BlobUploadStatus, |mut r| {
                r.set_io_key(upload.as_bytes())
            }),
            |r| {
                assert_eq!(
                    r.get_io_state().unwrap().get_submission().unwrap(),
                    &submission
                );
                let ready = r.get_blob_upload_state().unwrap().get_ready();
                if ready {
                    assert!(r.get_io_state().unwrap().get_has_exit());
                }
                ready
            },
        );
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "retain original upload; no replay"
        );
        thread::sleep(Duration::from_millis(5));
    }
    ok(
        &send(&mut s.app, wire::Action::BlobUploadRead, |mut r| {
            r.set_io_key(upload.as_bytes())
        }),
        |r| {
            let result = r.get_blob_upload_result().unwrap();
            assert_eq!(result.get_kind(), 1);
            assert_eq!(result.get_phase(), 2);
            assert!(!result.get_unknown());
            assert_eq!(result.get_exit_code(), 0);
            assert_eq!(result.get_execution_fault(), 0);
        },
    );
    ok(
        &send(&mut s.app, wire::Action::IoAcknowledge, |mut r| {
            r.set_io_key(upload.as_bytes())
        }),
        |_| {},
    );
    ok(&discover(&mut s), |r| {
        let p = r.get_blob_plan_state().unwrap();
        assert_eq!(p.get_upload_key().unwrap(), upload.as_bytes());
        assert_eq!(p.get_upload_submission().unwrap(), &submission);
    });
    ok(
        &command(&mut s.app, wire::Action::BlobPlanAcknowledge, key),
        |r| assert!(!r.get_blob_plan_state().unwrap().get_found()),
    );
    ok(&discover(&mut s), |r| {
        assert!(!r.get_blob_plan_state().unwrap().get_found())
    });
    read(&start_frame(&mut s, &[1; 32], 0, 120000), |r| {
        assert!(!r.get_error().unwrap().is_empty())
    });
    s.app.finish().unwrap();
    drop(s.app);
    let session = Session::open(
        &s.root.join("original.db"),
        Default::default(),
        OpenMode::Existing,
    )
    .unwrap();
    let store = session.runtime_ref().store_local();
    let record = store
        .inspect_blob_stage_local(&spec.command())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    store
        .export_blob_local(&record.blob().id, &mut bytes)
        .unwrap();
    assert_eq!(bytes, vec![73; 150017]);
    assert_eq!(Sha256::digest(&bytes).as_slice(), v.whole_sha256);
    store.integrity_check().unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    eprintln!(
        "PRIVATE_PLAN_ACCEPTED language={language} bytes=150017 plan_join=true upload_join=true missing_approval_reply_discovered=true protected_reopen=true"
    );
}
#[test]
fn rust_original_private_controls() {
    accepted("RUST");
}
#[test]
fn c_original_private_controls() {
    accepted("C");
}
#[test]
fn cpp_original_private_controls() {
    accepted("CPP");
}

#[test]
fn invalid_private_start_never_reserves_the_planning_nonce() {
    let mut s = Setup::new("private-invalid", "RUST", true);
    for (nonce, length, timeout) in [
        (vec![0; 32], 0, 120000),
        (vec![1; 31], 0, 120000),
        (vec![1; 32], 32769, 120000),
        (vec![1; 32], 0, 0),
    ] {
        read(&start_frame(&mut s, &nonce, length, timeout), |r| {
            assert!(!r.get_error().unwrap().is_empty())
        });
        assert!(s.app.upload_plans.attempt.is_none());
        assert!(s.app.io_status().key.is_none());
    }
    let key = start(&mut s);
    wait(&mut s, key);
    ok(
        &command(&mut s.app, wire::Action::BlobPlanCancel, key),
        |r| assert_eq!(r.get_blob_plan_state().unwrap().get_error(), 1),
    );
    ok(&command(&mut s.app, wire::Action::BlobPlanRead, key), |r| {
        let p = r.get_blob_plan_result().unwrap();
        assert_eq!(p.get_kind(), 2);
        assert_eq!(p.get_error(), 1);
        assert!(p.get_has_execution());
    });
    ok(
        &command(&mut s.app, wire::Action::BlobPlanAcknowledge, key),
        |_| {},
    );
    s.finish();
}
#[test]
fn lost_private_read_stays_consumed_and_can_only_observe_or_cancel() {
    let mut s = Setup::new("private-lost-read", "RUST", true);
    let key = start(&mut s);
    wait(&mut s, key);
    drop(command(&mut s.app, wire::Action::BlobPlanRead, key));
    ok(&discover(&mut s), |r| {
        assert!(r.get_blob_plan_state().unwrap().get_delivered())
    });
    ok(&command(&mut s.app, wire::Action::BlobPlanRead, key), |r| {
        assert_eq!(r.get_blob_plan_result().unwrap().get_kind(), 0)
    });
    ok(
        &command(&mut s.app, wire::Action::BlobPlanCancel, key),
        |_| {},
    );
    assert!(s.app.io_status().key.is_none());
    ok(
        &command(&mut s.app, wire::Action::BlobPlanAcknowledge, key),
        |_| {},
    );
    s.finish();
}
#[test]
fn failed_private_admission_preserves_original_consumed_submission() {
    let mut s = Setup::configured("private-admission-failed", "RUST", true, 1);
    let key = start(&mut s);
    let v = view(&mut s, key);
    read(&approve(&mut s, key, &v), |r| {
        assert!(!r.get_error().unwrap().is_empty())
    });
    let submission = ok(&discover(&mut s), |r| {
        let p = r.get_blob_plan_state().unwrap();
        assert!(p.get_consumed());
        assert!(p.get_submission_failed());
        assert!(p.get_upload_key().unwrap().is_empty());
        p.get_upload_submission().unwrap().to_vec()
    });
    assert_eq!(submission.len(), 32);
    assert!(s.app.io_status().key.is_none());
    read(&approve(&mut s, key, &v), |r| {
        assert!(!r.get_error().unwrap().is_empty())
    });
    ok(&discover(&mut s), |r| {
        assert_eq!(
            r.get_blob_plan_state()
                .unwrap()
                .get_upload_submission()
                .unwrap(),
            submission
        )
    });
    ok(
        &command(&mut s.app, wire::Action::BlobPlanAcknowledge, key),
        |_| {},
    );
    s.finish();
}
