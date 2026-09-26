//! Trusted-host capture of an already selected, open regular file.
use super::{FileBroker, FileClock, LocalClock, MAX_SPOOL_BYTES};
use crate::{
    io_binding::{self, IoBinding},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

/// Metadata for the immutable bytes retained by this broker, not a filesystem
/// identity, a point-in-time source snapshot, durable evidence or live authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedFile {
    pub reference: [u8; 32],
    pub length: u64,
    pub sha256: [u8; 32],
}

/// Capture never returns a partial reference. OS errors expose only their kind,
/// without disclosing paths or platform error messages to a guest-facing caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureError {
    Admission(io_binding::Error),
    NotRegularFile,
    Cancelled,
    SourceChanged,
    Io(std::io::ErrorKind),
    Allocation,
}
impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "selected file capture: {self:?}")
    }
}
impl std::error::Error for CaptureError {}
impl From<io_binding::Error> for CaptureError {
    fn from(error: io_binding::Error) -> Self {
        Self::Admission(error)
    }
}
impl From<std::io::Error> for CaptureError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl FileBroker {
    /// Fix the actual bytes read from an already selected regular file. This
    /// consumes the handle, seeks to zero, and never opens or resolves a path.
    /// The trusted platform adapter must establish user selection and exclusive
    /// use of the handle's cursor; a guest cannot turn a path into this authority.
    ///
    /// `max_bytes` is an explicit host ceiling (zero permits only an empty file).
    /// Before allocating/reading, reserve one job, one resource and the reported
    /// length plus one EOF-probe byte under the original shared instance budget.
    /// Failed or cancelled captures release slots but never refund admitted bytes.
    /// No reference is published until all bytes, EOF, length and final authority
    /// checks pass. Successful captures release the job and retain the resource.
    ///
    /// The trusted monotonic clock is checked around each bounded read and final
    /// delivery. A synchronous OS read/seek/metadata call cannot be interrupted;
    /// adapters must schedule this on the original owner execution thread rather
    /// than block the UI; opening another Store is not an ownership workaround.
    /// Same-length source edits can yield mixed-time bytes: the returned SHA-256
    /// describes the retained bytes only, not an atomic filesystem snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn grant_open_file(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        file: File,
        max_bytes: u64,
        clock: impl FnMut() -> u64,
    ) -> Result<SelectedFile, CaptureError> {
        self.capture_clock(
            manager,
            host,
            instance,
            binding,
            file,
            max_bytes,
            &mut LocalClock(clock),
            || false,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture_clock(
        &mut self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        mut file: File,
        max_bytes: u64,
        clock: &mut impl FileClock,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SelectedFile, CaptureError> {
        if cancelled() {
            return Err(CaptureError::Cancelled);
        }
        clock.with(|now| {
            binding.preflight_capability(manager, host, instance, IoCapability::FileRead, now)
        })?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(CaptureError::NotRegularFile);
        }
        let length = metadata.len();
        if length > max_bytes.min(MAX_SPOOL_BYTES) {
            return Err(io_binding::Error::Limit.into());
        }
        let size = usize::try_from(length).map_err(|_| io_binding::Error::Limit)?;
        let charged = length.checked_add(1).ok_or(io_binding::Error::Limit)?;
        if cancelled() {
            return Err(CaptureError::Cancelled);
        }
        let (reference, next, lease) = clock.with(|now| {
            self.reserve_file(manager, host, instance, binding, length, charged, now)
        })?;
        let mut check = || -> Result<(), CaptureError> {
            if cancelled() {
                return Err(CaptureError::Cancelled);
            }
            clock.with(|now| lease.check(manager, host, instance, now))?;
            Ok(())
        };
        check()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| CaptureError::Allocation)?;
        bytes.resize(size, 0);
        check()?;
        file.seek(SeekFrom::Start(0))?;
        let mut offset = 0;
        while offset < size {
            check()?;
            let end = offset
                .saturating_add(morrow_core::io::MAX_PAYLOAD_BYTES)
                .min(size);
            match file.read(&mut bytes[offset..end]) {
                Ok(0) => return Err(CaptureError::SourceChanged),
                Ok(count) => offset += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
            check()?;
        }
        let mut probe = [0; 1];
        loop {
            check()?;
            match file.read(&mut probe) {
                Ok(0) => break,
                Ok(_) => return Err(CaptureError::SourceChanged),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        check()?;
        if file.metadata()?.len() != length {
            return Err(CaptureError::SourceChanged);
        }
        let sha256 = Sha256::digest(&bytes).into();
        check()?;
        self.publish_file(reference, next, lease, bytes);
        Ok(SelectedFile {
            reference,
            length,
            sha256,
        })
    }
}
