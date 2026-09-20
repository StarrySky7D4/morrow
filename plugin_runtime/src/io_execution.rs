//! Unique live IO execution per operationId plus durable reconciliation.
//! A stored record is never a grant: every dispatch rechecks the live binding,
//! commits the dispatch boundary before any external effect, performs the
//! backend operation at most once, and refuses resend after interruption.
//! Backends are supplied by the trusted host; this module performs no I/O.
use crate::{
    io_binding::{Error as BindingError, IoBinding, IoCallLease, IoLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    io_evidence::{Kind, Material},
    io_intent::{Command, ObservationSource, Phase, Recovery},
    plugin_package::io::IoCapability,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The live instance, manager, host, approval or capability is not current.
    Denied,
    Expired,
    Clock,
    Limit,
    /// No live execution or durable history for this operation.
    NotFound,
    /// Request, stored command or history disagree.
    Conflict,
    /// A required protected original was never retained.
    EvidenceUnavailable,
    Integrity,
    /// Another live execution already owns this operationId.
    Duplicate,
    /// The send boundary was already crossed; never a resend permission.
    Dispatched,
    /// The backend result is unknown; reconcile before any new attempt.
    OutcomeUnknown,
    Cancelled,
    /// The durable commit outcome is unknown; treat as OutcomeUnknown.
    CommitUnknown,
    Storage,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IO execution: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl From<BindingError> for Error {
    fn from(value: BindingError) -> Self {
        match value {
            BindingError::Denied => Error::Denied,
            BindingError::Expired => Error::Expired,
            BindingError::Clock => Error::Clock,
            BindingError::Limit => Error::Limit,
        }
    }
}
pub(crate) fn storage(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::Limit
        | morrow_core::Error::EventCapacity
        | morrow_core::Error::StorageFull
        | morrow_core::Error::ArchiveCapacity => Error::Limit,
        morrow_core::Error::NotFound => Error::NotFound,
        morrow_core::Error::OperationConflict
        | morrow_core::Error::RevisionConflict
        | morrow_core::Error::UnsupportedVersion
        | morrow_core::Error::Invalid(_) => Error::Conflict,
        morrow_core::Error::EvidenceUnavailable => Error::EvidenceUnavailable,
        morrow_core::Error::Integrity => Error::Integrity,
        morrow_core::Error::CommitUnknown => Error::CommitUnknown,
        _ => Error::Storage,
    }
}
/// Durable classification after a reopen or interruption. Always historical:
/// never a live grant and never permission to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub recovery: Recovery,
    pub request_retained: bool,
    pub response_retained: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Prepared,
    Dispatched,
}
// A dispatch keeps this reservation alive even if its map entry is retired.
// Retirement revokes delivery, but cannot pretend that an in-flight backend stopped.
enum Reservation {
    Standalone { _lease: IoLease },
    Job(IoCallLease),
}
pub(crate) struct Live {
    binding: IoBinding,
    reservation: Reservation,
    retired: AtomicBool,
}
impl Live {
    pub(crate) fn check_liveness(&self, now: u64) -> Result<()> {
        if self.retired.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        self.binding.check_liveness(now)?;
        if let Reservation::Job(lease) = &self.reservation {
            lease.check(now)?;
        }
        if self.retired.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}
struct Active {
    live: Arc<Live>,
    subject: String,
    command: Command,
    state: State,
}
// Private phases, consumed once. No original owner borrow enters execution.
pub(crate) struct DispatchTicket {
    operation: String,
    subject: String,
    command: Command,
    live: Arc<Live>,
    request: Material,
    original_host: HostBinding,
    original_connection: ConnectionBinding,
    broker: Arc<()>,
}

impl DispatchTicket {
    pub(crate) fn execute(
        self,
        run: impl FnOnce(&[u8]) -> std::result::Result<Vec<u8>, ()>,
        guard: &mut impl FnMut(&Live) -> Result<()>,
    ) -> Result<DispatchObservation> {
        guard(&self.live)?;
        let response = run(self.request.payload()).map_err(|_| Error::OutcomeUnknown)?;
        if response.len() as u64 > self.command.response_limit {
            return Err(Error::OutcomeUnknown);
        }
        Ok(DispatchObservation {
            ticket: self,
            response,
        })
    }
}

pub(crate) struct DispatchObservation {
    ticket: DispatchTicket,
    response: Vec<u8>,
}

/// One broker per trusted host session. The registry is in-memory only: after a
/// restart the durable history alone decides whether a fresh attempt is even
/// possible, and an OutcomeUnknown history never is.
#[derive(Default)]
pub struct Broker {
    identity: Arc<()>,
    active: Mutex<BTreeMap<String, Active>>,
    host: Mutex<Option<HostBinding>>,
}
impl Broker {
    pub fn new() -> Self {
        Self::default()
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Active>> {
        self.active.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Number of live executions, including ones awaiting reconciliation.
    pub fn active(&self) -> usize {
        self.lock().len()
    }
    pub fn operations(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }
    /// Explicitly drop a live execution record. This never changes durable
    /// history and never refunds cumulative byte usage.
    pub fn retire(&self, operation: &str) -> bool {
        if let Some(entry) = self.lock().remove(operation) {
            entry.live.retired.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }
    /// Retire executions whose binding is expired or whose owner is gone.
    /// Returns how many were retired.
    pub fn maintain(&self, now: u64) -> usize {
        let mut active = self.lock();
        let before = active.len();
        active.retain(|_, entry| {
            let keep = !entry.live.binding.reclaimable(now);
            if !keep {
                entry.live.retired.store(true, Ordering::Release);
            }
            keep
        });
        before - active.len()
    }
    fn check_host(&self, host: &HostRuntime, bind: bool) -> Result<()> {
        let mut owner = self.host.lock().unwrap_or_else(|p| p.into_inner());
        match *owner {
            Some(value) if value != host.binding() => Err(Error::Denied),
            None if bind => {
                *owner = Some(host.binding());
                Ok(())
            }
            _ => Ok(()),
        }
    }
    /// Bind exactly one live execution for a durable Prepared command. This
    /// checks the live binding, the stored command, the retained request
    /// original and the pre-send reservations; it never performs external IO
    /// and never revives a history that is past its dispatch boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        capability: IoCapability,
        subject: &str,
        command: &Command,
        now: u64,
    ) -> Result<()> {
        if subject != command.subject
            || capability != command.capability
            || command.package_sha256 != instance.package().package().digest()
        {
            return Err(Error::Conflict);
        }
        {
            let active = self.lock();
            if let Some(existing) = active.get(&command.operation_id) {
                return Err(match existing.state {
                    State::Prepared => Error::Duplicate,
                    State::Dispatched => Error::Dispatched,
                });
            }
        }
        // Live admission precedes any durable read of another instance's data.
        binding.preflight_capability(manager, host, instance, capability, now)?;
        self.check_host(host, true)?;
        let stored = host
            .store_local()
            .lookup_io_intent(subject, &command.operation_id)
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        stored.matches_command(command).map_err(storage)?;
        match stored.phase() {
            Phase::Prepared => {}
            Phase::OutcomeUnknown => return Err(Error::OutcomeUnknown),
            Phase::Observed => return Err(Error::Dispatched),
            Phase::CancelledBeforeDispatch => return Err(Error::Cancelled),
            Phase::InvalidPhase => return Err(Error::Integrity),
        }
        // The protected request original must already be durable and exact.
        let request = host
            .store_local()
            .io_material(subject, &command.operation_id, Kind::Request)
            .map_err(storage)?
            .ok_or(Error::EvidenceUnavailable)?;
        if request.payload_sha256() != command.request_sha256
            || request.payload().len() as u64 != command.request_bytes
        {
            return Err(Error::Conflict);
        }
        // Reserve the whole post-send capacity before the boundary may be
        // crossed; idempotent and logical only.
        host.store_local_mut()
            .reserve_io_intent_followup(command, || Ok(()))
            .map_err(storage)?;
        host.store_local_mut()
            .reserve_io_materials(command, || Ok(()))
            .map_err(storage)?;
        let bytes = command
            .request_bytes
            .checked_add(command.response_limit)
            .ok_or(Error::Limit)?;
        let lease = binding.admit(manager, host, instance, capability, 1, bytes, now)?;
        let mut active = self.lock();
        if active.contains_key(&command.operation_id) {
            // A concurrent begin won; its own lease is dropped with this error.
            return Err(Error::Duplicate);
        }
        active.insert(
            command.operation_id.clone(),
            Active {
                live: Arc::new(Live {
                    binding: binding.duplicate(),
                    reservation: Reservation::Standalone { _lease: lease },
                    retired: AtomicBool::new(false),
                }),
                subject: subject.to_string(),
                command: command.clone(),
                state: State::Prepared,
            },
        );
        Ok(())
    }
    /// Accept one single-use subcall reservation from an authenticated worker.
    /// The parent already owns its job slot and prepaid response ceiling.
    pub(crate) fn begin_in_job(
        &self,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        lease: IoCallLease,
        now: u64,
    ) -> Result<()> {
        lease.validate_owner(host, instance)?;
        lease.check(now)?;
        let command = lease.command().clone();
        self.check_host(host, true)?;
        let stored = host
            .store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        stored.matches_command(&command).map_err(storage)?;
        match stored.phase() {
            Phase::Prepared => {}
            Phase::OutcomeUnknown => return Err(Error::OutcomeUnknown),
            Phase::Observed => return Err(Error::Dispatched),
            Phase::CancelledBeforeDispatch => return Err(Error::Cancelled),
            Phase::InvalidPhase => return Err(Error::Integrity),
        }
        let request = host
            .store_local()
            .io_material(&command.subject, &command.operation_id, Kind::Request)
            .map_err(storage)?
            .ok_or(Error::EvidenceUnavailable)?;
        if request.payload_sha256() != command.request_sha256
            || request.payload().len() as u64 != command.request_bytes
        {
            return Err(Error::Integrity);
        }
        host.store_local_mut()
            .reserve_io_intent_followup(&command, || Ok(()))
            .map_err(storage)?;
        host.store_local_mut()
            .reserve_io_materials(&command, || Ok(()))
            .map_err(storage)?;
        let mut active = self.lock();
        if active.contains_key(&command.operation_id) {
            return Err(Error::Duplicate);
        }
        active.insert(
            command.operation_id.clone(),
            Active {
                live: Arc::new(Live {
                    binding: lease.binding().duplicate(),
                    reservation: Reservation::Job(lease),
                    retired: AtomicBool::new(false),
                }),
                subject: command.subject.clone(),
                command,
                state: State::Prepared,
            },
        );
        Ok(())
    }
    /// Claim one local execution, commit the send boundary, then invoke the backend
    /// at most once. The host clock is read at admission, final Store authorization,
    /// immediately before the backend, and immediately before payload delivery.
    /// A failed claimed attempt requires explicit retirement/reconciliation; it is
    /// never automatically retried. Observed history may be retained after revocation,
    /// but a stopped, retired or expired execution cannot deliver its response.
    pub fn dispatch(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        operation: &str,
        run: impl FnOnce(&[u8]) -> std::result::Result<Vec<u8>, ()>,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Vec<u8>> {
        self.dispatch_checked(
            host,
            instance,
            operation,
            run,
            |binding, host, instance| {
                binding
                    .validate_identity(manager, host, instance)
                    .map_err(Error::from)
            },
            |live| live.check_liveness(clock()),
        )
    }
    pub(crate) fn dispatch_in_job(
        &self,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        operation: &str,
        run: impl FnOnce(&[u8]) -> std::result::Result<Vec<u8>, ()>,
        guard: impl FnMut(&Live) -> Result<()>,
    ) -> Result<Vec<u8>> {
        {
            let active = self.lock();
            let entry = active.get(operation).ok_or(Error::NotFound)?;
            if !matches!(entry.live.reservation, Reservation::Job(_)) {
                return Err(Error::Denied);
            }
        }
        self.dispatch_checked(
            host,
            instance,
            operation,
            run,
            |binding, host, instance| binding.validate_owner(host, instance).map_err(Error::from),
            guard,
        )
    }
    pub(crate) fn claim_in_job(
        &self,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        operation: &str,
        mut guard: impl FnMut(&Live) -> Result<()>,
    ) -> Result<DispatchTicket> {
        {
            let active = self.lock();
            let entry = active.get(operation).ok_or(Error::NotFound)?;
            if !matches!(entry.live.reservation, Reservation::Job(_)) {
                return Err(Error::Denied);
            }
        }
        self.claim_dispatch_checked(
            host,
            instance,
            operation,
            &|binding, host, instance| binding.validate_owner(host, instance).map_err(Error::from),
            &mut guard,
        )
    }
    pub(crate) fn complete_in_job(
        &self,
        observation: DispatchObservation,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        mut guard: impl FnMut(&Live) -> Result<()>,
    ) -> Result<Vec<u8>> {
        self.complete_dispatch_checked(
            observation,
            host,
            instance,
            &|binding, host, instance| binding.validate_owner(host, instance).map_err(Error::from),
            &mut guard,
        )
    }
    fn retire_matching(&self, operation: &str, live: &Arc<Live>) {
        let mut active = self.lock();
        if active
            .get(operation)
            .is_some_and(|entry| Arc::ptr_eq(&entry.live, live))
        {
            active.remove(operation);
        }
        live.retired.store(true, Ordering::Release);
    }

    fn claim_dispatch_checked(
        &self,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        operation: &str,
        identity: &impl Fn(&IoBinding, &HostRuntime, &ManagedInstance) -> Result<()>,
        guard: &mut impl FnMut(&Live) -> Result<()>,
    ) -> Result<DispatchTicket> {
        self.check_host(host, false)?;
        let (subject, command, live) = {
            let mut active = self.lock();
            let entry = active.get_mut(operation).ok_or(Error::NotFound)?;
            // A foreign instance cannot advance the trusted clock or claim this slot.
            identity(&entry.live.binding, host, instance)?;
            if entry.state != State::Prepared {
                return Err(Error::Dispatched);
            }
            entry.state = State::Dispatched;
            (
                entry.subject.clone(),
                entry.command.clone(),
                Arc::clone(&entry.live),
            )
        };
        guard(&live)?;
        let stored = host
            .store_local()
            .lookup_io_intent(&subject, operation)
            .map_err(storage)?
            .ok_or(Error::Integrity)?;
        match stored.phase() {
            Phase::Prepared => {}
            Phase::OutcomeUnknown => return Err(Error::OutcomeUnknown),
            Phase::Observed => return Err(Error::Dispatched),
            Phase::CancelledBeforeDispatch => return Err(Error::Cancelled),
            Phase::InvalidPhase => return Err(Error::Integrity),
        }
        stored
            .matches_command(&command)
            .map_err(|_| Error::Integrity)?;
        let request = host
            .store_local()
            .io_material(&subject, operation, Kind::Request)
            .map_err(storage)?
            .ok_or(Error::EvidenceUnavailable)?;
        if request.payload_sha256() != command.request_sha256
            || request.payload().len() as u64 != command.request_bytes
        {
            return Err(Error::Integrity);
        }
        let boundary = stored.propose_dispatch_boundary().map_err(storage)?;
        let mut rejected = None;
        let committed =
            host.store_local_mut()
                .claim_io_dispatch_local_authorized(&boundary, || {
                    guard(&live).map_err(|error| {
                        rejected = Some(error);
                        morrow_core::Error::Invalid("inactive IO dispatch")
                    })
                });
        if let Err(error) = committed {
            // A losing claim, including a lost commit receipt, never reaches
            // the backend. Release this attempt's live reservation without
            // deleting durable history or a replacement registry entry.
            self.retire_matching(operation, &live);
            return Err(rejected.unwrap_or_else(|| storage(error)));
        }
        // Nothing after this durable boundary can safely authorize automatic resend.
        identity(&live.binding, host, instance)?;
        Ok(DispatchTicket {
            operation: operation.to_string(),
            subject,
            command,
            live,
            request,
            original_host: host.binding(),
            original_connection: instance.connection().binding(),
            broker: Arc::clone(&self.identity),
        })
    }

    fn complete_dispatch_checked(
        &self,
        observation: DispatchObservation,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        identity: &impl Fn(&IoBinding, &HostRuntime, &ManagedInstance) -> Result<()>,
        guard: &mut impl FnMut(&Live) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let DispatchObservation {
            ticket:
                DispatchTicket {
                    operation,
                    subject,
                    command,
                    live,
                    request: _,
                    original_host,
                    original_connection,
                    broker,
                },
            response,
        } = observation;

        // Structural comparisons only: reject before any writes or clock samples.
        if !Arc::ptr_eq(&broker, &self.identity) {
            return Err(Error::Denied);
        }
        if host.binding() != original_host {
            return Err(Error::Denied);
        }
        if instance.connection().binding() != original_connection {
            return Err(Error::Denied);
        }

        let material = Material::encode(
            Kind::Response,
            &operation,
            &subject,
            command.request_sha256,
            &response,
        )
        .map_err(|_| Error::OutcomeUnknown)?;
        if let Err(error) =
            host.store_local_mut()
                .store_io_material(&subject, Kind::Response, &material, || Ok(()))
        {
            return Err(match storage(error) {
                Error::Integrity | Error::Conflict => Error::Integrity,
                _ => Error::OutcomeUnknown,
            });
        }
        // Retain the actual observation even when live delivery is no longer allowed.
        self.finish(
            host,
            &subject,
            &operation,
            material.payload_sha256(),
            ObservationSource::OriginalResponse,
        )?;
        let delivery = (|| {
            // Clock callbacks may retire this execution: never call them under
            // the registry lock. Recheck membership/identity after the guard.
            guard(&live)?;
            let active = self.lock();
            let entry = active.get(&operation).ok_or(Error::Cancelled)?;
            if !Arc::ptr_eq(&entry.live, &live) {
                return Err(Error::Cancelled);
            }
            identity(&live.binding, host, instance)
        })();
        self.retire_matching(&operation, &live);
        delivery?;
        Ok(response)
    }

    fn dispatch_checked(
        &self,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        operation: &str,
        run: impl FnOnce(&[u8]) -> std::result::Result<Vec<u8>, ()>,
        identity: impl Fn(&IoBinding, &HostRuntime, &ManagedInstance) -> Result<()>,
        mut guard: impl FnMut(&Live) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let ticket =
            self.claim_dispatch_checked(host, instance, operation, &identity, &mut guard)?;
        let observation = ticket.execute(run, &mut guard)?;
        self.complete_dispatch_checked(observation, host, instance, &identity, &mut guard)
    }
    /// Canonically cancel a live Prepared execution before any external effect.
    /// After the dispatch boundary only reconciliation is available.
    pub fn cancel(&self, host: &mut HostRuntime, subject: &str, operation: &str) -> Result<()> {
        self.check_host(host, false)?;
        let stored = host
            .store_local()
            .lookup_io_intent(subject, operation)
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        match stored.phase() {
            Phase::Prepared => {}
            Phase::CancelledBeforeDispatch => {
                self.retire(operation);
                return Ok(());
            }
            Phase::OutcomeUnknown | Phase::Observed => return Err(Error::Dispatched),
            Phase::InvalidPhase => return Err(Error::Integrity),
        }
        let cancel = stored.propose_cancel_before_dispatch().map_err(storage)?;
        host.store_local_mut()
            .append_io_intent_local_authorized(&cancel, || Ok(()))
            .map_err(storage)?;
        self.retire(operation);
        Ok(())
    }
    /// Durable classification after an interruption or reopen. Never a grant.
    pub fn recovery(&self, host: &HostRuntime, subject: &str, operation: &str) -> Result<Report> {
        self.check_host(host, false)?;
        let stored = host
            .store_local()
            .lookup_io_intent(subject, operation)
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        Ok(Report {
            recovery: stored.recovery(),
            request_retained: retained(host.store_local().io_material(
                subject,
                operation,
                Kind::Request,
            ))?,
            response_retained: retained(host.store_local().io_material(
                subject,
                operation,
                Kind::Response,
            ))?,
        })
    }
    /// Close an OutcomeUnknown execution. A retained response original is used
    /// when present; otherwise the caller's reconciliation digest is recorded
    /// after releasing the never-admitted quota. The live record is retired.
    pub fn reconcile(
        &self,
        host: &mut HostRuntime,
        subject: &str,
        operation: &str,
        digest: Option<[u8; 32]>,
    ) -> Result<Recovery> {
        self.check_host(host, false)?;
        let stored = host
            .store_local()
            .lookup_io_intent(subject, operation)
            .map_err(storage)?
            .ok_or(Error::NotFound)?;
        match stored.phase() {
            Phase::OutcomeUnknown => {}
            Phase::Prepared => return Err(Error::NotFound),
            Phase::Observed | Phase::CancelledBeforeDispatch => {
                self.retire(operation);
                return Err(Error::Dispatched);
            }
            Phase::InvalidPhase => return Err(Error::Integrity),
        }
        // Release every kind whose original was never admitted; an admitted
        // original is retained and its quota already consumed.
        let mut admitted = [false, false];
        for (index, kind) in [Kind::Request, Kind::Response].into_iter().enumerate() {
            admitted[index] = retained(host.store_local().io_material(subject, operation, kind))?;
        }
        for (index, kind) in [Kind::Request, Kind::Response].into_iter().enumerate() {
            if admitted[index] {
                continue;
            }
            match host
                .store_local_mut()
                .release_io_material_reconciliation(subject, operation, kind)
            {
                Ok(()) | Err(morrow_core::Error::NotFound) => {}
                Err(error) => return Err(storage(error)),
            }
        }
        let response = admitted[1];
        let (digest, source) = if response {
            let material = host
                .store_local()
                .io_material(subject, operation, Kind::Response)
                .map_err(storage)?
                .ok_or(Error::Integrity)?;
            if digest.is_some_and(|value| value != material.payload_sha256()) {
                return Err(Error::Conflict);
            }
            (
                material.payload_sha256(),
                ObservationSource::OriginalResponse,
            )
        } else {
            (
                digest.ok_or(Error::EvidenceUnavailable)?,
                ObservationSource::Reconciliation,
            )
        };
        self.finish(host, subject, operation, digest, source)?;
        self.retire(operation);
        Ok(Recovery::AlreadyObserved)
    }
    fn finish(
        &self,
        host: &mut HostRuntime,
        subject: &str,
        operation: &str,
        digest: [u8; 32],
        source: ObservationSource,
    ) -> Result<()> {
        let stored = host
            .store_local()
            .lookup_io_intent(subject, operation)
            .map_err(storage)?
            .ok_or(Error::Integrity)?;
        if stored.phase() != Phase::OutcomeUnknown {
            return Err(Error::Integrity);
        }
        let observed = stored
            .propose_observation(digest, source)
            .map_err(storage)?;
        match host
            .store_local_mut()
            .append_io_intent_local_authorized(&observed, || Ok(()))
        {
            Ok(_) => {}
            Err(morrow_core::Error::CommitUnknown) => return Err(Error::CommitUnknown),
            Err(error) => return Err(storage(error)),
        }
        Ok(())
    }
}
fn retained(value: morrow_core::Result<Option<Material>>) -> Result<bool> {
    match value {
        Ok(Some(_)) => Ok(true),
        Ok(None) => Ok(false),
        Err(morrow_core::Error::EvidenceUnavailable) => Ok(false),
        Err(error) => Err(storage(error)),
    }
}

#[cfg(test)]
mod phase_tests;
