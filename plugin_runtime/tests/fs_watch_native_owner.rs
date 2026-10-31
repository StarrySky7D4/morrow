//! Actual local Windows directory effects on one original thread per case.
//! These checks do not qualify the managed worker, picker, guest dispatch or UI.
#![cfg(all(feature = "packages", windows))]
#![deny(warnings)]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_fs_watch_v1::{Batch, End, Kind, wire_digest};
use morrow_plugin_runtime::{
    Limits,
    directory_io::{
        DirectoryAbsolutePath,
        watch::{Closed, DirectoryWatch, Error, Poll, WatchClock},
    },
    fs_watch_binding::{self, ApprovalRequest, WatchApproval},
    io_binding::{IoBinding, Usage},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    os::windows::process::CommandExt,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    time::Duration,
};
const ID: &str = "org.example.watch.native";
const GOOD: &str = r#"(module
    (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
    (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
    (memory (export "memory") 1 256)
    (func (export "morrow_run") (result i32) i32.const 0))"#;
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileList])
}
struct Clock {
    now: u64,
    samples: usize,
}
impl WatchClock for Clock {
    fn with<T>(&mut self, step: impl FnOnce(u64) -> Result<T, Error>) -> Result<T, Error> {
        self.samples += 1;
        step(self.now)
    }
}
struct Fixture {
    manager: Manager,
    host: HostRuntime,
    package: Package,
    dir: tempfile::TempDir,
    observed: PathBuf,
    label: &'static str,
}
impl Fixture {
    fn new(label: &'static str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Qualification supplies a fresh short D:\build_cache temp root.
        // Retained fixtures contain synthetic data only, never user's files.
        let root = Path::new(r"D:\build_cache");
        assert!(dir.path().starts_with(root));
        for ancestor in dir.path().ancestors().take_while(|p| *p != root) {
            let metadata = fs::symlink_metadata(ancestor).unwrap();
            use std::os::windows::fs::MetadataExt;
            assert_eq!(metadata.file_attributes() & 0x400, 0);
            assert!(!ancestor.join(".git").exists());
        }
        let wasm = wat::parse_str(GOOD).unwrap();
        let mut manifest =
            Package::manifest_for_fs_watch(ID, "1.0.0", &wasm, vec!["observe".into()]);
        let declared = manifest.fs_watch_declaration.as_mut().unwrap();
        declared.max_frames = 8;
        declared.max_wire_bytes = 512 * 1024;
        declared.max_window_ms = 25_000;
        let package = Package::build(manifest, &wasm).unwrap();
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
        manager
            .approve_fs_watch(ID, package.digest(), true, manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(
            Store::open(&dir.path().join("store"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let observed = dir.path().join("observed");
        fs::create_dir(&observed).unwrap();
        Self {
            manager,
            host,
            package,
            dir,
            observed,
            label,
        }
    }
    fn request(&self, path: &Path, window_ms: u64) -> ApprovalRequest {
        ApprovalRequest {
            expected_revision: self.manager.revision(),
            handler: "observe".into(),
            nominated_path: DirectoryAbsolutePath::new(path).unwrap(),
            max_frames: 8,
            max_wire_bytes: 512 * 1024,
            window_ms,
        }
    }
    fn approve(
        &self,
        i: &ManagedInstance,
        b: &IoBinding,
        path: &Path,
        window_ms: u64,
    ) -> WatchApproval {
        WatchApproval::issue(
            &self.manager,
            &self.host,
            i,
            b,
            self.request(path, window_ms),
            1,
        )
        .unwrap()
    }
}
fn record(label: &str, suffix: &str, bytes: &[u8]) {
    if let Some(root) = std::env::var_os("MORROW_WATCH_NATIVE_EVIDENCE") {
        let root = PathBuf::from(root);
        assert!(root.starts_with(r"D:\build_cache"));
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!("{label}-{suffix}")))
            .unwrap();
        out.write_all(bytes).unwrap();
        out.sync_all().unwrap();
    }
}
fn original_thread(
    label: &'static str,
    case: impl FnOnce(&mut Fixture, &ManagedInstance, &IoBinding, &mut Clock) + Send + 'static,
) {
    let parent = std::thread::current().id();
    let original = std::thread::spawn(move || {
        assert_ne!(std::thread::current().id(), parent);
        let mut f = Fixture::new(label);
        let i = f.manager.connect(ID, &mut f.host).unwrap();
        let b = f
            .manager
            .bind_io(
                &f.host,
                &i,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                29_000,
                1,
            )
            .unwrap();
        let mut clock = Clock { now: 1, samples: 0 };
        let result = catch_unwind(AssertUnwindSafe(|| case(&mut f, &i, &b, &mut clock)));
        let usage = b.usage();
        let disconnected = i.close(&mut f.host);
        let path = f.dir.keep();
        record(label, "worker.txt", format!("worker={:?}\nfixture={path:?}\nusage={usage:?}\nsamples={}\ndisconnect={disconnected:?}\n", std::thread::current().id(), clock.samples).as_bytes());
        assert_eq!(usage.resources, 0);
        assert_eq!(usage.jobs, 0);
        disconnected.unwrap();
        if let Err(error) = result {
            resume_unwind(error);
        }
    });
    let result = original.join();
    record(
        label,
        "join.txt",
        format!(
            "original_join_returned=true\noriginal_worker_succeeded={}\n",
            result.is_ok()
        )
        .as_bytes(),
    );
    result.unwrap();
}
fn known_retirement(label: &str, owner: &mut DirectoryWatch, b: &IoBinding) -> Closed {
    let before = owner.snapshot();
    let usage_before = b.usage();
    let result = owner.retire();
    let after = owner.snapshot();
    let usage_after = b.usage();
    record(label, "retire.txt", format!("before={before:?}\nusage_before={usage_before:?}\nafter={after:?}\nusage_after={usage_after:?}\nclosed={}\n", result.is_ok()).as_bytes());
    let closed = result.unwrap();
    assert!(closed.ancestor_snapshot().all_closed());
    let ancestors = closed.ancestor_snapshot();
    assert_eq!(ancestors.objects, ancestors.close_calls);
    if let Some(native) = closed.native_snapshot() {
        assert!(!native.unknown && native.retired && !native.read_in_flight);
        assert_eq!(native.owned_handles, native.close_calls);
        assert_eq!(native.close_calls, native.confirmed_closed);
        assert_eq!(native.read_calls, native.completions);
    }
    assert!(!after.unknown && after.retired && !after.job_held);
    assert_eq!(after.held_resources, 0);
    assert_eq!(usage_after.bytes, usage_before.bytes);
    assert_eq!(owner.retire().err(), Some(Error::Closed));
    assert_eq!(format!("{:?}", owner.snapshot()), format!("{after:?}"));
    closed
}
fn exercise(
    f: &mut Fixture,
    i: &ManagedInstance,
    b: &IoBinding,
    c: &mut Clock,
    approval: WatchApproval,
    work: impl FnOnce(&mut DirectoryWatch, &mut Fixture, &ManagedInstance, &IoBinding, &mut Clock),
) -> Closed {
    let mut owner = match DirectoryWatch::start(approval, &f.manager, &f.host, i, c) {
        Ok(owner) => owner,
        Err(mut failure) => {
            if let Some(owner) = failure.owner.as_mut() {
                known_retirement(f.label, owner, b);
            }
            panic!("capture refused: {failure:?}");
        }
    };
    let result = catch_unwind(AssertUnwindSafe(|| work(&mut owner, f, i, b, c)));
    let closed = known_retirement(f.label, &mut owner, b);
    drop(owner);
    if let Err(error) = result {
        resume_unwind(error);
    }
    closed
}
fn frame(
    owner: &mut DirectoryWatch,
    f: &Fixture,
    i: &ManagedInstance,
    c: &mut Clock,
) -> (u64, Vec<u8>, Batch) {
    for _ in 0..40 {
        match owner
            .poll(&f.manager, &f.host, i, c, Duration::from_millis(50))
            .unwrap()
        {
            Poll::Pending => {}
            Poll::DeadlineReached => panic!("observation expired before synthetic event"),
            Poll::Frame { sequence, wire } => {
                let batch = Batch::decode(&wire).unwrap();
                assert_eq!(batch.watch_epoch, owner.epoch());
                assert_eq!(batch.sequence, sequence);
                assert_eq!(batch.end, End::Continue);
                record(f.label, &format!("frame{sequence:03}.capnp"), &wire);
                return (sequence, wire, batch);
            }
        }
    }
    panic!("bounded observation did not produce the expected event");
}
fn exact_name(name: &str) -> Vec<u8> {
    name.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
fn assert_change(batch: &Batch, kind: Kind, name: &str) {
    assert!(
        batch
            .changes
            .iter()
            .any(|c| c.kind == kind && c.name_utf16_le == exact_name(name)),
        "missing {kind:?} in {batch:?}"
    );
}
#[test]
fn real_create_modify_rename_delete_and_exact_ack_preserve_original_accounting() {
    original_thread("events", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 20_000);
        let closed = exercise(f, i, b, c, approval, |w, f, i, b, c| {
            let opened = w.snapshot();
            assert_eq!(b.usage().resources as usize, opened.held_resources);
            assert_eq!(opened.ancestors.objects + 2, opened.held_resources);
            assert_eq!(b.usage().jobs, 1);
            assert!(b.usage().bytes > 0);
            assert!(opened.native.unwrap().asynchronous_verified);
            let old = "原始 😀.txt";
            let new = "renamed 空白.txt";
            let mut next_sequence = 1;
            for step in 0..4 {
                w.arm(&f.manager, &f.host, i, c).unwrap();
                match step {
                    0 => drop(fs::File::create(f.observed.join(old)).unwrap()),
                    1 => {
                        let mut file = fs::File::create(f.observed.join(old)).unwrap();
                        file.write_all(b"synthetic content").unwrap();
                        file.sync_all().unwrap();
                    }
                    2 => fs::rename(f.observed.join(old), f.observed.join(new)).unwrap(),
                    _ => fs::remove_file(f.observed.join(new)).unwrap(),
                }
                let required: &[(Kind, &str)] = match step {
                    0 => &[(Kind::Added, old)],
                    1 => &[(Kind::Modified, old)],
                    2 => &[(Kind::RenameOldName, old), (Kind::RenameNewName, new)],
                    _ => &[(Kind::Removed, new)],
                };
                let mut seen = vec![false; required.len()];
                // Windows buffers changes between reads. A filesystem action
                // need not correspond to exactly one completion or one frame.
                loop {
                    let (sequence, wire, batch) = frame(w, f, i, c);
                    assert_eq!(sequence, next_sequence);
                    next_sequence += 1;
                    for (index, &(kind, name)) in required.iter().enumerate() {
                        seen[index] |= batch.changes.iter().any(|change| {
                            change.kind == kind && change.name_utf16_le == exact_name(name)
                        });
                    }
                    let native = w.snapshot().native.unwrap();
                    let usage = b.usage();
                    let mut wrong = wire_digest(&wire);
                    wrong[0] ^= 1;
                    assert_eq!(
                        w.acknowledge(&f.manager, &f.host, i, c, sequence, wrong),
                        Err(Error::Correlation)
                    );
                    assert_eq!(w.arm(&f.manager, &f.host, i, c), Err(Error::Pending));
                    assert_eq!(w.snapshot().native.unwrap(), native);
                    assert_eq!(b.usage(), usage);
                    w.acknowledge(&f.manager, &f.host, i, c, sequence, wire_digest(&wire))
                        .unwrap();
                    assert_eq!(b.usage(), usage, "ACK is not a resource release");
                    if seen.iter().all(|seen| *seen) {
                        break;
                    }
                    assert!(sequence < 8, "original declared frame budget is finite");
                    w.arm(&f.manager, &f.host, i, c).unwrap();
                }
            }
        });
        let native = closed.native_snapshot().unwrap();
        assert!((4..=8).contains(&native.observed_frames));
        assert_eq!(native.read_calls, native.observed_frames);
        assert!(!native.cancel_called);
    });
}
#[test]
fn quiet_pending_read_is_cancelled_completed_and_closed_once() {
    original_thread("quiet", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 20_000);
        let closed = exercise(f, i, b, c, approval, |w, f, i, b, c| {
            w.arm(&f.manager, &f.host, i, c).unwrap();
            let usage = b.usage();
            assert_eq!(w.arm(&f.manager, &f.host, i, c), Err(Error::Pending));
            assert_eq!(b.usage(), usage);
            assert!(matches!(
                w.poll(&f.manager, &f.host, i, c, Duration::from_millis(20)),
                Ok(Poll::Pending)
            ));
            assert_eq!(w.snapshot().native.unwrap().read_calls, 1);
        });
        let native = closed.native_snapshot().unwrap();
        assert!(native.cancel_called);
        assert_eq!(native.observed_frames, 0);
        assert_eq!(native.read_calls, 1);
    });
}
#[test]
fn revoked_window_with_pending_completion_delivers_nothing_and_still_retires() {
    original_thread("revoked", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 20_000);
        let revoke = approval.revocation();
        let closed = exercise(f, i, b, c, approval, |w, f, i, b, c| {
            w.arm(&f.manager, &f.host, i, c).unwrap();
            let usage = b.usage();
            revoke.revoke();
            fs::write(f.observed.join("late-not-delivered"), b"synthetic").unwrap();
            assert!(matches!(
                w.poll(&f.manager, &f.host, i, c, Duration::ZERO),
                Err(Error::Approval(fs_watch_binding::Error::Denied))
            ));
            assert_eq!(b.usage(), usage);
            assert_eq!(w.snapshot().native.unwrap().observed_frames, 0);
        });
        assert_eq!(closed.native_snapshot().unwrap().observed_frames, 0);
    });
}
#[test]
fn logical_expiry_cannot_be_rescued_and_cleanup_does_not_need_a_new_grant() {
    original_thread("expiry-tick", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 20_000);
        exercise(f, i, b, c, approval, |w, f, i, b, c| {
            w.arm(&f.manager, &f.host, i, c).unwrap();
            let usage = b.usage();
            c.now = 20_001;
            assert!(matches!(
                w.poll(&f.manager, &f.host, i, c, Duration::ZERO),
                Err(Error::Approval(fs_watch_binding::Error::Expired))
            ));
            c.now = 2;
            assert_eq!(w.arm(&f.manager, &f.host, i, c), Err(Error::Closed));
            assert_eq!(b.usage(), usage);
        });
    });
}
#[test]
fn real_deadline_withholds_late_frame_even_when_host_tick_does_not_advance() {
    original_thread("expiry-real", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 750);
        exercise(f, i, b, c, approval, |w, f, i, b, c| {
            w.arm(&f.manager, &f.host, i, c).unwrap();
            fs::write(f.observed.join("late-real-deadline"), b"synthetic").unwrap();
            let usage = b.usage();
            std::thread::sleep(Duration::from_millis(800));
            assert!(matches!(
                w.poll(&f.manager, &f.host, i, c, Duration::ZERO),
                Err(Error::Approval(fs_watch_binding::Error::Expired))
            ));
            assert_eq!(b.usage(), usage);
            assert_eq!(w.snapshot().native.unwrap().observed_frames, 0);
        });
    });
}
fn refusal(
    f: &Fixture,
    i: &ManagedInstance,
    b: &IoBinding,
    c: &mut Clock,
    path: &Path,
) -> (Error, morrow_plugin_runtime::directory_io::watch::Snapshot) {
    let approval = f.approve(i, b, path, 20_000);
    let mut failure = DirectoryWatch::start(approval, &f.manager, &f.host, i, c).unwrap_err();
    let owner = failure
        .owner
        .as_mut()
        .expect("original admitted capture retained");
    record(
        f.label,
        "refusal.txt",
        format!(
            "error={:?}\nsnapshot={:?}\n",
            failure.error,
            owner.snapshot()
        )
        .as_bytes(),
    );
    let snapshot = owner.snapshot();
    // Retire the original admitted owner before any assertion about which
    // layer refused the source. A reparse object may itself have been opened
    // without following it, then rejected by the same-handle attribute check.
    known_retirement(f.label, owner, b);
    assert!(!snapshot.unknown);
    if let Some(native) = snapshot.native {
        assert_eq!(native.read_calls, 0);
        assert_eq!(native.observed_frames, 0);
        assert!(!native.mode_query_pending);
    }
    (failure.error, snapshot)
}
#[test]
fn nonexistent_final_component_preserves_original_failure_and_closes_ancestors() {
    original_thread("missing", |f, i, b, c| {
        let path = f.dir.path().join("not-present");
        let (error, snapshot) = refusal(f, i, b, c, &path);
        assert!(matches!(error, Error::RelativeOpen(status) if status < 0));
        assert!(snapshot.native.is_none());
        assert_eq!(snapshot.relative_open_calls, 1);
        assert!(!path.exists());
    });
}
#[test]
fn ordinary_file_cannot_be_used_as_a_directory_watch() {
    original_thread("file", |f, i, b, c| {
        let path = f.dir.path().join("ordinary-file");
        fs::write(&path, b"not a directory").unwrap();
        let (error, snapshot) = refusal(f, i, b, c, &path);
        assert!(matches!(error, Error::RelativeOpen(status) if status < 0));
        assert!(snapshot.native.is_none());
        assert_eq!(snapshot.relative_open_calls, 1);
        assert_eq!(fs::read(&path).unwrap(), b"not a directory");
    });
}
fn junction(f: &Fixture, link: &Path, target: &Path) {
    const CMD: &str = r"C:\Windows\System32\cmd.exe";
    assert!(link.starts_with(f.dir.path()) && target.starts_with(f.dir.path()));
    assert!(!link.exists() && target.is_dir());
    let status = std::process::Command::new(CMD)
        .args(["/D", "/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .current_dir(f.dir.path())
        .env_clear()
        .env("SYSTEMROOT", r"C:\Windows")
        .env("WINDIR", r"C:\Windows")
        .env("COMSPEC", CMD)
        .env("PATH", r"C:\Windows\System32")
        .env("TEMP", f.dir.path())
        .env("TMP", f.dir.path())
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert_eq!(
        status.status.code(),
        Some(0),
        "junction creation must actually succeed"
    );
}
#[test]
fn actual_leaf_junction_is_refused_without_observing_target() {
    original_thread("junction-leaf", |f, i, b, c| {
        let link = f.dir.path().join("junction");
        junction(f, &link, &f.observed);
        fs::write(f.observed.join("bait"), b"must not be observed").unwrap();
        let (error, snapshot) = refusal(f, i, b, c, &link);
        assert_eq!(snapshot.relative_open_calls, 1);
        match error {
            Error::RelativeOpen(status) if status < 0 => assert!(snapshot.native.is_none()),
            Error::Native(morrow_fs_watch_native_v1::windows::Fault::Identity) => {
                let native = snapshot.native.unwrap();
                assert_eq!(native.owned_handles, 1);
                assert_eq!(native.mode_query_calls, 0);
                assert!(!native.identity_checked);
            }
            other => panic!("unexpected refusal: {other:?}"),
        }
    });
}
#[test]
fn actual_intermediate_junction_is_refused_before_final_native_open() {
    original_thread("junction-parent", |f, i, b, c| {
        let link = f.dir.path().join("junction");
        fs::create_dir(f.observed.join("leaf")).unwrap();
        junction(f, &link, &f.observed);
        let (error, snapshot) = refusal(f, i, b, c, &link.join("leaf"));
        assert_eq!(snapshot.relative_open_calls, 0);
        assert!(snapshot.native.is_none());
        match error {
            Error::Directory(morrow_plugin_runtime::directory_io::Error::ReparsePoint) => {
                assert_eq!(snapshot.ancestors.objects, 5);
            }
            Error::ParentOpen(_, _) => assert_eq!(snapshot.ancestors.objects, 4),
            other => panic!("unexpected refusal: {other:?}"),
        }
    });
}
#[test]
fn other_original_reservations_are_not_refunded_by_watch_admission_failure() {
    original_thread("shared-capacity", |f, i, b, c| {
        let held = b
            .admit(&f.manager, &f.host, i, IoCapability::FileList, 4, 1, 1)
            .unwrap();
        assert_eq!(
            b.usage(),
            Usage {
                resources: 4,
                jobs: 1,
                bytes: 1
            }
        );
        let approval = f.approve(i, b, &f.observed, 20_000);
        let mut failure = DirectoryWatch::start(approval, &f.manager, &f.host, i, c).unwrap_err();
        assert_eq!(
            failure.error,
            Error::Admission(morrow_plugin_runtime::io_binding::Error::Limit)
        );
        let owner = failure.owner.as_mut().unwrap();
        assert_eq!(owner.snapshot().relative_open_calls, 0);
        assert_eq!(owner.snapshot().ancestors.objects, 0);
        assert_eq!(owner.snapshot().held_resources, 4);
        known_retirement(f.label, owner, b);
        assert_eq!(b.usage().resources, 4);
        assert_eq!(b.usage().jobs, 1);
        let bytes = b.usage().bytes;
        assert!(bytes > 1);
        drop(held);
        assert_eq!(
            b.usage(),
            Usage {
                resources: 0,
                jobs: 0,
                bytes
            }
        );
    });
}
#[test]
fn foreign_callers_cannot_sample_clock_charge_or_poison_the_original_watch() {
    original_thread("foreign", |f, i, b, c| {
        let approval = f.approve(i, b, &f.observed, 20_000);
        exercise(f, i, b, c, approval, |w, f, i, b, c| {
            let mut other = Fixture::new("foreign-other");
            let j = other.manager.connect(ID, &mut other.host).unwrap();
            w.arm(&f.manager, &f.host, i, c).unwrap();
            let usage = b.usage();
            let snapshot = format!("{:?}", w.snapshot());
            let mut wrong_clock = Clock {
                now: u64::MAX,
                samples: 0,
            };
            for (manager, host, instance) in [
                (&other.manager, &f.host, i),
                (&f.manager, &other.host, i),
                (&f.manager, &f.host, &j),
            ] {
                assert!(w.arm(manager, host, instance, &mut wrong_clock).is_err());
                assert!(
                    w.poll(manager, host, instance, &mut wrong_clock, Duration::ZERO)
                        .is_err()
                );
                assert!(
                    w.acknowledge(manager, host, instance, &mut wrong_clock, 1, [0; 32])
                        .is_err()
                );
            }
            assert_eq!(wrong_clock.samples, 0);
            assert_eq!(b.usage(), usage);
            assert_eq!(format!("{:?}", w.snapshot()), snapshot);
            fs::write(f.observed.join("legitimate"), b"original owner").unwrap();
            let (sequence, bytes, batch) = frame(w, f, i, c);
            assert_change(&batch, Kind::Added, "legitimate");
            w.acknowledge(&f.manager, &f.host, i, c, sequence, wire_digest(&bytes))
                .unwrap();
            j.close(&mut other.host).unwrap();
            let other_path = other.dir.keep();
            record(
                f.label,
                "foreign-other.txt",
                format!(
                    "original_other_fixture={other_path:?}\noriginal_other_disconnect=Ok(())\n"
                )
                .as_bytes(),
            );
        });
    });
}
#[test]
fn captured_leaf_and_parent_cannot_be_renamed_until_original_closes() {
    original_thread("protected-chain", |f, i, b, c| {
        let parent = f.dir.path().join("protected-parent");
        let leaf = parent.join("selected");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&leaf).unwrap();
        let approval = f.approve(i, b, &leaf, 20_000);
        exercise(f, i, b, c, approval, |w, f, i, _, c| {
            w.arm(&f.manager, &f.host, i, c).unwrap();
            assert_eq!(
                fs::rename(&leaf, parent.join("renamed"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert_eq!(
                fs::rename(&parent, f.dir.path().join("moved-parent"))
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert!(matches!(
                w.poll(&f.manager, &f.host, i, c, Duration::from_millis(10)),
                Ok(Poll::Pending)
            ));
        });
        fs::rename(&leaf, parent.join("renamed")).unwrap();
        fs::rename(&parent, f.dir.path().join("moved-parent")).unwrap();
    });
}
#[test]
fn volume_root_nomination_is_explicitly_unsupported_without_native_or_budget_work() {
    original_thread("volume-root", |f, i, b, c| {
        let approval = f.approve(i, b, Path::new(r"D:\"), 20_000);
        let failure = DirectoryWatch::start(approval, &f.manager, &f.host, i, c).unwrap_err();
        assert_eq!(failure.error, Error::VolumeRootUnsupported);
        assert!(failure.owner.is_none());
        assert_eq!(b.usage(), Usage::default());
        record(
            f.label,
            "refusal.txt",
            b"VolumeRootUnsupported; owner=None; native_work=0; usage=0\n",
        );
    });
}
