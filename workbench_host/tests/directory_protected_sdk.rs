//! Actual three-language directory guests through the protected Workbench owner.
//! New synthetic directories only. No native picker, user filesystem, rendered
//! GUI, ancestor provenance above the nominated root, or full SDK acceptance.
#![cfg(windows)]
#![deny(unsafe_code)]

use morrow_core::plugin_package::{
    Package,
    io::{self, IoCapability},
};
use morrow_fs_directory_request_v1::FsDirectoryPage;
use morrow_plugin_runtime::{
    Fault, Limits, Runner,
    directory_io::{CaptureLimits, DirectoryRelativePath, SelectedDirectory},
    io_jobs::{DirectoryGuestResult, DirectoryResponse, JobLimits},
};
use morrow_workbench_host::{
    Workbench,
    io_tasks::{StartOptions, StoragePhase, TaskKey},
    plugin_catalog::PluginEntry,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.directory-protected-sdk";
const HANDLER: &str = "directory.immediate-selected";
const WAIT: Duration = Duration::from_secs(15);

fn directory(path: &Path, sharing: u32) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(sharing)
        .custom_flags(0x0220_0000)
        .open(path)
}
fn guest(language: &str) -> Vec<u8> {
    let key = format!("MORROW_{language}_DIRECTORY_REQUEST_WASM");
    let path = PathBuf::from(std::env::var_os(&key).expect("compiled real guest required"));
    assert!(path.is_absolute());
    let expected = std::env::var(format!("{key}_SHA256")).expect("independent guest pin required");
    assert_eq!(expected.len(), 64);
    assert!(expected.bytes().all(|b| b.is_ascii_hexdigit()));
    let bytes = fs::read(path).unwrap();
    assert_eq!(bytes.get(..8), Some(b"\0asm\x01\0\0\0".as_slice()));
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        expected.to_ascii_lowercase()
    );
    assert!(Runner::new_directory_task(&bytes, Limits::default()).is_ok());
    assert!(matches!(
        Runner::new_task(&bytes, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    eprintln!(
        "PROTECTED_DIRECTORY_GUEST language={language} bytes={} sha256={expected}",
        bytes.len()
    );
    bytes
}
fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn entry(w: &Workbench) -> PluginEntry {
    let mut page = w.catalog_page("", None).unwrap();
    loop {
        if let Some(index) = page.entries.iter().position(|row| row.id == ID) {
            return page.entries.remove(index);
        }
        assert!(!page.cursor.is_empty());
        page = w.catalog_page(&page.cursor, Some(page.revision)).unwrap();
    }
}
fn install(w: &mut Workbench, temp: &Path, wasm: &[u8]) -> Package {
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", wasm, vec![]);
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
    let package = Package::build(manifest, wasm).unwrap();
    assert_eq!(package.module(), wasm);
    let path = temp.join("incoming.mplugin");
    fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(w))
        .unwrap();
    let row = entry(w);
    w.configure_external(ID, &row.digest, revision(w), &row.declared, true)
        .unwrap();
    assert!(entry(w).approved_io.is_empty());
    package
}
fn start(
    w: &mut Workbench,
    package: &Package,
    root: &Path,
) -> morrow_workbench_host::Result<TaskKey> {
    let options = StartOptions {
        package_id: ID.into(),
        digest: package.digest(),
        revision: revision(w),
        capabilities: BTreeSet::from([IoCapability::FileList]),
        lifetime: Duration::from_secs(10),
        limits: JobLimits::default(),
    };
    w.start_directory(
        options,
        directory(root, 7).unwrap(),
        DirectoryRelativePath::new(vec!["child".encode_utf16().collect()]).unwrap(),
        CaptureLimits::default(),
        HANDLER.into(),
    )
}
fn capture(w: &mut Workbench, key: TaskKey) -> SelectedDirectory {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(response) = w.read_directory_result(key).unwrap() {
            let DirectoryResponse::Captured(selected) = response else {
                panic!("not captured")
            };
            return selected;
        }
        assert!(
            Instant::now() < deadline,
            "capture deadline; original result retained"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn completed(w: &mut Workbench, key: TaskKey) -> DirectoryGuestResult {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(result) = w.read_directory_guest_result(key).unwrap() {
            return result;
        }
        assert!(Instant::now() < deadline, "guest deadline; never replay");
        thread::sleep(Duration::from_millis(1));
    }
}
fn reclaim(w: &mut Workbench, key: TaskKey, root: &Path) {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = w.poll_io(key).unwrap();
        if status.storage == StoragePhase::Reclaimed {
            let exit = status.exit.unwrap();
            assert_eq!(exit.execution, Ok(()));
            assert_eq!(exit.disconnect, Ok(()));
            assert_eq!(exit.maintenance, Ok(()));
            break;
        }
        assert!(
            matches!(
                status.storage,
                StoragePhase::Running | StoragePhase::Stopping
            ),
            "{status:?}"
        );
        assert!(Instant::now() < deadline, "original owner not reclaimed");
        thread::sleep(Duration::from_millis(1));
    }
    // Successful exclusive native opens prove the original directory handles
    // closed; Stop, terminal response and a requested join alone do not.
    drop(directory(root, 0).unwrap());
    drop(directory(&root.join("child"), 0).unwrap());
    w.acknowledge_io(key).unwrap();
    assert_eq!(w.io_status().storage, StoragePhase::Local);
    assert!(w.poll_io(key).is_err());
    assert_eq!(entry(w).approved_io, ["file-list"]);
}

// Independent bounded observer of the pinned flat Cap'n Proto Response layout.
// Actual production dispatch also requires exact equality with its own last
// validated import response before releasing a successful guest completion.
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
    let data = |n| root.get_pointer_field(n).get_data(None).unwrap();
    assert_eq!(
        root.get_data_field::<u16>(0),
        morrow_fs_directory_request_v1::VERSION
    );
    assert_eq!(root.get_data_field::<u16>(1), 1); // Next.
    assert_eq!(root.get_data_field::<u16>(2), 0);
    assert_eq!(root.get_data_field::<u16>(3), 1); // Frozen fs-directory-v1.
    assert_eq!(data(0), morrow_fs_directory_request_v1::schema_digest());
    assert_eq!(
        data(1),
        morrow_fs_directory_request_v1::page_schema_digest()
    );
    for n in [2, 3, 4] {
        assert_eq!(data(n).len(), 32);
        assert_ne!(data(n), [0; 32]);
    }
    assert_eq!(data(5), selected.selection_epoch);
    let sequence = count.div_ceil(32).max(1) as u64;
    assert_eq!(root.get_data_field::<u64>(1), sequence);
    assert_eq!(root.get_data_field::<u32>(4), 0);
    assert_eq!(root.get_data_field::<u32>(5), 0);
    assert_eq!(root.get_data_field::<u64>(3), 0);
    let page = FsDirectoryPage::decode(data(6)).unwrap();
    assert_eq!(page.encode().unwrap(), data(6));
    assert_eq!(page.selection_epoch, selected.selection_epoch);
    assert_eq!(page.page_sequence, sequence);
    assert!(page.terminal);
    assert_eq!(
        page.entries.len(),
        if count == 0 { 0 } else { (count - 1) % 32 + 1 }
    );
    let mut seen = BTreeSet::new();
    for entry in &page.entries {
        assert_eq!(entry.encoding as u32, 2); // Frozen Utf16Le.
        assert_eq!(entry.kind as u32, 1); // Frozen File.
        assert_ne!(entry.entry_id, [0; 32]);
        assert!(seen.insert(entry.entry_id));
        let units: Vec<_> = entry
            .name
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
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
fn qualify(language: &str, absolute_selection: bool) {
    assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
    let wasm = guest(language);
    let start = |w: &mut Workbench, package: &Package, root: &Path| {
        if !absolute_selection {
            return start(w, package, root);
        }
        let mut submission = [0; 32];
        getrandom::fill(&mut submission).unwrap();
        w.start_selected_directory(morrow_workbench_host::io_tasks::directory::DirectoryStart {
            submission,
            package_id: ID.into(),
            digest: package.digest(),
            revision: revision(w),
            handler: HANDLER.into(),
            selected_path: root.join("child"),
            max_entries: 1024,
            max_metadata_bytes: 1024 * 1024,
            timeout_ms: 10_000,
        })
    };
    // Retain all qualification data even if an assertion fails.
    let temp = tempfile::Builder::new()
        .prefix("protected-directory-")
        .tempdir()
        .unwrap()
        .keep();
    eprintln!("PROTECTED_DIRECTORY_STORE path={}", temp.display());
    let library = temp.join("library");
    let root = temp.join("nominated-root");
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();
    fs::write(
        root.join("outside-selection.txt"),
        b"must never appear in selected pages",
    )
    .unwrap();
    for index in 0..70 {
        fs::write(
            child.join(format!("entry-{index:03}")),
            vec![0x5a; index + 1],
        )
        .unwrap();
    }
    let mut w = Workbench::open_managed(&library, None).unwrap();
    let package = install(&mut w, &temp, &wasm);
    assert!(
        Workbench::open_managed(&library, None).is_err(),
        "original protected lease must remain exclusive"
    );
    assert!(
        start(&mut w, &package, &root).is_err(),
        "declaration is not FileList approval"
    );
    assert_eq!(w.io_status().storage, StoragePhase::Local);
    assert!(w.io_status().key.is_none());
    drop(directory(&root, 0).unwrap());
    assert!(
        w.configure_external_io(ID, &package.digest(), revision(&w), &["file-read".into()])
            .is_err()
    );
    assert!(entry(&w).approved_io.is_empty());
    w.configure_external_io(ID, &package.digest(), revision(&w), &["file-list".into()])
        .unwrap();
    let original_revision = revision(&w);
    let key = start(&mut w, &package, &root).unwrap();
    assert_eq!(w.io_status().storage, StoragePhase::Running);
    assert!(
        w.catalog_page("", None).is_err(),
        "local access cannot substitute for worker-owned state"
    );
    assert!(
        Workbench::open_managed(&library, None).is_err(),
        "worker carries original registry/session lease"
    );
    let selected = capture(&mut w, key);
    assert_eq!(selected.entries, 70);
    assert!(directory(&root, 0).is_err());
    assert!(directory(&child, 0).is_err());
    if absolute_selection {
        // Ancestors above the former nominated anchor now remain pinned too.
        assert!(directory(&temp, 0).is_err());
        assert!(fs::rename(&root, temp.join("moved-root")).is_err());
    }
    w.request_directory_guest_frame(key, Duration::from_secs(5))
        .unwrap();
    assert!(
        w.request_directory_guest_frame(key, Duration::from_secs(5))
            .is_err(),
        "one invocation only"
    );
    let result = completed(&mut w, key);
    assert!(!result.unknown);
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.execution.host_calls, 4);
    let page = final_page(result.response.as_ref().unwrap(), selected, 70);
    assert_eq!(page.entries.len(), 6);
    assert!(
        w.read_directory_guest_result(key).is_err(),
        "completion is once-only"
    );
    reclaim(&mut w, key, &root);
    if absolute_selection {
        drop(directory(&temp, 0).unwrap());
    }
    assert_eq!(revision(&w), original_revision);
    assert!(
        Workbench::open_managed(&library, None).is_err(),
        "same returned original protected owner still holds lease"
    );
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(&library, None).unwrap();
    assert_eq!(revision(&w), original_revision);
    assert_eq!(entry(&w).approved_io, ["file-list"]);
    assert!(entry(&w).enabled);
    assert_eq!(entry(&w).digest, package.digest());
    assert!(
        w.poll_io(key).is_err(),
        "old task identity is not revived by reopen"
    );
    let fresh = start(&mut w, &package, &root).unwrap();
    assert_ne!(fresh, key);
    let fresh_selected = capture(&mut w, fresh);
    assert_ne!(fresh_selected.selection_epoch, selected.selection_epoch);
    assert_eq!(fresh_selected.entries, 70);
    assert!(directory(&child, 0).is_err());
    w.cancel_io(fresh).unwrap();
    reclaim(&mut w, fresh, &root);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(&library, None).unwrap();
    assert_eq!(entry(&w).approved_io, ["file-list"]);
    assert_eq!(w.io_status().storage, StoragePhase::Local);
    w.configure_external_io(ID, &package.digest(), revision(&w), &[])
        .unwrap();
    assert!(
        start(&mut w, &package, &root).is_err(),
        "revoked original approval cannot be revived"
    );
    assert_eq!(w.io_status().storage, StoragePhase::Local);
    drop(directory(&root, 0).unwrap());
    drop(directory(&child, 0).unwrap());
    w.finish().unwrap();
    drop(w);
    eprintln!(
        "PROTECTED_DIRECTORY_PASS language={language} actual_guest=true protected_session=true root_and_child_closed=true approval_reopen=true original_owner_reclaimed=true cancelled_capture_closed=true picker_provenance=false"
    );
    if absolute_selection {
        eprintln!(
            "SELECTED_DIRECTORY_PASS language={language} actual_volume_root=true retained_ancestors=true original_limits=true picker_time_identity=false"
        );
    }
}

#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn rust_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("RUST", false);
}
#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn c_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("C", false);
}
#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn cpp_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("CPP", false);
}

#[test]
#[ignore = "explicit Windows qualification with pinned actual guest and shallow synthetic D root"]
fn rust_selected_directory_guest_retains_volume_ancestors_and_original_owner() {
    qualify("RUST", true);
}
#[test]
#[ignore = "explicit Windows qualification with pinned actual guest and shallow synthetic D root"]
fn c_selected_directory_guest_retains_volume_ancestors_and_original_owner() {
    qualify("C", true);
}
#[test]
#[ignore = "explicit Windows qualification with pinned actual guest and shallow synthetic D root"]
fn cpp_selected_directory_guest_retains_volume_ancestors_and_original_owner() {
    qualify("CPP", true);
}

// These cases exercise the public Workbench boundary on a real protected
// Windows library. They do not substitute an ordinary Store or injected owner.
struct LifecycleFixture {
    temp: PathBuf,
    root: PathBuf,
    library: PathBuf,
    package: Package,
    workbench: Workbench,
    revision: u64,
}
impl LifecycleFixture {
    fn new() -> Self {
        Self::new_language("RUST")
    }
    fn new_language(language: &str) -> Self {
        assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
        let temp = tempfile::Builder::new()
            .prefix("protected-directory-lifecycle-")
            .tempdir()
            .unwrap()
            .keep();
        eprintln!("PROTECTED_DIRECTORY_STORE path={}", temp.display());
        let root = temp.join("nominated-root");
        let library = temp.join("library");
        fs::create_dir_all(root.join("child")).unwrap();
        for index in 0..70 {
            fs::write(
                root.join("child").join(format!("entry-{index:03}")),
                vec![0x5a; index + 1],
            )
            .unwrap();
        }
        fs::write(
            root.join("outside-selection.txt"),
            b"original outside bytes",
        )
        .unwrap();
        let mut workbench = Workbench::open_managed(&library, None).unwrap();
        let package = install(&mut workbench, &temp, &guest(language));
        workbench
            .configure_external_io(
                ID,
                &package.digest(),
                revision(&workbench),
                &["file-list".into()],
            )
            .unwrap();
        let revision = revision(&workbench);
        Self {
            temp,
            root,
            library,
            package,
            workbench,
            revision,
        }
    }
    fn start(&mut self, leaf: &str, limits: CaptureLimits) -> TaskKey {
        let options = StartOptions {
            package_id: ID.into(),
            digest: self.package.digest(),
            revision: revision(&self.workbench),
            capabilities: BTreeSet::from([IoCapability::FileList]),
            lifetime: Duration::from_secs(10),
            limits: JobLimits::default(),
        };
        let key = self
            .workbench
            .start_directory(
                options,
                directory(&self.root, 7).unwrap(),
                DirectoryRelativePath::new(vec![leaf.encode_utf16().collect()]).unwrap(),
                limits,
                HANDLER.into(),
            )
            .unwrap();
        assert!(self.workbench.catalog_page("", None).is_err());
        assert!(Workbench::open_managed(&self.library, None).is_err());
        key
    }
    fn selected_request(
        &self,
        submission: u8,
        selected_path: PathBuf,
    ) -> morrow_workbench_host::io_tasks::directory::DirectoryStart {
        morrow_workbench_host::io_tasks::directory::DirectoryStart {
            submission: [submission; 32],
            package_id: ID.into(),
            digest: self.package.digest(),
            revision: self.revision,
            handler: HANDLER.into(),
            selected_path,
            max_entries: 1024,
            max_metadata_bytes: 1024 * 1024,
            timeout_ms: 10_000,
        }
    }
    fn reply(&mut self, key: TaskKey) -> morrow_workbench_host::Result<DirectoryResponse> {
        let deadline = Instant::now() + WAIT;
        loop {
            match self.workbench.read_directory_result(key) {
                Ok(None) => {}
                Ok(Some(value)) => return Ok(value),
                Err(error) => return Err(error),
            }
            assert!(
                Instant::now() < deadline,
                "original result retained; no replay"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn finish(mut self, key: TaskKey, label: &str) {
        reclaim(&mut self.workbench, key, &self.root);
        assert_eq!(revision(&self.workbench), self.revision);
        assert!(Workbench::open_managed(&self.library, None).is_err());
        self.workbench.finish().unwrap();
        drop(self.workbench);
        let mut reopened = Workbench::open_managed(&self.library, None).unwrap();
        assert_eq!(revision(&reopened), self.revision);
        assert_eq!(entry(&reopened).digest, self.package.digest());
        assert_eq!(entry(&reopened).approved_io, ["file-list"]);
        assert!(reopened.poll_io(key).is_err());
        assert!(reopened.read_directory_result(key).is_err());
        assert_eq!(
            fs::read(self.root.join("outside-selection.txt")).unwrap(),
            b"original outside bytes"
        );
        reopened.finish().unwrap();
        drop(reopened);
        eprintln!(
            "PROTECTED_DIRECTORY_LIFECYCLE_PASS case={label} protected_session=true original_owner_reclaimed=true original_handles_closed=true old_task_not_revived=true store={}",
            self.temp.display()
        );
    }
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn selected_directory_stale_revision_and_repeated_submission_cannot_rebind() {
    let mut f = LifecycleFixture::new();
    let mut stale = f.selected_request(51, f.root.join("child"));
    stale.revision -= 1;
    assert!(f.workbench.start_selected_directory(stale).is_err());
    assert_eq!(f.workbench.io_status().storage, StoragePhase::Local);
    assert!(f.workbench.io_status().key.is_none());
    // The rejected immutable submission stays spent even with corrected metadata.
    assert!(
        f.workbench
            .start_selected_directory(f.selected_request(51, f.root.join("child")))
            .is_err()
    );
    let key = f
        .workbench
        .start_selected_directory(f.selected_request(52, f.root.join("child")))
        .unwrap();
    assert_eq!(f.workbench.http_submission(), Some([52; 32]));
    let selected = capture(&mut f.workbench, key);
    assert_eq!(selected.entries, 70);
    assert!(
        f.workbench
            .start_selected_directory(f.selected_request(53, f.root.clone()))
            .is_err()
    );
    f.workbench.finish_directory(key).unwrap();
    assert!(matches!(f.reply(key).unwrap(), DirectoryResponse::Finished));
    reclaim(&mut f.workbench, key, &f.root);
    assert!(
        f.workbench
            .start_selected_directory(f.selected_request(52, f.root.clone()))
            .is_err()
    );
    let fresh = f
        .workbench
        .start_selected_directory(f.selected_request(54, f.root.join("child")))
        .unwrap();
    capture(&mut f.workbench, fresh);
    f.workbench.cancel_io(fresh).unwrap();
    f.finish(fresh, "selected-revision-and-submission");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with a new synthetic directory symlink"]
fn selected_directory_reparse_ancestor_is_refused_without_traversal() {
    let mut f = LifecycleFixture::new();
    let alias = f.temp.join("new-synthetic-alias");
    std::os::windows::fs::symlink_dir(&f.root, &alias).unwrap();
    let key = f
        .workbench
        .start_selected_directory(f.selected_request(61, alias.join("child")))
        .unwrap();
    let error = f.reply(key).unwrap_err().to_string();
    assert!(error.contains("ReparsePoint"), "{error}");
    assert!(
        f.workbench
            .request_directory_guest_frame(key, Duration::from_secs(5))
            .is_err()
    );
    f.finish(key, "selected-reparse-refused");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification preserving the original eight-resource cap"]
fn selected_directory_full_ancestor_chain_cannot_exceed_original_resource_limit() {
    let mut f = LifecycleFixture::new();
    let mut deep = f.root.clone();
    for _ in 0..8 {
        deep.push("child");
    }
    fs::create_dir_all(&deep).unwrap();
    let key = f
        .workbench
        .start_selected_directory(f.selected_request(71, deep))
        .unwrap();
    let error = f.reply(key).unwrap_err().to_string();
    assert!(error.contains("Limit"), "{error}");
    assert!(
        f.workbench
            .request_directory_guest_frame(key, Duration::from_secs(5))
            .is_err()
    );
    f.finish(key, "selected-original-resource-limit");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn missing_directory_refusal_reclaims_original_owner_and_reopens() {
    let mut f = LifecycleFixture::new();
    let key = f.start("missing", CaptureLimits::default());
    let error = f.reply(key).unwrap_err().to_string();
    assert!(error.contains("NotFound"), "{error}");
    assert!(f.workbench.read_directory_result(key).is_err());
    f.finish(key, "missing-directory");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn capture_limit_refusal_reclaims_original_owner_and_reopens() {
    let mut f = LifecycleFixture::new();
    let key = f.start(
        "child",
        CaptureLimits {
            max_entries: 1,
            ..CaptureLimits::default()
        },
    );
    let error = f.reply(key).unwrap_err().to_string();
    assert!(error.contains("Limit"), "{error}");
    assert!(f.workbench.read_directory_result(key).is_err());
    f.finish(key, "capture-limit");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn foreign_cursor_refusal_preserves_original_selection_and_guest() {
    let mut f = LifecycleFixture::new();
    let key = f.start("child", CaptureLimits::default());
    let selected = capture(&mut f.workbench, key);
    let mut wrong = selected.first_page();
    wrong.selection_epoch[0] ^= 1;
    assert!(f.workbench.request_directory_page(key, wrong).is_err());
    assert_eq!(f.workbench.io_status().storage, StoragePhase::Running);
    assert!(directory(&f.root.join("child"), 0).is_err());
    f.workbench
        .request_directory_guest_frame(key, Duration::from_secs(5))
        .unwrap();
    let result = completed(&mut f.workbench, key);
    assert!(!result.unknown);
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.execution.host_calls, 4);
    final_page(result.response.as_ref().unwrap(), selected, 70);
    f.finish(key, "foreign-cursor");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn replayed_page_refusal_preserves_next_page_and_explicit_finish() {
    let mut f = LifecycleFixture::new();
    let key = f.start("child", CaptureLimits::default());
    let selected = capture(&mut f.workbench, key);
    f.workbench
        .request_directory_page(key, selected.first_page())
        .unwrap();
    let DirectoryResponse::Page(first) = f.reply(key).unwrap() else {
        panic!("page required")
    };
    assert_eq!(first.page_sequence, 1);
    assert_eq!(first.entries.len(), 32);
    assert!(!first.terminal);
    assert!(
        f.workbench
            .request_directory_page(key, selected.first_page())
            .is_err()
    );
    assert!(
        f.workbench
            .request_directory_guest_frame(key, Duration::from_secs(5))
            .is_err()
    );
    let mut next = selected.first_page();
    next.page_sequence = 2;
    next.after_entry_id = Some(first.entries.last().unwrap().entry_id);
    f.workbench.request_directory_page(key, next).unwrap();
    let DirectoryResponse::Page(second) = f.reply(key).unwrap() else {
        panic!("page required")
    };
    assert_eq!(second.page_sequence, 2);
    assert_eq!(second.entries.len(), 32);
    assert!(!second.terminal);
    assert!(
        first
            .entries
            .iter()
            .all(|a| second.entries.iter().all(|b| a.entry_id != b.entry_id))
    );
    f.workbench.finish_directory(key).unwrap();
    assert!(matches!(f.reply(key).unwrap(), DirectoryResponse::Finished));
    assert!(f.workbench.read_directory_result(key).is_err());
    f.finish(key, "page-replay-and-finish");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn cancel_unread_capture_reclaims_original_native_handles_and_owner() {
    let mut f = LifecycleFixture::new();
    let key = f.start("child", CaptureLimits::default());
    // Do not consume Captured. Cancellation must dispose queued/running/unread
    // ownership through the original stop/join path in any scheduling order.
    f.workbench.cancel_io(key).unwrap();
    f.finish(key, "cancel-unread-capture");
}

#[test]
#[ignore = "explicit protected Windows lifecycle qualification with pinned guest"]
fn invalid_guest_deadline_refuses_before_admission_without_losing_selection() {
    let mut f = LifecycleFixture::new();
    let key = f.start("child", CaptureLimits::default());
    let selected = capture(&mut f.workbench, key);
    assert!(
        f.workbench
            .request_directory_guest_frame(key, Duration::ZERO)
            .is_err()
    );
    assert!(
        f.workbench
            .request_directory_guest_frame(key, Duration::from_secs(11))
            .is_err()
    );
    assert!(directory(&f.root.join("child"), 0).is_err());
    f.workbench
        .request_directory_guest_frame(key, Duration::from_secs(5))
        .unwrap();
    let result = completed(&mut f.workbench, key);
    assert!(!result.unknown);
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.execution.host_calls, 4);
    final_page(result.response.as_ref().unwrap(), selected, 70);
    f.finish(key, "invalid-guest-deadline");
}

include!("support/directory_private_protocol.rs");
