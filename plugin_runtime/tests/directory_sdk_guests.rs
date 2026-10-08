//! Actual fs-directory-request-v1 Rust/C/C++ Wasm on original managed owners.
//! Requires each separately compiled guest path and independently pinned SHA256.
//! Ordinary synthetic Store/TempDir only: no ProductionGUI, ProtectedSession,
//! picker provenance, real user data, or other-platform qualification.
#![cfg(all(feature = "packages", windows))]
#![deny(unsafe_code)]

use morrow_core::{
    dispatch::{HostBinding, HostRuntime},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::Store,
};
use morrow_fs_directory_request_v1::FsDirectoryPage;
use morrow_plugin_runtime::{
    Fault, Limits, Runner,
    directory_io::{CaptureLimits, SelectedDirectory},
    io_binding::IoBinding,
    io_jobs::{
        DirectoryCommandError, DirectoryCommandHandle, DirectoryGuestHandle, DirectoryGuestResult,
        DirectoryResponse, DirectorySession, HostOwner, IoWorker, JobError, JobLimits,
        ManagedHostOwner, OwnerCommandError, OwnerCommandPoll,
    },
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.directory-request-real-sdk";
const HANDLER: &str = "directory.immediate-selected";
const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
enum Language {
    Rust,
    C,
    Cpp,
}

fn guest(language: Language) -> Vec<u8> {
    let (path_key, pin_key) = match language {
        Language::Rust => (
            "MORROW_RUST_DIRECTORY_REQUEST_WASM",
            "MORROW_RUST_DIRECTORY_REQUEST_WASM_SHA256",
        ),
        Language::C => (
            "MORROW_C_DIRECTORY_REQUEST_WASM",
            "MORROW_C_DIRECTORY_REQUEST_WASM_SHA256",
        ),
        Language::Cpp => (
            "MORROW_CPP_DIRECTORY_REQUEST_WASM",
            "MORROW_CPP_DIRECTORY_REQUEST_WASM_SHA256",
        ),
    };
    let path = PathBuf::from(
        std::env::var_os(path_key).expect("real newly compiled directory SDK Wasm is mandatory"),
    );
    assert!(path.is_absolute(), "guest artifact path must be absolute");
    let expected =
        std::env::var(pin_key).expect("independently pinned guest artifact SHA256 is mandatory");
    assert_eq!(expected.len(), 64);
    assert!(expected.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"\0asm", "actual compiled Wasm, never WAT");
    let actual = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(actual, expected.to_ascii_lowercase());
    assert!(Runner::new_directory_task(&bytes, Limits::default()).is_ok());
    assert!(matches!(
        Runner::new_task(&bytes, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    eprintln!(
        "directory {language:?} guest wasm_sha256={actual} bytes={} synthetic_only=true",
        bytes.len()
    );
    bytes
}

struct Owner {
    host: HostRuntime,
    manager: Manager,
    package: Package,
    identity: Arc<()>,
    hooks: Arc<Mutex<Vec<(&'static str, thread::ThreadId)>>>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("prepare", thread::current().id()));
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("finish", thread::current().id()));
        Ok(())
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    data: PathBuf,
    owner: Owner,
    clock: Arc<AtomicU64>,
}
impl Fixture {
    fn new(language: Language, approve: bool, entries: usize) -> Self {
        let bytes = guest(language);
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("selected-synthetic");
        fs::create_dir(&data).unwrap();
        for number in 0..entries {
            fs::write(
                data.join(format!("entry-{number:03}")),
                vec![0x5a; number + 1],
            )
            .unwrap();
        }
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &bytes, vec![]);
        let mut declaration = io::declaration(vec![IoCapability::FileList], vec![HANDLER.into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_resources = 8;
        budget.max_jobs = io::MAX_JOBS;
        budget.max_bytes = io::MAX_BYTES;
        budget.max_job_bytes = io::MAX_JOB_BYTES;
        budget.max_duration_ms = 10_000;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        manifest
            .required_features
            .push(morrow_core::plugin_package::DIRECTORY_REQUEST_FEATURE.into());
        let package = Package::build(manifest, &bytes).unwrap();
        assert_eq!(package.module(), bytes.as_slice());
        let catalog = Catalog::open(&temp.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&temp.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        if approve {
            manager
                .approve_io(
                    ID,
                    package.digest(),
                    BTreeSet::from([IoCapability::FileList]),
                    manager.revision(),
                )
                .unwrap();
        }
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(
            Store::open(
                &temp.path().join("ordinary-keyless-store"),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        Self {
            temp,
            data,
            owner: Owner {
                host,
                manager,
                package,
                identity: Arc::new(()),
                hooks: Arc::new(Mutex::new(Vec::new())),
            },
            clock: Arc::new(AtomicU64::new(2)),
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.owner
            .manager
            .connect(ID, &mut self.owner.host)
            .unwrap()
    }
    fn bind(
        &self,
        instance: &ManagedInstance,
    ) -> Result<IoBinding, morrow_plugin_runtime::manager::ManagerError> {
        self.owner.manager.bind_io(
            &self.owner.host,
            instance,
            self.owner.package.digest(),
            self.owner.manager.revision(),
            &BTreeSet::from([IoCapability::FileList]),
            9000,
            1,
        )
    }
    fn start(mut self) -> Running {
        let instance = self.connect();
        let binding = self.bind(&instance).unwrap();
        let identity = self.owner.identity.clone();
        let host = self.owner.host.binding();
        let hooks = self.owner.hooks.clone();
        let sampled = self.clock.clone();
        let worker = IoWorker::spawn_managed_owner(
            self.owner,
            instance,
            binding,
            move || sampled.load(Ordering::SeqCst),
            io::MAX_JOBS as usize,
            JobLimits::default(),
        )
        .unwrap();
        Running {
            _temp: self.temp,
            data: self.data,
            worker,
            identity,
            host,
            hooks,
        }
    }
}
struct Running {
    _temp: tempfile::TempDir,
    data: PathBuf,
    worker: IoWorker<Owner>,
    identity: Arc<()>,
    host: HostBinding,
    hooks: Arc<Mutex<Vec<(&'static str, thread::ThreadId)>>>,
}
impl Running {
    fn capture(&self) -> (DirectorySession, SelectedDirectory) {
        let directory = OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0220_0000)
            .open(&self.data)
            .unwrap();
        let (session, mut handle) = self
            .worker
            .capture_directory_fresh(directory, CaptureLimits::default())
            .unwrap();
        let DirectoryResponse::Captured(selected) = directory_result(&mut handle).unwrap().unwrap()
        else {
            panic!("expected captured selection");
        };
        (session, selected)
    }
    fn submit(&self, session: DirectorySession) -> DirectoryGuestHandle {
        self.worker
            .submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(5))
            .unwrap()
    }
    fn reclaim(&mut self) {
        self.worker.stop();
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(exit) = self.worker.try_reclaim().unwrap() {
                assert!(Arc::ptr_eq(&self.identity, &exit.owner.identity));
                assert_eq!(exit.owner.host.binding(), self.host);
                assert_eq!(exit.disconnect, Ok(()));
                assert_eq!(self.worker.directory_usage(), (0, 0));
                let hooks = self.hooks.lock().unwrap();
                assert!(
                    hooks.len() >= 2,
                    "at least one prepared command before finish"
                );
                assert!(
                    hooks[..hooks.len() - 1]
                        .iter()
                        .all(|(name, _)| *name == "prepare")
                );
                assert_eq!(
                    hooks.iter().filter(|(name, _)| *name == "finish").count(),
                    1
                );
                assert_eq!(hooks.first().unwrap().0, "prepare");
                assert_eq!(hooks.last().unwrap().0, "finish");
                let worker_thread = hooks.first().unwrap().1;
                assert_ne!(worker_thread, thread::current().id());
                assert!(hooks.iter().all(|(_, tid)| *tid == worker_thread));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "original owner must actually join"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.worker.stop();
    }
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !predicate() {
        assert!(Instant::now() < deadline, "bounded original worker wait");
        thread::sleep(Duration::from_millis(1));
    }
}
fn directory_result(
    handle: &mut DirectoryCommandHandle,
) -> Result<Result<DirectoryResponse, DirectoryCommandError>, OwnerCommandError> {
    let deadline = Instant::now() + WAIT;
    loop {
        match handle.read() {
            Ok(None) => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Some(response)) => return Ok(response),
            Err(error) => return Err(error),
        }
    }
}
fn guest_result(
    handle: &mut DirectoryGuestHandle,
) -> Result<Result<DirectoryGuestResult, DirectoryCommandError>, OwnerCommandError> {
    let deadline = Instant::now() + WAIT;
    loop {
        match handle.read() {
            Ok(None) => {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Some(response)) => return Ok(response),
            Err(error) => return Err(error),
        }
    }
}

// A bounded independent observer for the pinned flat Response schema. Capnp's
// safe reader follows real pointers across segments; no guessed payload offsets
// or generated source with unsafe code is introduced by this test.
struct RawResponse<'a>(capnp::private::layout::StructReader<'a>);
impl<'a> capnp::traits::FromPointerReader<'a> for RawResponse<'a> {
    fn get_from_pointer(
        reader: &capnp::private::layout::PointerReader<'a>,
        default: Option<&'a [capnp::Word]>,
    ) -> capnp::Result<Self> {
        Ok(Self(reader.get_struct(default)?))
    }
}
fn final_page(bytes: &[u8], selected: SelectedDirectory, count: usize) -> FsDirectoryPage {
    assert!(bytes.len() <= morrow_fs_directory_request_v1::MAX_RESPONSE_BYTES);
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(morrow_fs_directory_request_v1::SCHEMA)
        ),
        "04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df"
    );
    let mut remaining = bytes;
    let message = capnp::serialize::read_message(
        &mut remaining,
        capnp::message::ReaderOptions {
            traversal_limit_in_words: Some(16384),
            nesting_limit: 8,
        },
    )
    .unwrap();
    assert!(remaining.is_empty());
    let root = message.get_root::<RawResponse<'_>>().unwrap().0;
    assert_eq!(root.get_data_section_size(), 4 * 64);
    assert_eq!(root.get_pointer_section_size(), 7);
    assert_eq!(root.total_size().unwrap().cap_count, 0);
    let data = |ordinal| root.get_pointer_field(ordinal).get_data(None).unwrap();
    assert_eq!(
        root.get_data_field::<u16>(0),
        morrow_fs_directory_request_v1::VERSION
    );
    assert_eq!(root.get_data_field::<u16>(1), 1); // Next, never the initial Opened response.
    assert_eq!(root.get_data_field::<u16>(2), 0);
    assert_eq!(
        root.get_data_field::<u16>(3),
        morrow_fs_directory_v1::VERSION
    );
    assert_eq!(data(0), morrow_fs_directory_request_v1::schema_digest());
    assert_eq!(
        data(1),
        morrow_fs_directory_request_v1::page_schema_digest()
    );
    for ordinal in [2, 3, 4] {
        assert_eq!(data(ordinal).len(), 32);
        assert_ne!(data(ordinal), [0; 32]);
    }
    assert_eq!(data(5), selected.selection_epoch);
    let sequence = count
        .div_ceil(morrow_fs_directory_v1::MAX_ENTRIES_PER_PAGE)
        .max(1) as u64;
    assert_eq!(root.get_data_field::<u64>(1), sequence);
    assert_eq!(root.get_data_field::<u32>(4), 0);
    assert_eq!(root.get_data_field::<u32>(5), 0);
    assert_eq!(root.get_data_field::<u64>(3), 0);
    let page = FsDirectoryPage::decode(data(6)).unwrap();
    assert_eq!(page.selection_epoch, selected.selection_epoch);
    assert_eq!(page.page_sequence, sequence);
    assert!(page.terminal);
    let tail = if count == 0 { 0 } else { (count - 1) % 32 + 1 };
    assert_eq!(page.entries.len(), tail);
    assert_eq!(
        page.encode().unwrap(),
        data(6),
        "exact canonical last page body"
    );
    let mut seen = BTreeSet::new();
    for entry in &page.entries {
        assert_eq!(
            entry.encoding,
            morrow_fs_directory_v1::NameEncoding::Utf16Le
        );
        assert_eq!(entry.kind, morrow_fs_directory_v1::EntryKind::File);
        assert_ne!(entry.entry_id, [0; 32]);
        assert!(seen.insert(entry.entry_id));
        let units: Vec<u16> = entry
            .name
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(units.len() * 2, entry.name.len());
        let name = String::from_utf16(&units).unwrap();
        let index: usize = name.strip_prefix("entry-").unwrap().parse().unwrap();
        assert!(index < count);
        assert_eq!(name, format!("entry-{index:03}"));
        assert_eq!(entry.logical_length, Some(index as u64 + 1));
    }
    page
}

fn pages_to_exact_terminal_completion_on_original_owner(language: Language) {
    let mut run = Fixture::new(language, true, 70).start();
    let (session, selected) = run.capture();
    assert_eq!(selected.entries, 70);
    let before = run.worker.directory_binding_usage().unwrap();
    assert_eq!(before.resources, 1);
    let mut handle = run.submit(session);
    wait_for(|| handle.poll() == OwnerCommandPoll::Ready);
    let unread = run.worker.directory_binding_usage().unwrap();
    assert_eq!(unread.resources, 0);
    assert_eq!(
        unread.jobs, 1,
        "unread real guest retains original job lease"
    );
    assert!(unread.bytes > before.bytes);
    let completed = guest_result(&mut handle).unwrap().unwrap();
    // The production managed dispatch releases completion only after byte-for-
    // byte equality with its exact last validated import response.
    assert_eq!(completed.execution.outcome, Ok(0));
    assert!(!completed.unknown);
    assert_eq!(
        completed.execution.host_calls, 4,
        "one Open plus three actual Next imports"
    );
    let page = final_page(completed.response.as_ref().unwrap(), selected, 70);
    assert_eq!(page.page_sequence, 3);
    assert_eq!(page.entries.len(), 6);
    assert_eq!(run.worker.directory_binding_usage().unwrap().jobs, 0);
    assert_eq!(
        run.worker.directory_binding_usage().unwrap().bytes,
        unread.bytes
    );
    assert!(matches!(
        run.worker
            .submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(5)),
        Err(OwnerCommandError::Closed)
    ));
    run.reclaim();
}

fn package_declaration_never_substitutes_for_file_list_approval(language: Language) {
    let mut fixture = Fixture::new(language, false, 1);
    assert!(
        fixture
            .owner
            .manager
            .approve_io(
                ID,
                fixture.owner.package.digest(),
                BTreeSet::from([IoCapability::FileRead]),
                fixture.owner.manager.revision()
            )
            .is_err(),
        "FileRead cannot be inferred as FileList"
    );
    let instance = fixture.connect();
    assert!(
        fixture.bind(&instance).is_err(),
        "unapproved FileList must deny before worker/native capture"
    );
    instance.close(&mut fixture.owner.host).unwrap();
}

fn foreign_worker_selection_is_denied_without_consuming_original(language: Language) {
    let mut original = Fixture::new(language, true, 1).start();
    let mut foreign = Fixture::new(language, true, 1).start();
    let (session, selected) = original.capture();
    let (foreign_session, foreign_selected) = foreign.capture();
    let foreign_before = foreign.worker.directory_binding_usage().unwrap();
    let before = original.worker.directory_binding_usage().unwrap();
    assert!(matches!(
        foreign.worker.submit_directory_guest_frame(
            session,
            HANDLER.into(),
            Duration::from_secs(5)
        ),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(original.worker.directory_usage().0, 1);
    assert_eq!(foreign.worker.directory_usage().0, 1);
    assert_eq!(
        foreign.worker.directory_binding_usage().unwrap().bytes,
        foreign_before.bytes
    );
    assert_eq!(
        original.worker.directory_binding_usage().unwrap().bytes,
        before.bytes
    );
    let completed = guest_result(&mut original.submit(session))
        .unwrap()
        .unwrap();
    assert_eq!(completed.execution.outcome, Ok(0));
    assert!(!completed.unknown);
    assert_eq!(completed.execution.host_calls, 2);
    final_page(completed.response.as_ref().unwrap(), selected, 1);
    let completed = guest_result(&mut foreign.submit(foreign_session))
        .unwrap()
        .unwrap();
    assert_eq!(completed.execution.outcome, Ok(0));
    assert!(!completed.unknown);
    assert_eq!(completed.execution.host_calls, 2);
    final_page(completed.response.as_ref().unwrap(), foreign_selected, 1);
    original.reclaim();
    foreign.reclaim();
}

fn cancelled_ready_output_is_unknown_and_never_replayed(language: Language) {
    let mut run = Fixture::new(language, true, 70).start();
    let (session, _) = run.capture();
    let mut handle = run.submit(session);
    wait_for(|| handle.poll() == OwnerCommandPoll::Ready);
    let charged = run.worker.directory_binding_usage().unwrap().bytes;
    assert_eq!(run.worker.directory_binding_usage().unwrap().jobs, 1);
    handle.cancel();
    assert!(matches!(handle.read(), Err(OwnerCommandError::Unknown)));
    wait_for(|| {
        run.worker.directory_usage() == (0, 0)
            && run.worker.directory_binding_usage().unwrap().jobs == 0
    });
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, charged);
    assert!(matches!(
        run.worker
            .submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(5)),
        Err(OwnerCommandError::Closed)
    ));
    run.reclaim();
}

macro_rules! language_cases {
    ($language:ident, $pages:ident, $approval:ident, $foreign:ident, $cancel:ident) => {
        #[test]
        fn $pages() {
            pages_to_exact_terminal_completion_on_original_owner(Language::$language);
        }
        #[test]
        fn $approval() {
            package_declaration_never_substitutes_for_file_list_approval(Language::$language);
        }
        #[test]
        fn $foreign() {
            foreign_worker_selection_is_denied_without_consuming_original(Language::$language);
        }
        #[test]
        fn $cancel() {
            cancelled_ready_output_is_unknown_and_never_replayed(Language::$language);
        }
    };
}

language_cases!(
    Rust,
    rust_pages,
    rust_requires_approval,
    rust_foreign_selection,
    rust_cancel_ready
);
language_cases!(
    C,
    c_pages,
    c_requires_approval,
    c_foreign_selection,
    c_cancel_ready
);
language_cases!(
    Cpp,
    cpp_pages,
    cpp_requires_approval,
    cpp_foreign_selection,
    cpp_cancel_ready
);
