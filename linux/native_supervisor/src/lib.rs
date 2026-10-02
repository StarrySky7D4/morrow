//! Bounded Linux executable/process-object prerequisites, not a product owner.
//!
//! No Store, profile, key provider, network or SDK binding is available here.
//! Sealed executable identity covers the main ELF only, not its interpreter or
//! shared libraries. A pidfd covers one process/thread group, not descendants.
//! An exited/reaped child alone is never reported as complete cleanup: both
//! captured output pipes must independently reach EOF as well.
#![cfg(target_os = "linux")]
#![deny(unsafe_op_in_unsafe_fn)]

mod executable;
mod process;

pub use executable::{MAX_EXECUTABLE_BYTES, SealedExecutable, SourceIdentity, digest};
pub use process::{ExitObservation, OutputObservation, OwnedProcess, SpawnFailure};

fn invalid(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}

fn kernel_error(operation: &'static str) -> std::io::Error {
    let error = std::io::Error::last_os_error();
    std::io::Error::new(error.kind(), format!("{operation}: {error}"))
}

fn above_stdio(file: std::fs::File) -> std::io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    if file.as_raw_fd() >= 3 {
        return Ok(file);
    }
    // SAFETY: file is held; fcntl creates a new CLOEXEC descriptor >=3.
    let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        return Err(kernel_error("duplicate descriptor away from stdio"));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}
