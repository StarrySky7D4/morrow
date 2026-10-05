//! Trusted host-nominated directory tasks on the original owner worker.
//! The supplied root and relative components do not attest to a native picker,
//! ancestors above that root, a durable pathname or public guest authority.
use super::{AccessError, Executor, StartOptions, TaskKey};
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::plugin_package::io::{self, IoCapability};
use morrow_plugin_runtime::{
    directory_io::{
        CaptureLimits, DirectoryRelativePath, MAX_ENTRIES, MAX_METADATA_BYTES, PageRequest,
    },
    io_jobs::{
        DirectoryCommandHandle, DirectoryResponse, DirectorySession, IoWorker, OwnerCommandPoll,
        Poll, DirectoryGuestHandle, DirectoryGuestResult,
    },
};
use std::{collections::BTreeSet, fs::File, time::Duration};

pub(super) struct Admission {
    root: File,
    relative: DirectoryRelativePath,
    limits: CaptureLimits,
    timeout: Duration,
    handler: String,
}
impl Admission {
    pub(super) fn timeout(&self) -> Duration {
        self.timeout
    }
    pub(super) fn submit(self, worker: &IoWorker<WorkbenchState>) -> Result<DirectoryTask> {
        let (session, handle) =
            worker.capture_directory_under(self.root, self.relative, self.limits)?;
        Ok(DirectoryTask {
            session,
            pending: Some(handle),
            pending_kind: CommandKind::Capture,
            captured: false,
            handler: self.handler,
            guest_pending: None,
            guest_invoked: false,
        })
    }
}

#[derive(Clone, Copy)]
enum CommandKind {
    Capture,
    Page,
    Finish,
}
pub(super) struct DirectoryTask {
    session: DirectorySession,
    pending: Option<DirectoryCommandHandle>,
    pending_kind: CommandKind,
    captured: bool,
    handler: String,
    guest_pending: Option<DirectoryGuestHandle>,
    guest_invoked: bool,
}
impl DirectoryTask {
    pub(super) fn poll(&self) -> Option<Poll> {
        if let Some(handle) = &self.guest_pending {
            return Some(match handle.poll() {
                OwnerCommandPoll::Pending => Poll::Pending,
                OwnerCommandPoll::Ready => Poll::Ready,
                OwnerCommandPoll::Consumed => Poll::Consumed,
            });
        }
        self.pending.as_ref().map(|handle| match handle.poll() {
            OwnerCommandPoll::Pending => Poll::Pending,
            OwnerCommandPoll::Ready => Poll::Ready,
            OwnerCommandPoll::Consumed => Poll::Consumed,
        })
    }
}

impl Workbench {
    /// Accept only an already host-nominated root and validated raw UTF-16
    /// components. No open, native identity query or traversal runs here. The
    /// original worker performs FileList validation and C08 fresh generation.
    /// The declaration's IO handler slot is checked without invoking a guest.
    pub fn start_directory(
        &mut self,
        options: StartOptions,
        root: File,
        relative: DirectoryRelativePath,
        limits: CaptureLimits,
        handler: String,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.local_state()?;
        if options.capabilities != BTreeSet::from([IoCapability::FileList])
            || handler.is_empty()
            || handler.len() > 256
            || limits.max_entries > MAX_ENTRIES
            || limits.max_metadata_bytes > MAX_METADATA_BYTES
            || limits.max_job_bytes == 0
            || limits.max_job_bytes > io::MAX_JOB_BYTES
        {
            return Err("invalid trusted directory options".into());
        }
        self.state.submission = None;
        let timeout = options.lifetime;
        self.start_task(options, move |prepared| {
            if !prepared
                .instance
                .package()
                .package()
                .io_declaration()
                .is_some_and(|declaration| declaration.handlers.contains(&handler))
            {
                return Err("directory handler is not declared".into());
            }
            Ok(Admission {
                root,
                relative,
                limits,
                timeout,
                // Clone retains at most the already bounded spelling, not a
                // caller-owned oversized String capacity on the worker queue.
                handler: handler.clone(),
            })
        })
    }

    /// Captured must be consumed before a page may be queued. A pending or
    /// unread result is Busy. The original worker checks the complete cursor;
    /// an invalid cursor must not silently retire some other selection.
    pub fn request_directory_page(&mut self, key: TaskKey, request: PageRequest) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        if !directory.captured || directory.pending.is_some() || directory.guest_pending.is_some() {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let handle = worker.next_directory_page(directory.session, request)?;
        directory.pending = Some(handle);
        directory.pending_kind = CommandKind::Page;
        Ok(())
    }

    pub fn finish_directory(&mut self, key: TaskKey) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        if !directory.captured || directory.pending.is_some() || directory.guest_pending.is_some() {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let handle = worker.finish_directory(directory.session)?;
        directory.pending = Some(handle);
        directory.pending_kind = CommandKind::Finish;
        Ok(())
    }

    /// One new-profile invocation after consuming the original captured result.
    /// The declaration handler is retained from start_directory; no guest path
    /// or replacement grant/root/clock is accepted by this submission method.
    pub fn request_directory_guest_frame(&mut self, key: TaskKey, timeout: Duration) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        if !directory.captured || directory.pending.is_some() || directory.guest_pending.is_some()
            || directory.guest_invoked {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let handle = worker.submit_directory_guest_frame(directory.session,
            directory.handler.clone(), timeout)?;
        directory.guest_pending = Some(handle);
        directory.guest_invoked = true;
        Ok(())
    }

    /// Consume this frame exactly once, then request original Stop. Success,
    /// Finish, Cancel, EOF or this read is not a join; local owner access still
    /// waits for the existing actual worker reclamation path. Unknown is never
    /// automatically resubmitted, even if the guest had produced a response.
    pub fn read_directory_guest_result(&mut self, key: TaskKey) -> Result<Option<DirectoryGuestResult>> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        let handle = directory.guest_pending.as_mut().ok_or(AccessError::StaleTask)?;
        let result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => result.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        directory.guest_pending = None;
        directory.captured = false;
        self.state.request_stop();
        result.map(Some).map_err(Into::into)
    }

    /// Once-only, live-authority delivery. Error or terminal response requests
    /// original Stop; only actual join restores local access to the same owner.
    /// No recapture, replay, native picker proof or guest import is performed.
    pub fn read_directory_result(&mut self, key: TaskKey) -> Result<Option<DirectoryResponse>> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        let handle = directory.pending.as_mut().ok_or(AccessError::StaleTask)?;
        let mut result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => result.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        directory.pending = None;
        let expected = match (&result, directory.pending_kind) {
            (Ok(DirectoryResponse::Captured(_)), CommandKind::Capture)
            | (Ok(DirectoryResponse::Page(_)), CommandKind::Page)
            | (Ok(DirectoryResponse::Finished), CommandKind::Finish)
            | (Err(_), _) => true,
            _ => false,
        };
        if !expected {
            result =
                Err("unexpected directory response; inspect task status before retrying".into());
        }
        if matches!(&result, Ok(DirectoryResponse::Captured(_))) {
            directory.captured = true;
        }
        let stop = match &result {
            Err(_) | Ok(DirectoryResponse::Finished) => true,
            Ok(DirectoryResponse::Page(page)) => page.terminal,
            Ok(DirectoryResponse::Captured(_)) => false,
        };
        if stop {
            directory.captured = false;
            self.state.request_stop();
        }
        result.map(Some).map_err(Into::into)
    }
}
