//! Synchronous dispatch inside the original upload worker. No host callback is
//! held across a guest step. End seals this one request stream; its call remains
//! suspended until Store commit. It is not a claim that the guest has terminated.
use super::{Effects, Error, GuestResult, validate};
use crate::{
    Cancellation, Fault,
    blob_upload::{self, FrameSource},
    blob_upload_binding::UploadApproval,
    continuation::{CallToken, Kind},
    io_jobs::ManagedHostOwner,
    manager::ManagedInstance,
    package::FrameExecution,
};
use morrow_blob_transfer_v1::{Acceptance, ActionRef as BlobAction, FrameRef, Receiver};
use morrow_blob_upload_request_v1::{
    ActionRef, Descriptor, Phase, ReplyRef, Request, Response, State,
};
use morrow_core::{
    blob_stage::Record,
    dispatch::{HostBinding, HostRuntime},
    store::BlobStageStoreBinding,
    task::Invocation,
};
use std::io;
use zeroize::Zeroizing;

fn live(approval: &UploadApproval, cancel: &Cancellation) -> Result<(), Error> {
    approval.check()?;
    match cancel.fault() {
        Some(Fault::Deadline) => Err(Error::Expired),
        Some(_) => Err(Error::Cancelled),
        None => Ok(()),
    }
}
fn store_guard(approval: &UploadApproval, cancel: &Cancellation) -> morrow_core::Result<()> {
    live(approval, cancel).map_err(|_| morrow_core::Error::Invalid("upload no longer live"))
}
fn storage(error: morrow_core::Error) -> Error {
    if error == morrow_core::Error::CommitUnknown {
        Error::Unknown
    } else {
        Error::Storage
    }
}
fn execution_error(report: &crate::Report) -> Error {
    match report.outcome {
        Err(Fault::Limits) => Error::Limit,
        Err(Fault::Deadline) => Error::Expired,
        Err(Fault::Cancelled) => Error::Cancelled,
        _ => Error::Protocol,
    }
}
fn record_execution(effects: &mut Effects, report: &crate::Report) {
    effects.execution_report = Some(crate::Report {
        outcome: report.outcome.clone(),
        host_calls: report.host_calls,
        fuel_remaining: report.fuel_remaining,
    });
}
fn checked_runtime<'a, O: ManagedHostOwner>(
    owner: &'a mut O,
    host: HostBinding,
    binding: &BlobStageStoreBinding,
) -> Result<&'a mut HostRuntime, Error> {
    let runtime = owner.runtime_mut();
    if runtime.binding() != host
        || runtime
            .store_local()
            .validate_blob_stage_store_binding(binding)
            .is_err()
    {
        return Err(Error::Denied);
    }
    Ok(runtime)
}
struct Pending {
    token: CallToken,
    request: Request,
}
struct Driver<'a> {
    frame: FrameExecution,
    approval: &'a UploadApproval,
    cancel: &'a Cancellation,
    nonces: Vec<[u8; 32]>,
    max_calls: usize,
    last_response: Option<Zeroizing<Vec<u8>>>,
    phase: Phase,
}
impl Driver<'_> {
    fn pending(&mut self) -> Result<Option<Pending>, Error> {
        live(self.approval, self.cancel)?;
        let Some(call) = self.frame.pending() else {
            return Ok(None);
        };
        if call.kind != Kind::BlobUpload {
            return Err(Error::Denied);
        }
        self.approval.charge_request(call.bytes.len())?;
        let request = Request::decode(&call.bytes)?;
        if request.nomination_ref() != self.approval.nomination_ref() {
            return Err(Error::Denied);
        }
        if self.nonces.len() >= self.max_calls || self.nonces.contains(&request.nonce()) {
            return Err(Error::Protocol);
        }
        self.nonces.push(request.nonce());
        live(self.approval, self.cancel)?;
        Ok(Some(Pending {
            token: call.token.clone(),
            request,
        }))
    }
    fn state(&self, phase: Phase) -> State {
        let spec = self.approval.spec();
        State {
            descriptor: Descriptor {
                identity: spec.identity,
                byte_length: spec.byte_length,
                sha256: spec.sha256,
            },
            phase,
            received_bytes: if matches!(phase, Phase::Staged | Phase::Released) {
                spec.byte_length
            } else {
                0
            },
            next_sequence: if matches!(phase, Phase::Ready | Phase::Receiving) {
                1
            } else {
                0
            },
        }
    }
    fn respond(&mut self, pending: Pending, reply: ReplyRef<'_>) -> Result<(), Error> {
        live(self.approval, self.cancel)?;
        let response = Response::new(&pending.request, reply)?;
        // Codec validation includes exact request digest, nomination, nonce and action.
        Response::decode_for(response.wire(), &pending.request)?;
        self.approval.charge_response(response.wire().len())?;
        live(self.approval, self.cancel)?;
        let bytes = response.wire().to_vec();
        self.last_response = Some(Zeroizing::new(bytes.clone()));
        self.frame
            .resume(&pending.token, Ok(bytes))
            .map_err(|_| Error::Protocol)?;
        live(self.approval, self.cancel)
    }
    fn native_charge(&self, staging: bool) -> Result<(), Error> {
        // Matching history verifies payload, and staging can also validate a
        // deduplicated existing payload. Small wire requests do not discount it.
        let multiplier = if staging { 3 } else { 1 };
        let bytes = self
            .approval
            .spec()
            .byte_length
            .checked_mul(multiplier)
            .and_then(|n| {
                n.checked_add(3 * 65536 + morrow_core::blob_stage::RESERVED_HISTORY_BYTES)
            })
            .ok_or(Error::Limit)?;
        self.approval.charge_native_work(bytes).map_err(Error::from)
    }
}
fn record_phase(record: &Record) -> Phase {
    if record.is_released() {
        Phase::Released
    } else {
        Phase::Staged
    }
}

/// This producer owns one bounded request stream, not an open-ended IO source.
/// After verified End it permanently closes without resuming End. No temporary
/// lack of input is EOF. Later guest Push calls are outside this sealed stream
/// and are refused, including after a committed End response.
struct Source<'a, 'b> {
    driver: &'a mut Driver<'b>,
    first: Option<Pending>,
    acknowledge: Option<(Pending, Vec<u8>)>,
    receiver: Receiver,
    end: Option<Pending>,
    closed: bool,
    failure: Option<Error>,
}
impl Source<'_, '_> {
    fn next(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        live(self.driver.approval, self.driver.cancel)?;
        if self.closed {
            return Ok(0);
        }
        if let Some((pending, bytes)) = self.acknowledge.take() {
            let response = Response::decode_for(&bytes, &pending.request)?;
            self.driver.respond(pending, response.reply())?;
        }
        let pending = match self.first.take() {
            Some(p) => p,
            None => self.driver.pending()?.ok_or(Error::Protocol)?,
        };
        let ActionRef::Push(bytes) = pending.request.action() else {
            return Err(Error::Protocol);
        };
        if bytes.len() > out.len() {
            return Err(Error::Limit);
        }
        let frame = FrameRef::decode(bytes).map_err(|_| Error::Protocol)?;
        let spec = self.driver.approval.spec();
        if frame.identity != spec.identity {
            return Err(Error::Denied);
        }
        match frame.action {
            BlobAction::Descriptor {
                total_length,
                whole_sha256,
            }
            | BlobAction::End {
                total_length,
                whole_sha256,
            } if total_length != spec.byte_length || whole_sha256 != spec.sha256 => {
                return Err(Error::Protocol);
            }
            _ => {}
        }
        let accepted = self.receiver.accept(bytes).map_err(|_| Error::Protocol)?;
        let count = bytes.len();
        out[..count].copy_from_slice(bytes);
        match accepted {
            Acceptance::DescriptorAccepted => {
                let reply = self.driver.state(Phase::Receiving);
                let response = Response::new(&pending.request, ReplyRef::State(reply))?;
                self.acknowledge = Some((pending, response.wire().to_vec()));
            }
            Acceptance::Receipt(receipt) => {
                let wire = receipt.encode().map_err(|_| Error::Protocol)?;
                let response = Response::new(&pending.request, ReplyRef::ChunkReceipt(&wire))?;
                self.acknowledge = Some((pending, response.wire().to_vec()));
            }
            Acceptance::VerifiedBytes { .. } => {
                self.closed = true;
                self.end = Some(pending);
            }
        }
        Ok(count)
    }
}
impl FrameSource for Source<'_, '_> {
    fn read_frame(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(io::Error::other("closed upload request stream"));
        }
        self.next(out).map_err(|error| {
            self.failure = Some(error);
            io::Error::other("upload request stream refused")
        })
    }
}

pub(super) fn execute<O: ManagedHostOwner>(
    owner: &mut O,
    instance: &ManagedInstance,
    approval: &UploadApproval,
    input: &Invocation,
    now: i64,
    cancel: &Cancellation,
    effects: &mut Effects,
) -> Result<GuestResult, Error> {
    live(approval, cancel)?;
    let original_host = owner.runtime().binding();
    let original_store = owner.runtime().store_local().blob_stage_store_binding();
    let output_limit = instance
        .package()
        .package()
        .blob_upload_handler(input.transform().ok_or(Error::Protocol)?)
        .map_err(|_| Error::Protocol)?
        .max_output_bytes as usize;
    let max_calls = instance.package().limits().host_calls as usize;
    let mut nonces = Vec::new();
    nonces
        .try_reserve_exact(max_calls)
        .map_err(|_| Error::Limit)?;
    if nonces.capacity() > max_calls {
        return Err(Error::Limit);
    }
    let frame = instance
        .package()
        .start_blob_upload_frame(input.bytes(), cancel.clone())
        .map_err(|run| {
            record_execution(effects, &run.report);
            execution_error(&run.report)
        })?;
    let mut driver = Driver {
        frame,
        approval,
        cancel,
        nonces,
        max_calls,
        last_response: None,
        phase: Phase::Ready,
    };
    let mut opened = false;
    let command = approval.spec().command();
    let dispatch = (|| {
        while let Some(pending) = driver.pending()? {
            validate(owner, instance, approval)?;
            if !opened && !matches!(pending.request.action(), ActionRef::Open) {
                return Err(Error::Protocol);
            }
            match pending.request.action() {
                ActionRef::Open => {
                    if opened {
                        return Err(Error::Protocol);
                    }
                    driver.native_charge(false)?;
                    let record = checked_runtime(owner, original_host, &original_store)?
                        .store_local()
                        .inspect_blob_stage_local_authorized(&command, || {
                            store_guard(approval, cancel)
                        })
                        .map_err(storage)?;
                    live(approval, cancel)?;
                    driver.phase = record.as_ref().map_or(Phase::Ready, record_phase);
                    opened = true;
                    let state = driver.state(driver.phase);
                    driver.respond(pending, ReplyRef::State(state))?;
                }
                ActionRef::Inspect => {
                    driver.native_charge(false)?;
                    let record = checked_runtime(owner, original_host, &original_store)?
                        .store_local()
                        .inspect_blob_stage_local_authorized(&command, || {
                            store_guard(approval, cancel)
                        })
                        .map_err(storage)?;
                    live(approval, cancel)?;
                    driver.phase = record.as_ref().map_or(Phase::Absent, record_phase);
                    let state = driver.state(driver.phase);
                    driver.respond(pending, ReplyRef::State(state))?;
                }
                ActionRef::Release => {
                    driver.native_charge(false)?;
                    // Ready/Absent has never created a stage in this driver. An
                    // exact current inspection is still required before absence.
                    let store =
                        checked_runtime(owner, original_host, &original_store)?.store_local_mut();
                    if matches!(driver.phase, Phase::Ready | Phase::Absent) {
                        let record = store
                            .inspect_blob_stage_local_authorized(&command, || {
                                store_guard(approval, cancel)
                            })
                            .map_err(storage)?;
                        if record.is_some() {
                            return Err(Error::Protocol);
                        }
                        driver.phase = Phase::Absent;
                    } else {
                        effects.in_store_call = true;
                        let result =
                            store.release_blob_stage_local_authorized(&command, now, || {
                                store_guard(approval, cancel)
                            });
                        effects.in_store_call = false;
                        if result == Err(morrow_core::Error::CommitUnknown) {
                            effects.commit_unknown = true;
                        }
                        let record = result.map_err(storage)?;
                        effects.committed = true;
                        driver.phase = record_phase(&record);
                    }
                    live(approval, cancel)?;
                    let state = driver.state(driver.phase);
                    driver.respond(pending, ReplyRef::State(state))?;
                }
                ActionRef::Push(_) => {
                    if driver.phase != Phase::Ready {
                        return Err(Error::Protocol);
                    }
                    driver.native_charge(true)?;
                    let spec = approval.spec();
                    let receiver = Receiver::new(spec.limits).map_err(|_| Error::Limit)?;
                    let mut source = Source {
                        driver: &mut driver,
                        first: Some(pending),
                        acknowledge: None,
                        receiver,
                        end: None,
                        closed: false,
                        failure: None,
                    };
                    effects.in_store_call = true;
                    let result = blob_upload::stage_local(
                        checked_runtime(owner, original_host, &original_store)?.store_local_mut(),
                        &mut source,
                        spec,
                        now,
                        || live(approval, cancel).is_ok(),
                    );
                    effects.in_store_call = false;
                    if matches!(
                        result,
                        Err(blob_upload::Error::Storage(
                            morrow_core::Error::CommitUnknown
                        )) | Err(blob_upload::Error::DeliveryUnknown)
                    ) {
                        effects.commit_unknown = true;
                    }
                    let result = result.map_err(|error| {
                        source.failure.unwrap_or(match error {
                            blob_upload::Error::Vetoed => {
                                live(approval, cancel).err().unwrap_or(Error::Denied)
                            }
                            blob_upload::Error::DeliveryUnknown => Error::Unknown,
                            blob_upload::Error::Storage(error) => storage(error),
                            _ => Error::Protocol,
                        })
                    });
                    let staged = result?;
                    effects.committed = true;
                    if staged.existing {
                        return Err(Error::Unknown);
                    }
                    let end = source.end.take().ok_or(Error::Unknown)?;
                    drop(source);
                    driver.phase = Phase::Staged;
                    let state = driver.state(Phase::Staged);
                    driver.respond(end, ReplyRef::State(state))?;
                }
            }
        }
        validate(owner, instance, approval)?;
        live(approval, cancel)
    })();
    if let Err(error) = dispatch {
        // A terminated guest may already have reported OutOfFuel. Do not replace
        // that actual failure with a later "missing next frame" protocol error.
        if driver.frame.pending().is_some() {
            driver.frame.abort(error.fault());
        }
    }
    let run = driver.frame.finish();
    record_execution(effects, &run.report);
    if let Err(error) = dispatch {
        return Err(if error == Error::Protocol && run.report.outcome.is_err() {
            execution_error(&run.report)
        } else {
            error
        });
    }
    if run.report.outcome != Ok(0)
        || !opened
        || !matches!(
            driver.phase,
            Phase::Staged | Phase::Released | Phase::Absent
        )
        || !driver.last_response.as_ref().is_some_and(|last| {
            run.completion.as_ref().is_some_and(|done| {
                done.len() <= output_limit && done.as_slice() == last.as_slice()
            })
        })
    {
        return Err(execution_error(&run.report));
    }
    Ok(GuestResult {
        execution: run.report,
        phase: driver.phase,
    })
}
