//! Native lease and atomic-file implementation; the shared rules own serialization.
use super::{MAX_CONTAINER, Registry, RegistryStorage};
use crate::{
    Error, Result,
    plugin_package::{Package, catalog::Catalog},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
struct FileStorage {
    root: PathBuf,
    catalog: Catalog,
    _lease: File,
}
fn regular_or_absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(Error::Invalid("registry file type")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Error::Io),
    }
}
impl Registry {
    /// Registry and catalog roots belong to the trusted host, not to package metadata.
    pub fn open(root: &Path, catalog: Catalog) -> Result<Self> {
        if let Ok(meta) = fs::symlink_metadata(root)
            && !meta.file_type().is_dir()
        {
            return Err(Error::Invalid("registry directory type"));
        }
        fs::create_dir_all(root).map_err(|_| Error::Io)?;
        let root = root.canonicalize().map_err(|_| Error::Io)?;
        let lock = root.join("registry.lock");
        regular_or_absent(&lock)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3); // Keep the locked file from being deleted/replaced while live.
        }
        let lease = options.open(lock).map_err(|_| Error::Io)?;
        lease.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => Error::StorageBusy,
            std::fs::TryLockError::Error(_) => Error::Io,
        })?;
        Self::from_storage(Box::new(FileStorage {
            root,
            catalog,
            _lease: lease,
        }))
    }
}
impl FileStorage {
    fn path(&self) -> PathBuf {
        self.root.join("selection.morrow")
    }
}
impl RegistryStorage for FileStorage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        let path = self.path();
        if !regular_or_absent(&path)? {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| Error::Io)?
            .take(MAX_CONTAINER as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        Ok(Some(bytes))
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        #[cfg(windows)]
        let original = checked_snapshot(self.read()?)?;
        regular_or_absent(&self.path())?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(|_| Error::Io)?;
        staged.write_all(bytes).map_err(|_| Error::Io)?;
        staged.as_file().sync_all().map_err(|_| Error::Io)?;
        #[cfg(windows)]
        persist_windows(staged, &self.path(), original, || self.read())?;
        #[cfg(not(windows))]
        staged.persist(self.path()).map_err(|_| Error::Io)?;
        Ok(())
    }
    fn load_package(&self, digest: [u8; 32]) -> Result<Package> {
        self.catalog.load(digest)
    }
    fn install_package(&self, package: &Package) -> Result<()> {
        self.catalog.install(package).map(|_| ())
    }
}

#[cfg(windows)]
fn checked_snapshot(snapshot: Option<Vec<u8>>) -> Result<Option<Vec<u8>>> {
    if snapshot.as_ref().is_some_and(|bytes| bytes.len() > MAX_CONTAINER) {
        return Err(Error::Limit);
    }
    Ok(snapshot)
}

#[cfg(windows)]
fn persist_windows_with(
    mut staged: tempfile::NamedTempFile,
    target: &Path,
    original: Option<Vec<u8>>,
    mut persist: impl FnMut(
        tempfile::NamedTempFile,
        &Path,
    ) -> std::result::Result<File, tempfile::PersistError>,
    mut read: impl FnMut() -> Result<Option<Vec<u8>>>,
    mut wait: impl FnMut(std::time::Duration),
) -> Result<()> {
    for attempt in 0..4 {
        let failure = match persist(staged, target) {
            Ok(_) => return Ok(()),
            Err(failure) => failure,
        };
        // An error must not authorize replacing a target whose old bytes cannot
        // still be confirmed. Registry will latch CommitUnknown until reopen.
        regular_or_absent(target).map_err(|_| Error::CommitUnknown)?;
        let observed = checked_snapshot(read().map_err(|_| Error::CommitUnknown)?)
            .map_err(|_| Error::CommitUnknown)?;
        if observed != original {
            return Err(Error::CommitUnknown);
        }
        if attempt == 3
            || !matches!(failure.error.raw_os_error(), Some(5 | 32 | 33))
        {
            return Err(Error::Io);
        }
        // Retain this exact synced temporary file; never replay the decision.
        staged = failure.file;
        wait(std::time::Duration::from_millis(10));
        // Recheck after the wait as well: an observed intervening writer must
        // not have its bytes overwritten by our next attempt.
        regular_or_absent(target).map_err(|_| Error::CommitUnknown)?;
        let observed = checked_snapshot(read().map_err(|_| Error::CommitUnknown)?)
            .map_err(|_| Error::CommitUnknown)?;
        if observed != original {
            return Err(Error::CommitUnknown);
        }
    }
    unreachable!("bounded publication attempts return or fail")
}

#[cfg(windows)]
fn persist_windows(
    staged: tempfile::NamedTempFile,
    target: &Path,
    original: Option<Vec<u8>>,
    read: impl FnMut() -> Result<Option<Vec<u8>>>,
) -> Result<()> {
    persist_windows_with(
        staged,
        target,
        original,
        |file, path| file.persist(path),
        read,
        std::thread::sleep,
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{cell::Cell, os::windows::{fs::OpenOptionsExt, io::AsRawHandle}, time::{Duration, Instant}};

    fn staged(root: &Path, bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new_in(root).unwrap();
        file.write_all(bytes).unwrap();
        file.as_file().sync_all().unwrap();
        file
    }

    fn read(path: &Path) -> Result<Option<Vec<u8>>> {
        if !regular_or_absent(path)? {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        File::open(path).map_err(|_| Error::Io)?
            .take(MAX_CONTAINER as u64 + 1)
            .read_to_end(&mut bytes).map_err(|_| Error::Io)?;
        Ok(Some(bytes))
    }

    fn failed(file: tempfile::NamedTempFile, code: i32) -> tempfile::PersistError {
        tempfile::PersistError { error: std::io::Error::from_raw_os_error(code), file }
    }

    #[test]
    fn transient_failure_reuses_the_same_synced_temp_and_stops_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("selection.morrow");
        let file = staged(dir.path(), b"new synthetic bytes");
        let source = file.path().to_path_buf();
        let handle = file.as_file().as_raw_handle();
        let calls = Cell::new(0);
        let waits = Cell::new(0);
        let result = persist_windows_with(file, &target, None, |file, _| {
            assert_eq!(file.path(), source);
            assert_eq!(file.as_file().as_raw_handle(), handle);
            assert_eq!(fs::read(file.path()).unwrap(), b"new synthetic bytes");
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(failed(file, 5))
            } else {
                // This seam isolates retry control from another OS rename race.
                Ok(file.into_file())
            }
        }, || read(&target), |duration| {
            assert_eq!(duration, Duration::from_millis(10));
            waits.set(waits.get() + 1);
        });
        assert_eq!(result, Ok(()));
        assert_eq!(calls.get(), 2);
        assert_eq!(waits.get(), 1);
    }

    #[test]
    fn all_allowed_codes_exhaust_four_attempts_without_changing_old_bytes() {
        for code in [5, 32, 33] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("selection.morrow");
            fs::write(&target, b"old synthetic bytes").unwrap();
            let original = read(&target).unwrap();
            let file = staged(dir.path(), b"new synthetic bytes");
            let source = file.path().to_path_buf();
            let handle = file.as_file().as_raw_handle();
            let calls = Cell::new(0);
            let waits = Cell::new(0);
            let result = persist_windows_with(file, &target, original.clone(), |file, _| {
                assert_eq!(file.path(), source);
                assert_eq!(file.as_file().as_raw_handle(), handle);
                calls.set(calls.get() + 1);
                Err(failed(file, code))
            }, || read(&target), |duration| {
                assert_eq!(duration, Duration::from_millis(10));
                waits.set(waits.get() + 1);
            });
            assert_eq!(result, Err(Error::Io));
            assert_eq!(calls.get(), 4);
            assert_eq!(waits.get(), 3);
            assert_eq!(read(&target).unwrap(), original);
        }
    }

    #[test]
    fn other_os_errors_confirm_old_bytes_once_without_retry() {
        for code in [2, 3, 6, 87, 112] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("selection.morrow");
            let calls = Cell::new(0);
            let reads = Cell::new(0);
            let result = persist_windows_with(staged(dir.path(), b"synthetic"), &target,
                None, |file, _| {
                    calls.set(calls.get() + 1);
                    Err(failed(file, code))
                }, || {
                    reads.set(reads.get() + 1);
                    read(&target)
                }, |_| panic!("must not wait"));
            assert_eq!(result, Err(Error::Io));
            assert_eq!(calls.get(), 1);
            assert_eq!(reads.get(), 1);
        }
    }

    #[test]
    fn changed_target_latches_unknown_before_another_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("selection.morrow");
        fs::write(&target, b"old").unwrap();
        let calls = Cell::new(0);
        let result = persist_windows_with(staged(dir.path(), b"new"), &target,
            Some(b"old".to_vec()), |file, target| {
                calls.set(calls.get() + 1);
                fs::write(target, b"changed synthetic target").unwrap();
                Err(failed(file, 5))
            }, || read(&target), |_| panic!("unknown must not wait"));
        assert_eq!(result, Err(Error::CommitUnknown));
        assert_eq!(calls.get(), 1);
        assert_eq!(fs::read(&target).unwrap(), b"changed synthetic target");
    }

    #[test]
    fn non_retry_error_with_changed_target_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("selection.morrow");
        fs::write(&target, b"old").unwrap();
        let calls = Cell::new(0);
        let result = persist_windows_with(staged(dir.path(), b"new"), &target,
            Some(b"old".to_vec()), |file, target| {
                calls.set(calls.get() + 1);
                fs::write(target, b"changed").unwrap();
                Err(failed(file, 87))
            }, || read(&target), |_| panic!("unknown must not wait"));
        assert_eq!(result, Err(Error::CommitUnknown));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn target_change_during_wait_stops_before_the_second_persist() {
        for mode in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("selection.morrow");
            fs::write(&target, b"old").unwrap();
            let calls = Cell::new(0);
            let result = persist_windows_with(staged(dir.path(), b"new"), &target,
                Some(b"old".to_vec()), |file, _| {
                    calls.set(calls.get() + 1);
                    Err(failed(file, 5))
                }, || read(&target), |_| {
                    if mode == 0 {
                        fs::write(&target, b"changed while waiting").unwrap();
                    } else {
                        fs::remove_file(&target).unwrap();
                        if mode == 2 {
                            fs::create_dir(&target).unwrap();
                        }
                    }
                });
            assert_eq!(result, Err(Error::CommitUnknown));
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn fourth_failure_with_changed_or_unreadable_target_is_unknown() {
        for unreadable in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("selection.morrow");
            fs::write(&target, b"old").unwrap();
            let calls = Cell::new(0);
            let waits = Cell::new(0);
            let result = persist_windows_with(staged(dir.path(), b"new"), &target,
                Some(b"old".to_vec()), |file, target| {
                    calls.set(calls.get() + 1);
                    if calls.get() == 4 && !unreadable {
                        fs::write(target, b"changed on final failure").unwrap();
                    }
                    Err(failed(file, 5))
                }, || {
                    if calls.get() == 4 && unreadable {
                        Err(Error::Io)
                    } else {
                        read(&target)
                    }
                }, |_| waits.set(waits.get() + 1));
            assert_eq!(result, Err(Error::CommitUnknown));
            assert_eq!(calls.get(), 4);
            assert_eq!(waits.get(), 3);
        }
    }

    #[test]
    fn unreadable_or_oversized_observation_latches_unknown() {
        for observation in [Err(Error::Io), Ok(Some(vec![0; MAX_CONTAINER + 1]))] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("selection.morrow");
            let calls = Cell::new(0);
            let result = persist_windows_with(staged(dir.path(), b"new"), &target, None,
                |file, _| {
                    calls.set(calls.get() + 1);
                    Err(failed(file, 32))
                }, || observation.clone(), |_| panic!("unknown must not wait"));
            assert_eq!(result, Err(Error::CommitUnknown));
            assert_eq!(calls.get(), 1);
        }
    }

    #[test]
    fn non_regular_target_latches_unknown_without_reading() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("selection.morrow");
        fs::create_dir(&target).unwrap();
        let calls = Cell::new(0);
        let result = persist_windows_with(staged(dir.path(), b"new"), &target, None,
            |file, _| {
                calls.set(calls.get() + 1);
                Err(failed(file, 33))
            }, || panic!("non-regular target must not read"), |_| panic!("must not wait"));
        assert_eq!(result, Err(Error::CommitUnknown));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn initial_snapshot_is_bounded_before_publication() {
        assert_eq!(checked_snapshot(None), Ok(None));
        assert_eq!(checked_snapshot(Some(vec![0; MAX_CONTAINER])).unwrap().unwrap().len(), MAX_CONTAINER);
        assert_eq!(checked_snapshot(Some(vec![0; MAX_CONTAINER + 1])), Err(Error::Limit));
    }

    #[test]
    fn real_readonly_file_failure_preserves_permissions_and_old_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("selection.morrow");
        fs::write(&target, b"readonly synthetic bytes").unwrap();
        let original_permissions = fs::metadata(&target).unwrap().permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&target, readonly).unwrap();
        let original = read(&target).unwrap();
        let calls = Cell::new(0);
        let result = persist_windows_with(staged(dir.path(), b"new synthetic bytes"),
            &target, original.clone(), |file, path| {
                calls.set(calls.get() + 1);
                file.persist(path)
            }, || read(&target), |_| {});
        let unchanged = fs::metadata(&target).unwrap().permissions().readonly()
            && read(&target).unwrap() == original;
        // Only this owned synthetic fixture's original attributes are restored
        // for cleanup. The publication implementation never changes them.
        fs::set_permissions(&target, original_permissions).unwrap();
        assert!(unchanged);
        assert_eq!(result, Err(Error::Io));
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn real_windows_publication_loop_is_bounded_and_preserves_each_result() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let lock = root.join("registry.lock");
        let lease = OpenOptions::new().read(true).write(true).create(true)
            .truncate(false).share_mode(3).open(&lock).unwrap();
        lease.try_lock().unwrap();
        let target = root.join("selection.morrow");
        let started = Instant::now();
        let mut completed = 0;
        let mut attempts = 0;
        let mut failures = Vec::new();
        for iteration in 0..1000 {
            if started.elapsed() >= Duration::from_secs(110) {
                println!("REGISTRY_LOOP_BOUND completed={completed} attempts={attempts} elapsed_ms={}", started.elapsed().as_millis());
                panic!("real publication loop reached its bound");
            }
            let original = checked_snapshot(read(&target).unwrap()).unwrap();
            regular_or_absent(&target).unwrap();
            let payload = format!("ordinary synthetic registry publication iteration={iteration:04}\n").into_bytes();
            let file = staged(&root, &payload);
            let source = file.path().to_path_buf();
            let result = persist_windows_with(file, &target, original.clone(), |file, path| {
                attempts += 1;
                let result = file.persist(path);
                if let Err(error) = &result {
                    failures.push((iteration, error.error.raw_os_error(), format!("{:?}", error.error.kind())));
                }
                result
            }, || read(&target), std::thread::sleep);
            if let Err(error) = result {
                let after = read(&target);
                println!("REGISTRY_LOOP_FAILURE iteration={iteration} completed={completed} attempts={attempts} failures={failures:?} result={error:?} old={original:?} after={after:?} unchanged={} source={source:?} target={target:?}",
                    after.as_ref().map(|v| v == &original).unwrap_or(false));
                panic!("bounded real publication failed");
            }
            assert_eq!(read(&target).unwrap().as_deref(), Some(payload.as_slice()));
            completed += 1;
        }
        println!("REGISTRY_LOOP_SUMMARY completed={completed} attempts={attempts} failures={failures:?} retries={} elapsed_ms={} target={target:?} final_bytes={:?}",
            attempts - completed, started.elapsed().as_millis(), fs::read(&target).unwrap());
    }
}
