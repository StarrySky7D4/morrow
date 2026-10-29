//! Trusted directory tasks on the original owner worker. Open anchors retain
//! their prior provenance boundary; native-selected absolute paths are resolved
//! from a verified volume root. Neither API attests to picker-time identity or
//! grants a durable pathname or public guest authority.
use super::{AccessError, Executor, StartOptions, TaskKey};
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::plugin_package::io::{self, IoCapability};
use morrow_plugin_runtime::{
    directory_io::{
        CaptureLimits, DirectoryAbsolutePath, DirectoryRelativePath, DirectoryRetirementSnapshot,
        Error as DirectoryError, MAX_ENTRIES, MAX_METADATA_BYTES, PageRequest,
    },
    io_jobs::{
        DirectoryCommandHandle, DirectoryGuestHandle, DirectoryGuestResult, DirectoryResponse,
        DirectorySession, IoWorker, OwnerCommandPoll, Poll,
    },
};
use std::{collections::BTreeSet, fs::File, path::PathBuf, time::Duration};

/// A once-only delivery. A definitive refusal is separate from transport or
/// native-close uncertainty and is never evidence that the worker has joined.
pub enum DirectoryDelivery {
    Response(DirectoryResponse),
    Refused(DirectoryError),
}

fn classify_directory_delivery(
    value: std::result::Result<DirectoryResponse, DirectoryError>,
    retirement: DirectoryRetirementSnapshot,
) -> std::result::Result<DirectoryDelivery, String> {
    match value {
        Ok(value) => Ok(DirectoryDelivery::Response(value)),
        Err(error) if error != DirectoryError::NativeCloseUnknown && retirement.all_closed() => {
            Ok(DirectoryDelivery::Refused(error))
        }
        Err(_) => Err("directory refusal has unconfirmed native retirement".into()),
    }
}

/// Private application request. A native selection is required at the UI
/// boundary; the worker proves the object only when this request is captured.
pub struct DirectoryStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub handler: String,
    pub selected_path: PathBuf,
    pub max_entries: usize,
    pub max_metadata_bytes: u64,
    pub timeout_ms: u32,
}

/// Bound metadata for an already opened host selection. This is trusted host
/// input, not native-picker provenance or a guest pathname authority.
pub struct DirectoryObjectStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub handler: String,
    pub max_entries: usize,
    pub max_metadata_bytes: u64,
    pub timeout_ms: u32,
}

enum Source<'a> {
    Under {
        root: File,
        relative: DirectoryRelativePath,
    },
    Selected(DirectoryAbsolutePath),
    NativePicker,
    Retained(&'a mut Option<File>),
}

pub(super) struct Admission<'a> {
    source: Source<'a>,
    limits: CaptureLimits,
    timeout: Duration,
    handler: String,
}
impl Admission<'_> {
    pub(super) fn timeout(&self) -> Duration {
        self.timeout
    }
    pub(super) fn submit(self, worker: &IoWorker<WorkbenchState>) -> Result<DirectoryTask> {
        let (session, handle) = match self.source {
            Source::Under { root, relative } => {
                worker.capture_directory_under(root, relative, self.limits)?
            }
            Source::Selected(selected) => {
                worker.capture_selected_directory(selected, self.limits)?
            }
            Source::NativePicker => worker.capture_directory_native_picker(self.limits)?,
            Source::Retained(original) => {
                let file = original
                    .take()
                    .ok_or("directory selection already transferred")?;
                match worker.capture_directory_fresh_retained(file, self.limits) {
                    Ok(accepted) => accepted,
                    Err((error, file)) => {
                        *original = Some(file);
                        return Err(error.into());
                    }
                }
            }
        };
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
        self.state.submission = None;
        self.start_directory_source(options, Source::Under { root, relative }, limits, handler)
    }

    /// No filesystem operation before authority moves to the original worker.
    /// Once admitted, a submission cannot be used for another selection, even
    /// after error/cancellation. File and directory selections share the same
    /// existing 512-entry submission ledger rather than raising its capacity.
    pub fn start_selected_directory(&mut self, request: DirectoryStart) -> Result<TaskKey> {
        let selected = DirectoryAbsolutePath::new(&request.selected_path)?;
        self.start_directory_submission(
            DirectoryObjectStart {
                submission: request.submission,
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                handler: request.handler,
                max_entries: request.max_entries,
                max_metadata_bytes: request.max_metadata_bytes,
                timeout_ms: request.timeout_ms,
            },
            Source::Selected(selected),
        )
    }

    /// Borrow the original owner's selection through preparation, binding,
    /// worker creation and product admission. Known pre-enqueue refusal leaves
    /// that exact File in `original`; it is not cloned, reopened or closed.
    /// Successful enqueue takes it once. An empty slot on return is a transfer,
    /// never permission to reopen, retry or infer checked worker retirement.
    /// A rejected submission may still be spent; inspect the original task's
    /// status before any separate explicit request. No native picker is added.
    pub fn start_open_directory_retained(
        &mut self,
        request: DirectoryObjectStart,
        original: &mut Option<File>,
    ) -> Result<TaskKey> {
        if original.is_none() {
            return Err("directory selection already transferred".into());
        }
        self.start_directory_submission(request, Source::Retained(original))
    }

    /// Prepare the original FileList owner before it opens the native chooser.
    /// No caller-supplied path or OS handle is accepted on this private lane.
    pub fn start_native_directory_picker(
        &mut self,
        request: DirectoryObjectStart,
    ) -> Result<TaskKey> {
        self.start_directory_submission(request, Source::NativePicker)
    }

    fn start_directory_submission(
        &mut self,
        request: DirectoryObjectStart,
        source: Source<'_>,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        let owner = self.local_state()?;
        if request.submission == [0; 32]
            || self.state.file_submissions.contains(&request.submission)
            || self.state.file_submissions.len() >= 512
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
            || request.max_entries > MAX_ENTRIES
            || request.max_metadata_bytes > MAX_METADATA_BYTES
        {
            return Err("invalid, repeated or exhausted directory submission".into());
        }
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        let budget = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing directory IO budget")?;
        if u64::from(request.timeout_ms) > budget.max_duration_ms {
            return Err("directory task exceeds declared duration".into());
        }
        let limits = morrow_plugin_runtime::io_jobs::JobLimits::new(
            morrow_plugin_runtime::io_jobs::JobLimits::default().max_calls,
            budget.max_job_bytes.min(io::MAX_JOB_BYTES),
            budget.max_bytes.min(io::MAX_BYTES),
        )
        .map_err(|_| "invalid directory job budget")?;
        let capture = CaptureLimits {
            max_entries: request.max_entries,
            max_metadata_bytes: request.max_metadata_bytes,
            max_job_bytes: budget.max_job_bytes.min(io::MAX_JOB_BYTES),
        };
        self.state.file_submissions.insert(request.submission);
        self.state.submission = Some(request.submission);
        self.start_directory_source(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([IoCapability::FileList]),
                lifetime: Duration::from_millis(u64::from(request.timeout_ms)),
                limits,
            },
            source,
            capture,
            request.handler,
        )
    }

    fn start_directory_source(
        &mut self,
        options: StartOptions,
        source: Source<'_>,
        limits: CaptureLimits,
        handler: String,
    ) -> Result<TaskKey> {
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
                source,
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
        if !directory.captured
            || directory.pending.is_some()
            || directory.guest_pending.is_some()
            || directory.guest_invoked
        {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let handle = worker.submit_directory_guest_frame(
            directory.session,
            directory.handler.clone(),
            timeout,
        )?;
        directory.guest_pending = Some(handle);
        directory.guest_invoked = true;
        Ok(())
    }

    /// Consume this frame exactly once, then request original Stop. Success,
    /// Finish, Cancel, EOF or this read is not a join; local owner access still
    /// waits for the existing actual worker reclamation path. Unknown is never
    /// automatically resubmitted, even if the guest had produced a response.
    pub fn read_directory_guest_result(
        &mut self,
        key: TaskKey,
    ) -> Result<Option<DirectoryGuestResult>> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        let handle = directory
            .guest_pending
            .as_mut()
            .ok_or(AccessError::StaleTask)?;
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
        match self.read_directory_delivery(key)? {
            None => Ok(None),
            Some(DirectoryDelivery::Response(value)) => Ok(Some(value)),
            Some(DirectoryDelivery::Refused(error)) => Err(error.to_string().into()),
        }
    }

    /// Private UI transport keeps a live, delivered refusal typed. Original
    /// authority, once-only read, Stop, and actual reclamation stay unchanged.
    pub fn read_directory_delivery(&mut self, key: TaskKey) -> Result<Option<DirectoryDelivery>> {
        let task = self.state.checked_task(key)?;
        let directory = task.directory.as_mut().ok_or(AccessError::StaleTask)?;
        let handle = directory.pending.as_mut().ok_or(AccessError::StaleTask)?;
        let mut result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => classify_directory_delivery(result, handle.retirement_snapshot()),
            Err(error) => Err(error.to_string()),
        };
        directory.pending = None;
        let expected = matches!((&result, directory.pending_kind),
            (
                Ok(DirectoryDelivery::Response(DirectoryResponse::Captured(_))),
                CommandKind::Capture,
            )
            | (Ok(DirectoryDelivery::Response(DirectoryResponse::Page(_))), CommandKind::Page)
            | (Ok(DirectoryDelivery::Response(DirectoryResponse::Finished)), CommandKind::Finish)
            | (Ok(DirectoryDelivery::Refused(_)), _)
            | (Err(_), _)
        );
        if !expected {
            result =
                Err("unexpected directory response; inspect task status before retrying".into());
        }
        if matches!(
            &result,
            Ok(DirectoryDelivery::Response(DirectoryResponse::Captured(_)))
        ) {
            directory.captured = true;
        }
        let stop = match &result {
            Err(_)
            | Ok(DirectoryDelivery::Refused(_))
            | Ok(DirectoryDelivery::Response(DirectoryResponse::Finished)) => true,
            Ok(DirectoryDelivery::Response(DirectoryResponse::Page(page))) => page.terminal,
            Ok(DirectoryDelivery::Response(DirectoryResponse::Captured(_))) => false,
        };
        if stop {
            directory.captured = false;
            self.state.request_stop();
        }
        result.map(Some).map_err(Into::into)
    }
}

#[cfg(test)]
mod refusal_delivery_tests {
    use super::*;

    #[test]
    fn known_cancel_requires_original_picker_and_native_objects_closed() {
        let closed = DirectoryRetirementSnapshot {
            worker_bound: true,
            picker_started: true,
            picker_closed: true,
            objects: 1,
            close_calls: 1,
            confirmed_closed: 1,
            unknown: 0,
        };
        assert!(matches!(
            classify_directory_delivery(Err(DirectoryError::Cancelled), closed),
            Ok(DirectoryDelivery::Refused(DirectoryError::Cancelled))
        ));
        for incomplete in [
            DirectoryRetirementSnapshot {
                worker_bound: false,
                ..closed
            },
            DirectoryRetirementSnapshot {
                picker_closed: false,
                ..closed
            },
            DirectoryRetirementSnapshot {
                confirmed_closed: 0,
                ..closed
            },
            DirectoryRetirementSnapshot {
                unknown: 1,
                ..closed
            },
        ] {
            assert!(
                classify_directory_delivery(Err(DirectoryError::Cancelled), incomplete).is_err()
            );
        }
        assert!(
            classify_directory_delivery(Err(DirectoryError::NativeCloseUnknown), closed).is_err()
        );
    }

    #[test]
    fn live_delivered_rejection_preserves_kind_without_synthesizing_completion() {
        let closed = DirectoryRetirementSnapshot {
            worker_bound: true,
            ..Default::default()
        };
        for error in [
            DirectoryError::Admission(morrow_plugin_runtime::io_binding::Error::Expired),
            DirectoryError::ReparsePoint,
            DirectoryError::Limit,
            DirectoryError::Entropy,
        ] {
            assert!(matches!(classify_directory_delivery(Err(error), closed),
                Ok(DirectoryDelivery::Refused(actual)) if actual == error));
        }
    }
}
