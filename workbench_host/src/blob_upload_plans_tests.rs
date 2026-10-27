//! Opt-in actual Windows protected owners and independently pinned SDK guests.
use super::*;
use morrow_audit::session::{OpenMode, Session};
use morrow_blob_upload_proposal_v1::{MAX_BOUND_INPUT_BYTES, MAX_PROPOSAL_BYTES, OUTPUT_TYPE};
use morrow_blob_upload_request_v1::Phase;
use morrow_blob_upload_sample::{INPUT_TYPE, LABEL, Options};
use morrow_core::plugin_package::{Package, proto};
use std::{fs, path::PathBuf};
#[path = "blob_plan_protocol_tests.rs"]
mod private_controls;
const ID: &str = "org.example.product-planner";
struct Setup {
    app: Workbench,
    package: Package,
    root: PathBuf,
}
fn revision(app: &Workbench) -> u64 {
    app.catalog_page("", None).unwrap().revision
}
impl Setup {
    fn new(name: &str, language: &str, approved: bool) -> Self {
        Self::configured(name, language, approved, MAX_BOUND_INPUT_BYTES as u32)
    }
    fn configured(name: &str, language: &str, approved: bool, target_limit: u32) -> Self {
        assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
        let root = PathBuf::from(
            std::env::var_os("MORROW_PLANNER_PRODUCT_STORE_ROOT")
                .expect("fresh test root required"),
        );
        assert!(root.is_absolute() && root.starts_with("D:\\build_cache"));
        let root = root.join(name);
        fs::create_dir(&root).expect("fresh case only; no replay");
        let path =
            PathBuf::from(std::env::var_os(format!("MORROW_{language}_UPLOAD_WASM")).unwrap());
        assert!(path.is_absolute());
        let wasm = fs::read(path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&wasm)),
            std::env::var(format!("MORROW_{language}_UPLOAD_WASM_SHA256")).unwrap()
        );
        let mut handlers = vec![proto::TransformHandler {
            handler: "blob.plan".into(),
            input_type: INPUT_TYPE.into(),
            output_type: OUTPUT_TYPE.into(),
            max_input_bytes: 256,
            max_output_bytes: MAX_PROPOSAL_BYTES as u32,
        }];
        for name in ["blob.upload", "blob.upload.release"] {
            handlers.push(proto::TransformHandler {
                handler: name.into(),
                input_type: BOUND_INPUT_TYPE.into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: target_limit,
                max_output_bytes: 4096,
            });
        }
        let mut manifest = Package::manifest_for_blob_upload_proposals(
            ID,
            "1.0.0",
            &wasm,
            handlers,
            vec!["blob.upload".into(), "blob.upload.release".into()],
            vec![proto::BlobUploadProposalHandler {
                handler: "blob.plan".into(),
                upload_handler: "blob.upload".into(),
            }],
        );
        manifest.budget.as_mut().unwrap().fuel = limits().fuel;
        manifest.budget.as_mut().unwrap().host_calls = 1024;
        let package = Package::build(manifest, &wasm).unwrap();
        let mut app = Workbench::open(&root.join("original.db"), None).unwrap();
        let incoming = root.join("incoming.mplugin");
        fs::write(&incoming, package.archive()).unwrap();
        app.import_plugin(&incoming, &package.digest(), revision(&app))
            .unwrap();
        app.configure_external(ID, &package.digest(), revision(&app), &[], true)
            .unwrap();
        if approved {
            app.configure_external_blob_upload(ID, &package.digest(), revision(&app), true)
                .unwrap();
        }
        Self { app, package, root }
    }
    fn options(&self, length: u64) -> PlanStart {
        PlanStart {
            submission: [1; 32],
            package_id: ID.into(),
            digest: self.package.digest(),
            revision: revision(&self.app),
            input: Invocation::new_transform(
                "original-plan",
                Transform {
                    handler: "blob.plan".into(),
                    input_type: INPUT_TYPE.into(),
                    output_type: OUTPUT_TYPE.into(),
                    input: Options::new(length, 73).unwrap().encode(),
                },
            )
            .unwrap(),
            lifetime: Duration::from_secs(120),
        }
    }
    fn start(&mut self, length: u64) -> PlanKey {
        self.app
            .start_blob_upload_plan(self.options(length))
            .unwrap()
    }
    fn wait(&mut self, key: PlanKey) -> PlanSnapshot {
        let deadline = Instant::now() + Duration::from_secs(125);
        loop {
            let s = self.app.poll_blob_upload_plan(key).unwrap();
            if !s.running {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "retain original planner; no replay"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn view(&mut self, key: PlanKey) -> PlanView {
        assert!(self.wait(key).original_worker_joined);
        let result = self.app.read_blob_upload_plan(key).unwrap().unwrap();
        assert!(result.error.is_none());
        let report = result.execution.unwrap();
        assert_eq!(report.outcome, Ok(0));
        assert_eq!(report.host_calls, 0);
        result.view.unwrap()
    }
    fn finish(mut self) {
        self.app.finish().unwrap();
        drop(self.app);
        let session = Session::open(
            &self.root.join("original.db"),
            Default::default(),
            OpenMode::Existing,
        )
        .unwrap();
        session
            .runtime_ref()
            .store_local()
            .integrity_check()
            .unwrap();
        assert_eq!(
            session.runtime_ref().store_local().pending_usage().unwrap(),
            (0, 0)
        );
    }
}
fn approval(key: PlanKey, view: &PlanView) -> PlanApproval {
    PlanApproval {
        key,
        package_digest: view.package_digest,
        registry_revision: view.registry_revision,
        proposal_sha256: view.proposal_sha256,
    }
}
fn accepted(language: &str, length: u64) {
    let mut s = Setup::new(&format!("accepted-{language}-{length}"), language, true);
    let original = s.options(length);
    let input_sha256: [u8; 32] = Sha256::digest(&original.input.transform().unwrap().input).into();
    assert_ne!(
        input_sha256,
        <[u8; 32]>::from(Sha256::digest(original.input.bytes()))
    );
    let key = s.app.start_blob_upload_plan(original).unwrap();
    assert_eq!(
        s.app
            .discover_blob_upload_plan([1; 32])
            .unwrap()
            .unwrap()
            .key,
        key
    );
    assert!(s.app.discover_blob_upload_plan([2; 32]).unwrap().is_none());
    assert!(s.app.start_blob_upload_plan(s.options(length)).is_err());
    assert!(s.app.acknowledge_blob_upload_plan(key).is_err());
    assert!(s.app.poll_blob_upload_plan(PlanKey([99; 32])).is_err());
    assert!(Workbench::open(&s.root.join("original.db"), None).is_err());
    let view = s.view(key);
    assert_eq!(view.planning_input_sha256, input_sha256);
    assert_eq!(view.label, LABEL);
    assert_eq!(view.byte_length, length);
    assert_eq!(
        view.whole_sha256,
        Options::new(length, 73).unwrap().sha256()
    );
    assert!(s.app.io_status().key.is_none());
    assert!(s.app.read_blob_upload_plan(key).unwrap().is_none());
    let mut wrong = approval(key, &view);
    wrong.proposal_sha256[0] ^= 1;
    assert!(s.app.approve_blob_upload_plan(wrong).is_err());
    assert!(!s.app.poll_blob_upload_plan(key).unwrap().consumed);
    let upload = s
        .app
        .approve_blob_upload_plan(approval(key, &view))
        .unwrap();
    let spec = s
        .app
        .upload_plans
        .attempt
        .as_ref()
        .unwrap()
        .submitted_spec
        .as_ref()
        .unwrap()
        .clone();
    assert_ne!(spec.identity.transfer_epoch, [1; 32]);
    assert_ne!(spec.identity.transfer_epoch, spec.identity.object_ref);
    assert_ne!(spec.owner_domain, spec.identity.operation_id);
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    assert!(s.app.cancel_blob_upload_plan(key).is_err());
    let original = s.app.discover_blob_upload_plan([1; 32]).unwrap().unwrap();
    assert!(original.consumed);
    assert_eq!(original.upload_key, Some(upload));
    assert!(!original.submission_failed);
    let deadline = Instant::now() + Duration::from_secs(125);
    loop {
        let state = s.app.poll_blob_upload(upload).unwrap();
        if state.ready {
            let exit = state.task.exit.unwrap();
            assert_eq!(exit.disconnect, Ok(()));
            assert_eq!(exit.maintenance, Ok(()));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "retain original protected upload; no replay"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let result = s.app.read_blob_upload(upload).unwrap().unwrap();
    assert_eq!(result.outcome, Ok(Phase::Staged));
    assert!(!result.unknown);
    assert_eq!(result.usage.calls, length.div_ceil(61440) as u32 + 4);
    s.app.acknowledge_io(upload).unwrap();
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    assert!(s.app.start_blob_upload_plan(s.options(length)).is_err());
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
    assert!(!record.is_released());
    let mut bytes = Vec::new();
    store
        .export_blob_local(&record.blob().id, &mut bytes)
        .unwrap();
    assert_eq!(bytes, vec![73; length as usize]);
    assert_eq!(<[u8; 32]>::from(Sha256::digest(&bytes)), spec.sha256);
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
    eprintln!(
        "PRODUCT_PLAN_ACCEPTED language={language} bytes={length} original_plan_join=true original_upload_join=true exact_bytes_sha=true protected_reopen=true"
    );
}
macro_rules! accepted_case {
    ($name:ident,$language:literal,$length:expr) => {
        #[test]
        fn $name() {
            accepted($language, $length);
        }
    };
}
accepted_case!(rust_multichunk, "RUST", 150017);
accepted_case!(c_multichunk, "C", 150017);
accepted_case!(cpp_multichunk, "CPP", 150017);
accepted_case!(rust_maximum, "RUST", 16777216);
accepted_case!(c_maximum, "C", 16777216);
accepted_case!(cpp_maximum, "CPP", 16777216);
#[test]
fn pure_planning_does_not_require_or_create_upload_category_approval() {
    let mut s = Setup::new("category", "RUST", false);
    let key = s.start(7);
    let view = s.view(key);
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    assert!(s.app.io_status().key.is_none());
    s.app
        .configure_external_blob_upload(ID, &s.package.digest(), revision(&s.app), true)
        .unwrap();
    assert_eq!(
        s.app.poll_blob_upload_plan(key).unwrap().error,
        Some(PlanError::StaleSelection)
    );
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn cancel_after_original_join_vetoes_delivery_without_rewriting_execution() {
    let mut s = Setup::new("cancel-read", "C", true);
    let key = s.start(7);
    s.wait(key);
    s.app.cancel_blob_upload_plan(key).unwrap();
    let result = s.app.read_blob_upload_plan(key).unwrap().unwrap();
    assert_eq!(result.error, Some(PlanError::Cancelled));
    assert!(result.view.is_none());
    assert_eq!(result.execution.unwrap().outcome, Ok(0));
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn expire_after_original_join_vetoes_approval() {
    let mut s = Setup::new("expire", "CPP", true);
    let key = s.start(7);
    let view = s.view(key);
    s.app.upload_plans.attempt.as_mut().unwrap().deadline = Instant::now();
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    assert_eq!(
        s.app.poll_blob_upload_plan(key).unwrap().error,
        Some(PlanError::Expired)
    );
    assert!(s.app.io_status().key.is_none());
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn replaced_or_closed_controller_never_renews_original_plan() {
    for closed in [false, true] {
        let mut s = Setup::new(
            if closed {
                "gate-closed"
            } else {
                "gate-replaced"
            },
            "RUST",
            true,
        );
        let key = s.start(7);
        let view = s.view(key);
        if closed {
            s.app.product_gate().revoke();
        } else {
            s.app.bind_product_gate(ProductGate::default());
        }
        assert_eq!(
            s.app.poll_blob_upload_plan(key).unwrap().error,
            Some(PlanError::GateClosed)
        );
        assert!(
            s.app
                .approve_blob_upload_plan(approval(key, &view))
                .is_err()
        );
        assert!(s.app.io_status().key.is_none());
        s.app.acknowledge_blob_upload_plan(key).unwrap();
        s.finish();
    }
}
#[test]
fn spawn_failure_has_no_worker_or_upload_and_submission_is_not_reused() {
    let mut s = Setup::new("spawn-failure", "RUST", true);
    FAIL_SPAWN.set(true);
    let key = s.start(7);
    let snapshot = s.app.poll_blob_upload_plan(key).unwrap();
    assert!(!snapshot.running && !snapshot.original_worker_joined);
    let result = s.app.read_blob_upload_plan(key).unwrap().unwrap();
    assert_eq!(result.error, Some(PlanError::Spawn));
    assert!(result.view.is_none() && result.execution.is_none());
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    assert!(s.app.start_blob_upload_plan(s.options(7)).is_err());
    s.finish();
}
#[test]
fn finish_cancels_and_requires_the_original_background_join() {
    let mut s = Setup::new("finish-running", "CPP", true);
    let (release, pause) = std::sync::mpsc::channel();
    HOLD_WORKER.set(Some(pause));
    let key = s.start(16777216);
    assert!(s.app.finish().is_err());
    assert!(Workbench::open(&s.root.join("original.db"), None).is_err());
    release.send(()).unwrap();
    let snapshot = s.wait(key);
    assert!(snapshot.original_worker_joined);
    assert_eq!(snapshot.error, Some(PlanError::Cancelled));
    let result = s.app.read_blob_upload_plan(key).unwrap().unwrap();
    assert!(result.view.is_none());
    assert!(s.app.io_status().key.is_none());
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn cancellation_before_read_and_unreviewed_approval_are_distinct() {
    let mut s = Setup::new("unreviewed", "RUST", true);
    let key = s.start(7);
    s.wait(key);
    let view = s.app.upload_plans.attempt.as_ref().unwrap().view().unwrap();
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    assert!(!s.app.poll_blob_upload_plan(key).unwrap().consumed);
    s.app.cancel_blob_upload_plan(key).unwrap();
    s.app.read_blob_upload_plan(key).unwrap().unwrap();
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn failed_upload_admission_consumes_only_the_original_decision_without_retry() {
    let mut s = Setup::configured("admission-failed", "C", true, 1);
    let key = s.start(7);
    let view = s.view(key);
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    let snapshot = s.app.poll_blob_upload_plan(key).unwrap();
    assert!(snapshot.consumed && snapshot.submission_failed);
    assert!(snapshot.upload_key.is_none());
    assert!(s.app.io_status().key.is_none());
    assert!(
        s.app
            .approve_blob_upload_plan(approval(key, &view))
            .is_err()
    );
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
#[test]
fn malformed_options_end_the_original_planner_without_store_admission() {
    let mut s = Setup::new("malformed-options", "RUST", true);
    let mut start = s.options(7);
    start.input = Invocation::new_transform(
        "original-plan",
        Transform {
            handler: "blob.plan".into(),
            input_type: INPUT_TYPE.into(),
            output_type: OUTPUT_TYPE.into(),
            input: vec![1],
        },
    )
    .unwrap();
    let key = s.app.start_blob_upload_plan(start).unwrap();
    assert!(s.wait(key).original_worker_joined);
    let result = s.app.read_blob_upload_plan(key).unwrap().unwrap();
    assert_eq!(result.error, Some(PlanError::Execution));
    assert!(result.view.is_none());
    assert_eq!(
        result.execution.unwrap().outcome,
        Err(morrow_plugin_runtime::Fault::TaskProtocol)
    );
    assert!(s.app.io_status().key.is_none());
    s.app.acknowledge_blob_upload_plan(key).unwrap();
    s.finish();
}
