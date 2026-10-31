//! Exact live approval for one filesystem observation window. No native capture
//! or operation is performed here. The owned spelling is a host nomination, not
//! picker-time identity; the original worker must protect and validate its chain.
#![deny(warnings)]
use crate::{
    directory_io::DirectoryAbsolutePath,
    io_binding::{self, IoBinding, Usage},
    manager::{ManagedInstance, Manager},
    monotonic::Instant,
};
use morrow_core::dispatch::HostRuntime;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Limit,
    Expired,
    Binding(io_binding::Error),
}
impl From<io_binding::Error> for Error {
    fn from(value: io_binding::Error) -> Self {
        Self::Binding(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "watch approval: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Host-proposed restrictions. These values are not permission or OS identity.
pub struct ApprovalRequest {
    pub expected_revision: u64,
    pub handler: String,
    pub nominated_path: DirectoryAbsolutePath,
    pub max_frames: u32,
    pub max_wire_bytes: u64,
    pub window_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_frames: u32,
    pub max_wire_bytes: u64,
    pub window_ms: u64,
}
struct Restriction {
    revoked: AtomicBool,
    registry_revision: Arc<AtomicBool>,
    deadline: Instant,
    expires: u64,
}
/// Restriction only. It cannot issue, restart, rebind, extend or consume a window.
#[derive(Clone)]
pub struct WatchRevocation {
    state: Arc<Restriction>,
}
impl WatchRevocation {
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
}
/// Nonclone, nonserializable authority tied to the original Manager, HostRuntime,
/// connection, IoContext, registry revision and owned nomination. The declaration
/// and persistent approval only provide ceilings; they cannot construct this.
pub struct WatchApproval {
    binding: IoBinding,
    state: Arc<Restriction>,
    path: DirectoryAbsolutePath,
    handler: String,
    limits: Limits,
    revision: u64,
    native_confirmation: bool,
}
impl WatchApproval {
    pub(crate) fn original_binding(&self) -> &IoBinding {
        &self.binding
    }
    pub(crate) fn remaining_duration(&self) -> Duration {
        self.state
            .deadline
            .saturating_duration_since(Instant::now())
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.state.deadline
    }
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        request: ApprovalRequest,
        now: u64,
    ) -> Result<Self, Error> {
        // Authenticate before checking or advancing any shared clock/accounting.
        binding.preflight(manager, host, instance, now)?;
        let package = instance.package().package();
        let declaration = package.fs_watch_declaration().ok_or(Error::Denied)?;
        let selection = manager
            .selection(&package.manifest().package_id)
            .ok_or(Error::Denied)?;
        if request.expected_revision != manager.revision()
            || !selection.enabled
            || !selection.approved_fs_watch
            || selection.digest != package.digest()
            || instance.connection().package_digest() != Some(package.digest())
            || !declaration.handlers.contains(&request.handler)
        {
            return Err(Error::Denied);
        }
        if request.max_frames == 0
            || request.max_frames > declaration.max_frames
            || request.max_wire_bytes == 0
            || request.max_wire_bytes > declaration.max_wire_bytes
            || request.window_ms == 0
            || request.window_ms > declaration.max_window_ms
        {
            return Err(Error::Limit);
        }
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(request.window_ms))
            .ok_or(Error::Limit)?;
        let expires = now.checked_add(request.window_ms).ok_or(Error::Limit)?;
        let state = Arc::new(Restriction {
            revoked: AtomicBool::new(false),
            registry_revision: manager.changes_revision_guard(),
            deadline,
            expires,
        });
        let binding = binding.claim_fs_watch(manager, host, instance, request.window_ms, now)?;
        Ok(Self {
            binding,
            state,
            path: request.nominated_path,
            handler: request.handler,
            limits: Limits {
                max_frames: request.max_frames,
                max_wire_bytes: request.max_wire_bytes,
                window_ms: request.window_ms,
            },
            revision: request.expected_revision,
            native_confirmation: false,
        })
    }
    /// Recheck under the original owner clock before capture, native reads and
    /// delivery. A wrong identity cannot poison the legitimate window's clock.
    pub fn check(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<(), Error> {
        if let Err(error) = self.binding.preflight(manager, host, instance, now) {
            if matches!(error, io_binding::Error::Clock | io_binding::Error::Expired) {
                self.state.revoked.store(true, Ordering::Release);
            }
            return Err(error.into());
        }
        self.check_window(now)?;
        if let Err(error) = self.binding.check(manager, host, instance, now) {
            if matches!(error, io_binding::Error::Clock | io_binding::Error::Expired) {
                self.state.revoked.store(true, Ordering::Release);
            }
            return Err(error.into());
        }
        self.check_window(now)
    }
    fn check_window(&self, now: u64) -> Result<(), Error> {
        if self.state.revoked.load(Ordering::Acquire)
            || self.state.registry_revision.load(Ordering::Acquire)
        {
            return Err(Error::Denied);
        }
        if now >= self.state.expires || Instant::now() >= self.state.deadline {
            self.state.revoked.store(true, Ordering::Release);
            return Err(Error::Expired);
        }
        Ok(())
    }
    pub fn revocation(&self) -> WatchRevocation {
        WatchRevocation {
            state: self.state.clone(),
        }
    }
    /// Diagnostic original-instance usage. Reading it creates no reservation.
    pub fn usage(&self) -> Usage {
        self.binding.usage()
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn registry_revision(&self) -> u64 {
        self.revision
    }
    pub fn handler(&self) -> &str {
        &self.handler
    }
    /// Further restrict this one original window to a native chooser confirmation.
    /// The selection must match the already approved raw spelling. This cannot
    /// replace the nomination, renew its deadline, grant paths, or create credit.
    pub fn require_native_confirmation(mut self) -> Self {
        self.native_confirmation = true;
        self
    }
    pub fn requires_native_confirmation(&self) -> bool {
        self.native_confirmation
    }
    /// Host inspection of the original spelling, not proof of protected capture.
    pub fn nominated_path(&self) -> &DirectoryAbsolutePath {
        &self.path
    }
}
