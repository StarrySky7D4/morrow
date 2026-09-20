//! Trusted broker admission only: no file/network execution, resource path or guest grant.
//! Every delivery must check its lease again. Reservations do not prove an external effect.
use crate::{
    Cancellation, Fault,
    manager::{Control, ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    io::{self, Action, Request},
    io_intent::Command,
    lifecycle::InstancePhase,
    plugin_package::io::{IoCapability, proto::IoBudget},
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Expired,
    Clock,
    Limit,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IO admission: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub resources: u32,
    pub jobs: u32,
    /// Cumulative admitted bytes, not current resident bytes; releasing a lease never refunds them.
    pub bytes: u64,
}
#[derive(Default)]
struct State {
    usage: Usage,
    last_tick: u64,
    service_run: Option<ServiceRun>,
}
struct ServiceRun {
    deadline: Instant,
    failed: Option<Error>,
}
/// One context per actual managed instance, owned by its Control, never by a bind request.
pub(crate) struct IoContext {
    budget: IoBudget,
    max_run_ms: Option<u64>,
    state: Mutex<State>,
}
impl IoContext {
    pub(crate) fn new(budget: &IoBudget, max_run_ms: Option<u64>) -> Self {
        Self {
            budget: *budget,
            max_run_ms,
            state: Mutex::new(State::default()),
        }
    }
}

/// An opaque live binding, deliberately without serialization or a public constructor.
/// Clocks are trusted host monotonic milliseconds, shared across this instance's bindings.
pub struct IoBinding {
    manager: Weak<()>,
    control: Weak<Control>,
    host: HostBinding,
    connection: ConnectionBinding,
    context: Arc<IoContext>,
    capabilities: BTreeSet<IoCapability>,
    expires: u64,
}
impl IoBinding {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        capabilities: BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
        service_run: bool,
    ) -> Result<Self> {
        let control = instance.io_control();
        let context = control.io.as_ref().ok_or(Error::Denied)?.clone();
        let duration = expires
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or(Error::Expired)?;
        // A service profile cannot escape its one-shot lifetime through the legacy API.
        if service_run != context.max_run_ms.is_some() {
            return Err(Error::Denied);
        }
        if duration > context.max_run_ms.unwrap_or(context.budget.max_duration_ms) {
            return Err(Error::Limit);
        }
        let mut state = context.state.lock().map_err(|_| Error::Denied)?;
        if now < state.last_tick {
            return Err(Error::Clock);
        }
        if !control.active() {
            return Err(Error::Denied);
        }
        if service_run {
            if state.service_run.is_some() {
                return Err(Error::Denied);
            }
            let deadline = Instant::now()
                .checked_add(Duration::from_millis(duration))
                .ok_or(Error::Limit)?;
            state.service_run = Some(ServiceRun {
                deadline,
                failed: None,
            });
        }
        state.last_tick = now;
        drop(state);
        Ok(Self {
            manager: manager.identity(),
            control: Arc::downgrade(control),
            host: host.binding(),
            connection: instance.connection().binding(),
            context,
            capabilities,
            expires,
        })
    }
    pub(crate) fn validate_identity(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        if !Weak::ptr_eq(&self.manager, &manager.identity())
            || self.manager.upgrade().is_none()
            || self.host != host.binding()
            || self.connection != instance.connection().binding()
            || !Weak::ptr_eq(&self.control, &Arc::downgrade(instance.io_control()))
        {
            return Err(Error::Denied);
        }
        manager
            .validate_instance(host, instance)
            .map_err(|_| Error::Denied)?;
        if !self.control.upgrade().is_some_and(|c| c.active()) {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// Check the original, already authenticated owner after it moved to a worker.
    /// This cannot authenticate a different manager or establish a fresh approval.
    pub(crate) fn validate_owner(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        let control = instance.io_control();
        if self.manager.upgrade().is_none()
            || self.host != host.binding()
            || self.connection != instance.connection().binding()
            || !Weak::ptr_eq(&self.control, &Arc::downgrade(control))
            || !control
                .io
                .as_ref()
                .is_some_and(|context| Arc::ptr_eq(context, &self.context))
            || instance.connection().package_digest() != Some(instance.package().package().digest())
            || host.connection_phase(instance.connection()) != Ok(InstancePhase::Ready)
            || !control.active()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// Compare two already authenticated bindings without accepting a replacement
    /// owner or deriving any new capability from package metadata.
    pub(crate) fn validate_same_owner(&self, other: &Self) -> Result<()> {
        if self.host != other.host
            || self.connection != other.connection
            || !Weak::ptr_eq(&self.manager, &other.manager)
            || !Weak::ptr_eq(&self.control, &other.control)
            || !Arc::ptr_eq(&self.context, &other.context)
            || self.manager.upgrade().is_none()
            || !self
                .control
                .upgrade()
                .is_some_and(|control| control.active())
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn require_capability(&self, capability: IoCapability) -> Result<()> {
        if !self.capabilities.contains(&capability) {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn validate_time(&self, state: &mut State, now: u64) -> Result<()> {
        if let Some(run) = &mut state.service_run {
            if let Some(error) = run.failed {
                return Err(error);
            }
            let error = if now < state.last_tick {
                Some(Error::Clock)
            } else if now >= self.expires || Instant::now() >= run.deadline {
                Some(Error::Expired)
            } else {
                None
            };
            if let Some(error) = error {
                run.failed = Some(error);
                return Err(error);
            }
        } else {
            if now < state.last_tick {
                return Err(Error::Clock);
            }
            if now >= self.expires {
                return Err(Error::Expired);
            }
        }
        Ok(())
    }
    /// Validate before examining another resource, without advancing shared accounting.
    /// An authenticated service-run clock failure permanently invalidates the run.
    pub(crate) fn preflight(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.validate_identity(manager, host, instance)?;
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&mut state, now)
    }
    pub(crate) fn preflight_capability(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        capability: IoCapability,
        now: u64,
    ) -> Result<()> {
        self.validate_identity(manager, host, instance)?;
        if !self.capabilities.contains(&capability) {
            return Err(Error::Denied);
        }
        self.preflight(manager, host, instance, now)
    }
    /// Only after exact host/manager/instance identity has already been checked.
    /// Used inside a Store final guard where the host is exclusively borrowed.
    /// This retains the original approval control; it cannot authorize a new owner.
    pub(crate) fn check_liveness(&self, now: u64) -> Result<()> {
        let active = || {
            self.manager.upgrade().is_some()
                && self
                    .control
                    .upgrade()
                    .is_some_and(|control| control.active())
        };
        if !active() {
            return Err(Error::Denied);
        }
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&mut state, now)?;
        if !active() {
            return Err(Error::Denied);
        }
        state.last_tick = now;
        Ok(())
    }
    /// Recheck immediately before a broker starts work or delivers an observed result.
    pub fn check(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.validate_identity(manager, host, instance)?;
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&mut state, now)?;
        if !self.control.upgrade().is_some_and(|c| c.active()) {
            return Err(Error::Denied);
        }
        state.last_tick = now;
        Ok(())
    }
    /// Reserve one concurrent job and its held resources, charging the declared byte
    /// amount permanently. No I/O is performed. A real broker must account for both
    /// request and result bytes and must not deliver beyond its admitted reservation.
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        capability: IoCapability,
        resources: u32,
        bytes: u64,
        now: u64,
    ) -> Result<IoLease> {
        // Invalid identity/capability, time or quota cannot mutate another instance's clock/budget.
        self.validate_identity(manager, host, instance)?;
        if !self.capabilities.contains(&capability) {
            return Err(Error::Denied);
        }
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&mut state, now)?;
        let next = Usage {
            resources: state
                .usage
                .resources
                .checked_add(resources)
                .ok_or(Error::Limit)?,
            jobs: state.usage.jobs.checked_add(1).ok_or(Error::Limit)?,
            bytes: state.usage.bytes.checked_add(bytes).ok_or(Error::Limit)?,
        };
        let budget = &self.context.budget;
        if next.resources > budget.max_resources
            || next.jobs > budget.max_jobs
            || next.bytes > budget.max_bytes
            || bytes > budget.max_job_bytes
        {
            return Err(Error::Limit);
        }
        if !self.control.upgrade().is_some_and(|c| c.active()) {
            return Err(Error::Denied);
        }
        state.usage = next;
        state.last_tick = now;
        Ok(IoLease {
            binding: self.duplicate(),
            resources,
            bytes,
            capability,
        })
    }
    /// Hold one resource under the existing approved ceiling, without consuming
    /// a concurrent job or byte budget. Concrete resource selection is host policy.
    pub(crate) fn admit_resource(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        capabilities: &[IoCapability],
        now: u64,
    ) -> Result<IoResourceLease> {
        self.validate_identity(manager, host, instance)?;
        if capabilities.is_empty() || capabilities.iter().any(|c| !self.capabilities.contains(c)) {
            return Err(Error::Denied);
        }
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&mut state, now)?;
        let resources = state.usage.resources.checked_add(1).ok_or(Error::Limit)?;
        if resources > self.context.budget.max_resources {
            return Err(Error::Limit);
        }
        self.validate_owner(host, instance)?;
        state.usage.resources = resources;
        state.last_tick = now;
        Ok(IoResourceLease {
            binding: self.duplicate(),
            resources: 1,
        })
    }
    /// Internal job admission after an exact managed owner was authenticated at
    /// worker construction. Uses the original instance context, never a new grant.
    pub(crate) fn admit_job_authenticated(
        &self,
        bytes: u64,
        limit: u64,
        now: u64,
    ) -> Result<IoJobLease> {
        let mut state = self.context.state.lock().map_err(|_| Error::Denied)?;
        if self.manager.upgrade().is_none() || !self.control.upgrade().is_some_and(|c| c.active()) {
            return Err(Error::Denied);
        }
        self.validate_time(&mut state, now)?;
        let budget = &self.context.budget;
        let total = state.usage.bytes.checked_add(bytes).ok_or(Error::Limit)?;
        if limit == 0
            || limit > budget.max_job_bytes
            || bytes > limit
            || total > budget.max_bytes
            || state.usage.jobs >= budget.max_jobs
        {
            return Err(Error::Limit);
        }
        state.usage.jobs += 1;
        state.usage.bytes = total;
        state.last_tick = now;
        Ok(IoJobLease {
            binding: self.duplicate(),
            limit,
            bytes: Mutex::new(bytes),
        })
    }
    pub(crate) fn reclaimable(&self, now: u64) -> bool {
        now >= self.expires
            || self.context.state.lock().map_or(true, |state| {
                state
                    .service_run
                    .as_ref()
                    .is_some_and(|run| run.failed.is_some() || Instant::now() >= run.deadline)
            })
            || self.manager.upgrade().is_none()
            || !self.control.upgrade().is_some_and(|c| c.active())
    }
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            control: self.control.clone(),
            host: self.host,
            connection: self.connection,
            context: self.context.clone(),
            capabilities: self.capabilities.clone(),
            expires: self.expires,
        }
    }
    /// Host diagnostic counters only. Reading them does not check or confer live authority.
    pub fn usage(&self) -> Usage {
        self.context
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .usage
    }
}
/// Host-owned reservation. Stop invalidates authority immediately; Drop releases
/// concurrent capacity, not cumulative bytes. This is not a guest-visible resource handle.
pub struct IoLease {
    binding: IoBinding,
    resources: u32,
    bytes: u64,
    capability: IoCapability,
}
impl IoLease {
    /// Keep an idle resource without consuming a concurrent job slot.
    pub(crate) fn into_resource(mut self) -> IoResourceLease {
        let held = IoResourceLease {
            binding: self.binding.duplicate(),
            resources: self.resources,
        };
        self.resources = 0;
        held
    }

    pub fn check(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.binding.check(manager, host, instance, now)
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn capability(&self) -> IoCapability {
        self.capability
    }
}
impl Drop for IoLease {
    fn drop(&mut self) {
        let mut state = self
            .binding
            .context
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        state.usage.resources -= self.resources;
        state.usage.jobs -= 1;
    }
}

/// Private held capacity; it carries the original live identity and expiry.
pub(crate) struct IoResourceLease {
    binding: IoBinding,
    resources: u32,
}
impl IoResourceLease {
    pub(crate) fn binding(&self) -> &IoBinding {
        &self.binding
    }
    pub(crate) fn preflight(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.binding.preflight(manager, host, instance, now)
    }

    pub(crate) fn delivery_guard(&self) -> IoBinding {
        self.binding.duplicate()
    }
    pub(crate) fn check(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.binding.check(manager, host, instance, now)
    }
    pub(crate) fn reclaimable(&self, now: u64) -> bool {
        self.binding.reclaimable(now)
    }
}
impl Drop for IoResourceLease {
    fn drop(&mut self) {
        let mut state = self
            .binding
            .context
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        state.usage.resources -= self.resources;
    }
}

/// One managed job slot, held from queue admission through final read/drop.
/// Incremental charges share the original instance's nonrefundable byte budget.
/// This lease does not itself grant a file, origin, credential or listener.
pub(crate) struct IoJobLease {
    binding: IoBinding,
    limit: u64,
    bytes: Mutex<u64>,
}
impl IoJobLease {
    pub(crate) fn binding(&self) -> &IoBinding {
        &self.binding
    }
    /// Reserve only the response ceiling and one held call resource. The worker
    /// already charged the exact request frame, and the parent owns the job slot.
    /// Validation and checked accounting precede every mutation of either budget.
    pub(crate) fn reserve_call(
        self: &Arc<Self>,
        host: &HostRuntime,
        instance: &ManagedInstance,
        command: &Command,
        request: &Request,
        cancel: Cancellation,
        now: u64,
    ) -> Result<IoCallLease> {
        self.binding.validate_owner(host, instance)?;
        check_cancel(&cancel)?;
        let Action::SubmitHttp(http) = request.action() else {
            return Err(Error::Denied);
        };
        if command.package_sha256 != instance.package().package().digest()
            || command.protocol_sha256 != io::schema_digest()
            || command.request_sha256 != request.digest()
            || command.request_bytes != request.bytes().len() as u64
            || command.operation_id.as_bytes() != http.operation_id.as_slice()
            || command.capability != IoCapability::HttpRequest
            || !self
                .binding
                .capabilities
                .contains(&IoCapability::HttpRequest)
            || (!http.credential.is_empty()
                && !self
                    .binding
                    .capabilities
                    .contains(&IoCapability::CredentialUse))
        {
            return Err(Error::Denied);
        }
        if command.response_limit == 0 || command.response_limit > io::MAX_FRAME_BYTES as u64 {
            return Err(Error::Limit);
        }
        let mut job = self.bytes.lock().map_err(|_| Error::Denied)?;
        let mut state = self
            .binding
            .context
            .state
            .lock()
            .map_err(|_| Error::Denied)?;
        self.binding.validate_time(&mut state, now)?;
        let next_job = job
            .checked_add(command.response_limit)
            .ok_or(Error::Limit)?;
        let next_bytes = state
            .usage
            .bytes
            .checked_add(command.response_limit)
            .ok_or(Error::Limit)?;
        let next_resources = state.usage.resources.checked_add(1).ok_or(Error::Limit)?;
        if next_job > self.limit
            || next_bytes > self.binding.context.budget.max_bytes
            || next_resources > self.binding.context.budget.max_resources
        {
            return Err(Error::Limit);
        }
        // Cancellation or revocation during validation must not create a reservation.
        check_cancel(&cancel)?;
        self.binding.validate_owner(host, instance)?;
        let command = command.clone();
        *job = next_job;
        state.usage.bytes = next_bytes;
        state.usage.resources = next_resources;
        state.last_tick = now;
        Ok(IoCallLease {
            job: Arc::clone(self),
            command,
            cancel,
        })
    }
    pub(crate) fn charge(&self, capabilities: &[IoCapability], bytes: u64, now: u64) -> Result<()> {
        if capabilities
            .iter()
            .any(|c| !self.binding.capabilities.contains(c))
        {
            return Err(Error::Denied);
        }
        let mut job = self.bytes.lock().map_err(|_| Error::Denied)?;
        let mut state = self
            .binding
            .context
            .state
            .lock()
            .map_err(|_| Error::Denied)?;
        self.binding.validate_time(&mut state, now)?;
        if self.binding.manager.upgrade().is_none()
            || !self.binding.control.upgrade().is_some_and(|c| c.active())
        {
            return Err(Error::Denied);
        }
        let next_job = job.checked_add(bytes).ok_or(Error::Limit)?;
        let total = state.usage.bytes.checked_add(bytes).ok_or(Error::Limit)?;
        if next_job > self.limit || total > self.binding.context.budget.max_bytes {
            return Err(Error::Limit);
        }
        *job = next_job;
        state.usage.bytes = total;
        state.last_tick = now;
        Ok(())
    }
}
impl Drop for IoJobLease {
    fn drop(&mut self) {
        let mut state = self
            .binding
            .context
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        state.usage.jobs -= 1;
    }
}

fn check_cancel(cancel: &Cancellation) -> Result<()> {
    match cancel.fault() {
        None => Ok(()),
        Some(Fault::Deadline) => Err(Error::Expired),
        Some(_) => Err(Error::Denied),
    }
}

/// One immutable HTTP command reserved under its original managed job. Retaining
/// a call also retains that job's slot; dropping never refunds cumulative bytes.
pub(crate) struct IoCallLease {
    job: Arc<IoJobLease>,
    command: Command,
    cancel: Cancellation,
}
impl IoCallLease {
    pub(crate) fn binding(&self) -> &IoBinding {
        &self.job.binding
    }
    pub(crate) fn command(&self) -> &Command {
        &self.command
    }
    pub(crate) fn check(&self, now: u64) -> Result<()> {
        check_cancel(&self.cancel)?;
        self.binding().check_liveness(now)?;
        check_cancel(&self.cancel)
    }
    pub(crate) fn validate_owner(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        self.binding().validate_owner(host, instance)
    }
}
impl Drop for IoCallLease {
    fn drop(&mut self) {
        let mut state = self
            .job
            .binding
            .context
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        state.usage.resources -= 1;
    }
}
