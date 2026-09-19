//! Trusted broker admission only: no file/network execution, resource path or guest grant.
//! Every delivery must check its lease again. Reservations do not prove an external effect.
use crate::manager::{Control, ManagedInstance, Manager};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    plugin_package::io::{IoCapability, proto::IoBudget},
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, Weak},
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
}
/// One context per actual managed instance, owned by its Control, never by a bind request.
pub(crate) struct IoContext {
    budget: IoBudget,
    state: Mutex<State>,
}
impl IoContext {
    pub(crate) fn new(budget: &IoBudget) -> Self {
        Self {
            budget: *budget,
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
    pub(crate) fn new(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        capabilities: BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
    ) -> Result<Self> {
        let control = instance.io_control();
        let context = control.io.as_ref().ok_or(Error::Denied)?.clone();
        let duration = expires
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or(Error::Expired)?;
        if duration > context.budget.max_duration_ms {
            return Err(Error::Limit);
        }
        let mut state = context.state.lock().map_err(|_| Error::Denied)?;
        if now < state.last_tick {
            return Err(Error::Clock);
        }
        if !control.active() {
            return Err(Error::Denied);
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
    fn validate_time(&self, state: &State, now: u64) -> Result<()> {
        if now < state.last_tick {
            return Err(Error::Clock);
        }
        if now >= self.expires {
            return Err(Error::Expired);
        }
        Ok(())
    }
    /// Validate before examining another resource, without advancing shared accounting.
    pub(crate) fn preflight(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.validate_identity(manager, host, instance)?;
        let state = self.context.state.lock().map_err(|_| Error::Denied)?;
        self.validate_time(&state, now)
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
        self.validate_time(&state, now)?;
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
        self.validate_time(&state, now)?;
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
        self.validate_time(&state, now)?;
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
    pub(crate) fn reclaimable(&self, now: u64) -> bool {
        now >= self.expires
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
