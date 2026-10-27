//! Trusted selected-directory commands on the complete original owner worker.
//! Fresh capture generates its secret only on this original worker. No path
//! lookup, guest import, fresh approval or recursion is added.
use super::*;
use crate::directory_io::{
    CaptureLimits, DirectoryAbsolutePath, DirectoryBroker, DirectoryClock, DirectoryRelativePath,
    DirectoryRetirementSnapshot, PageRequest, SelectedDirectory,
    retirement::{Ledger, TrackedFile},
};
use morrow_fs_directory_v1::FsDirectoryPage;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "directory_commands/guest.rs"]
mod guest;
pub(super) use guest::GuestDispatch;
pub use guest::{DirectoryGuestHandle, DirectoryGuestResult};
pub(in crate::io_jobs) use guest::{GuestReply, GuestRequest};

pub const MAX_DIRECTORY_OBSERVATIONS: usize = 8;
const MAX_WIRE: u64 = 65536;
const MAX_PAGE_OWNED: u64 = 16384 + 32 * 128;
/// Complete native command bookkeeping; a diagnostic bound, not an ABI.
pub const DIRECTORY_COMMAND_FIXED_BYTES: u64 =
    (size_of::<Request>() + size_of::<DirectoryResponse>()) as u64 + Ledger::OWNED_BYTES;
/// Includes allocator/encoded-wire coexistence as well as owned page storage.
pub const DIRECTORY_PAGE_CHARGE: u64 =
    105 + 3 * MAX_WIRE + 64 + MAX_PAGE_OWNED + DIRECTORY_COMMAND_FIXED_BYTES + 7 * 64;
pub const DIRECTORY_FINISH_CHARGE: u64 = 64 + DIRECTORY_COMMAND_FIXED_BYTES;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

// Checked original serial domain. Failed admission burns a serial; no rollback,
// zero identity or wraparound is accepted. Tests use their own local counter.
fn next_directory_serial(counter: &AtomicU64) -> Result<u64, OwnerCommandError> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1).filter(|_| next != 0)
        })
        .map_err(|_| OwnerCommandError::Limit)
}

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
    ledger: Arc<Ledger>,
}
impl std::fmt::Debug for DirectoryCommandHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectoryCommandHandle")
            .finish_non_exhaustive()
    }
}
impl DirectoryCommandHandle {
    /// Finite original-worker observations; not a grant or proof of join.
    pub fn retirement_snapshot(&self) -> DirectoryRetirementSnapshot {
        self.ledger.snapshot()
    }
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
        if self.ledger.snapshot().unknown != 0 {
            return Err(OwnerCommandError::Unknown);
        }
        match self.inner.read_reply()? {
            Some(Reply::Directory(result)) => {
                let terminal = matches!(&*result, Ok(DirectoryResponse::Finished))
                    || matches!(&*result, Ok(DirectoryResponse::Page(page)) if page.terminal);
                if terminal && !self.ledger.snapshot().all_closed() {
                    return Err(OwnerCommandError::Unknown);
                }
                Ok(Some(*result))
            }
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

struct Admitted {
    ledger: Arc<Ledger>,
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
    #[cfg(test)]
    entropy_probe: Option<Arc<EntropyProbe>>,
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
            retire_admission(&control, self.serial);
        }
    }
}
pub(super) struct Request {
    id: DirectorySession,
    ledger: Arc<Ledger>,
    action: Action,
    retire: Retirement,
}
enum Action {
    Capture {
        source: CaptureSource,
        limits: CaptureLimits,
        secret: CaptureSecret,
    },
    Page(PageRequest),
    Finish,
}
enum CaptureSource {
    Open(CaptureInput),
    SelectedPath(DirectoryAbsolutePath),
    NativePicker,
    Under {
        anchor: CaptureInput,
        relative: DirectoryRelativePath,
    },
}
enum CaptureInput {
    Unsubmitted(Option<File>),
    Tracked(TrackedFile),
}
impl CaptureInput {
    fn new(file: File) -> Self {
        Self::Unsubmitted(Some(file))
    }
    fn enter_worker(&mut self, ledger: &Arc<Ledger>) {
        if let Self::Unsubmitted(file) = self {
            let file = file.take().expect("one original queued input");
            *self = Self::Tracked(ledger.track(file));
        }
    }
    fn into_tracked(self) -> TrackedFile {
        match self {
            Self::Tracked(file) => file,
            Self::Unsubmitted(_) => unreachable!("accepted input enters the original worker first"),
        }
    }
    fn into_unsubmitted(self) -> File {
        match self {
            Self::Unsubmitted(Some(file)) => file,
            _ => unreachable!("only an unsubmitted original File can be returned"),
        }
    }
}
impl CaptureSource {
    fn input_bytes(&self) -> u64 {
        match self {
            Self::Open(_) => 0,
            Self::NativePicker => crate::directory_io::native_picker::INPUT_BYTES,
            Self::SelectedPath(path) => path.retained_input_bytes() as u64,
            Self::Under { relative, .. } => relative.retained_input_bytes() as u64,
        }
    }
}
// No production entropy provider or public injection parameter exists.
enum CaptureSecret {
    Provided(Zeroizing<[u8; 32]>),
    Fresh(FreshSecret),
}
struct FreshSecret {
    #[cfg(test)]
    probe: Option<Arc<EntropyProbe>>,
}

struct EntropyBuffer {
    bytes: Zeroizing<[u8; 32]>,
    #[cfg(test)]
    probe: Option<Arc<EntropyProbe>>,
}
// Production relies on the Zeroizing field's Drop. This test-only observer
// wipes live owned memory first, then records a boolean, never freed bytes.
#[cfg(test)]
impl Drop for EntropyBuffer {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.bytes.zeroize();
        if let Some(probe) = &self.probe {
            probe
                .wipe_observed
                .store(self.bytes.iter().all(|byte| *byte == 0), Ordering::Release);
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum EntropyTestMode {
    OperatingSystem,
    Nonzero,
    AllZero,
    PartialError,
}
#[cfg(test)]
struct EntropyProbe {
    mode: EntropyTestMode,
    calls: std::sync::atomic::AtomicUsize,
    thread: Mutex<Option<std::thread::ThreadId>>,
    before_fill: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    after_fill: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    wipe_observed: std::sync::atomic::AtomicBool,
}
#[cfg(test)]
impl EntropyProbe {
    fn new(mode: EntropyTestMode) -> Self {
        Self {
            mode,
            calls: std::sync::atomic::AtomicUsize::new(0),
            thread: Mutex::new(None),
            before_fill: Mutex::new(None),
            after_fill: Mutex::new(None),
            wipe_observed: std::sync::atomic::AtomicBool::new(false),
        }
    }
    fn run_once(
        hook: &Mutex<Option<Box<dyn FnOnce() + Send>>>,
    ) -> Result<(), DirectoryCommandError> {
        let hook = hook
            .lock()
            .map_err(|_| DirectoryCommandError::Entropy)?
            .take();
        if let Some(hook) = hook {
            hook();
        }
        Ok(())
    }
}

fn fill_entropy(buffer: &mut EntropyBuffer) -> Result<(), DirectoryCommandError> {
    #[cfg(test)]
    if let Some(probe) = &buffer.probe {
        probe.calls.fetch_add(1, Ordering::Relaxed);
        *probe
            .thread
            .lock()
            .map_err(|_| DirectoryCommandError::Entropy)? = Some(std::thread::current().id());
        EntropyProbe::run_once(&probe.before_fill)?;
        match probe.mode {
            EntropyTestMode::OperatingSystem => {
                getrandom::fill(&mut buffer.bytes[..])
                    .map_err(|_| DirectoryCommandError::Entropy)?;
            }
            EntropyTestMode::Nonzero => buffer.bytes.fill(0xA5),
            EntropyTestMode::AllZero => buffer.bytes.fill(0),
            EntropyTestMode::PartialError => {
                buffer.bytes[..16].fill(0xA5);
                return Err(DirectoryCommandError::Entropy);
            }
        }
        EntropyProbe::run_once(&probe.after_fill)?;
        return Ok(());
    }
    getrandom::fill(&mut buffer.bytes[..]).map_err(|_| DirectoryCommandError::Entropy)
}

fn fresh_directory_secret(
    id: DirectorySession,
    fresh: FreshSecret,
    recheck: impl FnOnce() -> Result<(), DirectoryCommandError>,
) -> Result<Zeroizing<[u8; 32]>, DirectoryCommandError> {
    if id.worker == 0 || id.serial == 0 {
        return Err(DirectoryCommandError::Limit);
    }
    let FreshSecret {
        #[cfg(test)]
        probe,
    } = fresh;
    let mut random = EntropyBuffer {
        bytes: Zeroizing::new([0; 32]),
        #[cfg(test)]
        probe,
    };
    // A partial OS error never reaches hashing; owned entropy drops on every
    // error and unwind. An all-zero successful fill is also refused.
    fill_entropy(&mut random)?;
    if random.bytes.iter().all(|byte| *byte == 0) {
        return Err(DirectoryCommandError::Entropy);
    }
    recheck()?;
    let mut hash = Sha256::new();
    hash.update(b"morrow.io-owner.directory.fresh-secret.v1\0");
    hash.update(id.worker.to_le_bytes());
    hash.update(id.serial.to_le_bytes());
    hash.update(&random.bytes[..]);
    let mut secret = Zeroizing::new([0; 32]);
    hash.finalize_into((&mut *secret).into());
    if secret.iter().all(|byte| *byte == 0) {
        return Err(DirectoryCommandError::Entropy);
    }
    Ok(secret)
}

fn capture_veto(
    control: &Control,
    ticket: &Ticket,
    authority: &super::super::Authority,
) -> Result<(), DirectoryCommandError> {
    // Never call Control::fault or acquire Control/clock from this predicate.
    // Each ticket guard ends before any clock step or OS entropy call.
    if ticket.lock().cancelled || control.revocation.is_revoked() {
        return Err(DirectoryCommandError::Cancelled);
    }
    if let Some(fault) = authority.cancellation.fault() {
        return Err(match fault {
            super::super::Fault::Deadline => {
                DirectoryCommandError::Admission(BindingError::Expired)
            }
            _ => DirectoryCommandError::Cancelled,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "directory_commands/secret_factory_tests.rs"]
mod secret_factory_tests;

impl Request {
    pub(super) fn enter_worker(&mut self) {
        self.ledger.bind_worker();
        if let Action::Capture { source, .. } = &mut self.action {
            match source {
                CaptureSource::Open(file) => file.enter_worker(&self.ledger),
                CaptureSource::Under { anchor, .. } => anchor.enter_worker(&self.ledger),
                CaptureSource::SelectedPath(_) | CaptureSource::NativePicker => {}
            }
        }
    }
    fn into_capture_source(self) -> CaptureSource {
        let Self {
            action, mut retire, ..
        } = self;
        let Action::Capture { source, .. } = action else {
            unreachable!("only an unsubmitted capture can return its source");
        };
        // Remove the unused admission while the original File is still held.
        // No request reached the worker, so this is not resource retirement.
        if let Some(control) = retire.control.upgrade() {
            admission(&control).selections.remove(&retire.serial);
        }
        retire.complete();
        source
    }
    fn fixed_charge() -> u64 {
        DIRECTORY_COMMAND_FIXED_BYTES
    }
    pub(super) fn charge(&self) -> u64 {
        match &self.action {
            Action::Capture { source, limits, .. } => {
                limits.max_job_bytes + Self::fixed_charge() + source.input_bytes()
            }
            // Request105, builder/encoded-wire coexistence, two identity queries, owned page
            // name/container storage and complete command/reply bookkeeping.
            Action::Page(_) => DIRECTORY_PAGE_CHARGE,
            Action::Finish => DIRECTORY_FINISH_CHARGE,
        }
    }
    fn reservation(&self) -> Result<usize, OwnerCommandError> {
        let variable = match &self.action {
            Action::Capture { source, limits, .. } => {
                limits.max_metadata_bytes + source.input_bytes()
            }
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
    ledger: Arc<Ledger>,
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
            retire_admission(&control, self.serial);
        }
    }
}
fn retire_admission(control: &Control, serial: u64) {
    let ledger = admission(control)
        .selections
        .get(&serial)
        .map(|slot| Arc::clone(&slot.ledger));
    let closed = ledger.is_some_and(|ledger| ledger.settle());
    let mut state = admission(control);
    if closed {
        state.selections.remove(&serial);
    } else if let Some(slot) = state.selections.get_mut(&serial) {
        slot.retired = true;
        slot.pending = false;
    }
}

pub(in crate::io_jobs) fn native_close_unknown(control: &Control) -> bool {
    admission(control)
        .selections
        .values()
        .any(|slot| slot.ledger.snapshot().unknown != 0)
}

pub(in crate::io_jobs) fn retain_retirements(control: &Control) -> Vec<Arc<Ledger>> {
    admission(control)
        .selections
        .values()
        .map(|slot| Arc::clone(&slot.ledger))
        .collect()
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
        self.capture_directory_request(
            CaptureSource::Open(CaptureInput::new(file)),
            limits,
            CaptureSecret::Provided(Zeroizing::new(fresh_host_secret)),
        )
    }
    /// File is already selected by the trusted host. This queues generation of
    /// fresh OS entropy on the original owner worker after identity/liveness
    /// checks. It supplies no approval, path, picker or ancestor provenance.
    /// Entropy errors fail closed. System RNG work is cooperative, not an OS
    /// interrupt, and no original clock/Control guard spans it. Lost delivery
    /// retains original Unknown/no-replay semantics and cumulative charges.
    pub fn capture_directory_fresh(
        &self,
        file: File,
        limits: CaptureLimits,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        self.capture_directory_request(
            CaptureSource::Open(CaptureInput::new(file)),
            limits,
            CaptureSecret::Fresh(FreshSecret {
                #[cfg(test)]
                probe: None,
            }),
        )
    }
    /// Like `capture_directory_fresh`, but a refused enqueue returns the exact
    /// original File. No clone, reopen, close or transfer to the worker occurs
    /// on Err. The caller remains responsible for its checked retirement.
    /// Ok transfers ownership once; a later lost result is Unknown and never
    /// returns the File or authorizes reopening the selection. This API does
    /// not attest to native-picker provenance or worker-side close completion.
    pub fn capture_directory_fresh_retained(
        &self,
        file: File,
        limits: CaptureLimits,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), (OwnerCommandError, File)> {
        match self.capture_directory_request_retained(
            CaptureSource::Open(CaptureInput::new(file)),
            limits,
            CaptureSecret::Fresh(FreshSecret {
                #[cfg(test)]
                probe: None,
            }),
        ) {
            Ok(accepted) => Ok(accepted),
            Err((error, CaptureSource::Open(file))) => Err((error, file.into_unsubmitted())),
            Err(_) => unreachable!("the original capture source variant is preserved"),
        }
    }
    /// Original-worker selection beneath a trusted, already opened anchor.
    /// Only the exact retained UTF16 components are opened, without reparse
    /// traversal. Root nomination/above-root/picker provenance stay with the
    /// trusted host; this host API supplies no guest pathname or child grant.
    /// Every held ancestor and leaf shares the original FileList resource cap;
    /// OS work remains cooperative, and lost delivery is Unknown/no-replay.
    pub fn capture_directory_under(
        &self,
        anchor: File,
        relative: DirectoryRelativePath,
        limits: CaptureLimits,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        self.capture_directory_request(
            CaptureSource::Under {
                anchor: CaptureInput::new(anchor),
                relative,
            },
            limits,
            CaptureSecret::Fresh(FreshSecret {
                #[cfg(test)]
                probe: None,
            }),
        )
    }
    /// A trusted native selection resolved only on the original worker, after
    /// FileList approval. Every component from the actual volume root shares
    /// the existing resource cap. No picker-time or durable path claim is made.
    pub fn capture_selected_directory(
        &self,
        selected: DirectoryAbsolutePath,
        limits: CaptureLimits,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        self.capture_directory_request(
            CaptureSource::SelectedPath(selected),
            limits,
            CaptureSecret::Fresh(FreshSecret {
                #[cfg(test)]
                probe: None,
            }),
        )
    }
    /// Open the native directory chooser on this original owner worker. The
    /// original FileList binding is checked before and during the dialog; its
    /// confirmation callback captures the selected object and all ancestors.
    /// This adds no guest path or OS-handle input and creates no second owner.
    pub fn capture_directory_native_picker(
        &self,
        limits: CaptureLimits,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        self.capture_directory_request(
            CaptureSource::NativePicker,
            limits,
            CaptureSecret::Fresh(FreshSecret {
                #[cfg(test)]
                probe: None,
            }),
        )
    }
    #[cfg(test)]
    fn install_entropy_probe(&self, probe: Arc<EntropyProbe>) {
        admission(&self.control).entropy_probe = Some(probe);
    }
    fn capture_directory_request(
        &self,
        source: CaptureSource,
        limits: CaptureLimits,
        secret: CaptureSecret,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), OwnerCommandError> {
        self.capture_directory_request_retained(source, limits, secret)
            .map_err(|(error, _unsubmitted)| error)
    }
    fn capture_directory_request_retained(
        &self,
        source: CaptureSource,
        limits: CaptureLimits,
        secret: CaptureSecret,
    ) -> Result<(DirectorySession, DirectoryCommandHandle), (OwnerCommandError, CaptureSource)>
    {
        // Early errors retain the source before any request/retirement exists.
        macro_rules! retaining {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(error) => return Err((error, source)),
                }
            };
        }
        if self.control.authority.is_none() {
            return Err((OwnerCommandError::Closed, source));
        }
        let mut limits = retaining!(self.directory_capture_budget(limits));
        let available = retaining!(
            self.control
                .limits
                .max_job_bytes
                .checked_sub(Request::fixed_charge())
                .and_then(|value| value.checked_sub(source.input_bytes()))
                .filter(|value| *value != 0)
                .ok_or(OwnerCommandError::Limit)
        );
        limits.max_job_bytes = limits.max_job_bytes.min(available);
        let mut state = admission(&self.control);
        if state.selections.len() >= MAX_DIRECTORY_OBSERVATIONS {
            return Err((OwnerCommandError::Busy, source));
        }
        let serial = retaining!(next_directory_serial(&NEXT_DIRECTORY));
        #[cfg(test)]
        let secret = match secret {
            CaptureSecret::Fresh(_) => CaptureSecret::Fresh(FreshSecret {
                probe: state.entropy_probe.clone(),
            }),
            provided => provided,
        };
        let id = DirectorySession {
            worker: self.control.id,
            serial,
        };
        let ledger = Ledger::new();
        state.selections.insert(
            serial,
            Admitted {
                ledger: Arc::clone(&ledger),
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
            ledger: Arc::clone(&ledger),
            action: Action::Capture {
                source,
                limits,
                secret,
            },
            retire: Retirement {
                control: Arc::downgrade(&self.control),
                serial,
                armed: true,
            },
        };
        let bytes = match request.reservation() {
            Ok(bytes) => bytes,
            Err(error) => return Err((error, request.into_capture_source())),
        };
        let inner = match self.enqueue_owner_command_retained(
            CommandKind::Directory {
                request,
                dispatch: dispatch::<O>,
            },
            bytes,
            None,
        ) {
            Ok(inner) => inner,
            Err((error, original)) => match *original {
                CommandKind::Directory { request, .. } => {
                    return Err((error, request.into_capture_source()));
                }
                _ => unreachable!("the original command variant is preserved"),
            },
        };
        Ok((id, DirectoryCommandHandle { inner, ledger }))
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
        let ledger = Arc::clone(&slot.ledger);
        drop(state);
        let request = Request {
            id,
            ledger: Arc::clone(&ledger),
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
        Ok(DirectoryCommandHandle { inner, ledger })
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
            source,
            limits,
            secret,
        } => {
            // This remains readonly runtime getter #2; #1 validated identity.
            // Fresh preflight reuses this exact host reference, without another
            // getter or original-clock guard across the entropy/derive phase.
            let host = owner.runtime();
            let secret = match secret {
                CaptureSecret::Provided(secret) => secret,
                CaptureSecret::Fresh(fresh) => {
                    let mut validate = || {
                        capture_veto(control, ticket, authority)?;
                        clock.with(|now| {
                            authority
                                .binding
                                .preflight_capability(
                                    manager,
                                    host,
                                    instance,
                                    morrow_core::plugin_package::io::IoCapability::FileList,
                                    now,
                                )
                                .map_err(DirectoryCommandError::from)
                        })?;
                        capture_veto(control, ticket, authority)
                    };
                    validate()?;
                    let secret = fresh_directory_secret(id, fresh, &mut validate)?;
                    // Cancellation may race derivation; no broker is created or
                    // observation published without this further original veto.
                    capture_veto(control, ticket, authority)?;
                    secret
                }
            };
            let mut broker = DirectoryBroker::from_secret(secret);
            let selected = match source {
                CaptureSource::NativePicker => {
                    use crate::directory_io::native_picker::{self, Event};
                    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};
                    let mut captured = None;
                    native_picker::show(&request.ledger, |event| {
                        capture_veto(control, ticket, authority)?;
                        clock.with(|now| {
                            authority
                                .binding
                                .preflight_capability(
                                    manager,
                                    host,
                                    instance,
                                    morrow_core::plugin_package::io::IoCapability::FileList,
                                    now,
                                )
                                .map_err(DirectoryCommandError::from)
                        })?;
                        if let Event::Select(raw) = event {
                            if captured.is_some() {
                                return Err(DirectoryCommandError::InvalidCursor);
                            }
                            let spelling = OsString::from_wide(raw);
                            let path = DirectoryAbsolutePath::new(Path::new(&spelling))
                                .map_err(|_| DirectoryCommandError::InvalidNativeRecord)?;
                            captured = Some(broker.grant_selected_with_retirement(
                                manager,
                                host,
                                instance,
                                &authority.binding,
                                path,
                                limits,
                                &mut clock,
                                || ticket.lock().cancelled,
                                Arc::clone(&request.ledger),
                            )?);
                            capture_veto(control, ticket, authority)?;
                        }
                        Ok(())
                    })?;
                    capture_veto(control, ticket, authority)?;
                    clock.with(|now| {
                        authority
                            .binding
                            .preflight_capability(
                                manager,
                                host,
                                instance,
                                morrow_core::plugin_package::io::IoCapability::FileList,
                                now,
                            )
                            .map_err(DirectoryCommandError::from)
                    })?;
                    captured.ok_or(DirectoryCommandError::InvalidNativeRecord)?
                }
                CaptureSource::SelectedPath(selected) => broker.grant_selected_with_retirement(
                    manager,
                    host,
                    instance,
                    &authority.binding,
                    selected,
                    limits,
                    &mut clock,
                    || ticket.lock().cancelled,
                    Arc::clone(&request.ledger),
                )?,
                CaptureSource::Open(file) => broker.grant_tracked_directory(
                    manager,
                    host,
                    instance,
                    &authority.binding,
                    file.into_tracked(),
                    None,
                    limits,
                    &mut clock,
                    || ticket.lock().cancelled,
                )?,
                CaptureSource::Under { anchor, relative } => broker.grant_tracked_directory(
                    manager,
                    host,
                    instance,
                    &authority.binding,
                    anchor.into_tracked(),
                    Some(relative),
                    limits,
                    &mut clock,
                    || ticket.lock().cancelled,
                )?,
            };
            if ticket.lock().cancelled {
                return Err(DirectoryCommandError::Cancelled);
            }
            // Arm native-first cleanup BEFORE publishing resident metadata;
            // every error/unwind after publication drops this owning guard.
            let resource = Resource {
                broker: Some(broker),
                ledger: Arc::clone(&request.ledger),
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
                retire_admission(control, id.serial);
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
            retire_admission(control, id.serial);
            DirectoryResponse::Finished
        }
    };
    request.retire.complete();
    Ok(result)
}
