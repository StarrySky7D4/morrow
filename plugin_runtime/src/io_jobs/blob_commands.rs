//! Opt-in trusted-host blob staging on the original executor and Store.
//! No public import or guest authority is added by this lane.
use super::*;
use crate::blob_upload::{self, Error, FrameSource, Spec, Staged};
use morrow_core::blob_stage::Record;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobCommandKind {
    Stage,
    Inspect,
    Release,
}

/// Explicit application opt-in. The application validates its original local
/// authority for this exact operation; an opaque identity or old receipt is
/// insufficient. This callback runs on the existing owner before Store access.
/// Guest-facing integrations need their own versioned, approved capability.
pub trait BlobUploadOwner: HostOwner {
    fn validate_blob_command(&mut self, kind: BlobCommandKind, spec: &Spec) -> Result<(), Error>;
}

#[derive(Debug)]
pub enum BlobResponse {
    Staged(Staged),
    Inspected(Option<Record>),
    Released(Record),
}

pub struct BlobCommandHandle {
    inner: OwnerCommandHandle,
}
impl BlobCommandHandle {
    pub fn is_started(&self) -> bool {
        self.inner.is_started()
    }
    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    /// The outer delivery result retains the original cancellation/Unknown
    /// semantics. A consumed/lost success never authorizes an automatic retry.
    pub fn read(&mut self) -> Result<Option<Result<BlobResponse, Error>>, OwnerCommandError> {
        match self.inner.read_reply()? {
            Some(Reply::Blob(result)) => Ok(Some(*result)),
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

pub(super) type Dispatch<O> = fn(&mut O, &Control, &Ticket, Request) -> Result<BlobResponse, Error>;
pub(super) struct Request {
    kind: BlobCommandKind,
    spec: Spec,
    source: Option<Box<dyn FrameSource>>,
    now: i64,
    deadline: Instant,
}
const MEMORY_RESERVATION: usize = 3 * morrow_blob_transfer_v1::MAX_FRAME_BYTES
    + morrow_core::blob_stage::RESERVED_HISTORY_BYTES as usize;
impl Request {
    pub(super) fn charge(&self) -> u64 {
        // The live receiver also enforces its exact per-frame charges. These
        // conservative host reservations are not refunded on error or cancel.
        let traffic = match self.kind {
            // Include verification of a matching durable object even when no
            // replacement frames are read. Tiny wire limits cannot discount
            // a repeated lookup's full original payload verification.
            BlobCommandKind::Stage => self
                .spec
                .limits
                .max_wire_bytes
                .saturating_add(self.spec.byte_length),
            _ => self.spec.byte_length,
        };
        traffic.saturating_add(MEMORY_RESERVATION as u64)
    }
}

impl<O: BlobUploadOwner> IoWorker<O> {
    pub fn stage_blob_frames_local(
        &self,
        source: impl FrameSource + 'static,
        spec: Spec,
        now: i64,
    ) -> Result<BlobCommandHandle, OwnerCommandError> {
        self.blob_command(BlobCommandKind::Stage, spec, Some(Box::new(source)), now)
    }
    pub fn inspect_blob_stage_local(
        &self,
        spec: Spec,
    ) -> Result<BlobCommandHandle, OwnerCommandError> {
        self.blob_command(BlobCommandKind::Inspect, spec, None, 0)
    }
    pub fn release_blob_stage_local(
        &self,
        spec: Spec,
        now: i64,
    ) -> Result<BlobCommandHandle, OwnerCommandError> {
        self.blob_command(BlobCommandKind::Release, spec, None, now)
    }
    fn blob_command(
        &self,
        kind: BlobCommandKind,
        spec: Spec,
        source: Option<Box<dyn FrameSource>>,
        now: i64,
    ) -> Result<BlobCommandHandle, OwnerCommandError> {
        spec.validate().map_err(|_| OwnerCommandError::Limit)?;
        let deadline = Instant::now()
            .checked_add(self.control.timeout)
            .ok_or(OwnerCommandError::Limit)?;
        let inner = self.enqueue_owner_command(
            CommandKind::Blob {
                request: Box::new(Request {
                    kind,
                    spec,
                    source,
                    now,
                    deadline,
                }),
                dispatch: dispatch::<O>,
            },
            MEMORY_RESERVATION + size_of::<Request>() + size_of::<BlobResponse>(),
            None,
        )?;
        Ok(BlobCommandHandle { inner })
    }
}

fn dispatch<O: BlobUploadOwner>(
    owner: &mut O,
    control: &Control,
    ticket: &Ticket,
    mut request: Request,
) -> Result<BlobResponse, Error> {
    let check = || {
        let state = control.lock();
        !closed(control, state.phase)
            && !ticket.lock().cancelled
            && Instant::now() < request.deadline
    };
    if !check() {
        return Err(Error::Vetoed);
    }
    owner.validate_blob_command(request.kind, &request.spec)?;
    let runtime = checked_runtime(owner, control.host).map_err(|_| Error::Vetoed)?;
    if !check() {
        return Err(Error::Vetoed);
    }
    let store = runtime.store_local_mut();
    let vetoed = std::cell::Cell::new(false);
    let authorize = || {
        if check() {
            Ok(())
        } else {
            vetoed.set(true);
            Err(morrow_core::Error::Invalid("blob command veto"))
        }
    };
    let storage_error = |error| {
        if vetoed.get() {
            Error::Vetoed
        } else {
            Error::Storage(error)
        }
    };
    let response = match request.kind {
        BlobCommandKind::Stage => BlobResponse::Staged(blob_upload::stage_local(
            store,
            request.source.as_deref_mut().ok_or(Error::Source)?,
            &request.spec,
            request.now,
            check,
        )?),
        BlobCommandKind::Inspect => BlobResponse::Inspected(
            store
                .inspect_blob_stage_local_authorized(&request.spec.command(), authorize)
                .map_err(storage_error)?,
        ),
        BlobCommandKind::Release => BlobResponse::Released(
            store
                .release_blob_stage_local_authorized(
                    &request.spec.command(),
                    request.now,
                    authorize,
                )
                .map_err(storage_error)?,
        ),
    };
    if !check() {
        return Err(Error::DeliveryUnknown);
    }
    Ok(response)
}
