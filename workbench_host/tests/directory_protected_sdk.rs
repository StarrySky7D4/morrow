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
fn qualify(language: &str) {
    assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
    let wasm = guest(language);
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
}

#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn rust_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("RUST");
}
#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn c_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("C");
}
#[test]
#[ignore = "explicit Windows qualification requires independently pinned actual compiled guest"]
fn cpp_directory_guest_preserves_protected_owner_through_reclaim_and_reopen() {
    qualify("CPP");
}
