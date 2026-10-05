//! Fifth-profile guest execution on the complete original owner and its existing selections.
//! Guest Open claims one host nomination; it never captures or opens a native object.
use super::*;
use crate::io_binding::IoJobLease;
use morrow_core::plugin_package::io::IoCapability;
use morrow_fs_directory_request_v1::{
    Action as WireAction, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, Opened, Reply as WireReply,
    Request as WireRequest, Response as WireResponse,
};
use std::{
    time::{Duration, Instant},
};

pub struct DirectoryGuestResult {
    pub execution: crate::Report,
    /// Only the exact last validated response; the caller owns these metadata bytes.
    pub response: Option<Vec<u8>>,
    /// Native page work may have advanced despite a lost or rejected final response.
    pub unknown: bool,
}
impl std::fmt::Debug for DirectoryGuestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryGuestResult")
            .field("response_bytes", &self.response.as_ref().map(Vec::len))
            .field("unknown", &self.unknown)
            .finish_non_exhaustive()
    }
}
pub(in crate::io_jobs) struct GuestReply {
    result: DirectoryGuestResult,
    // Original job admission remains held through unread Ready delivery/drop.
    _wire_lease: Arc<IoJobLease>,
}
pub struct DirectoryGuestHandle {
    inner: OwnerCommandHandle,
}
impl DirectoryGuestHandle {
    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn is_started(&self) -> bool {
        self.inner.is_started()
    }
    pub fn read(
        &mut self,
    ) -> Result<Option<Result<DirectoryGuestResult, DirectoryCommandError>>, OwnerCommandError>
    {
        match self.inner.read_reply()? {
            Some(Reply::DirectoryGuest(reply)) => Ok(Some((*reply).map(|value| value.result))),
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}
pub(in crate::io_jobs) struct GuestRequest {
    id: DirectorySession,
    handler: String,
    deadline: Instant,
    nonces: Vec<[u8; 32]>,
    retire: Retirement,
}
impl GuestRequest {
    pub(in crate::io_jobs) fn charge(&self) -> u64 {
        (size_of::<Self>() + size_of::<DirectoryGuestResult>()
            + self.handler.capacity()
            + self.nonces.capacity() * size_of::<[u8; 32]>()) as u64
    }
}
pub(in crate::io_jobs::owner_commands) type GuestDispatch<O> = fn(
    &mut O,
    Option<&crate::manager::ManagedInstance>,
    &Control,
    &Ticket,
    &mut Resources,
    GuestRequest,
) -> Result<GuestReply, DirectoryCommandError>;

impl<O: ManagedHostOwner> IoWorker<O> {
    /// Invoke only the new negotiated guest against this worker's already delivered capture.
    /// Admission accepts no guest path, root, secret, grant or replacement clock.
    pub fn submit_directory_guest_frame(
        &self,
        id: DirectorySession,
        handler: String,
        timeout: Duration,
    ) -> Result<DirectoryGuestHandle, OwnerCommandError> {
        if !self.control.directory_request_enabled
            || id.worker != self.control.id
            || handler.is_empty()
            || handler.len() > 256
            || handler.capacity() > 256
            || handler.chars().any(char::is_control)
            || timeout.is_zero()
            || timeout > self.control.timeout
        {
            return Err(OwnerCommandError::Closed);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(OwnerCommandError::Limit)?;
        // Fallible, bounded allocation before admission locks. No later nonce
        // insertion may grow this retained vector or invent a ledger.
        let nonce_limit = usize::try_from(self.control.limits.max_calls)
            .map_err(|_| OwnerCommandError::Limit)?;
        let mut nonces = Vec::<[u8; 32]>::new();
        nonces.try_reserve_exact(nonce_limit).map_err(|_| OwnerCommandError::Limit)?;
        if nonces.capacity() > nonce_limit {
            return Err(OwnerCommandError::Limit);
        }
        let nonce_bytes = nonces.capacity().checked_mul(size_of::<[u8; 32]>())
            .ok_or(OwnerCommandError::Limit)?;
        let reservation = size_of::<GuestRequest>()
            .checked_add(handler.capacity())
            .and_then(|bytes| bytes.checked_add(nonce_bytes))
            .and_then(|bytes| bytes.checked_add(3 * MAX_RESPONSE_BYTES))
            .and_then(|bytes| bytes.checked_add(2 * MAX_REQUEST_BYTES))
            .ok_or(OwnerCommandError::Limit)?;
        let mut state = admission(&self.control);
        let slot = state
            .selections
            .get_mut(&id.serial)
            .ok_or(OwnerCommandError::Closed)?;
        if slot.retired {
            return Err(OwnerCommandError::Closed);
        }
        let selected = slot.selected.ok_or(OwnerCommandError::Busy)?;
        if !slot
            .expected
            .is_some_and(|expected| same_cursor(expected, selected.first_page()))
        {
            return Err(OwnerCommandError::Closed);
        }
        if slot.pending {
            return Err(OwnerCommandError::Busy);
        }
        if !slot.delivery.as_ref().is_some_and(|delivery| {
            let status = delivery.lock().unwrap_or_else(|error| error.into_inner());
            status.delivered
        }) {
            return Err(OwnerCommandError::Busy);
        }
        slot.pending = true;
        drop(state);
        let request = GuestRequest {
            id,
            handler,
            deadline,
            nonces,
            retire: Retirement {
                control: Arc::downgrade(&self.control),
                serial: id.serial,
                armed: true,
            },
        };
        // Exact retained handler/nonce allocation plus bounded wire buffers.
        let inner = self.enqueue_owner_command(
            CommandKind::DirectoryGuest {
                request,
                dispatch: dispatch_guest::<O>,
            },
            reservation,
            None,
        )?;
        Ok(DirectoryGuestHandle { inner })
    }
}

fn validate<O: ManagedHostOwner>(
    owner: &O,
    instance: &crate::manager::ManagedInstance,
    control: &Control,
    ticket: &Ticket,
    cancel: &Cancellation,
) -> Result<(), DirectoryCommandError> {
    let authority = control
        .authority
        .as_ref()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    capture_veto(control, ticket, authority)?;
    if let Some(fault) = cancel.fault() {
        return Err(if fault == crate::Fault::Deadline {
            DirectoryCommandError::Admission(BindingError::Expired)
        } else {
            DirectoryCommandError::Cancelled
        });
    }
    let manager = owner
        .manager()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let host = owner.runtime();
    let mut clock = OriginalDirectoryClock {
        clock: &authority.clock,
    };
    clock.with(|now| {
        authority
            .binding
            .preflight_capability(manager, host, instance, IoCapability::FileList, now)
            .map_err(DirectoryCommandError::from)
    })
}

/// Preserve original Control -> clock -> instance-context order. Sampling and
/// exact identity/FileList/lease accounting are one step; no codec/native work occurs here.
fn charge_wire<O: ManagedHostOwner>(
    owner: &O,
    instance: &crate::manager::ManagedInstance,
    control: &Control,
    lease: &IoJobLease,
    wire: usize,
    native_host_charge: u64,
    command_bytes: &mut u64,
) -> Result<(), DirectoryCommandError> {
    let manager = owner
        .manager()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let host = owner.runtime();
    let authority = control
        .authority
        .as_ref()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let amount = (wire as u64)
        .checked_add(native_host_charge)
        .ok_or(DirectoryCommandError::Limit)?;
    let next_command = command_bytes
        .checked_add(amount)
        .filter(|bytes| *bytes <= control.limits.max_job_bytes)
        .ok_or(DirectoryCommandError::Limit)?;
    let mut state = control.lock();
    let next = state
        .bytes
        .checked_add(amount)
        .filter(|bytes| *bytes <= control.limits.max_total_bytes)
        .ok_or(DirectoryCommandError::Limit)?;
    let mut clock = OriginalDirectoryClock {
        clock: &authority.clock,
    };
    clock.with(|now| {
        authority.binding.preflight_capability(
            manager,
            host,
            instance,
            IoCapability::FileList,
            now,
        )?;
        lease.charge(&[IoCapability::FileList], wire as u64, now)?;
        Ok(())
    })?;
    state.bytes = next;
    *command_bytes = next_command;
    Ok(())
}

fn dispatch_guest<O: ManagedHostOwner>(
    owner: &mut O,
    instance: Option<&crate::manager::ManagedInstance>,
    control: &Control,
    ticket: &Ticket,
    resources: &mut Resources,
    mut request: GuestRequest,
) -> Result<GuestReply, DirectoryCommandError> {
    let instance = instance.ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let package = instance.package();
    if !package
        .package()
        .manifest()
        .required_features
        .iter()
        .any(|feature| feature == morrow_core::plugin_package::DIRECTORY_REQUEST_FEATURE)
        || !package
            .package()
            .io_declaration()
            .is_some_and(|declaration| declaration.handlers.contains(&request.handler))
    {
        request.retire.preserve();
        return Err(DirectoryCommandError::Admission(BindingError::Denied));
    }
    if request.id.worker != control.id {
        request.retire.preserve();
        return Err(DirectoryCommandError::InvalidCursor);
    }
    let cancel = Cancellation::linked(
        instance.cancellation(),
        Cancellation::until(request.deadline),
    );
    validate(owner, instance, control, ticket, &cancel)?;
    let selected = resources
        .0
        .get(&request.id.serial)
        .ok_or(DirectoryCommandError::InvalidCursor)?
        .selected;
    {
        let mut state = admission(control);
        let slot = state
            .selections
            .get_mut(&request.id.serial)
            .ok_or(DirectoryCommandError::InvalidCursor)?;
        if slot.retired
            || slot.selected != Some(selected)
            || !slot
                .expected
                .is_some_and(|expected| same_cursor(expected, selected.first_page()))
        {
            drop(state);
            request.retire.preserve();
            return Err(DirectoryCommandError::InvalidCursor);
        }
        slot.delivery = Some(Arc::clone(&ticket.status));
    }
    let authority = control
        .authority
        .as_ref()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let manager = owner
        .manager()
        .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
    let host = owner.runtime();
    let mut clock = OriginalDirectoryClock {
        clock: &authority.clock,
    };
    let lease = Arc::new(clock.with(|now| {
        authority.binding.preflight_capability(
            manager,
            host,
            instance,
            IoCapability::FileList,
            now,
        )?;
        authority
            .binding
            .admit_job_authenticated(0, control.limits.max_job_bytes, now)
            .map_err(DirectoryCommandError::from)
    })?);
    let invocation = DirectorySession {
        worker: control.id,
        serial: next_directory_serial(&NEXT_DIRECTORY).map_err(|_| DirectoryCommandError::Limit)?,
    };
    let fresh = FreshSecret {
        #[cfg(test)]
        probe: None,
    };
    let secret = fresh_directory_secret(invocation, fresh, || {
        validate(owner, instance, control, ticket, &cancel)
    })?;
    let derive = |domain: &[u8]| {
        let mut hash = Sha256::new();
        hash.update(domain);
        hash.update(&*secret);
        hash.update(selected.reference);
        hash.update(invocation.worker.to_le_bytes());
        hash.update(invocation.serial.to_le_bytes());
        <[u8; 32]>::from(hash.finalize())
    };
    let nomination = derive(b"morrow.directory-guest.nomination.v1\0");
    let initial_nonce = derive(b"morrow.directory-guest.initial-nonce.v1\0");
    if nomination == [0; 32] || initial_nonce == [0; 32] {
        return Err(DirectoryCommandError::Entropy);
    }
    drop(secret);
    let initial = WireRequest::new(nomination, initial_nonce, WireAction::Open)
        .map_err(|_| DirectoryCommandError::Codec)?;
    let input = Zeroizing::new(initial.encode().map_err(|_| DirectoryCommandError::Codec)?);
    let mut command_bytes = request.charge();
    charge_wire(
        owner,
        instance,
        control,
        &lease,
        input.len(),
        0,
        &mut command_bytes,
    )?;
    validate(owner, instance, control, ticket, &cancel)?;
    let mut frame = package
        .start_directory_frame(&input, cancel.clone())
        .map_err(|_| DirectoryCommandError::Codec)?;
    let mut opened = false;
    let mut terminal = false;
    let mut advanced = false;
    let mut nonces = std::mem::take(&mut request.nonces);
    let mut last_response = None;
    let mut calls = 0u32;
    while let Some(pending) = frame.pending() {
        let token = pending.token.clone();
        let reply = (|| {
            validate(owner, instance, control, ticket, &cancel)?;
            if pending.kind != crate::continuation::Kind::Directory
                || terminal
                || pending.bytes.is_empty()
                || pending.bytes.len() > MAX_REQUEST_BYTES
                || calls >= control.limits.max_calls
            {
                return Err(DirectoryCommandError::Limit);
            }
            charge_wire(
                owner,
                instance,
                control,
                &lease,
                pending.bytes.len(),
                0,
                &mut command_bytes,
            )?;
            calls += 1;
            let wire =
                WireRequest::decode(&pending.bytes).map_err(|_| DirectoryCommandError::Codec)?;
            if wire.nomination_ref() != nomination || nonces.contains(&wire.nonce()) {
                return Err(DirectoryCommandError::InvalidCursor);
            }
            if nonces.len() >= nonces.capacity() {
                return Err(DirectoryCommandError::Limit);
            }
            nonces.push(wire.nonce());
            let response = match wire.action() {
                WireAction::Open if !opened && wire.nonce() == initial_nonce => {
                    opened = true;
                    WireReply::Opened(Opened {
                        selection_epoch: selected.selection_epoch,
                        page_sequence: 1,
                        after_entry_id: None,
                        entries: selected.entries as u32,
                        metadata_bytes: selected.metadata_bytes,
                    })
                }
                WireAction::Next {
                    selection_epoch,
                    page_sequence,
                    after_entry_id,
                } if opened => {
                    if selection_epoch != selected.selection_epoch {
                        return Err(DirectoryCommandError::InvalidCursor);
                    }
                    let cursor = PageRequest {
                        reference: selected.reference,
                        selection_epoch,
                        page_sequence,
                        after_entry_id,
                    };
                    if !admission(control)
                        .selections
                        .get(&request.id.serial)
                        .and_then(|slot| slot.expected)
                        .is_some_and(|expected| same_cursor(expected, cursor))
                    {
                        return Err(DirectoryCommandError::InvalidCursor);
                    }
                    charge_wire(
                        owner,
                        instance,
                        control,
                        &lease,
                        0,
                        DIRECTORY_PAGE_CHARGE,
                        &mut command_bytes,
                    )?;
                    advanced = true;
                    let action = Request {
                        id: request.id,
                        action: Action::Page(cursor),
                        retire: Retirement {
                            control: ticket.control.clone(),
                            serial: request.id.serial,
                            armed: true,
                        },
                    };
                    match dispatch(owner, Some(instance), control, ticket, resources, action)? {
                        DirectoryResponse::Page(page) => {
                            terminal = page.terminal;
                            WireReply::Page(page)
                        }
                        _ => return Err(DirectoryCommandError::Codec),
                    }
                }
                action @ (WireAction::Finish { selection_epoch }
                | WireAction::Cancel { selection_epoch })
                    if opened =>
                {
                    if selection_epoch != selected.selection_epoch {
                        return Err(DirectoryCommandError::InvalidCursor);
                    }
                    charge_wire(
                        owner,
                        instance,
                        control,
                        &lease,
                        0,
                        DIRECTORY_FINISH_CHARGE,
                        &mut command_bytes,
                    )?;
                    advanced = true;
                    let native = Request {
                        id: request.id,
                        action: Action::Finish,
                        retire: Retirement {
                            control: ticket.control.clone(),
                            serial: request.id.serial,
                            armed: true,
                        },
                    };
                    dispatch(owner, Some(instance), control, ticket, resources, native)?;
                    terminal = true;
                    if matches!(action, WireAction::Cancel { .. }) {
                        WireReply::Cancelled
                    } else {
                        WireReply::Finished
                    }
                }
                _ => return Err(DirectoryCommandError::InvalidCursor),
            };
            let bytes = WireResponse::new(&wire, response)
                .and_then(|response| response.encode())
                .map_err(|_| DirectoryCommandError::Codec)?;
            if bytes.len() > MAX_RESPONSE_BYTES {
                return Err(DirectoryCommandError::Limit);
            }
            charge_wire(
                owner,
                instance,
                control,
                &lease,
                bytes.len(),
                0,
                &mut command_bytes,
            )?;
            validate(owner, instance, control, ticket, &cancel)?;
            WireResponse::decode_for(&bytes, &wire).map_err(|_| DirectoryCommandError::Codec)?;
            last_response = Some(Zeroizing::new(bytes.clone()));
            Ok(bytes)
        })();
        match reply {
            Ok(bytes) => frame
                .resume(&token, Ok(bytes))
                .expect("same directory pending call"),
            Err(error) => {
                frame.abort(match error {
                    DirectoryCommandError::Limit => crate::Fault::Limits,
                    DirectoryCommandError::Admission(BindingError::Expired) => {
                        crate::Fault::Deadline
                    }
                    _ => crate::Fault::TaskProtocol,
                });
                break;
            }
        }
    }
    let mut run = frame.finish();
    if let Err(error) = validate(owner, instance, control, ticket, &cancel) {
        if run.report.outcome.is_ok() {
            run.report.outcome = Err(match error {
                DirectoryCommandError::Admission(BindingError::Expired) => crate::Fault::Deadline,
                _ => crate::Fault::TaskProtocol,
            });
        }
    }
    let completed = run.report.outcome == Ok(0)
        && opened
        && last_response.as_ref().is_some_and(|last| {
            run.completion
                .as_ref()
                .is_some_and(|done| done.as_slice() == last.as_slice())
        });
    if !completed {
        if run.report.outcome.is_ok() {
            run.report.outcome = Err(crate::Fault::TaskProtocol);
        }
        // Drop native broker/root/leases before quota release; no cursor replay.
        resources.0.remove(&request.id.serial);
        admission(control).selections.remove(&request.id.serial);
    } else if !terminal {
        if let Some(slot) = admission(control).selections.get_mut(&request.id.serial) {
            slot.pending = false;
        }
    }
    request.retire.complete();
    let response = if completed {
        run.completion.take()
    } else {
        None
    };
    Ok(GuestReply {
        result: DirectoryGuestResult {
            execution: run.report,
            response,
            unknown: !completed && advanced,
        },
        _wire_lease: lease,
    })
}
