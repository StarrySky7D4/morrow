//! Trusted selected-directory commands on the complete original owner worker.
//! No path lookup, guest import, factory, fresh approval or recursion is added.
use super::*;
use crate::directory_io::{
    CaptureLimits, DirectoryBroker, DirectoryClock, PageRequest, SelectedDirectory,
};
use morrow_fs_directory_v1::FsDirectoryPage;
use std::{
    collections::BTreeMap,
    fs::File,
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_DIRECTORY_OBSERVATIONS: usize = 8;
const MAX_WIRE: u64 = 65536;
const MAX_PAGE_OWNED: u64 = 16384 + 32 * 128;
/// Complete native command bookkeeping; a diagnostic bound, not an ABI.
pub const DIRECTORY_COMMAND_FIXED_BYTES: u64 =
    (size_of::<Request>() + size_of::<DirectoryResponse>()) as u64;
/// Includes allocator/encoded-wire coexistence as well as owned page storage.
pub const DIRECTORY_PAGE_CHARGE: u64 =
    105 + 3 * MAX_WIRE + 64 + MAX_PAGE_OWNED + DIRECTORY_COMMAND_FIXED_BYTES;
pub const DIRECTORY_FINISH_CHARGE: u64 = 64 + DIRECTORY_COMMAND_FIXED_BYTES;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

/// Opaque worker identity, never a path, child authority or native OS handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectorySession {
    worker: u64,
    serial: u64,
}
pub type DirectoryCommandError = crate::directory_io::Error;
#[derive(Debug)]
pub enum DirectoryResponse {
    Captured(SelectedDirectory),
    Page(FsDirectoryPage),
    Finished,
}
pub struct DirectoryCommandHandle {
    inner: OwnerCommandHandle,
}
impl std::fmt::Debug for DirectoryCommandHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectoryCommandHandle")
            .finish_non_exhaustive()
    }
}
impl DirectoryCommandHandle {
    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn is_started(&self) -> bool {
        self.inner.is_started()
    }
    /// Original delivery rules apply: lost delivery after start is Unknown.
    /// Successful read transfers one page; it never rewinds its native cursor.
    pub fn read(
        &mut self,
    ) -> Result<Option<Result<DirectoryResponse, DirectoryCommandError>>, OwnerCommandError> {
        match self.inner.read_reply()? {
            Some(Reply::Directory(result)) => Ok(Some(*result)),
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

struct Admitted {
    selected: Option<SelectedDirectory>,
    expected: Option<PageRequest>,
    pending: bool,
    retired: bool,
    delivery: Option<Arc<Mutex<Status>>>,
    metadata_bytes: u64,
}
#[derive(Default)]
pub(in crate::io_jobs) struct Admission {
    selections: BTreeMap<u64, Admitted>,
}
fn admission(control: &Control) -> std::sync::MutexGuard<'_, Admission> {
    control
        .directories
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}
fn same_cursor(a: PageRequest, b: PageRequest) -> bool {
    a.reference == b.reference
        && a.selection_epoch == b.selection_epoch
        && a.page_sequence == b.page_sequence
        && a.after_entry_id == b.after_entry_id
}

/// This guard owns only an exact admitted operation, not a broker or Ticket.
/// Rejected foreign identities are never allowed to arm retirement. Dropping
/// queued valid work retires its exact selection; the actual broker/File stays
/// on the original worker until maintenance/dequeue/exit, with no refunds.
struct Retirement {
    control: Weak<Control>,
    serial: u64,
    armed: bool,
}
impl Retirement {
    fn complete(&mut self) {
        self.armed = false;
    }
    fn preserve(&mut self) {
        if let Some(control) = self.control.upgrade()
            && let Some(slot) = admission(&control).selections.get_mut(&self.serial)
        {
            slot.pending = false;
        }
        self.armed = false;
    }
}
impl Drop for Retirement {
    fn drop(&mut self) {
        if self.armed
            && let Some(control) = self.control.upgrade()
        {
            let mut state = admission(&control);
            if let Some(slot) = state.selections.get_mut(&self.serial) {
                if slot.selected.is_some() {
                    // Resident quota includes this tombstone until Resource's
                    // worker-owned Drop has actually released broker/root/lease.
                    slot.retired = true;
                } else {
                    // Request.action drops its queued-only File before this last
                    // field's guard. No worker-owned observation exists yet.
                    state.selections.remove(&self.serial);
                }
            }
        }
    }
}
pub(super) struct Request {
    id: DirectorySession,
    action: Action,
    retire: Retirement,
}
enum Action {
    Capture {
        file: File,
        limits: CaptureLimits,
        secret: Zeroizing<[u8; 32]>,
    },
    Page(PageRequest),
    Finish,
}
impl Request {
    fn fixed_charge() -> u64 {
        DIRECTORY_COMMAND_FIXED_BYTES
    }
    pub(super) fn charge(&self) -> u64 {
        match self.action {
            Action::Capture { limits, .. } => limits.max_job_bytes + Self::fixed_charge(),
            // Request105, builder/encoded-wire coexistence, two identity queries, owned page
            // name/container storage and complete command/reply bookkeeping.
            Action::Page(_) => DIRECTORY_PAGE_CHARGE,
            Action::Finish => DIRECTORY_FINISH_CHARGE,
        }
    }
    fn reservation(&self) -> Result<usize, OwnerCommandError> {
        let variable = match self.action {
            Action::Capture { limits, .. } => limits.max_metadata_bytes,
            Action::Page(_) => 3 * MAX_WIRE + MAX_PAGE_OWNED,
            Action::Finish => 0,
        };
        usize::try_from(
            Self::fixed_charge()
                .checked_add(variable)
                .ok_or(OwnerCommandError::Limit)?,
        )
        .map_err(|_| OwnerCommandError::Limit)
    }
}
pub(super) type Dispatch<O> = fn(
    &mut O,
    Option<&crate::manager::ManagedInstance>,
    &Control,
    &Ticket,
    &mut Resources,
    Request,
) -> Result<DirectoryResponse, DirectoryCommandError>;
/// Serialize each time sample and its original validation/accounting together.
/// The guard never escapes a step. Native work, codec allocation, callbacks and
/// root/spool/lease cleanup must remain outside these short original-clock steps.
/// This cannot bound a slow trusted clock closure or unrelated original lock use.
struct OriginalDirectoryClock<'a> {
    clock: &'a crate::http_io::SharedClock,
}
impl DirectoryClock for OriginalDirectoryClock<'_> {
    fn with<T>(
        &mut self,
        step: impl FnOnce(u64) -> Result<T, DirectoryCommandError>,
    ) -> Result<T, DirectoryCommandError> {
        let mut clock = self
            .clock
            .lock()
            .map_err(|_| DirectoryCommandError::Admission(BindingError::Denied))?;
        step(clock())
    }
}

struct Resource {
    broker: Option<DirectoryBroker>,
    serial: u64,
    selected: SelectedDirectory,
    control: Weak<Control>,
}
impl Drop for Resource {
    fn drop(&mut self) {
        // Native objects/leases/spool are gone BEFORE a new capture can reserve
        // this slot. This covers retain(false), explicit removal and unwind.
        drop(self.broker.take());
        if let Some(control) = self.control.upgrade() {
            admission(&control).selections.remove(&self.serial);
        }
    }
}
#[derive(Default)]
pub(in crate::io_jobs) struct Resources(BTreeMap<u64, Resource>);
impl Resources {
    pub(in crate::io_jobs) fn reap(&mut self) {
        self.0.retain(|serial, resource| {
            let Some(control) = resource.control.upgrade() else {
                return false;
            };
            let delivery_live = {
                let state = admission(&control);
                state.selections.get(serial).is_some_and(|slot| {
                    !slot.retired
                        && slot.delivery.as_ref().is_none_or(|status| {
                            let status = status.lock().unwrap_or_else(|error| error.into_inner());
                            // read_reply marks delivered before setting cancelled=true.
                            !status.cancelled || status.delivered
                        })
                })
            };
            let active = control.authority.as_ref().is_some_and(|authority| {
                let Some(broker) = resource.broker.as_mut() else {
                    return false;
                };
                let mut clock = OriginalDirectoryClock {
                    clock: &authority.clock,
                };
                // Classify lease eligibility and original binding liveness in
                // the same sampling domain. No native drop occurs in this step.
                let eligibility = clock.with(|now| {
                    let plan = broker.reclaimable_at(now);
                    let live = authority.binding.check_liveness(now).is_ok();
                    Ok((plan, live))
                });
                match eligibility {
                    Ok((plan, live)) => {
                        // The short clock guard has returned. Resource::drop
                        // still frees its native broker before releasing quota.
                        broker.release_reclaimed(plan);
                        live && broker.usage().0 != 0
                    }
                    Err(_) => false,
                }
            });
            delivery_live && active && !control.revocation.is_revoked()
        });
    }
}

impl<O: ManagedHostOwner> IoWorker<O> {
    /// Original whole-IoBinding shared accounting, not directory-only usage.
    /// No new binding, clock sample, liveness/approval or join evidence is made.
    /// Only a pure-directory fixture can attribute resources8->7 to this lane.
    pub fn directory_binding_usage(&self) -> Option<crate::io_binding::Usage> {
        self.control
            .authority
            .as_ref()
            .map(|authority| authority.binding.usage())
    }
    /// Diagnostic admitted/resident count and current metadata allowance used.
    /// Does not expose names/handles/keys, assert liveness or establish join.
    pub fn directory_usage(&self) -> (usize, u64) {
        let state = admission(&self.control);
        (
            state.selections.len(),
            state
                .selections
                .values()
                .map(|slot| slot.metadata_bytes)
                .sum(),
        )
    }
    /// Effective native ceiling: never expands caller limits; reserves complete
    /// fixed command input/reply inside this worker's original byte ceiling.
    /// Spool costs stay inside the native job ceiling plus a separate resident
    /// cap (eight times at most1MiB) and command memory reservation.
    pub fn directory_capture_budget(
        &self,
        mut limits: CaptureLimits,
    ) -> Result<CaptureLimits, OwnerCommandError> {
        if limits.max_entries > crate::directory_io::MAX_ENTRIES
            || limits.max_metadata_bytes > crate::directory_io::MAX_METADATA_BYTES
            || limits.max_job_bytes == 0
            || limits.max_job_bytes > morrow_core::plugin_package::io::MAX_JOB_BYTES
        {
            return Err(OwnerCommandError::Limit);
        }
        let available = self
            .control
            .limits
            .max_job_bytes
            .checked_sub(Request::fixed_charge())
            .filter(|value| *value != 0)
            .ok_or(OwnerCommandError::Limit)?;
        limits.max_job_bytes = limits.max_job_bytes.min(available);
        Ok(limits)
    }
    /// File is already selected by the trusted host. No metadata/query/open is
    /// performed here. The caller supplies fresh trusted entropy for this broker;
    /// this API neither generates it nor proves provenance/ancestor selection.
    pub fn capture_directory(
        &self,
        file: File,
        limits: CaptureLimits,
        fresh_host_secret: [u8; 32],
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        let secret = Zeroizing::new(fresh_host_secret);
        if self.control.authority.is_none() {
            return Err(OwnerCommandError::Closed);
        }
        let limits = self.directory_capture_budget(limits)?;
        let mut state = admission(&self.control);
        if state.selections.len() >= MAX_DIRECTORY_OBSERVATIONS {
            return Err(OwnerCommandError::Busy);
        }
        let serial = NEXT_DIRECTORY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| OwnerCommandError::Limit)?;
        let id = DirectorySession {
            worker: self.control.id,
            serial,
        };
        state.selections.insert(
            serial,
            Admitted {
                selected: None,
                expected: None,
                pending: true,
                retired: false,
                delivery: None,
                metadata_bytes: 0,
            },
        );
        drop(state);
        let request = Request {
            id,
            action: Action::Capture {
                file,
                limits,
                secret,
            },
            retire: Retirement {
                control: Arc::downgrade(&self.control),
                serial,
                armed: true,
            },
        };
        let bytes = request.reservation()?;
        let inner = self.enqueue_owner_command(
            CommandKind::Directory {
                request,
                dispatch: dispatch::<O>,
            },
            bytes,
            None,
        )?;
        Ok((id, DirectoryCommandHandle { inner }))
    }
    pub fn next_directory_page(
        &self,
        id: DirectorySession,
        request: PageRequest,
    ) -> Result<DirectoryCommandHandle, OwnerCommandError> {
        self.directory_command(id, Action::Page(request))
    }
    /// Retires one valid live selection. Terminal pages already retire it;
    /// subsequent calls are Closed, never synthetic idempotent success.
    pub fn finish_directory(
        &self,
        id: DirectorySession,
    ) -> Result<DirectoryCommandHandle, OwnerCommandError> {
        self.directory_command(id, Action::Finish)
    }
    fn directory_command(
        &self,
        id: DirectorySession,
        action: Action,
    ) -> Result<DirectoryCommandHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        let mut state = admission(&self.control);
        let slot = state
            .selections
            .get_mut(&id.serial)
            .ok_or(OwnerCommandError::Closed)?;
        if slot.retired {
            return Err(OwnerCommandError::Closed);
        }
        if let Action::Page(request) = action
            && !slot
                .expected
                .is_some_and(|expected| same_cursor(expected, request))
        {
            return Err(OwnerCommandError::Closed);
        }
        if slot.pending || slot.selected.is_none() {
            return Err(OwnerCommandError::Busy);
        }
        if let Some(status) = &slot.delivery {
            let status = status.lock().unwrap_or_else(|error| error.into_inner());
            if status.cancelled && !status.delivered {
                return Err(OwnerCommandError::Closed);
            }
            if !status.delivered {
                return Err(OwnerCommandError::Busy);
            }
        }
        slot.pending = true;
        drop(state);
        let request = Request {
            id,
            action,
            retire: Retirement {
                control: Arc::downgrade(&self.control),
                serial: id.serial,
                armed: true,
            },
        };
        let bytes = request.reservation()?;
        let inner = self.enqueue_owner_command(
            CommandKind::Directory {
                request,
                dispatch: dispatch::<O>,
            },
            bytes,
            None,
        )?;
        Ok(DirectoryCommandHandle { inner })
    }
}

fn dispatch<O: ManagedHostOwner>(
    owner: &mut O,
    instance: Option<&crate::manager::ManagedInstance>,
    control: &Control,
    ticket: &Ticket,
    resources: &mut Resources,
    mut request: Request,
) -> Result<DirectoryResponse, DirectoryCommandError> {
    let is_capture = matches!(request.action, Action::Capture { .. });
    let owner_identity = (|| {
        let instance = instance.ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
        let manager = owner
            .manager()
            .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
        let authority = control
            .authority
            .as_ref()
            .ok_or(DirectoryCommandError::Admission(BindingError::Denied))?;
        authority
            .binding
            .validate_identity(manager, owner.runtime(), instance)?;
        Ok((instance, manager, authority))
    })();
    let (instance, manager, authority) = match owner_identity {
        Ok(value) => value,
        Err(error) => {
            if !is_capture {
                request.retire.preserve();
            }
            return Err(error);
        }
    };
    let id = request.id;
    if id.worker != control.id {
        request.retire.preserve();
        return Err(DirectoryCommandError::InvalidCursor);
    }
    // Exact owner identity precedes the broker's own cursor/retirement checks.
    // This adapter holds only a reference. Each broker step locks the original
    // clock through sampling plus its matching check; native work is lock-free
    // with respect to that clock. Runtime getters retain their original order.
    let mut clock = OriginalDirectoryClock {
        clock: &authority.clock,
    };
    let result = match request.action {
        Action::Capture {
            file,
            limits,
            secret,
        } => {
            let mut broker = DirectoryBroker::new(*secret);
            let selected = broker.grant_open_directory_with_clock_and_cancel(
                manager,
                owner.runtime(),
                instance,
                &authority.binding,
                file,
                limits,
                &mut clock,
                || ticket.lock().cancelled,
            )?;
            if ticket.lock().cancelled {
                return Err(DirectoryCommandError::Cancelled);
            }
            // Arm native-first cleanup BEFORE publishing resident metadata;
            // every error/unwind after publication drops this owning guard.
            let resource = Resource {
                broker: Some(broker),
                serial: id.serial,
                selected,
                control: ticket.control.clone(),
            };
            let mut state = admission(control);
            let slot = state
                .selections
                .get_mut(&id.serial)
                .ok_or(DirectoryCommandError::Cancelled)?;
            slot.selected = Some(selected);
            slot.expected = Some(selected.first_page());
            slot.metadata_bytes = selected.metadata_bytes;
            slot.pending = false;
            slot.delivery = Some(Arc::clone(&ticket.status));
            drop(state);
            resources.0.insert(id.serial, resource);
            DirectoryResponse::Captured(selected)
        }
        Action::Page(page_request) => {
            {
                let mut state = admission(control);
                let slot = state
                    .selections
                    .get_mut(&id.serial)
                    .ok_or(DirectoryCommandError::InvalidCursor)?;
                if !slot
                    .expected
                    .is_some_and(|expected| same_cursor(expected, page_request))
                {
                    drop(state);
                    request.retire.preserve();
                    return Err(DirectoryCommandError::InvalidCursor);
                }
                slot.delivery = Some(Arc::clone(&ticket.status));
            }
            let resource = resources
                .0
                .get_mut(&id.serial)
                .ok_or(DirectoryCommandError::InvalidCursor)?;
            let page = resource
                .broker
                .as_mut()
                .ok_or(DirectoryCommandError::InvalidCursor)?
                .next_page_with_clock_and_cancel(
                    manager,
                    owner.runtime(),
                    instance,
                    &authority.binding,
                    page_request,
                    &mut clock,
                    || ticket.lock().cancelled,
                )?;
            if ticket.lock().cancelled {
                return Err(DirectoryCommandError::Cancelled);
            }
            if page.terminal {
                resources.0.remove(&id.serial);
                admission(control).selections.remove(&id.serial);
            } else {
                let mut state = admission(control);
                let slot = state
                    .selections
                    .get_mut(&id.serial)
                    .ok_or(DirectoryCommandError::Cancelled)?;
                slot.expected = Some(PageRequest {
                    reference: page_request.reference,
                    selection_epoch: page.selection_epoch,
                    page_sequence: page
                        .page_sequence
                        .checked_add(1)
                        .ok_or(DirectoryCommandError::Limit)?,
                    after_entry_id: page.entries.last().map(|entry| entry.entry_id),
                });
                slot.pending = false;
            }
            DirectoryResponse::Page(page)
        }
        Action::Finish => {
            let resource = resources
                .0
                .get_mut(&id.serial)
                .ok_or(DirectoryCommandError::InvalidCursor)?;
            if ticket.lock().cancelled {
                return Err(DirectoryCommandError::Cancelled);
            }
            resource
                .broker
                .as_mut()
                .ok_or(DirectoryCommandError::InvalidCursor)?
                .cancel_with_clock(
                    manager,
                    owner.runtime(),
                    instance,
                    &authority.binding,
                    resource.selected,
                    &mut clock,
                )?;
            resources.0.remove(&id.serial);
            admission(control).selections.remove(&id.serial);
            DirectoryResponse::Finished
        }
    };
    request.retire.complete();
    Ok(result)
}
