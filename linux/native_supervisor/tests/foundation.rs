#![cfg(target_os = "linux")]
use morrow_linux_supervisor_foundation::{
    ExitObservation, OwnedProcess, SealedExecutable, SpawnFailure, digest,
};
use std::{
    ffi::OsStr,
    fs,
    os::{
        fd::AsRawFd,
        unix::fs::{PermissionsExt, symlink},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    hash: [u8; 32],
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "morrow-linux-foundation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let artifact = root.join("guest");
        let bytes = fs::read(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture")).unwrap();
        fs::write(&artifact, &bytes).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            artifact,
            hash: digest(&bytes),
        }
    }
    fn sealed(&self) -> SealedExecutable {
        SealedExecutable::read(&self.artifact, self.hash).unwrap()
    }
    fn spawn(&self, mode: &str) -> OwnedProcess {
        OwnedProcess::spawn(self.sealed(), &self.root, &[OsStr::new(mode)]).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn sealed_object_rejects_write_growth_shrink_and_execute_bit_change() {
    let fixture = Fixture::new();
    let executable = fixture.sealed();
    let fd = executable.as_fd().as_raw_fd();
    assert_eq!(unsafe { libc::pwrite(fd, b"x".as_ptr().cast(), 1, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );
    assert_eq!(unsafe { libc::ftruncate(fd, 0) }, -1);
    assert_eq!(
        unsafe { libc::ftruncate(fd, executable.source_identity().length as i64 + 1) },
        -1
    );
    assert_eq!(unsafe { libc::fchmod(fd, 0o400) }, -1);
    let seals = unsafe { libc::fcntl(fd, libc::F_GET_SEALS) };
    assert_eq!(seals & libc::F_SEAL_EXEC, libc::F_SEAL_EXEC);
    executable.verify().unwrap();
}

#[test]
fn bad_hash_and_scripts_are_rejected_before_launch() {
    let fixture = Fixture::new();
    assert!(SealedExecutable::read(&fixture.artifact, [0; 32]).is_err());
    fs::write(&fixture.artifact, b"#!/bin/sh\nexit 0\n").unwrap();
    assert!(SealedExecutable::read(&fixture.artifact, digest(b"#!/bin/sh\nexit 0\n")).is_err());
}

#[test]
fn source_symlinks_hardlinks_and_shared_writes_are_refused() {
    let fixture = Fixture::new();
    let linked = fixture.root.join("link");
    symlink(&fixture.artifact, &linked).unwrap();
    assert!(SealedExecutable::read(&linked, fixture.hash).is_err());
    fs::remove_file(&linked).unwrap();
    fs::hard_link(&fixture.artifact, &linked).unwrap();
    assert!(SealedExecutable::read(&fixture.artifact, fixture.hash).is_err());
    fs::remove_file(&linked).unwrap();
    fs::set_permissions(&fixture.artifact, fs::Permissions::from_mode(0o720)).unwrap();
    assert!(SealedExecutable::read(&fixture.artifact, fixture.hash).is_err());
}

#[test]
fn replacing_original_path_cannot_replace_executed_memfd() {
    let fixture = Fixture::new();
    let executable = fixture.sealed();
    fs::rename(&fixture.artifact, fixture.root.join("original")).unwrap();
    fs::write(&fixture.artifact, b"changed pathname after sealing").unwrap();
    let mut child = OwnedProcess::spawn(executable, &fixture.root, &[OsStr::new("echo")]).unwrap();
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
    assert_eq!(child.exit_observation(), Some(ExitObservation::Exited(17)));
    assert_eq!(child.artifact_digest(), fixture.hash);
    let stdout = String::from_utf8(child.output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("sealed-main-elf\nenv-count=0\nstdin-bytes=0\n"),
        "{stdout}"
    );
    assert!(stdout.contains(&format!("cwd={}\n", fixture.root.display())));
    assert_eq!(child.output().stderr, b"sealed-stderr\n");
    assert!(child.output().stdout_eof && child.output().stderr_eof);
}

#[test]
fn pidfd_signal_reaps_exact_child_and_records_both_eofs() {
    let fixture = Fixture::new();
    let mut child = fixture.spawn("sleep");
    assert!(!child.observe().unwrap());
    assert!(child.pid() > 0);
    child.terminate().unwrap();
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
    assert_eq!(
        child.exit_observation(),
        Some(ExitObservation::Signaled(libc::SIGKILL))
    );
    let observed = child.exit_observation();
    child.terminate().unwrap();
    assert!(child.observe().unwrap());
    assert_eq!(child.exit_observation(), observed);
}

#[test]
fn output_is_capped_and_excess_drained_before_cleanup() {
    let fixture = Fixture::new();
    let mut child = fixture.spawn("flood");
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
    assert_eq!(child.output().stdout.len(), 64 * 1024);
    assert_eq!(child.output().stderr.len(), 64 * 1024);
    assert_eq!(child.output().stdout_discarded, 192 * 1024);
    assert_eq!(child.output().stderr_discarded, 64 * 1024);
    assert_eq!(child.exit_observation(), Some(ExitObservation::Exited(0)));
}

#[test]
fn direct_exit_does_not_prove_descendant_pipe_cleanup() {
    let fixture = Fixture::new();
    let mut child = fixture.spawn("retain-output");
    assert!(!child.wait_bounded(Duration::from_millis(25)).unwrap());
    assert_eq!(child.exit_observation(), Some(ExitObservation::Exited(0)));
    assert!(!child.cleanup_complete());
    assert!(!child.output().stdout_eof || !child.output().stderr_eof);
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
}

#[test]
fn invalid_cwd_arguments_and_relative_artifacts_are_not_created() {
    let fixture = Fixture::new();
    assert!(SealedExecutable::read(Path::new("guest"), fixture.hash).is_err());
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        OwnedProcess::spawn(fixture.sealed(), &fixture.root, &[]),
        Err(SpawnFailure::NotCreated(_))
    ));
    fs::set_permissions(&fixture.root, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(matches!(
        OwnedProcess::spawn(fixture.sealed(), &fixture.root, &[OsStr::new("nul\0")]),
        Err(SpawnFailure::NotCreated(_))
    ));
}

#[test]
fn closed_standard_descriptors_cannot_clobber_artifact_or_cwd() {
    let fixture = Fixture::new();
    let receipt = fixture.root.join("closed-stdio.receipt");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture"))
        .arg("closed-stdio-check")
        .arg(&fixture.artifact)
        .arg(&fixture.root)
        .arg(&receipt)
        .status()
        .unwrap();
    assert!(result.success());
    assert_eq!(
        fs::read(receipt).unwrap(),
        b"closed-stdio-sealed-exec-pidfd-cleanup-ok\n"
    );
}

#[test]
fn exec_failure_keeps_real_owner_until_reap_and_both_eofs() {
    let fixture = Fixture::new();
    let mut malformed = [0u8; 64];
    malformed[..4].copy_from_slice(b"\x7fELF");
    malformed[4..7].copy_from_slice(&[2, 1, 1]);
    malformed[16..18].copy_from_slice(&2u16.to_le_bytes());
    let machine = if cfg!(target_arch = "x86_64") {
        62u16
    } else {
        183u16
    };
    malformed[18..20].copy_from_slice(&machine.to_le_bytes());
    fs::write(&fixture.artifact, malformed).unwrap();
    let executable = SealedExecutable::read(&fixture.artifact, digest(&malformed)).unwrap();
    let failure = OwnedProcess::spawn(executable, &fixture.root, &[])
        .err()
        .unwrap();
    match failure {
        SpawnFailure::Created { error, mut owner } => {
            assert!(error.to_string().contains("execveat"));
            assert!(owner.pid() > 0);
            assert!(owner.wait_bounded(Duration::from_secs(2)).unwrap());
            assert_eq!(owner.exit_observation(), Some(ExitObservation::Exited(127)));
            assert!(owner.cleanup_complete());
        }
        SpawnFailure::NotCreated(error) => panic!("child ownership was lost: {error}"),
    }
}

#[test]
fn unrelated_inheritable_descriptor_is_closed_in_child_only() {
    let fixture = Fixture::new();
    let marker = fs::File::open(&fixture.artifact).unwrap();
    let fd = marker.as_raw_fd();
    assert!(fd >= 3);
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }, 0);
    let mut child = fixture.spawn("fd-check");
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
    assert_eq!(child.output().stdout, b"extra-open-fds=0\n");
    assert_eq!(
        unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
        0
    );
}
