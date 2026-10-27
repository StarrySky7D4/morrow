//! Actual independently compiled Rust/C/C++ guests and original-owner Store effects.
//! Ordinary host test owner only; does not qualify ProtectedSession or product UI.
#![cfg(all(feature = "blob-upload-guest-qualification", windows))]
#![deny(unsafe_code)]
use morrow_blob_transfer_v1::{Action, Frame, Identity, Limits as FrameLimits};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto, registry::Registry},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Fault, Limits, Runner,
    blob_upload::Spec,
    blob_upload_binding::{ApprovalRequest, UploadApproval},
    blob_upload_worker::{Error, Poll, UploadWorker},
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
const FUEL: u64 = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;
const ID: &str = "org.example.upload-real-sdk";
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
fn limits() -> Limits {
    Limits {
        fuel: FUEL,
        host_calls: 1024,
        ..Limits::default()
    }
}
fn guest(language: &str) -> Vec<u8> {
    let path = PathBuf::from(
        std::env::var_os(format!("MORROW_{language}_UPLOAD_WASM"))
            .expect("compiled guest required"),
    );
    assert!(path.is_absolute());
    let digest = std::env::var(format!("MORROW_{language}_UPLOAD_WASM_SHA256"))
        .expect("independent hash pin required");
    let bytes = fs::read(path).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), digest);
    assert!(Runner::new_blob_upload_task(&bytes, limits()).is_ok());
    assert!(matches!(
        Runner::new_task(&bytes, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    eprintln!(
        "UPLOAD_GUEST language={language} sha256={digest} bytes={}",
        bytes.len()
    );
    bytes
}
fn invoke(
    mut owner: Owner,
    spec: &Spec,
    release: bool,
    expected_calls: u32,
    phase: Phase,
    expect_limit: bool,
) -> Owner {
    let instance = owner.manager.connect(ID, &mut owner.host).unwrap();
    let initial_fuel = instance.package().limits().fuel;
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
        "actual-sdk-upload",
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
                identity: spec.identity,
                action: Action::Descriptor {
                    total_length: spec.byte_length,
                    whole_sha256: spec.sha256,
                },
            }
            .encode()
            .unwrap(),
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
            "original worker did not close; retain this evidence and owner"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let exit = worker.try_join().unwrap().unwrap();
    assert_eq!(worker.poll(), Poll::Consumed);
    assert!(matches!(worker.try_join(), Err(Error::Consumed)));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert!(exit.instance.is_none());
    eprintln!(
        "UPLOAD_RETURN bytes={} release={release} result={:?} unknown={} calls={} wire={} native={}",
        spec.byte_length,
        exit.result.as_ref().map(|r| (&r.execution, r.phase)),
        exit.unknown,
        exit.usage.calls,
        exit.usage.wire_bytes,
        exit.usage.native_work_bytes
    );
    let record = exit
        .owner
        .host
        .store_local()
        .inspect_blob_stage_local(&spec.command())
        .unwrap();
    eprintln!(
        "UPLOAD_HISTORY present={} released={}",
        record.is_some(),
        record.as_ref().is_some_and(|r| r.is_released())
    );
    assert!(!exit.unknown);
    if expect_limit {
        assert!(matches!(exit.result, Err(Error::Limit)));
        let report = exit
            .execution_report
            .as_ref()
            .expect("actual failed interpreter report");
        eprintln!("UPLOAD_LIMIT_REPORT {report:?} initial_fuel={initial_fuel}");
        assert_eq!(initial_fuel, 100_000_000);
        assert_eq!(report.outcome, Err(Fault::Limits));
        // Wasmi charges instruction blocks. OutOfFuel can leave fewer units
        // than the next block costs; it does not promise a near-zero remainder.
        assert!(report.fuel_remaining < initial_fuel);
        assert!(
            report.host_calls > 2 && report.host_calls < spec.byte_length.div_ceil(61440) as u32
        );
        assert!(record.is_none());
        exit.owner.host.store_local().integrity_check().unwrap();
        return exit.owner;
    }
    let result = exit
        .result
        .as_ref()
        .expect("real SDK execution must succeed");
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.phase, phase);
    assert_eq!(result.execution.host_calls, expected_calls);
    assert_eq!(exit.usage.calls, expected_calls);
    assert!(result.execution.fuel_remaining < initial_fuel);
    assert_eq!(
        exit.execution_report.as_ref().unwrap().outcome,
        result.execution.outcome
    );
    exit.owner.host.store_local().integrity_check().unwrap();
    exit.owner
}
fn run(language: &str, length: usize, limited: bool) {
    let root = PathBuf::from(
        std::env::var_os("MORROW_UPLOAD_GUEST_STORE_ROOT").expect("fresh evidence root required"),
    );
    assert!(root.is_absolute());
    let root = root.join(format!("{language}-{length}-limited-{limited}"));
    fs::create_dir(&root).expect("fresh store required; never replay an earlier case");
    let payload = vec![42; length];
    let spec = Spec {
        owner_domain: [72; 32],
        identity: Identity {
            transfer_epoch: [71; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        },
        byte_length: length as u64,
        sha256: Sha256::digest(&payload).into(),
        limits: FrameLimits::default(),
    };
    let module = guest(language);
    let mut manifest = Package::manifest_for_blob_upload(
        ID,
        "1.0.0",
        &module,
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
    let budget = manifest.budget.as_mut().unwrap();
    budget.fuel = if limited { 100_000_000 } else { FUEL };
    budget.host_calls = 1024;
    let package = Package::build(manifest, &module).unwrap();
    let catalog = Catalog::open(&root.join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&root.join("registry"), catalog).unwrap(),
        limits(),
    );
    manager.select(&package, 0).unwrap();
    manager
        .approve_blob_upload(ID, package.digest(), true, manager.revision())
        .unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
    let host =
        HostRuntime::new(Store::open(&root.join("original"), Default::default()).unwrap()).unwrap();
    let owner = Owner {
        host,
        manager,
        prepared: 0,
        finished: 0,
    };
    let owner = invoke(
        owner,
        &spec,
        false,
        length.div_ceil(61440) as u32 + 4,
        Phase::Staged,
        limited,
    );
    if limited {
        assert_eq!(owner.prepared, 1);
        assert_eq!(owner.finished, 1);
        drop(owner);
        let reopened = Store::open_existing(&root.join("original"), Default::default()).unwrap();
        assert!(
            reopened
                .inspect_blob_stage_local(&spec.command())
                .unwrap()
                .is_none()
        );
        reopened.integrity_check().unwrap();
        eprintln!("UPLOAD_LIMIT_REFUSED original_worker_joined=true no_staged_record=true");
        return;
    }
    let staged = owner
        .host
        .store_local()
        .inspect_blob_stage_local(&spec.command())
        .unwrap()
        .unwrap();
    assert_eq!(staged.staged_at_unix_ms(), 7);
    assert!(!staged.is_released());
    let mut bytes = vec![];
    owner
        .host
        .store_local()
        .export_blob_local(&staged.blob().id, &mut bytes)
        .unwrap();
    assert_eq!(bytes, payload);
    assert_eq!(<[u8; 32]>::from(Sha256::digest(&bytes)), spec.sha256);
    // Each new invocation receives a fresh explicit approval on the returned
    // original owner. A known successful stage is observed, never re-uploaded.
    let owner = invoke(owner, &spec, false, 2, Phase::Staged, false);
    assert_eq!(
        owner
            .host
            .store_local()
            .inspect_blob_stage_local(&spec.command())
            .unwrap()
            .unwrap(),
        staged
    );
    let owner = invoke(owner, &spec, true, 2, Phase::Released, false);
    assert_eq!(owner.prepared, 3);
    assert_eq!(owner.finished, 3);
    let released = owner
        .host
        .store_local()
        .inspect_blob_stage_local(&spec.command())
        .unwrap()
        .unwrap();
    assert_eq!(released.released_at_unix_ms(), Some(13));
    assert_eq!(released.blob(), staged.blob());
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
        "UPLOAD_ACCEPTED language={language} bytes={length} chunks={} original_workers_joined=3 durable_release=true",
        length.div_ceil(61440)
    );
}
macro_rules! case {
    ($name:ident, $language:literal, $size:expr) => {
        #[test]
        fn $name() {
            run($language, $size, false);
        }
    };
}
case!(rust_empty, "RUST", 0);
case!(rust_multichunk, "RUST", 150_017);
case!(rust_16mib, "RUST", 16 * 1024 * 1024);
case!(c_empty, "C", 0);
case!(c_multichunk, "C", 150_017);
case!(c_16mib, "C", 16 * 1024 * 1024);
case!(cpp_empty, "CPP", 0);
case!(cpp_multichunk, "CPP", 150_017);
case!(cpp_16mib, "CPP", 16 * 1024 * 1024);
#[test]
fn rust_small_package_fuel_is_not_refilled_by_chunk_resumes() {
    run("RUST", 16 * 1024 * 1024, true);
}
