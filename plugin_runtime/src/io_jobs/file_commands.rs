//! Native selected-file commands on the original owner's bounded command lane.
use super::*;
use crate::{
    file_io::{CaptureError, FileBroker, FileClock, SelectedFile},
    io_jobs::Authority,
};
use morrow_core::io::{MAX_PAYLOAD_BYTES, Request as IoRequest, Status as IoStatus};
use std::{
    collections::BTreeMap,
    fs::File,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(1);
const MAX_FILES: usize = 128;

/// Native task identity, tied to one original executor. Not a guest file grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSession {
    worker: u64,
    serial: u64,
}
#[derive(Debug)]
pub enum FileResponse {
    Captured(SelectedFile),
    Chunk {
        offset: u64,
        bytes: Zeroizing<Vec<u8>>,
        eof: bool,
    },
    Finished,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileCommandError {
    Capture(CaptureError),
    Invalid,
    Missing,
    Guest(crate::Fault),
    Rejected,
}
impl std::fmt::Display for FileCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file command: {self:?}")
    }
}
impl std::error::Error for FileCommandError {}

pub struct FileCommandHandle {
    inner: OwnerCommandHandle,
}
impl FileCommandHandle {
    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    /// Outer error describes lost/cancelled delivery, never a file mutation.
    /// A terminal result is consumed once. Stop/expiry suppresses unread bytes.
    pub fn read(
        &mut self,
    ) -> Result<Option<Result<FileResponse, FileCommandError>>, OwnerCommandError> {
        match self.inner.read_reply()? {
            Some(Reply::File(result)) => Ok(Some(result)),
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

pub(super) type Dispatch<O> = fn(
    &mut O,
    Option<&crate::manager::ManagedInstance>,
    &Control,
    &Ticket,
    &mut Resources,
    Request,
) -> Result<FileResponse, FileCommandError>;

pub(super) enum Request {
    Capture {
        id: FileSession,
        file: File,
        handler: String,
        max_bytes: u64,
        secret: [u8; 32],
    },
    Read {
        id: FileSession,
        offset: u64,
        limit: u32,
    },
    Finish {
        id: FileSession,
    },
}
impl Request {
    pub(super) fn charge(&self) -> u64 {
        match self {
            Self::Capture { max_bytes, .. } => max_bytes.saturating_add(1),
            // Conservative full-frame reservation, independent of payload size.
            Self::Read { .. } | Self::Finish { .. } => 2 * morrow_core::io::MAX_FRAME_BYTES as u64,
        }
    }
}
struct Resource {
    broker: FileBroker,
    selected: SelectedFile,
    handler: String,
    call: u64,
    capture: Arc<Mutex<Status>>,
}
#[derive(Default)]
pub(in crate::io_jobs) struct Resources(BTreeMap<u64, Resource>);
impl Resources {
    pub(in crate::io_jobs) fn reap_cancelled(&mut self) {
        self.0.retain(|_, resource| {
            let status = resource.capture.lock().unwrap_or_else(|e| e.into_inner());
            !status.cancelled || status.delivered
        });
    }
}
struct Clock<'a>(&'a Authority);
impl FileClock for Clock<'_> {
    fn with<T>(&mut self, action: impl FnOnce(u64) -> T) -> T {
        // A poisoned trusted clock fails into the worker's existing panic cleanup.
        let mut clock = self.0.clock.lock().expect("original file clock");
        action(clock())
    }
}

impl<O: ManagedHostOwner> IoWorker<O> {
    /// The caller already owns an explicitly selected regular-file handle.
    /// No file metadata/read/seek runs on this admission thread; rejection drops
    /// the passed handle. Original owner prepare,
    /// capture, guest execution and cleanup all use the existing worker thread.
    /// The secret is fresh trusted-host entropy, never guest input.
    pub fn capture_file(
        &self,
        file: File,
        handler: String,
        max_bytes: u64,
        secret: [u8; 32],
    ) -> Result<(FileSession, FileCommandHandle), OwnerCommandError> {
        if handler.is_empty() || handler.len() > 256 || max_bytes > 256 * 1024 * 1024 {
            return Err(OwnerCommandError::Limit);
        }
        if self.control.authority.is_none() {
            return Err(OwnerCommandError::Closed);
        }
        let serial = NEXT_FILE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| OwnerCommandError::Limit)?;
        let id = FileSession {
            worker: self.control.id,
            serial,
        };
        let bytes = size_of::<Request>() + handler.len() + size_of::<SelectedFile>();
        let inner = self.enqueue_owner_command(
            CommandKind::File {
                request: Request::Capture {
                    id,
                    file,
                    handler,
                    max_bytes,
                    secret,
                },
                dispatch: dispatch::<O>,
            },
            bytes,
            None,
        )?;
        Ok((id, FileCommandHandle { inner }))
    }
    /// At most 64 KiB per guest call; zero selects that protocol bound.
    pub fn read_file_chunk(
        &self,
        id: FileSession,
        offset: u64,
        limit: u32,
    ) -> Result<FileCommandHandle, OwnerCommandError> {
        if limit as usize > MAX_PAYLOAD_BYTES {
            return Err(OwnerCommandError::Limit);
        }
        self.file_command(
            id,
            Request::Read { id, offset, limit },
            if limit == 0 {
                MAX_PAYLOAD_BYTES
            } else {
                limit as usize
            },
        )
    }
    /// Runs the declared guest's Finish once, then retires this file even on error.
    /// Stop is the fallback when quotas or a faulty guest prevent a normal Finish.
    pub fn finish_file(&self, id: FileSession) -> Result<FileCommandHandle, OwnerCommandError> {
        self.file_command(id, Request::Finish { id }, 0)
    }
    fn file_command(
        &self,
        id: FileSession,
        request: Request,
        reply_bytes: usize,
    ) -> Result<FileCommandHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        let inner = self.enqueue_owner_command(
            CommandKind::File {
                request,
                dispatch: dispatch::<O>,
            },
            size_of::<Request>() + size_of::<FileResponse>() + reply_bytes,
            None,
        )?;
        Ok(FileCommandHandle { inner })
    }
}

fn dispatch<O: ManagedHostOwner>(
    owner: &mut O,
    instance: Option<&crate::manager::ManagedInstance>,
    control: &Control,
    ticket: &Ticket,
    resources: &mut Resources,
    request: Request,
) -> Result<FileResponse, FileCommandError> {
    let Some(instance) = instance else {
        return Err(FileCommandError::Invalid);
    };
    let manager = owner.manager().ok_or(FileCommandError::Invalid)?;
    let authority = control
        .authority
        .as_ref()
        .ok_or(FileCommandError::Invalid)?;
    let mut clock = Clock(authority);
    match request {
        Request::Capture {
            id,
            file,
            handler,
            max_bytes,
            secret,
        } => {
            if id.worker != control.id
                || resources.0.len() >= MAX_FILES
                || !instance
                    .package()
                    .package()
                    .io_declaration()
                    .is_some_and(|d| d.handlers.contains(&handler))
            {
                return Err(FileCommandError::Invalid);
            }
            let mut broker = FileBroker::new(secret);
            let selected = broker
                .capture_clock(
                    manager,
                    owner.runtime(),
                    instance,
                    &authority.binding,
                    file,
                    max_bytes,
                    &mut clock,
                    || ticket.lock().cancelled,
                )
                .map_err(FileCommandError::Capture)?;
            if ticket.lock().cancelled {
                return Err(FileCommandError::Rejected);
            }
            resources.0.insert(
                id.serial,
                Resource {
                    broker,
                    selected,
                    handler,
                    call: 0,
                    capture: Arc::clone(&ticket.status),
                },
            );
            Ok(FileResponse::Captured(selected))
        }
        request => {
            let (id, finish) = match &request {
                Request::Read { id, .. } => (*id, false),
                Request::Finish { id } => (*id, true),
                _ => unreachable!(),
            };
            if id.worker != control.id {
                return Err(FileCommandError::Invalid);
            }
            let resource = resources
                .0
                .get_mut(&id.serial)
                .ok_or(FileCommandError::Missing)?;
            let result = (|| {
                if ticket.lock().cancelled {
                    return Err(FileCommandError::Rejected);
                }
                resource.call = resource
                    .call
                    .checked_add(1)
                    .ok_or(FileCommandError::Rejected)?;
                let input = match request {
                    Request::Read { offset, limit, .. } => IoRequest::encode_read(
                        resource.call,
                        &resource.selected.reference,
                        offset,
                        limit,
                    ),
                    _ => IoRequest::encode_finish(resource.call, &resource.selected.reference),
                }
                .map_err(|_| FileCommandError::Invalid)?;
                let result = resource.broker.run_clock(
                    manager,
                    owner.runtime(),
                    instance,
                    &authority.binding,
                    &resource.handler,
                    &input,
                    &mut clock,
                );
                result.execution.outcome.map_err(FileCommandError::Guest)?;
                let response = result.response.ok_or(FileCommandError::Rejected)?;
                if response.status != IoStatus::Completed {
                    return Err(FileCommandError::Rejected);
                }
                if finish {
                    Ok(FileResponse::Finished)
                } else {
                    Ok(FileResponse::Chunk {
                        offset: response.offset,
                        bytes: Zeroizing::new(response.payload),
                        eof: response.eof,
                    })
                }
            })();
            if finish || result.is_err() || ticket.lock().cancelled {
                resources.0.remove(&id.serial);
            }
            result
        }
    }
}
