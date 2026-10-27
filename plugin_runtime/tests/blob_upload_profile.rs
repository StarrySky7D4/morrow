//! Static import isolation and original live approval; no product or Store upload execution.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_blob_transfer_v1::{Identity, Limits as FrameLimits};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner,
    blob_upload::Spec,
    blob_upload_binding::{ApprovalRequest, Error, UploadApproval},
    manager::{ManagedInstance, Manager},
    package::PreparedPackage,
};
use sha2::{Digest, Sha256};
use std::time::Duration;
fn module(imports: &[(&str, &str, usize)]) -> Vec<u8> {
    let mut s = String::from("(module ");
    for (namespace, name, n) in imports {
        let args = std::iter::repeat_n("i32", *n).collect::<Vec<_>>().join(" ");
        s.push_str(&format!(
            "(import \"{namespace}\" \"{name}\" (func (param {args}) (result i32))) "
        ));
    }
    s.push_str(
        "(memory (export \"memory\") 4) (func (export \"morrow_run\") (result i32) i32.const 0))",
    );
    wat::parse_str(s).unwrap()
}
fn imports() -> Vec<(&'static str, &'static str, usize)> {
    vec![
        ("morrow_task_v1", "read_input", 2),
        ("morrow_task_v1", "complete", 2),
        ("morrow_blob_upload_v1", "call", 4),
    ]
}
fn package() -> Package {
    let bytes = module(&imports());
    let m = Package::manifest_for_blob_upload(
        "org.example.upload-profile",
        "1.0.0",
        &bytes,
        vec![proto::TransformHandler {
            handler: "blob.upload".into(),
            input_type: "upload.input.v1".into(),
            output_type: "upload.result.v1".into(),
            max_input_bytes: 1024,
            max_output_bytes: 1024,
        }],
    );
    Package::build(m, &bytes).unwrap()
}
#[test]
fn independent_fuel_requires_both_package_and_host_without_relaxing_legacy_limits() {
    let hard = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;
    assert_eq!(hard, morrow_core::plugin_package::blob_upload::MAX_FUEL);
    assert_eq!(Limits::default().fuel, 20_000_000);
    let bytes = module(&imports());
    let base = package();
    let mut manifest = base.manifest().clone();
    manifest.budget.as_mut().unwrap().fuel = hard;
    let package = Package::build(manifest, &bytes).unwrap();
    let raised = Limits {
        fuel: hard,
        ..Limits::default()
    };
    assert_eq!(
        PreparedPackage::new(Package::decode(package.archive()).unwrap(), raised)
            .unwrap()
            .limits()
            .fuel,
        hard
    );
    assert_eq!(
        PreparedPackage::new(
            Package::decode(package.archive()).unwrap(),
            Limits::default()
        )
        .unwrap()
        .limits()
        .fuel,
        20_000_000
    );
    assert_eq!(
        PreparedPackage::new(base, raised).unwrap().limits().fuel,
        20_000_000
    );
    let excess = Limits {
        fuel: hard + 1,
        ..Limits::default()
    };
    assert!(matches!(
        PreparedPackage::new(package, excess),
        Err(Fault::Limits)
    ));
    assert!(matches!(
        Runner::new_blob_upload_task(&bytes, excess),
        Err(Fault::Limits)
    ));
    assert!(matches!(
        Runner::new_task(&bytes, raised),
        Err(Fault::Limits)
    ));
    let legacy_bytes = module(&[
        ("morrow_task_v1", "read_input", 2),
        ("morrow_task_v1", "complete", 2),
    ]);
    let legacy = Package::manifest_for_transform(
        "org.example.legacy",
        "1.0.0",
        &legacy_bytes,
        vec![proto::TransformHandler {
            handler: "pure".into(),
            input_type: "pure.in".into(),
            output_type: "pure.out".into(),
            max_input_bytes: 16,
            max_output_bytes: 16,
        }],
    );
    let legacy = Package::build(legacy, &legacy_bytes).unwrap();
    assert!(matches!(
        PreparedPackage::new(legacy, raised),
        Err(Fault::Limits)
    ));
}
struct Fixture {
    d: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
}
#[test]
fn explicit_upload_connection_keeps_approval_and_default_manager_limits_independent() {
    let d = tempfile::tempdir().unwrap();
    let base = package();
    let mut manifest = base.manifest().clone();
    manifest.budget.as_mut().unwrap().fuel = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;
    manifest.budget.as_mut().unwrap().host_calls = 1024;
    let package = Package::build(manifest, base.module()).unwrap();
    let catalog = Catalog::open(&d.path().join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&d.path().join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    let mut host =
        HostRuntime::new(Store::open(&d.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let id = &package.manifest().package_id;
    manager.select(&package, manager.revision()).unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
    let raised = Limits {
        fuel: morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL,
        host_calls: 1024,
        ..Limits::default()
    };
    assert!(manager.connect_blob_upload(id, &mut host, raised).is_err());
    assert!(!manager.selection(id).unwrap().approved_blob_upload);
    manager
        .approve_blob_upload(id, package.digest(), true, manager.revision())
        .unwrap();
    let revision = manager.revision();
    let explicit = manager.connect_blob_upload(id, &mut host, raised).unwrap();
    assert_eq!(explicit.package().limits().fuel, raised.fuel);
    assert_eq!(explicit.package().limits().host_calls, 1024);
    assert_eq!(manager.revision(), revision);
    explicit.close(&mut host).unwrap();
    let ordinary = manager.connect(id, &mut host).unwrap();
    assert_eq!(ordinary.package().limits().fuel, Limits::default().fuel);
    assert_eq!(
        ordinary.package().limits().host_calls,
        Limits::default().host_calls
    );
    ordinary.close(&mut host).unwrap();
    let excessive = Limits {
        fuel: raised.fuel + 1,
        ..raised
    };
    assert!(
        manager
            .connect_blob_upload(id, &mut host, excessive)
            .is_err()
    );
    manager
        .approve_blob_upload(id, package.digest(), false, manager.revision())
        .unwrap();
    assert!(manager.connect_blob_upload(id, &mut host, raised).is_err());
}
fn fixture(approved: bool) -> Fixture {
    fixture_budget(approved, 64 * 1024 * 1024)
}
fn fixture_budget(approved: bool, max_wire_bytes: u64) -> Fixture {
    let d = tempfile::tempdir().unwrap();
    let base = package();
    let mut m = base.manifest().clone();
    m.blob_upload_declaration.as_mut().unwrap().max_wire_bytes = max_wire_bytes;
    let p = Package::build(m, base.module()).unwrap();
    let catalog = Catalog::open(&d.path().join("packages")).unwrap();
    catalog.install(&p).unwrap();
    let mut manager = Manager::new(
        Registry::open(&d.path().join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    manager.select(&p, 0).unwrap();
    let id = &p.manifest().package_id;
    if approved {
        manager
            .approve_blob_upload(id, p.digest(), true, manager.revision())
            .unwrap();
    }
    manager
        .set_enabled(id, p.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&d.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let instance = manager.connect(id, &mut host).unwrap();
    Fixture {
        d,
        manager,
        host,
        instance,
    }
}
fn request(f: &Fixture) -> ApprovalRequest {
    ApprovalRequest {
        expected_revision: f.manager.revision(),
        nomination_ref: [6; 32],
        spec: Spec {
            owner_domain: [7; 32],
            identity: Identity {
                transfer_epoch: [1; 32],
                object_ref: [2; 32],
                operation_id: [3; 32],
            },
            byte_length: 0,
            sha256: Sha256::digest([]).into(),
            limits: FrameLimits::default(),
        },
        lifetime: Duration::from_secs(30),
    }
}
fn issue(f: &Fixture) -> UploadApproval {
    UploadApproval::issue(&f.manager, &f.host, &f.instance, request(f)).unwrap()
}
#[test]
fn exact_upload_import_prepares_but_every_legacy_factory_refuses_it() {
    let bytes = module(&imports());
    type Factory = fn(&[u8], Limits) -> Result<Runner, Fault>;
    let factories: [Factory; 9] = [
        Runner::new,
        Runner::new_task,
        Runner::new_dependency_task,
        Runner::new_io_task,
        Runner::new_mutation_task,
        Runner::new_channel_task,
        Runner::new_directory_task,
        Runner::new_agent_session_exec_task,
        Runner::new_agent_session_process_task,
    ];
    for factory in factories {
        assert!(matches!(
            factory(&bytes, Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    assert!(PreparedPackage::new(package(), Limits::default()).is_ok());
    let runner = Runner::new_blob_upload_task(&bytes, Limits::default()).unwrap();
    let run = runner.run_task(
        &[1],
        &mut |_| panic!("bare callback must not gain upload access"),
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(run.report.host_calls, 0);
}
#[test]
fn upload_profile_requires_exactly_the_three_fixed_imports() {
    for index in 0..3 {
        let mut i = imports();
        i.remove(index);
        assert!(matches!(
            Runner::new_blob_upload_task(&module(&i), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    for extra in [
        ("morrow_v1", "exchange", 4),
        ("morrow_io_v1", "call", 4),
        ("morrow_channel_v1", "call", 4),
        ("morrow_mutation_v1", "call", 4),
        ("morrow_fs_directory_v1", "call", 4),
        ("morrow_dependency_v1", "call", 4),
        ("wasi_snapshot_preview1", "call", 4),
    ] {
        let mut i = imports();
        i.push(extra);
        assert!(matches!(
            Runner::new_blob_upload_task(&module(&i), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    for wrong in [
        ("morrow_blob_upload_v2", "call", 4),
        ("morrow_blob_upload_v1", "read", 4),
        ("morrow_blob_upload_v1", "call", 3),
        ("morrow_blob_upload_v1", "call", 5),
    ] {
        let mut i = imports();
        i[2] = wrong;
        assert!(matches!(
            Runner::new_blob_upload_task(&module(&i), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
}
#[test]
fn package_and_import_must_agree_in_both_directions() {
    let bytes = module(&imports());
    let legacy = Package::manifest_for_task("ordinary", "1.0.0", &bytes, vec![]);
    assert!(matches!(
        PreparedPackage::new(Package::build(legacy, &bytes).unwrap(), Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let bytes = module(&imports()[..2]);
    let mut m = package().manifest().clone();
    m.module_sha256 = Sha256::digest(&bytes).to_vec();
    assert!(matches!(
        PreparedPackage::new(Package::build(m, &bytes).unwrap(), Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
}
#[test]
fn declaration_enable_and_identity_alone_do_not_issue_upload_approval() {
    let f = fixture(false);
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, request(&f)),
        Err(Error::Denied)
    ));
    let f = fixture(true);
    let mut r = request(&f);
    r.expected_revision -= 1;
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, r),
        Err(Error::Denied)
    ));
    let mut r = request(&f);
    r.nomination_ref = [0; 32];
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, r),
        Err(Error::Denied)
    ));
    let approval = issue(&f);
    approval
        .validate_owner(&f.manager, &f.host, &f.instance)
        .unwrap();
    assert_eq!(approval.nomination_ref(), [6; 32]);
    assert_eq!(approval.spec().byte_length, 0);
}
#[test]
fn original_instance_claim_and_cumulative_charges_never_reset_on_drop() {
    let f = fixture(true);
    let a = issue(&f);
    a.charge_request(32).unwrap();
    a.charge_response(64).unwrap();
    assert_eq!(a.usage().calls, 1);
    assert_eq!(a.usage().wire_bytes, 96);
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, request(&f)),
        Err(Error::AlreadyBound)
    ));
    for _ in 1..16 {
        a.charge_request(1).unwrap();
    }
    let before = a.usage();
    assert_eq!(a.charge_request(1), Err(Error::Limit));
    assert_eq!(a.usage(), before);
    drop(a);
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, request(&f)),
        Err(Error::AlreadyBound)
    ));
}
#[test]
fn exhausted_wire_budget_keeps_original_fees_without_creating_replacement_credit() {
    let f = fixture_budget(true, 64);
    let mut r = request(&f);
    r.spec.limits.max_wire_bytes = 64;
    r.spec.limits.max_request_bytes = 64;
    r.spec.limits.max_response_bytes = 64;
    let a = UploadApproval::issue(&f.manager, &f.host, &f.instance, r).unwrap();
    a.charge_request(50).unwrap();
    assert_eq!(a.charge_response(20), Err(Error::Limit));
    assert_eq!(a.usage().wire_bytes, 50);
    assert_eq!(a.charge_request(30), Err(Error::Limit));
    assert_eq!(a.usage().calls, 2);
    assert_eq!(a.usage().wire_bytes, 50);
    a.charge_response(14).unwrap();
    assert_eq!(a.usage().wire_bytes, 64);
    assert_eq!(a.charge_response(1), Err(Error::Limit));
    a.revocation().revoke();
    assert_eq!(a.usage().calls, 2);
    assert_eq!(a.usage().wire_bytes, 64);
}
#[test]
fn malformed_or_oversized_approval_fails_before_consuming_the_instance_claim() {
    let f = fixture(true);
    for case in 0..5 {
        let mut r = request(&f);
        match case {
            0 => r.lifetime = Duration::ZERO,
            1 => r.lifetime = Duration::from_secs(3601),
            2 => r.spec.byte_length = 16 * 1024 * 1024 + 1,
            3 => r.spec.limits.max_wire_bytes = 64 * 1024 * 1024 + 1,
            _ => r.spec.sha256 = [0; 32],
        };
        assert!(matches!(
            UploadApproval::issue(&f.manager, &f.host, &f.instance, r),
            Err(Error::Limit)
        ));
    }
    let a = issue(&f);
    let before = a.usage();
    assert_eq!(a.charge_request(65537), Err(Error::Limit));
    assert_eq!(a.charge_response(4097), Err(Error::Limit));
    assert_eq!(a.usage(), before);
}
#[test]
fn wrong_owner_is_denied_without_transferring_or_reviving_the_original() {
    let f = fixture(true);
    let other = fixture(true);
    let a = issue(&f);
    assert_eq!(
        a.validate_owner(&other.manager, &f.host, &f.instance),
        Err(Error::Denied)
    );
    assert_eq!(
        a.validate_owner(&f.manager, &other.host, &f.instance),
        Err(Error::Denied)
    );
    assert_eq!(
        a.validate_owner(&f.manager, &f.host, &other.instance),
        Err(Error::Denied)
    );
    a.validate_owner(&f.manager, &f.host, &f.instance).unwrap();
}
#[test]
fn observed_original_store_substitution_is_permanently_denied() {
    let mut f = fixture(true);
    let a = issue(&f);
    let replacement = Store::open(&f.d.path().join("other-db"), EventBudget::default()).unwrap();
    let original = std::mem::replace(f.host.store_local_mut(), replacement);
    assert_eq!(
        a.validate_owner(&f.manager, &f.host, &f.instance),
        Err(Error::Denied)
    );
    *f.host.store_local_mut() = original;
    assert_eq!(a.check(), Err(Error::Denied));
    assert_eq!(
        a.validate_owner(&f.manager, &f.host, &f.instance),
        Err(Error::Denied)
    );
}
#[test]
fn receiver_stop_manager_drop_and_approval_revoke_are_one_way() {
    let f = fixture(true);
    let a = issue(&f);
    f.instance.request_stop();
    assert_eq!(a.check(), Err(Error::Denied));
    let f = fixture(true);
    let a = issue(&f);
    drop(f.manager);
    assert_eq!(a.check(), Err(Error::Denied));
    let f = fixture(true);
    let a = issue(&f);
    let revoke = a.revocation();
    revoke.revoke();
    assert_eq!(a.charge_request(1), Err(Error::Denied));
    assert_eq!(a.usage().calls, 0);
    let mut f = fixture(true);
    let a = issue(&f);
    f.host.disconnect(f.instance.connection()).unwrap();
    assert_eq!(a.check(), Err(Error::Denied));
}
#[test]
fn registry_change_invalidates_the_original_live_approval() {
    let mut f = fixture(true);
    let a = issue(&f);
    let p = package();
    f.manager
        .approve_blob_upload(
            &p.manifest().package_id,
            p.digest(),
            false,
            f.manager.revision(),
        )
        .unwrap();
    assert_eq!(a.check(), Err(Error::Denied));
    f.manager
        .approve_blob_upload(
            &p.manifest().package_id,
            p.digest(),
            true,
            f.manager.revision(),
        )
        .unwrap();
    assert_eq!(a.check(), Err(Error::Denied));
}
#[test]
fn original_deadline_expires_without_waiting_on_owner_locks_or_rebinding() {
    let f = fixture(true);
    let mut r = request(&f);
    r.lifetime = Duration::from_millis(20);
    let a = UploadApproval::issue(&f.manager, &f.host, &f.instance, r).unwrap();
    let deadline = a.deadline();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(a.check(), Err(Error::Expired));
    assert_eq!(a.deadline(), deadline);
    assert!(matches!(
        UploadApproval::issue(&f.manager, &f.host, &f.instance, request(&f)),
        Err(Error::AlreadyBound)
    ));
}
