//! Original live manager/connection/IO context checks; no native watch dispatch.
#![cfg(all(feature = "packages", windows))]
#![deny(warnings)]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    directory_io::DirectoryAbsolutePath,
    fs_watch_binding::{ApprovalRequest, Error, WatchApproval},
    io_binding::{Error as IoError, IoBinding, Usage},
    manager::{ManagedInstance, Manager},
    package::PreparedPackage,
};
use std::{collections::BTreeSet, time::Duration};
const ID: &str = "org.example.watch.live";
const GOOD: &str = r#"(module
    (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
    (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
    (memory (export "memory") 1 256)
    (func (export "morrow_run") (result i32) i32.const 0))"#;
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileList])
}
fn package(wat: &str, watch: bool) -> Package {
    let wasm = wat::parse_str(wat).unwrap();
    let mut m = Package::manifest_for_fs_watch(ID, "1.0.0", &wasm, vec!["observe".into()]);
    let d = m.fs_watch_declaration.as_mut().unwrap();
    d.max_frames = 4;
    d.max_wire_bytes = 256;
    d.max_window_ms = 10_000;
    let b = m.io_declaration.as_mut().unwrap().budget.as_mut().unwrap();
    b.max_resources = 2;
    b.max_jobs = 2;
    b.max_bytes = 512;
    b.max_job_bytes = 256;
    b.max_duration_ms = 10_000;
    if !watch {
        m.fs_watch_declaration = None;
        m.required_features.retain(|f| f != "fs-watch-v1");
    }
    Package::build(m, &wasm).unwrap()
}
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    package: Package,
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new(watch: bool, approved: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package(GOOD, watch);
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(&package, 0).unwrap();
        manager
            .approve_io(ID, package.digest(), caps(), manager.revision())
            .unwrap();
        if approved {
            manager
                .approve_fs_watch(ID, package.digest(), true, manager.revision())
                .unwrap();
        }
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(
            Store::open(&dir.path().join("store"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        Self {
            manager,
            host,
            package,
            dir,
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn binding(&self, i: &ManagedInstance) -> IoBinding {
        self.manager
            .bind_io(
                &self.host,
                i,
                self.package.digest(),
                self.manager.revision(),
                &caps(),
                9_000,
                1,
            )
            .unwrap()
    }
    fn request(&self) -> ApprovalRequest {
        ApprovalRequest {
            expected_revision: self.manager.revision(),
            handler: "observe".into(),
            nominated_path: DirectoryAbsolutePath::new(
                &self.dir.path().join("selection-not-opened"),
            )
            .unwrap(),
            max_frames: 4,
            max_wire_bytes: 128,
            window_ms: 5_000,
        }
    }
    fn issue(&self, i: &ManagedInstance, b: &IoBinding) -> Result<WatchApproval, Error> {
        WatchApproval::issue(&self.manager, &self.host, i, b, self.request(), 1)
    }
}
#[test]
fn exact_watch_profile_accepts_only_input_and_completion_imports() {
    assert!(PreparedPackage::new(package(GOOD, true), Limits::default()).is_ok());
    for wat in [
        r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32))) (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#,
    ] {
        assert!(matches!(
            PreparedPackage::new(package(wat, true), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    for (module, name, arity) in [
        ("morrow_v1", "exchange", 4),
        ("morrow_io_v1", "call", 4),
        ("morrow_channel_v1", "call", 4),
        ("wasi_snapshot_preview1", "fd_write", 4),
    ] {
        let import = format!(
            "(import \"{module}\" \"{name}\" (func (param {}) (result i32)))",
            vec!["i32"; arity].join(" ")
        );
        let changed = GOOD.replacen("(module", &format!("(module {import}"), 1);
        assert!(matches!(
            PreparedPackage::new(package(&changed, true), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
}
#[test]
fn list_approval_and_connection_do_not_create_watch_approval() {
    let mut f = Fixture::new(true, false);
    let old = f.connect();
    let b = f.binding(&old);
    assert!(matches!(f.issue(&old, &b), Err(Error::Denied)));
    assert!(old.run(&mut f.host, || 1).outcome.is_err());
    f.manager
        .approve_fs_watch(ID, f.package.digest(), true, f.manager.revision())
        .unwrap();
    assert!(f.host.revocation(old.connection()).unwrap().is_revoked());
    assert!(f.issue(&old, &b).is_err());
    let current = f.connect();
    let b = f.binding(&current);
    let a = f.issue(&current, &b).unwrap();
    a.check(&f.manager, &f.host, &current, 2).unwrap();
    assert!(!f.dir.path().join("selection-not-opened").exists());
    drop(a);
    current.close(&mut f.host).unwrap();
    old.close(&mut f.host).unwrap();
}
#[test]
fn invalid_requests_do_not_claim_a_window_or_spend_the_original_budget() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    for case in 0..8 {
        let mut r = f.request();
        match case {
            0 => r.expected_revision -= 1,
            1 => r.handler = "other".into(),
            2 => r.max_frames = 0,
            3 => r.max_frames = 5,
            4 => r.max_wire_bytes = 257,
            5 => r.window_ms = 0,
            6 => r.window_ms = 10_001,
            _ => r.window_ms = 9_000,
        }
        assert!(
            WatchApproval::issue(&f.manager, &f.host, &i, &b, r, 1).is_err(),
            "case {case}"
        );
        assert_eq!(b.usage(), Usage::default());
    }
    let a = f.issue(&i, &b).unwrap();
    assert_eq!(a.registry_revision(), f.manager.revision());
    assert_eq!(a.handler(), "observe");
    assert_eq!(a.limits().max_wire_bytes, 128);
    a.check(&f.manager, &f.host, &i, 2).unwrap();
    i.close(&mut f.host).unwrap();
}
#[test]
fn dropped_or_revoked_approval_cannot_reset_the_same_instance_window() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let a = f.issue(&i, &b).unwrap();
    let revoke = a.revocation();
    revoke.revoke();
    assert_eq!(a.check(&f.manager, &f.host, &i, 2), Err(Error::Denied));
    drop(a);
    assert!(f.issue(&i, &b).is_err());
    let again = f.binding(&i);
    assert!(f.issue(&i, &again).is_err());
    assert_eq!(b.usage(), Usage::default());
    i.close(&mut f.host).unwrap();
}
#[test]
fn approval_observes_original_shared_resources_jobs_and_nonrefundable_bytes() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let again = f.binding(&i);
    let held = b
        .admit(&f.manager, &f.host, &i, IoCapability::FileList, 2, 200, 1)
        .unwrap();
    let a = f.issue(&i, &again).unwrap();
    assert_eq!(
        a.usage(),
        Usage {
            resources: 2,
            jobs: 1,
            bytes: 200
        }
    );
    assert!(
        again
            .admit(&f.manager, &f.host, &i, IoCapability::FileList, 1, 1, 2)
            .is_err()
    );
    drop(held);
    assert_eq!(
        a.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 200
        }
    );
    let held = again
        .admit(&f.manager, &f.host, &i, IoCapability::FileList, 1, 200, 2)
        .unwrap();
    drop(held);
    assert_eq!(a.usage().bytes, 400);
    assert!(
        b.admit(&f.manager, &f.host, &i, IoCapability::FileList, 0, 113, 3)
            .is_err()
    );
    assert_eq!(a.usage().bytes, 400);
    i.close(&mut f.host).unwrap();
}
#[test]
fn foreign_manager_host_instance_and_binding_cannot_advance_the_legitimate_clock() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let mut other = Fixture::new(true, true);
    let j = other.connect();
    let wrong = other.binding(&j);
    assert!(WatchApproval::issue(&f.manager, &f.host, &i, &wrong, f.request(), 1).is_err());
    assert!(WatchApproval::issue(&other.manager, &f.host, &i, &b, f.request(), 1).is_err());
    assert!(WatchApproval::issue(&f.manager, &other.host, &i, &b, f.request(), 1).is_err());
    let a = f.issue(&i, &b).unwrap();
    assert!(a.check(&other.manager, &f.host, &i, u64::MAX).is_err());
    assert!(a.check(&f.manager, &other.host, &i, u64::MAX).is_err());
    assert!(a.check(&f.manager, &f.host, &j, u64::MAX).is_err());
    a.check(&f.manager, &f.host, &i, 2).unwrap();
    i.close(&mut f.host).unwrap();
    j.close(&mut other.host).unwrap();
}
#[test]
fn rollback_and_expiration_are_sticky_and_cannot_be_rescued_by_new_binding() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let a = f.issue(&i, &b).unwrap();
    a.check(&f.manager, &f.host, &i, 10).unwrap();
    assert_eq!(
        a.check(&f.manager, &f.host, &i, 9),
        Err(Error::Binding(IoError::Clock))
    );
    assert_eq!(a.check(&f.manager, &f.host, &i, 11), Err(Error::Denied));
    i.close(&mut f.host).unwrap();
    let i = f.connect();
    let b = f.binding(&i);
    let a = f.issue(&i, &b).unwrap();
    assert_eq!(a.check(&f.manager, &f.host, &i, 5_001), Err(Error::Expired));
    assert_eq!(a.check(&f.manager, &f.host, &i, 2), Err(Error::Denied));
    assert!(f.issue(&i, &b).is_err());
    i.close(&mut f.host).unwrap();
}
#[test]
fn wall_clock_deadline_is_enforced_even_when_the_host_tick_stays_constant() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let mut r = f.request();
    r.window_ms = 1;
    let a = WatchApproval::issue(&f.manager, &f.host, &i, &b, r, 1).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(a.check(&f.manager, &f.host, &i, 1), Err(Error::Expired));
    i.close(&mut f.host).unwrap();
}
#[test]
fn invalid_and_noop_manager_approvals_leave_live_instance_intact_but_revocation_stops_it() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let a = f.issue(&i, &b).unwrap();
    let rev = f.manager.revision();
    assert!(f.manager.approve_fs_watch(ID, [9; 32], false, rev).is_err());
    assert!(
        f.manager
            .approve_fs_watch(ID, f.package.digest(), false, rev - 1)
            .is_err()
    );
    f.manager
        .approve_fs_watch(ID, f.package.digest(), true, rev)
        .unwrap();
    assert_eq!(f.manager.revision(), rev);
    a.check(&f.manager, &f.host, &i, 2).unwrap();
    f.manager
        .approve_fs_watch(ID, f.package.digest(), false, rev)
        .unwrap();
    assert!(a.check(&f.manager, &f.host, &i, 3).is_err());
    f.manager
        .approve_fs_watch(ID, f.package.digest(), true, f.manager.revision())
        .unwrap();
    assert!(a.check(&f.manager, &f.host, &i, 4).is_err());
    i.close(&mut f.host).unwrap();
}
#[test]
fn ordinary_list_profile_cannot_issue_watch_and_invalid_approval_does_not_revoke_it() {
    let mut f = Fixture::new(false, false);
    let i = f.connect();
    let b = f.binding(&i);
    assert!(matches!(f.issue(&i, &b), Err(Error::Denied)));
    let before = f.manager.persisted_snapshot().unwrap();
    assert!(
        f.manager
            .approve_fs_watch(ID, f.package.digest(), true, f.manager.revision())
            .is_err()
    );
    assert_eq!(f.manager.persisted_snapshot().unwrap(), before);
    b.check(&f.manager, &f.host, &i, 2).unwrap();
    i.close(&mut f.host).unwrap();
}
#[test]
fn original_instance_stop_invalidates_approval_without_refunding_held_reservations() {
    let mut f = Fixture::new(true, true);
    let i = f.connect();
    let b = f.binding(&i);
    let a = f.issue(&i, &b).unwrap();
    let lease = b
        .admit(&f.manager, &f.host, &i, IoCapability::FileList, 2, 200, 1)
        .unwrap();
    i.request_stop();
    assert!(a.check(&f.manager, &f.host, &i, 2).is_err());
    assert_eq!(
        a.usage(),
        Usage {
            resources: 2,
            jobs: 1,
            bytes: 200
        }
    );
    drop(lease);
    assert_eq!(
        a.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 200
        }
    );
    i.close(&mut f.host).unwrap();
}
