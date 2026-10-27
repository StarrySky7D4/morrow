//! Actual separately compiled three-language planners and original native workers.
//! Synthetic bytes and exact UTF-8; a test approval is not a rendered approval.
#![cfg(all(feature = "blob-upload-guest-qualification", windows))]
use morrow_blob_transfer_v1::{Identity, Limits as FrameLimits};
use morrow_blob_upload_proposal_v1::{
    BOUND_INPUT_TYPE, BoundInput, MAX_BOUND_INPUT_BYTES, MAX_PROPOSAL_BYTES, OUTPUT_TYPE, Proposal,
};
use morrow_blob_upload_request_v1::Phase;
use morrow_blob_upload_sample::{
    INPUT_TYPE, LABEL, MAX_TEXT_BYTES, Options, TEXT_INPUT_TYPE, TEXT_LABEL,
};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner,
    blob_upload::Spec,
    blob_upload_binding::{ApprovalRequest, UploadApproval},
    blob_upload_worker::{Error, Poll, UploadWorker},
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
    package::PreparedPackage,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
const ID: &str = "org.example.planned-upload";
fn limits() -> Limits {
    Limits {
        fuel: morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL,
        host_calls: 1024,
        ..Limits::default()
    }
}
struct Owner {
    host: HostRuntime,
    manager: Manager,
    prepared: u32,
    finished: u32,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.prepared += 1;
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.finished += 1;
        Ok(())
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}
fn package(language: &str) -> Package {
    package_for(language, false)
}
fn package_for(language: &str, text: bool) -> Package {
    let path = PathBuf::from(
        std::env::var_os(format!("MORROW_{language}_UPLOAD_WASM")).expect("fresh guest required"),
    );
    assert!(path.is_absolute());
    let bytes = fs::read(path).unwrap();
    let expected = std::env::var(format!("MORROW_{language}_UPLOAD_WASM_SHA256"))
        .expect("independent pin required");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected);
    assert!(Runner::new_blob_upload_task(&bytes, limits()).is_ok());
    assert!(matches!(
        Runner::new_task(&bytes, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let planner_name = if text { "blob.plan.text" } else { "blob.plan" };
    let upload_name = if text {
        "blob.upload.text"
    } else {
        "blob.upload"
    };
    let release_name = if text {
        "blob.upload.text.release"
    } else {
        "blob.upload.release"
    };
    let mut handlers = vec![proto::TransformHandler {
        handler: planner_name.into(),
        input_type: if text { TEXT_INPUT_TYPE } else { INPUT_TYPE }.into(),
        output_type: OUTPUT_TYPE.into(),
        max_input_bytes: if text { MAX_TEXT_BYTES as u32 } else { 256 },
        max_output_bytes: MAX_PROPOSAL_BYTES as u32,
    }];
    for name in [upload_name, release_name] {
        handlers.push(proto::TransformHandler {
            handler: name.into(),
            input_type: BOUND_INPUT_TYPE.into(),
            output_type: "upload.result.v1".into(),
            max_input_bytes: MAX_BOUND_INPUT_BYTES as u32,
            max_output_bytes: 4096,
        });
    }
    let mut manifest = Package::manifest_for_blob_upload_proposals(
        ID,
        "1.0.0",
        &bytes,
        handlers,
        vec![upload_name.into(), release_name.into()],
        vec![proto::BlobUploadProposalHandler {
            handler: planner_name.into(),
            upload_handler: upload_name.into(),
        }],
    );
    let budget = manifest.budget.as_mut().unwrap();
    budget.fuel = limits().fuel;
    budget.host_calls = limits().host_calls;
    eprintln!(
        "PLANNER_GUEST language={language} sha256={expected} bytes={}",
        bytes.len()
    );
    Package::build(manifest, &bytes).unwrap()
}
fn plan_input(bytes: Vec<u8>) -> Invocation {
    plan_input_for(bytes, false)
}
fn plan_input_for(bytes: Vec<u8>, text: bool) -> Invocation {
    Invocation::new_transform(
        "planned-object",
        Transform {
            handler: if text { "blob.plan.text" } else { "blob.plan" }.into(),
            input_type: if text { TEXT_INPUT_TYPE } else { INPUT_TYPE }.into(),
            output_type: OUTPUT_TYPE.into(),
            input: bytes,
        },
    )
    .unwrap()
}
fn invoke(mut owner: Owner, spec: &Spec, bound: &BoundInput, release: bool, text: bool) -> Owner {
    let instance = owner
        .manager
        .connect_blob_upload(ID, &mut owner.host, limits())
        .unwrap();
    let approval = UploadApproval::issue(
        &owner.manager,
        &owner.host,
        &instance,
        ApprovalRequest {
            expected_revision: owner.manager.revision(),
            nomination_ref: spec.identity.transfer_epoch,
            spec: spec.clone(),
            lifetime: Duration::from_secs(120),
        },
    )
    .unwrap();
    let input = Invocation::new_transform(
        if release {
            "release-planned"
        } else {
            "stage-planned"
        },
        Transform {
            handler: match (text, release) {
                (true, true) => "blob.upload.text.release",
                (true, false) => "blob.upload.text",
                (false, true) => "blob.upload.release",
                (false, false) => "blob.upload",
            }
            .into(),
            input_type: BOUND_INPUT_TYPE.into(),
            output_type: "upload.result.v1".into(),
            input: bound.wire().to_vec(),
        },
    )
    .unwrap();
    let mut worker = UploadWorker::spawn_owned(
        owner,
        instance,
        approval,
        input,
        if release { 13 } else { 7 },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(125);
    while worker.poll() == Poll::Running {
        assert!(
            Instant::now() < deadline,
            "retain original worker and evidence; no replay"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let exit = worker.try_join().unwrap().unwrap();
    assert_eq!(worker.poll(), Poll::Consumed);
    assert!(matches!(worker.try_join(), Err(Error::Consumed)));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert!(exit.instance.is_none());
    assert!(!exit.unknown);
    let actual = exit.result.as_ref().unwrap();
    assert_eq!(actual.execution.outcome, Ok(0));
    assert_eq!(
        actual.phase,
        if release {
            Phase::Released
        } else {
            Phase::Staged
        }
    );
    assert_eq!(
        exit.usage.calls,
        if release {
            2
        } else {
            spec.byte_length.div_ceil(61440) as u32 + 4
        }
    );
    eprintln!(
        "PLANNER_WORKER_RETURN bytes={} release={release} calls={} fuel_remaining={} unknown=false original_join=true",
        spec.byte_length, exit.usage.calls, actual.execution.fuel_remaining
    );
    exit.owner
}
fn run(language: &str, length: usize, fill: u8) {
    run_payload(
        language,
        &format!("{language}-{length}-{fill}"),
        vec![fill; length],
        Options::new(length as u64, fill).unwrap().encode(),
        false,
    );
}
fn run_text(language: &str, name: &str, payload: Vec<u8>) {
    run_payload(
        language,
        &format!("text-{language}-{name}"),
        payload.clone(),
        payload,
        true,
    );
}
fn run_payload(language: &str, name: &str, payload: Vec<u8>, producer_input: Vec<u8>, text: bool) {
    let length = payload.len();
    let root = PathBuf::from(
        std::env::var_os("MORROW_PLANNER_GUEST_STORE_ROOT").expect("fresh evidence root required"),
    );
    assert!(root.is_absolute());
    let root = root.join(name);
    fs::create_dir(&root).expect("new case only; no replay");
    let package = package_for(language, text);
    let input = plan_input_for(producer_input.clone(), text);
    // There is deliberately no Store/Manager at this stage: this is pure planning.
    let planner = PreparedPackage::new(package, limits()).unwrap();
    let package = planner.package();
    let report = planner.run_blob_upload_proposal(
        &input,
        Cancellation::until(Instant::now() + Duration::from_secs(120)),
    );
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 0);
    assert!(report.failure.is_none());
    let proposal = report.proposal.unwrap();
    let digest: [u8; 32] = Sha256::digest(&payload).into();
    assert_eq!(
        proposal,
        Proposal::new(
            if text { TEXT_LABEL } else { LABEL },
            length as u64,
            digest,
            &producer_input
        )
        .unwrap()
    );
    eprintln!(
        "PLANNER_RETURN language={language} bytes={length} text={text} upload_calls=0 fuel_remaining={} proposal_sha256={:x}",
        report.execution.fuel_remaining,
        Sha256::digest(proposal.wire())
    );
    let spec = Spec {
        owner_domain: [72; 32],
        identity: Identity {
            transfer_epoch: [71; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: proposal.byte_length(),
        sha256: proposal.whole_sha256(),
        limits: FrameLimits::default(),
    };
    let bound = BoundInput::new(&proposal, spec.identity).unwrap();
    let catalog = Catalog::open(&root.join("packages")).unwrap();
    catalog.install(package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&root.join("registry"), catalog).unwrap(),
        limits(),
    );
    manager.select(package, 0).unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&root.join("original"), Default::default()).unwrap()).unwrap();
    assert!(
        manager
            .connect_blob_upload(ID, &mut host, limits())
            .is_err()
    );
    assert!(
        host.store_local()
            .inspect_blob_stage_local(&spec.command())
            .unwrap()
            .is_none()
    );
    manager
        .approve_blob_upload(ID, package.digest(), true, manager.revision())
        .unwrap();
    let owner = Owner {
        host,
        manager,
        prepared: 0,
        finished: 0,
    };
    let owner = invoke(owner, &spec, &bound, false, text);
    let staged = owner
        .host
        .store_local()
        .inspect_blob_stage_local(&spec.command())
        .unwrap()
        .unwrap();
    assert!(!staged.is_released());
    let mut stored = Vec::new();
    owner
        .host
        .store_local()
        .export_blob_local(&staged.blob().id, &mut stored)
        .unwrap();
    assert_eq!(stored, payload);
    assert_eq!(<[u8; 32]>::from(Sha256::digest(&stored)), digest);
    let owner = invoke(owner, &spec, &bound, true, text);
    assert_eq!((owner.prepared, owner.finished), (2, 2));
    let released = owner
        .host
        .store_local()
        .inspect_blob_stage_local(&spec.command())
        .unwrap()
        .unwrap();
    assert!(released.is_released());
    assert_eq!(released.blob(), staged.blob());
    owner.host.store_local().integrity_check().unwrap();
    drop(owner);
    let reopened = Store::open_existing(&root.join("original"), Default::default()).unwrap();
    assert_eq!(
        reopened
            .inspect_blob_stage_local(&spec.command())
            .unwrap()
            .unwrap(),
        released
    );
    reopened.integrity_check().unwrap();
    eprintln!(
        "PLANNER_ACCEPTED language={language} bytes={length} text={text} original_workers_joined=2 exact_bytes_sha=true durable_release=true"
    );
}
macro_rules! case {
    ($name:ident,$language:literal,$length:expr,$fill:expr) => {
        #[test]
        fn $name() {
            run($language, $length, $fill);
        }
    };
}
case!(rust_empty, "RUST", 0, 0);
case!(rust_multichunk, "RUST", 150017, 73);
case!(rust_16mib, "RUST", 16777216, 219);
case!(c_empty, "C", 0, 0);
case!(c_multichunk, "C", 150017, 73);
case!(c_16mib, "C", 16777216, 219);
case!(cpp_empty, "CPP", 0, 0);
case!(cpp_multichunk, "CPP", 150017, 73);
case!(cpp_16mib, "CPP", 16777216, 219);
macro_rules! text_case {
    ($name:ident, $language:literal, $case:literal, $payload:expr) => {
        #[test]
        fn $name() {
            run_text($language, $case, $payload);
        }
    };
}
const UNICODE: &str = "  第一行 😀\r\nsecond line  \0";
text_case!(rust_text_empty, "RUST", "empty", vec![]);
text_case!(
    rust_text_unicode,
    "RUST",
    "unicode",
    UNICODE.as_bytes().to_vec()
);
text_case!(rust_text_limit, "RUST", "limit", vec![b'a'; MAX_TEXT_BYTES]);
text_case!(c_text_empty, "C", "empty", vec![]);
text_case!(c_text_unicode, "C", "unicode", UNICODE.as_bytes().to_vec());
text_case!(c_text_limit, "C", "limit", vec![b'a'; MAX_TEXT_BYTES]);
text_case!(cpp_text_empty, "CPP", "empty", vec![]);
text_case!(
    cpp_text_unicode,
    "CPP",
    "unicode",
    UNICODE.as_bytes().to_vec()
);
text_case!(cpp_text_limit, "CPP", "limit", vec![b'a'; MAX_TEXT_BYTES]);
#[test]
fn actual_text_planners_reject_invalid_utf8_and_oversize_without_effects() {
    for language in ["RUST", "C", "CPP"] {
        let planner = PreparedPackage::new(package_for(language, true), limits()).unwrap();
        for invalid in [
            vec![0x80],
            vec![0xc0, 0x80],
            vec![0xed, 0xa0, 0x80],
            vec![0xf4, 0x90, 0x80, 0x80],
            vec![0xf0, 0x9f],
            vec![b'a'; MAX_TEXT_BYTES + 1],
        ] {
            let result = planner
                .run_blob_upload_proposal(&plan_input_for(invalid, true), Cancellation::default());
            assert!(result.execution.outcome.is_err());
            assert_eq!(result.execution.host_calls, 0);
            assert!(result.proposal.is_none());
            // The host may reject an oversized input before entering the guest.
        }
    }
}
#[test]
fn actual_cpp_planner_low_fuel_has_no_proposal_or_upload_calls() {
    let p = PreparedPackage::new(
        package("CPP"),
        Limits {
            fuel: 100_000_000,
            ..limits()
        },
    )
    .unwrap();
    let input = plan_input(Options::new(16777216, 219).unwrap().encode());
    let r = p.run_blob_upload_proposal(&input, Cancellation::default());
    assert_eq!(r.execution.outcome, Err(Fault::Limits));
    assert_eq!(r.execution.host_calls, 0);
    assert!(r.proposal.is_none() && r.failure.is_none());
}
#[test]
fn actual_rust_planner_rejects_malformed_business_options() {
    let p = PreparedPackage::new(package("RUST"), limits()).unwrap();
    let r = p.run_blob_upload_proposal(&plan_input(vec![1]), Cancellation::default());
    assert_eq!(r.execution.outcome, Err(Fault::TaskProtocol));
    assert_eq!(r.execution.host_calls, 0);
    assert!(r.proposal.is_none() && r.failure.is_none());
}
