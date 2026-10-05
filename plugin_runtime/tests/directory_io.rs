#![cfg(all(feature = "packages", windows))]
//! Real Windows temporary directories and original Catalog/Registry/Manager/
//! HostRuntime/Control approvals. Ordinary synthetic Store only, not production.
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_fs_directory_v1::{EntryKind, FsDirectoryPage, NameEncoding};
use morrow_plugin_runtime::{
    Limits,
    directory_io::{CaptureLimits, DirectoryBroker, Error, SelectedDirectory},
    io_binding::{Error as AdmissionError, IoBinding},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        process::CommandExt,
    },
    path::{Path, PathBuf},
};

const ID: &str = "org.example.directory-native";
const HANDLER: &str = "directory.immediate-selected";

struct Fixture {
    _temp: tempfile::TempDir,
    data: PathBuf,
    manager: Manager,
    host: HostRuntime,
    package: Package,
    capabilities: BTreeSet<IoCapability>,
}
impl Fixture {
    fn new(caps: &[IoCapability]) -> Self {
        Self::budget(caps, 2, io::MAX_BYTES, io::MAX_JOB_BYTES)
    }
    fn budget(caps: &[IoCapability], resources: u32, bytes: u64, job: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("selected");
        fs::create_dir(&data).unwrap();
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
            (import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let caps: BTreeSet<_> = caps.iter().copied().collect();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut decl = io::declaration(caps.iter().copied().collect(), vec![HANDLER.into()]);
        let budget = decl.budget.as_mut().unwrap();
        budget.max_resources = resources;
        budget.max_jobs = 4;
        budget.max_bytes = bytes;
        budget.max_job_bytes = job;
        budget.max_duration_ms = 10_000;
        manifest.io_declaration = Some(decl);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&temp.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&temp.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(
            Store::open(&temp.path().join("synthetic-store"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        Self {
            _temp: temp,
            data,
            manager,
            host,
            package,
            capabilities: caps,
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn bind(&self, i: &ManagedInstance, expires: u64) -> IoBinding {
        self.manager
            .bind_io(
                &self.host,
                i,
                self.package.digest(),
                self.manager.revision(),
                &self.capabilities,
                expires,
                1,
            )
            .unwrap()
    }
    fn capture(
        &self,
        b: &mut DirectoryBroker,
        i: &ManagedInstance,
        binding: &IoBinding,
        limits: CaptureLimits,
    ) -> Result<SelectedDirectory, Error> {
        b.grant_open_directory(
            &self.manager,
            &self.host,
            i,
            binding,
            open_directory(&self.data),
            limits,
            || 2,
        )
    }
}
fn open_directory(p: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0220_0000)
        .open(p)
        .unwrap()
}
fn list_all(
    f: &Fixture,
    b: &mut DirectoryBroker,
    i: &ManagedInstance,
    binding: &IoBinding,
    s: SelectedDirectory,
) -> Vec<FsDirectoryPage> {
    let mut request = s.first_page();
    let mut pages = Vec::new();
    loop {
        let page = b
            .next_page(&f.manager, &f.host, i, binding, request, || 3)
            .unwrap();
        let wire = page.encode().unwrap();
        let decoded = FsDirectoryPage::decode(&wire).unwrap();
        assert_eq!(decoded.selection_epoch, s.selection_epoch);
        assert_eq!(decoded.page_sequence, request.page_sequence);
        assert_eq!(decoded.entries, page.entries);
        assert!(wire.len() <= 65536);
        let terminal = page.terminal;
        request.page_sequence += 1;
        request.after_entry_id = page.entries.last().map(|e| e.entry_id);
        pages.push(page);
        if terminal {
            break;
        }
    }
    pages
}
fn raw_name(name: &std::ffi::OsStr) -> Vec<u8> {
    name.encode_wide().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn actual_empty_directory_terminal_page_releases_root_resource_job_and_not_bytes() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([11; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert_eq!(s.entries, 0);
    assert_eq!(binding.usage().resources, 1);
    assert_eq!(binding.usage().jobs, 0);
    let spent = binding.usage().bytes;
    assert!(spent >= 16384);
    let pages = list_all(&f, &mut b, &i, &binding, s);
    assert_eq!(pages.len(), 1);
    assert!(pages[0].terminal && pages[0].entries.is_empty());
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert!(binding.usage().bytes > spent);
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 4),
        Err(Error::InvalidCursor)
    );
}

#[test]
fn actual_unicode_zero_long_names_immediate_directories_and_multiple_native_batches_pages() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let mut expected = BTreeMap::new();
    for n in 0..260 {
        let name = if n == 0 {
            OsString::from("雪😀-zero")
        } else if n == 1 {
            OsString::from("x".repeat(230))
        } else {
            OsString::from(format!("entry-{n:04}"))
        };
        let length = n % 19;
        fs::write(f.data.join(&name), vec![n as u8; length]).unwrap();
        expected.insert(raw_name(&name), (EntryKind::File, Some(length as u64)));
    }
    fs::create_dir(f.data.join("child-directory")).unwrap();
    fs::write(f.data.join("child-directory/hidden-grandchild"), [9]).unwrap();
    expected.insert(
        raw_name(std::ffi::OsStr::new("child-directory")),
        (EntryKind::Directory, None),
    );
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([12; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert_eq!(s.entries, expected.len());
    let pages = list_all(&f, &mut b, &i, &binding, s);
    assert!(pages.len() >= 9);
    let mut actual = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for p in pages {
        assert!(p.entries.len() <= 32);
        for e in p.entries {
            assert_eq!(e.encoding, NameEncoding::Utf16Le);
            assert!(ids.insert(e.entry_id));
            actual.insert(e.name, (e.kind, e.logical_length));
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn actual_windows_isolated_surrogate_name_units_are_retained_without_lossy_utf8() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let name = OsString::from_wide(&[b'a' as u16, 0xd800, b'z' as u16]);
    fs::write(f.data.join(&name), [1,2,3]).expect("actual Windows filesystem must support fixture UTF16 name; refusal is retained failure, not skipped PASS");
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([13; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    let pages = list_all(&f, &mut b, &i, &binding, s);
    assert_eq!(pages[0].entries[0].name, raw_name(&name));
    assert_eq!(pages[0].entries[0].logical_length, Some(3));
}

#[test]
fn file_read_and_file_create_approvals_do_not_authorize_list_and_no_native_bytes_admitted() {
    for cap in [IoCapability::FileRead, IoCapability::FileCreate] {
        let mut f = Fixture::new(&[cap]);
        let i = f.connect();
        let binding = f.bind(&i, 9000);
        let mut b = DirectoryBroker::new([14; 32]);
        assert_eq!(
            f.capture(&mut b, &i, &binding, CaptureLimits::default()),
            Err(Error::Admission(AdmissionError::Denied))
        );
        assert_eq!(binding.usage().bytes, 0);
        assert_eq!(binding.usage().jobs, 0);
        assert_eq!(binding.usage().resources, 0);
    }
}

#[test]
fn foreign_manager_host_instance_broker_epoch_sequence_and_entry_cursor_refuse_without_consumption()
{
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..40 {
        fs::write(f.data.join(format!("f-{n}")), [1]).unwrap();
    }
    let i = f.connect();
    let other_i = f.connect();
    let binding = f.bind(&i, 9000);
    let other_binding = f.bind(&other_i, 9000);
    let mut other_f = Fixture::new(&[IoCapability::FileList]);
    let foreign_i = other_f.connect();
    let foreign_binding = other_f.bind(&foreign_i, 9000);
    let mut b = DirectoryBroker::new([15; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    let spent = binding.usage().bytes;
    assert!(matches!(
        b.next_page(
            &other_f.manager,
            &other_f.host,
            &foreign_i,
            &foreign_binding,
            s.first_page(),
            || 3
        ),
        Err(Error::Admission(AdmissionError::Denied))
    ));
    assert!(matches!(
        b.next_page(
            &f.manager,
            &f.host,
            &other_i,
            &other_binding,
            s.first_page(),
            || 3
        ),
        Err(Error::Admission(AdmissionError::Denied))
    ));
    assert!(matches!(
        b.next_page(
            &f.manager,
            &other_f.host,
            &i,
            &binding,
            s.first_page(),
            || 3
        ),
        Err(Error::Admission(AdmissionError::Denied))
    ));
    let mut foreign_b = DirectoryBroker::new([16; 32]);
    assert_eq!(
        foreign_b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 3),
        Err(Error::InvalidCursor)
    );
    let mut bad = s.first_page();
    bad.selection_epoch = [9; 32];
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, bad, || 3),
        Err(Error::InvalidCursor)
    );
    bad = s.first_page();
    bad.page_sequence = 0;
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, bad, || 3),
        Err(Error::InvalidCursor)
    );
    bad = s.first_page();
    bad.after_entry_id = Some([8; 32]);
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, bad, || 3),
        Err(Error::InvalidCursor)
    );
    assert_eq!(binding.usage().bytes, spent);
    assert_eq!(b.usage().0, 1);
    let p = b
        .next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 3)
        .unwrap();
    assert!(!p.terminal);
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 4),
        Err(Error::InvalidCursor)
    );
    let mut next = s.first_page();
    next.page_sequence = 2;
    next.after_entry_id = Some([7; 32]);
    assert_eq!(
        b.next_page(&f.manager, &f.host, &i, &binding, next, || 4),
        Err(Error::InvalidCursor)
    );
    next.after_entry_id = p.entries.last().map(|e| e.entry_id);
    assert!(
        b.next_page(&f.manager, &f.host, &i, &binding, next, || 4)
            .unwrap()
            .terminal
    );
}

#[test]
fn actual_1025_children_refuse_without_partial_reference_and_release_slots_not_bytes() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..1025 {
        File::create(f.data.join(format!("f{n:04}"))).unwrap();
    }
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([17; 32]);
    assert_eq!(
        f.capture(&mut b, &i, &binding, CaptureLimits::default()),
        Err(Error::Limit)
    );
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert!(binding.usage().bytes > 0);
    fs::remove_file(f.data.join("f1024")).unwrap();
    let accepted = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert_eq!(accepted.entries, 1024);
    let pages = list_all(&f, &mut b, &i, &binding, accepted);
    assert_eq!(pages.len(), 32);
    assert_eq!(pages.iter().map(|p| p.entries.len()).sum::<usize>(), 1024);
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn actual_host_metadata_entry_and_original_job_cumulative_resource_limits_fail_closed() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("child"), [0]).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([18; 32]);
    let mut limits = CaptureLimits {
        max_metadata_bytes: 1,
        ..CaptureLimits::default()
    };
    assert_eq!(f.capture(&mut b, &i, &binding, limits), Err(Error::Limit));
    limits = CaptureLimits {
        max_entries: 0,
        ..CaptureLimits::default()
    };
    assert_eq!(f.capture(&mut b, &i, &binding, limits), Err(Error::Limit));
    assert_eq!(b.usage(), (0, 0));
    let mut small = Fixture::budget(&[IoCapability::FileList], 1, 40_000, 40_000);
    let si = small.connect();
    let sb = small.bind(&si, 9000);
    limits = CaptureLimits {
        max_job_bytes: 40_000,
        ..CaptureLimits::default()
    };
    assert!(matches!(
        small.capture(&mut b, &si, &sb, limits),
        Err(Error::Admission(AdmissionError::Limit))
    ));
    assert_eq!(sb.usage().resources, 0);
    assert_eq!(sb.usage().jobs, 0);
    assert!(sb.usage().bytes > 0);
    let mut one = Fixture::budget(
        &[IoCapability::FileList],
        1,
        io::MAX_BYTES,
        io::MAX_JOB_BYTES,
    );
    let oi = one.connect();
    let ob = one.bind(&oi, 9000);
    let s = one
        .capture(&mut b, &oi, &ob, CaptureLimits::default())
        .unwrap();
    assert!(matches!(
        one.capture(&mut b, &oi, &ob, CaptureLimits::default()),
        Err(Error::Admission(AdmissionError::Limit))
    ));
    let before_cancel_bytes = ob.usage().bytes;
    b.cancel(&one.manager, &one.host, &oi, &ob, s, 3).unwrap();
    assert_eq!(ob.usage().resources, 0);
    assert_eq!(ob.usage().bytes, before_cancel_bytes);
    // 1001 names of 230 UTF16 units: each retained-entry/name guard costs
    // 128+2*460=1048 bytes. The 1MiB metadata ceiling fails before 1024 entries;
    // native buffer/job allowances are independently below the 16MiB job cap.
    let mut large = Fixture::new(&[IoCapability::FileList]);
    for n in 0..1001 {
        File::create(large.data.join(format!("{}-{n:04}", "m".repeat(225)))).unwrap();
    }
    let li = large.connect();
    let lb = large.bind(&li, 9000);
    assert_eq!(
        large.capture(&mut b, &li, &lb, CaptureLimits::default()),
        Err(Error::Limit)
    );
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(lb.usage().resources, 0);
    assert_eq!(lb.usage().jobs, 0);
}

#[test]
fn stop_and_registry_revoke_suppress_delivery_then_original_maintenance_releases() {
    for revoke in [false, true] {
        let mut f = Fixture::new(&[IoCapability::FileList]);
        let i = f.connect();
        let binding = f.bind(&i, 9000);
        let mut b = DirectoryBroker::new([19; 32]);
        let s = f
            .capture(&mut b, &i, &binding, CaptureLimits::default())
            .unwrap();
        let bytes = binding.usage().bytes;
        if revoke {
            f.manager
                .set_enabled(ID, f.package.digest(), false, f.manager.revision())
                .unwrap();
        } else {
            i.request_stop();
        }
        assert!(matches!(
            b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 3),
            Err(Error::Admission(AdmissionError::Denied))
        ));
        // Preflight denial withholds delivery but does not run trusted idle cleanup.
        assert_eq!(b.usage().0, 1);
        assert_eq!(binding.usage().resources, 1);
        b.reap(3);
        assert_eq!(b.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
        assert_eq!(binding.usage().bytes, bytes);
    }
}

#[test]
fn original_lease_expiry_and_backwards_clock_prevent_capture_or_delivery() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let i = f.connect();
    let binding = f.bind(&i, 20);
    let mut b = DirectoryBroker::new([20; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert!(matches!(
        b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 1),
        Err(Error::Admission(AdmissionError::Clock))
    ));
    assert!(matches!(
        b.next_page(&f.manager, &f.host, &i, &binding, s.first_page(), || 20),
        Err(Error::Admission(AdmissionError::Expired))
    ));
    b.reap(20);
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn stop_during_actual_capture_batch_withholds_all_observation_and_releases() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..200 {
        File::create(f.data.join(format!("f{n}"))).unwrap();
    }
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([21; 32]);
    let mut samples = 0;
    let result = b.grant_open_directory(
        &f.manager,
        &f.host,
        &i,
        &binding,
        open_directory(&f.data),
        CaptureLimits::default(),
        || {
            samples += 1;
            if samples == 18 {
                i.request_stop();
            }
            2
        },
    );
    assert!(matches!(
        result,
        Err(Error::Admission(AdmissionError::Denied))
    ));
    assert!(samples >= 18);
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert!(binding.usage().bytes > 0);
}

#[test]
fn retained_root_survives_namespace_rename_replacement_and_not_guest_path_reopen() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("original-child"), [4]).unwrap();
    let root = open_directory(&f.data);
    let renamed = f._temp.path().join("renamed-original");
    fs::rename(&f.data, &renamed).unwrap();
    fs::create_dir(&f.data).unwrap();
    fs::write(f.data.join("replacement-child"), [7]).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([22; 32]);
    let s = b
        .grant_open_directory(
            &f.manager,
            &f.host,
            &i,
            &binding,
            root,
            CaptureLimits::default(),
            || 2,
        )
        .unwrap();
    let pages = list_all(&f, &mut b, &i, &binding, s);
    assert_eq!(
        pages[0].entries[0].name,
        raw_name(std::ffi::OsStr::new("original-child"))
    );
    assert!(renamed.join("original-child").exists());
    assert!(f.data.join("replacement-child").exists());
}

fn junction(link: &Path, target: &Path) {
    let result = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "synthetic unprivileged junction creation refused; no skip/privilege change"
    );
}
#[test]
fn actual_junction_root_or_child_is_refused_without_following_target_or_partial_reference() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let target = f._temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("never-follow"), [8]).unwrap();
    let root_link = f._temp.path().join("root-link");
    junction(&root_link, &target);
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([23; 32]);
    assert_eq!(
        b.grant_open_directory(
            &f.manager,
            &f.host,
            &i,
            &binding,
            open_directory(&root_link),
            CaptureLimits::default(),
            || 2
        ),
        Err(Error::ReparsePoint)
    );
    fs::write(f.data.join("ordinary-before-link"), [0]).unwrap();
    junction(&f.data.join("child-link"), &target);
    assert_eq!(
        f.capture(&mut b, &i, &binding, CaptureLimits::default()),
        Err(Error::ReparsePoint)
    );
    assert_eq!(b.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert!(target.join("never-follow").exists());
}

#[test]
fn selected_regular_file_is_not_directory_and_broker_drop_releases_original_resource() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("regular"), [5]).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([24; 32]);
    assert_eq!(
        b.grant_open_directory(
            &f.manager,
            &f.host,
            &i,
            &binding,
            File::open(f.data.join("regular")).unwrap(),
            CaptureLimits::default(),
            || 2
        ),
        Err(Error::NotDirectory)
    );
    let attributes_only = OpenOptions::new()
        .read(true)
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x0220_0000)
        .open(&f.data)
        .unwrap();
    assert_eq!(
        b.grant_open_directory(
            &f.manager,
            &f.host,
            &i,
            &binding,
            attributes_only,
            CaptureLimits::default(),
            || 2
        ),
        Err(Error::Io(std::io::ErrorKind::PermissionDenied))
    );
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert_eq!(s.entries, 1);
    assert_eq!(binding.usage().resources, 1);
    let bytes = binding.usage().bytes;
    drop(b);
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().bytes, bytes);
}

#[test]
fn exact_manager_digest_revision_and_requested_capabilities_gate_original_binding() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let i = f.connect();
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &i,
                [99; 32],
                f.manager.revision(),
                &f.capabilities,
                9000,
                1
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &i,
                f.package.digest(),
                f.manager.revision().saturating_add(1),
                &f.capabilities,
                9000,
                1
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &i,
                f.package.digest(),
                f.manager.revision(),
                &BTreeSet::from([IoCapability::FileRead]),
                9000,
                1
            )
            .is_err()
    );
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([25; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    b.cancel(&f.manager, &f.host, &i, &binding, s, 3).unwrap();
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn hardlinks_with_same_native_file_id_receive_distinct_name_scoped_opaque_ids() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("first"), [1, 2, 3, 4]).unwrap();
    fs::hard_link(f.data.join("first"), f.data.join("second")).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut b = DirectoryBroker::new([26; 32]);
    let s = f
        .capture(&mut b, &i, &binding, CaptureLimits::default())
        .unwrap();
    let pages = list_all(&f, &mut b, &i, &binding, s);
    assert_eq!(pages[0].entries.len(), 2);
    assert_ne!(pages[0].entries[0].entry_id, pages[0].entries[1].entry_id);
    assert_ne!(pages[0].entries[0].name, pages[0].entries[1].name);
    assert!(pages[0].entries.iter().all(|e| e.logical_length == Some(4)));
}

#[test]
fn stop_after_actual_page_encoding_withholds_delivery_releases_root_and_preserves_charges() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("metadata-only"), [6; 17]).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut broker = DirectoryBroker::new([27; 32]);
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    let spent = binding.usage().bytes;
    let mut samples = 0;
    let result = broker.next_page(
        &f.manager,
        &f.host,
        &i,
        &binding,
        selected.first_page(),
        || {
            samples += 1;
            // Checks 1..5 precede/complete the retained-root query; the sixth
            // checkpoint follows real page encode/validate, before final delivery.
            if samples == 6 {
                i.request_stop();
            }
            3
        },
    );
    assert_eq!(result, Err(Error::Admission(AdmissionError::Denied)));
    assert_eq!(samples, 6);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    // Request metadata + full response capacity + two root-query reservations.
    // Even the last query suppressed by Stop is reserved, without any refund.
    assert_eq!(binding.usage().bytes, spent + 73 + 65536 + 64);
}
