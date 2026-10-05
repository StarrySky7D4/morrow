#![cfg(all(feature = "packages", windows))]
//! Cancellation-only checkpoints on real Windows synthetic directories and the
//! original managed approval/budget chain. No production protected-owner proof.
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
use morrow_plugin_runtime::{
    Limits,
    directory_io::{CaptureLimits, DirectoryBroker, Error, SelectedDirectory},
    io_binding::{Error as AdmissionError, IoBinding},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
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

#[test]
fn cancelled_capture_before_first_native_query_releases_leases_without_fee_refund() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut broker = DirectoryBroker::new([31; 32]);
    let result = broker.grant_open_directory_with_cancel(
        &f.manager,
        &f.host,
        &i,
        &binding,
        open_directory(&f.data),
        CaptureLimits::default(),
        || 2,
        || true,
    );
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().bytes, 32); // initial query allowance was admitted
    // The original instance remains approved and usable; cancel is command-local.
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert!(
        broker
            .next_page(
                &f.manager,
                &f.host,
                &i,
                &binding,
                selected.first_page(),
                || 3
            )
            .unwrap()
            .terminal
    );
    assert!(binding.usage().bytes > 32);
}

#[test]
fn cancelled_capture_after_actual_native_batch_discards_all_observation_and_keeps_costs() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..150 {
        File::create(f.data.join(format!("batch-{n:04}"))).unwrap();
    }
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut broker = DirectoryBroker::new([32; 32]);
    let checks = AtomicUsize::new(0);
    let result = broker.grant_open_directory_with_cancel(
        &f.manager,
        &f.host,
        &i,
        &binding,
        open_directory(&f.data),
        CaptureLimits::default(),
        || 2,
        || checks.fetch_add(1, Ordering::Relaxed) + 1 == 7,
    );
    // Checkpoint7 follows the first actual Buffer::next call, including its
    // unchanged bounded native parser. No callback can preempt that OS query.
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(checks.load(Ordering::Relaxed), 7);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().bytes, 32 + 16384 + 32 + 32768);
    let spent = binding.usage().bytes;
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    assert_eq!(selected.entries, 150);
    broker
        .cancel(&f.manager, &f.host, &i, &binding, selected, 3)
        .unwrap();
    assert!(binding.usage().bytes > spent);
    assert_eq!(binding.usage().resources, 0);
}

#[test]
fn cancelled_capture_before_broker_entry_retention_returns_no_partial_reference() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    fs::write(f.data.join("metadata-child"), [1, 2]).unwrap();
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut broker = DirectoryBroker::new([33; 32]);
    let checks = AtomicUsize::new(0);
    let result = broker.grant_open_directory_with_cancel(
        &f.manager,
        &f.host,
        &i,
        &binding,
        open_directory(&f.data),
        CaptureLimits::default(),
        || 2,
        || checks.fetch_add(1, Ordering::Relaxed) + 1 == 10,
    );
    // Checkpoint10 is after metadata admission and before the duplicate-name
    // guard clone/resident entry allocation. Native row parsing is already done.
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(checks.load(Ordering::Relaxed), 10);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    let name_bytes = "metadata-child".encode_utf16().count() as u64 * 2;
    assert_eq!(
        binding.usage().bytes,
        32 + 16384 + 32 + 32768 + 32 + 128 + name_bytes * 2
    );
}

#[test]
fn cancelled_page_after_actual_encoding_retires_nonterminal_cursor_without_delivery_or_refund() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..40 {
        File::create(f.data.join(format!("page-{n:04}"))).unwrap();
    }
    let i = f.connect();
    let binding = f.bind(&i, 9000);
    let mut broker = DirectoryBroker::new([34; 32]);
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    let spent = binding.usage().bytes;
    let checks = AtomicUsize::new(0);
    let result = broker.next_page_with_cancel(
        &f.manager,
        &f.host,
        &i,
        &binding,
        selected.first_page(),
        || 3,
        || checks.fetch_add(1, Ordering::Relaxed) + 1 == 4,
    );
    // Checkpoint4 immediately follows real SDK defaultAllocator encoding.
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(checks.load(Ordering::Relaxed), 4);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    assert_eq!(binding.usage().bytes, spent + 73 + 65536 + 64);
    // Neither the old cursor nor a guessed next cursor can retrieve a page.
    assert_eq!(
        broker.next_page(
            &f.manager,
            &f.host,
            &i,
            &binding,
            selected.first_page(),
            || 3
        ),
        Err(Error::InvalidCursor)
    );
    let mut guessed = selected.first_page();
    guessed.page_sequence += 1;
    assert_eq!(
        broker.next_page(&f.manager, &f.host, &i, &binding, guessed, || 3),
        Err(Error::InvalidCursor)
    );
}

#[test]
fn cancellation_before_cursor_commit_retires_only_that_selection_other_instance_stays_live() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    for n in 0..40 {
        File::create(f.data.join(format!("owned-{n:04}"))).unwrap();
    }
    let i = f.connect();
    let other = f.connect();
    let binding = f.bind(&i, 9000);
    let other_binding = f.bind(&other, 9000);
    let mut broker = DirectoryBroker::new([35; 32]);
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    let unrelated = f
        .capture(
            &mut broker,
            &other,
            &other_binding,
            CaptureLimits::default(),
        )
        .unwrap();
    let other_spent = other_binding.usage().bytes;
    let checks = AtomicUsize::new(0);
    let result = broker.next_page_with_cancel(
        &f.manager,
        &f.host,
        &i,
        &binding,
        selected.first_page(),
        || 3,
        || checks.fetch_add(1, Ordering::Relaxed) + 1 == 7,
    );
    assert_eq!(result, Err(Error::Cancelled));
    assert_eq!(checks.load(Ordering::Relaxed), 7);
    assert_eq!(broker.usage().0, 1);
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(other_binding.usage().resources, 1);
    assert_eq!(other_binding.usage().bytes, other_spent);
    let page = broker
        .next_page(
            &f.manager,
            &f.host,
            &other,
            &other_binding,
            unrelated.first_page(),
            || 3,
        )
        .unwrap();
    assert!(!page.terminal);
    assert_eq!(page.page_sequence, 1);
    assert_eq!(page.entries.len(), 32);
    broker
        .cancel(&f.manager, &f.host, &other, &other_binding, unrelated, 3)
        .unwrap();
    assert_eq!(other_binding.usage().resources, 0);
    assert_eq!(broker.usage(), (0, 0));
}

#[test]
fn foreign_owner_epoch_sequence_and_cursor_reject_before_veto_and_cannot_erase_selection() {
    let mut f = Fixture::new(&[IoCapability::FileList]);
    let i = f.connect();
    let other = f.connect();
    let binding = f.bind(&i, 9000);
    let other_binding = f.bind(&other, 9000);
    let mut broker = DirectoryBroker::new([36; 32]);
    let selected = f
        .capture(&mut broker, &i, &binding, CaptureLimits::default())
        .unwrap();
    let spent = binding.usage().bytes;
    assert_eq!(
        broker.next_page_with_cancel(
            &f.manager,
            &f.host,
            &other,
            &other_binding,
            selected.first_page(),
            || 3,
            || panic!("foreign owner reached cancellation veto")
        ),
        Err(Error::Admission(AdmissionError::Denied))
    );
    let mut requests = [selected.first_page(); 3];
    requests[0].selection_epoch = [99; 32];
    requests[1].page_sequence = 0;
    requests[2].after_entry_id = Some([88; 32]);
    for request in requests {
        assert_eq!(
            broker.next_page_with_cancel(
                &f.manager,
                &f.host,
                &i,
                &binding,
                request,
                || 3,
                || panic!("invalid cursor reached cancellation veto")
            ),
            Err(Error::InvalidCursor)
        );
    }
    assert_eq!(broker.usage().0, 1);
    assert_eq!(binding.usage().resources, 1);
    assert_eq!(binding.usage().bytes, spent);
    assert!(
        broker
            .next_page(
                &f.manager,
                &f.host,
                &i,
                &binding,
                selected.first_page(),
                || 3
            )
            .unwrap()
            .terminal
    );
}

#[test]
fn cancellation_predicate_does_not_authorize_file_read_or_override_stop_expiry_and_clock() {
    let mut read = Fixture::new(&[IoCapability::FileRead]);
    let ri = read.connect();
    let rb = read.bind(&ri, 9000);
    let mut broker = DirectoryBroker::new([37; 32]);
    assert_eq!(
        broker.grant_open_directory_with_cancel(
            &read.manager,
            &read.host,
            &ri,
            &rb,
            open_directory(&read.data),
            CaptureLimits::default(),
            || 2,
            || panic!("FileRead reached cancellation predicate")
        ),
        Err(Error::Admission(AdmissionError::Denied))
    );
    assert_eq!(rb.usage().bytes, 0);
    for mode in 0..4 {
        let mut f = Fixture::new(&[IoCapability::FileList]);
        let i = f.connect();
        let binding = f.bind(&i, 20);
        let selected = f
            .capture(&mut broker, &i, &binding, CaptureLimits::default())
            .unwrap();
        let spent = binding.usage().bytes;
        let (now, expected) = match mode {
            0 => {
                i.request_stop();
                (3, AdmissionError::Denied)
            }
            1 => {
                f.manager
                    .set_enabled(ID, f.package.digest(), false, f.manager.revision())
                    .unwrap();
                (3, AdmissionError::Denied)
            }
            2 => (20, AdmissionError::Expired),
            _ => (1, AdmissionError::Clock),
        };
        assert_eq!(
            broker.next_page_with_cancel(
                &f.manager,
                &f.host,
                &i,
                &binding,
                selected.first_page(),
                || now,
                || panic!("failed original preflight reached command veto")
            ),
            Err(Error::Admission(expected))
        );
        assert_eq!(broker.usage().0, 1);
        assert_eq!(binding.usage().resources, 1);
        assert_eq!(binding.usage().bytes, spent);
        // Original trusted maintenance semantics remain separate from a command veto.
        broker.reap(20);
        assert_eq!(broker.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
        assert_eq!(binding.usage().bytes, spent);
    }
}
