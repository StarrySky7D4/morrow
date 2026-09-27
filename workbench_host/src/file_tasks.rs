//! Trusted selected-handle file tasks, using the same original-owner executor.
//! Trusted private paths are opened only on the original worker, never by a guest.
use super::{AccessError, Executor, PreparedJob, StartOptions, TaskKey};
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::plugin_package::io::IoCapability;
use morrow_plugin_runtime::io_jobs::{
    FileCommandHandle, FileResponse, FileSession, IoWorker, JobHandle, OwnerCommandPoll, Poll,
};
use std::{collections::BTreeSet, fs::File, path::PathBuf, time::Duration};

pub(super) enum Source {
    Open(File),
    SelectedPath(PathBuf),
}

pub struct FileStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub handler: String,
    pub selected_path: PathBuf,
    pub max_bytes: u64,
    pub timeout_ms: u32,
}

pub(super) enum Admission {
    Io(PreparedJob),
    #[cfg(windows)]
    Mutation(super::mutation::Admission),
    File {
        source: Source,
        handler: String,
        max_bytes: u64,
        secret: [u8; 32],
        timeout: Duration,
    },
}
pub(super) enum Submitted {
    Io(JobHandle),
    #[cfg(windows)]
    Mutation(super::mutation::MutationTask),
    File(FileTask),
}
impl Admission {
    pub(super) fn timeout(&self) -> Duration {
        match self {
            Self::Io(job) => job.timeout,
            #[cfg(windows)]
            Self::Mutation(job) => job.timeout,
            Self::File { timeout, .. } => *timeout,
        }
    }
    pub(super) fn submit(self, worker: &IoWorker<WorkbenchState>) -> Result<Submitted> {
        match self {
            #[cfg(windows)]
            Self::Mutation(job) => Ok(Submitted::Mutation(job.submit(worker)?)),
            Self::Io(job) => Ok(Submitted::Io(
                worker
                    .submit_brokered(job.input, job.router, job.timeout)
                    .map_err(|e| format!("IO submission: {e:?}"))?,
            )),
            Self::File {
                source,
                handler,
                max_bytes,
                secret,
                ..
            } => {
                let (session, handle) = match source {
                    Source::Open(file) => worker.capture_file(file, handler, max_bytes, secret),
                    Source::SelectedPath(path) => {
                        worker.capture_selected_path(path, handler, max_bytes, secret)
                    }
                }?;
                Ok(Submitted::File(FileTask {
                    session,
                    pending: Some(handle),
                    captured: false,
                }))
            }
        }
    }
}
pub(super) struct FileTask {
    session: FileSession,
    pending: Option<FileCommandHandle>,
    captured: bool,
}
impl FileTask {
    pub(super) fn poll(&self) -> Option<Poll> {
        self.pending.as_ref().map(|p| match p.poll() {
            OwnerCommandPoll::Pending => Poll::Pending,
            OwnerCommandPoll::Ready => Poll::Ready,
            OwnerCommandPoll::Consumed => Poll::Consumed,
        })
    }
}
impl Workbench {
    /// Admission accepts only a trusted platform-selected handle, never a guest path.
    /// Capturing starts after the full WorkbenchState moves to its original worker.
    /// The caller must poll/read Captured before requesting chunks, then Finish.
    /// cancel_io/repair_io/acknowledge_io retain their original join/cleanup rules.
    pub fn start_file(
        &mut self,
        options: StartOptions,
        file: File,
        handler: String,
        max_bytes: u64,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.local_state()?;
        self.state.submission = None;
        self.start_file_source(options, Source::Open(file), handler, max_bytes)
    }
    /// Explicit trusted native selection. No filesystem call occurs before
    /// ownership moves. This path does not assert picker-time file identity.
    pub fn start_selected_file(&mut self, request: FileStart) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        let owner = self.local_state()?;
        let path = request
            .selected_path
            .to_str()
            .ok_or("invalid selected path")?;
        if request.submission == [0; 32]
            || self.state.file_submissions.contains(&request.submission)
            || self.state.file_submissions.len() >= 512
            || !request.selected_path.is_absolute()
            || path.len() > 4096
            || path.contains('\0')
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
            || request.max_bytes > 256 * 1024 * 1024
        {
            return Err("invalid, repeated or exhausted file submission".into());
        }
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        let budget = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing file IO budget")?;
        if u64::from(request.timeout_ms) > budget.max_duration_ms {
            return Err("file task exceeds declared duration".into());
        }
        let limits = morrow_plugin_runtime::io_jobs::JobLimits::new(
            1,
            budget.max_job_bytes.min(256 * 1024 * 1024 + 1),
            budget.max_bytes.min(512 * 1024 * 1024),
        )
        .map_err(|_| "invalid file job budget")?;
        self.state.file_submissions.insert(request.submission);
        self.state.submission = Some(request.submission);
        self.start_file_source(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([IoCapability::FileRead]),
                lifetime: Duration::from_millis(u64::from(request.timeout_ms)),
                limits,
            },
            Source::SelectedPath(request.selected_path),
            request.handler,
            request.max_bytes,
        )
    }
    fn start_file_source(
        &mut self,
        options: StartOptions,
        source: Source,
        handler: String,
        max_bytes: u64,
    ) -> Result<TaskKey> {
        if options.capabilities != BTreeSet::from([IoCapability::FileRead])
            || handler.is_empty()
            || handler.len() > 256
            || max_bytes > 256 * 1024 * 1024
            || max_bytes
                .checked_add(1)
                .is_none_or(|n| n > options.limits.max_job_bytes)
        {
            return Err("invalid selected-file options".into());
        }
        let timeout = options.lifetime;
        self.start_task(options, move |p| {
            if !p
                .instance
                .package()
                .package()
                .io_declaration()
                .is_some_and(|d| d.handlers.contains(&handler))
            {
                return Err("file handler is not declared".into());
            }
            let mut secret = [0; 32];
            getrandom::fill(&mut secret)?;
            Ok(Admission::File {
                source,
                handler,
                max_bytes,
                secret,
                timeout,
            })
        })
    }
    /// One pending/unread result per application file task. Too-early/repeated
    /// admission is Busy; offsets stay explicit and are never advanced by retry.
    pub fn request_file_chunk(&mut self, key: TaskKey, offset: u64, limit: u32) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let file = task.file.as_mut().ok_or(AccessError::StaleTask)?;
        if !file.captured || file.pending.is_some() {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        file.pending = Some(worker.read_file_chunk(file.session, offset, limit)?);
        Ok(())
    }
    pub fn finish_file(&mut self, key: TaskKey) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let file = task.file.as_mut().ok_or(AccessError::StaleTask)?;
        if !file.captured || file.pending.is_some() {
            return Err(AccessError::Busy.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        file.pending = Some(worker.finish_file(file.session)?);
        Ok(())
    }
    /// Nonblocking, once-only delivery with a final live-authority check. Any
    /// terminal command error stops this task; no automatic recapture or reread.
    /// Receiving Finished requests stop; only poll's actual join restores access.
    pub fn read_file_result(&mut self, key: TaskKey) -> Result<Option<FileResponse>> {
        let task = self.state.checked_task(key)?;
        let file = task.file.as_mut().ok_or(AccessError::StaleTask)?;
        let handle = file.pending.as_mut().ok_or(AccessError::StaleTask)?;
        let result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => result.map_err(|e| e.to_string()),
            Err(error) => Err(error.to_string()),
        };
        file.pending = None;
        if matches!(&result, Ok(FileResponse::Captured(_))) {
            file.captured = true;
        }
        let stop = result.is_err() || matches!(&result, Ok(FileResponse::Finished));
        if stop {
            self.state.request_stop();
        }
        result.map(Some).map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "file_tasks_tests.rs"]
mod tests;
