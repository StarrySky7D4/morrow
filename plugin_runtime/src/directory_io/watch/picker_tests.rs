//! Callback admission tests with actual original Windows capture. Only the
//! cancelled preflight calls the native bridge; these do not prove a shown UI.
use super::*;
use crate::{
    Limits,
    fs_watch_binding::{ApprovalRequest, WatchApproval},
    io_binding::IoBinding,
};
use morrow_core::{
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::PathBuf,
};
struct Clock {
    samples: usize,
    panic: bool,
}
impl WatchClock for Clock {
    fn with<T>(&mut self, step: impl FnOnce(u64) -> Result<T, Error>) -> Result<T, Error> {
        self.samples += 1;
        assert!(!self.panic, "one original callback clock panic");
        step(1)
    }
}
struct Fixture {
    m: Manager,
    h: HostRuntime,
    i: ManagedInstance,
    b: IoBinding,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap().keep();
        assert!(root.starts_with(r"D:\build_cache"));
        for path in root
            .ancestors()
            .take_while(|p| *p != Path::new(r"D:\build_cache"))
        {
            assert_eq!(
                fs::symlink_metadata(path).unwrap().file_attributes() & 0x400,
                0
            );
            assert!(!path.join(".git").exists());
        }
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
            (memory (export "memory") 1 256)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let package = Package::build(
            Package::manifest_for_fs_watch(
                "org.example.watch.picker",
                "1.0.0",
                &wasm,
                vec!["observe".into()],
            ),
            &wasm,
        )
        .unwrap();
        let id = package.manifest().package_id.as_str();
        let caps = BTreeSet::from([IoCapability::FileList]);
        let catalog = Catalog::open(&root.join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut m = Manager::new(
            Registry::open(&root.join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        m.select(&package, 0).unwrap();
        m.approve_io(id, package.digest(), caps.clone(), m.revision())
            .unwrap();
        m.approve_fs_watch(id, package.digest(), true, m.revision())
            .unwrap();
        m.set_enabled(id, package.digest(), true, m.revision())
            .unwrap();
        let mut h =
            HostRuntime::new(Store::open(&root.join("store"), EventBudget::default()).unwrap())
                .unwrap();
        let i = m.connect(id, &mut h).unwrap();
        let b = m
            .bind_io(&h, &i, package.digest(), m.revision(), &caps, 29_000, 1)
            .unwrap();
        let path = root.join("observed 空白😀");
        fs::create_dir(&path).unwrap();
        Self { m, h, i, b, path }
    }
    fn approval(&self) -> WatchApproval {
        WatchApproval::issue(
            &self.m,
            &self.h,
            &self.i,
            &self.b,
            ApprovalRequest {
                expected_revision: self.m.revision(),
                handler: "observe".into(),
                nominated_path: DirectoryAbsolutePath::new(&self.path).unwrap(),
                max_frames: 2,
                max_wire_bytes: 512 * 1024,
                window_ms: 25_000,
            },
            1,
        )
        .unwrap()
        .require_native_confirmation()
    }
    fn prepare(&self, c: &mut Clock) -> DirectoryWatch {
        let approval = self.approval();
        assert!(approval.requires_native_confirmation());
        DirectoryWatch::prepare(approval, &self.m, &self.h, &self.i, c).unwrap()
    }
    fn finish(&mut self) {
        assert_eq!((self.b.usage().resources, self.b.usage().jobs), (0, 0));
        self.i.request_stop();
        self.i.close(&mut self.h).unwrap();
    }
}
fn clock() -> Clock {
    Clock {
        samples: 0,
        panic: false,
    }
}
fn record(label: &str, watch: &DirectoryWatch, f: &Fixture, c: &Clock, native_preflight: bool) {
    let root = PathBuf::from(
        std::env::var_os("MORROW_WATCH_PICKER_EVIDENCE").expect("explicit qualification evidence"),
    );
    assert!(root.starts_with(r"D:\build_cache"));
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("{label}.txt")))
        .unwrap();
    writeln!(out, "native_bridge_preflight={native_preflight}\nshown_native_dialog=false\nmanaged_worker=false\nsnapshot={:?}\nusage={:?}\nclock_samples={}", watch.snapshot(), f.b.usage(), c.samples).unwrap();
    out.sync_all().unwrap();
}

#[test]
fn original_native_picker_cancel_preflight_closes_without_capture() {
    let mut f = Fixture::new();
    let mut c = clock();
    let failure =
        DirectoryWatch::start_with_cancellation(f.approval(), &f.m, &f.h, &f.i, &mut c, || true)
            .unwrap_err();
    assert_eq!(
        failure.error,
        Error::Directory(crate::directory_io::Error::Cancelled)
    );
    let mut watch = failure.owner.unwrap();
    let s = watch.snapshot();
    assert!(s.ancestors.picker_started && s.ancestors.picker_closed);
    assert!(s.native.is_none() && s.ancestors.objects == 0 && s.held_resources == 0 && s.job_held);
    let closed = watch.retire().unwrap();
    assert!(closed.ancestor_snapshot().all_closed());
    record("native-preflight-cancel", &watch, &f, &c, true);
    f.finish();
}

#[test]
fn different_native_nomination_is_refused_before_open_and_is_sticky() {
    let mut f = Fixture::new();
    let mut c = clock();
    let mut watch = f.prepare(&mut c);
    let mut confirmation = Confirmation::default();
    let other: Vec<u16> = f
        .path
        .with_file_name("different")
        .as_os_str()
        .encode_wide()
        .collect();
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Select(&other)
            )
            .is_err()
    );
    assert_eq!(confirmation.failure, Some(Error::SelectionMismatch));
    let samples = c.samples;
    let original: Vec<_> = f.path.as_os_str().encode_wide().collect();
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Select(&original)
            )
            .is_err()
    );
    assert_eq!(c.samples, samples);
    assert_eq!(watch.snapshot().relative_open_calls, 0);
    assert_eq!(watch.snapshot().ancestors.objects, 0);
    watch.retire().unwrap();
    record("mismatched-selection", &watch, &f, &c, false);
    f.finish();
}

#[test]
fn exact_confirmation_holds_original_objects_and_duplicate_never_reopens() {
    let mut f = Fixture::new();
    let mut c = clock();
    let mut watch = f.prepare(&mut c);
    let mut confirmation = Confirmation::default();
    let selected: Vec<_> = f.path.as_os_str().encode_wide().collect();
    confirmation
        .event(
            &mut watch,
            &f.m,
            &f.h,
            &f.i,
            &mut c,
            &mut || false,
            Event::Select(&selected),
        )
        .unwrap();
    assert!(confirmation.captured);
    let s = watch.snapshot();
    assert_eq!(s.ancestors.objects, 4);
    assert_eq!(s.relative_open_calls, 1);
    assert_eq!(s.held_resources, 6);
    assert!(s.native.unwrap().asynchronous_verified);
    assert!(fs::rename(&f.path, f.path.with_file_name("moved")).is_err());
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Select(&selected)
            )
            .is_err()
    );
    assert_eq!(confirmation.failure, Some(Error::Correlation));
    assert_eq!(watch.snapshot().relative_open_calls, 1);
    let closed = watch.retire().unwrap();
    assert_eq!(closed.native_snapshot().unwrap().confirmed_closed, 2);
    assert_eq!(closed.ancestor_snapshot().confirmed_closed, 4);
    record("exact-selection-and-duplicate", &watch, &f, &c, false);
    f.finish();
}

#[test]
fn original_revocation_refuses_confirmation_without_native_open() {
    let mut f = Fixture::new();
    let mut c = clock();
    let approval = f.approval();
    let revoke = approval.revocation();
    let mut watch = DirectoryWatch::prepare(approval, &f.m, &f.h, &f.i, &mut c).unwrap();
    revoke.revoke();
    let selected: Vec<_> = f.path.as_os_str().encode_wide().collect();
    let mut confirmation = Confirmation::default();
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Select(&selected)
            )
            .is_err()
    );
    assert_eq!(
        confirmation.failure,
        Some(Error::Approval(crate::fs_watch_binding::Error::Denied))
    );
    assert_eq!(watch.snapshot().relative_open_calls, 0);
    watch.retire().unwrap();
    record("revoked-before-selection", &watch, &f, &c, false);
    f.finish();
}

#[test]
fn cancellation_after_confirmation_retires_the_same_captured_objects() {
    let mut f = Fixture::new();
    let mut c = clock();
    let mut watch = f.prepare(&mut c);
    let selected: Vec<_> = f.path.as_os_str().encode_wide().collect();
    let mut confirmation = Confirmation::default();
    let mut checks = 0;
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || {
                    checks += 1;
                    checks == 2
                },
                Event::Select(&selected)
            )
            .is_err()
    );
    assert!(confirmation.captured);
    assert_eq!(
        confirmation.failure,
        Some(Error::Directory(crate::directory_io::Error::Cancelled))
    );
    assert_eq!(watch.snapshot().relative_open_calls, 1);
    let closed = watch.retire().unwrap();
    assert_eq!(closed.native_snapshot().unwrap().confirmed_closed, 2);
    assert_eq!(closed.ancestor_snapshot().confirmed_closed, 4);
    record("cancel-after-selection", &watch, &f, &c, false);
    f.finish();
}

#[test]
fn original_callback_panic_retains_whole_fixture_and_original_job() {
    let f = Fixture::new();
    let mut c = clock();
    let mut watch = f.prepare(&mut c);
    c.panic = true;
    let mut confirmation = Confirmation::default();
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Check
            )
            .is_err()
    );
    assert_eq!(confirmation.failure, Some(Error::Unknown));
    let samples = c.samples;
    assert!(
        confirmation
            .event(
                &mut watch,
                &f.m,
                &f.h,
                &f.i,
                &mut c,
                &mut || false,
                Event::Check
            )
            .is_err()
    );
    assert_eq!(samples, c.samples);
    assert!(matches!(watch.retire(), Err(Error::Unknown)));
    assert!(watch.snapshot().unknown && watch.snapshot().job_held);
    assert_eq!((f.b.usage().resources, f.b.usage().jobs), (0, 1));
    record("panic-retains-original", &watch, &f, &c, false);
    // No disconnect, maintenance, retry or native cleanup after the panic.
    // This exact owner fixture remains until this isolated test process exits.
    let _ = Box::leak(Box::new((f, watch)));
}
