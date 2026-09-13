//! Trusted automatic activation and shared provider ownership. No task is automatically retried.
use crate::{
    Cancellation,
    dynamic_dependencies::{self, Context, GraphLimits, RoutedOutput},
    manager::{ActivationPlan, ManagedInstance, Manager, ManagerError, OptionalDependency},
    shared_objects::SharedObjects,
};
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{GrantKind, Revocation},
    task::Invocation,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Manager(ManagerError),
    Execution(crate::dependency::Error),
    Denied,
    Limit,
    RetryBudget,
    Cooldown,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "instance pool: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl From<ManagerError> for Error {
    fn from(e: ManagerError) -> Self {
        Self::Manager(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_sessions: usize,
    pub max_providers: usize,
    pub max_restarts: u32,
    pub retry_delay: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_sessions: 16,
            max_providers: 64,
            max_restarts: 3,
            retry_delay: 1,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub sessions: usize,
    pub providers: usize,
}
struct State {
    live: AtomicBool,
    binding: ConnectionBinding,
    revocation: Revocation,
    cancel: Cancellation,
}
impl State {
    fn stop(&self) {
        self.live.store(false, Ordering::Release);
        self.revocation.revoke();
        self.cancel.cancel();
    }
    fn active(&self) -> bool {
        self.live.load(Ordering::Acquire) && !self.revocation.is_revoked()
    }
}
/// Opaque single-owner lease. Dropping it immediately revokes only this session's root.
pub struct Session {
    id: u64,
    state: Arc<State>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.state.stop();
    }
}
struct Entry {
    root: ManagedInstance,
    plan: ActivationPlan,
    state: Weak<State>,
    closed: bool,
    restarts: u32,
    last_attempt: Option<u64>,
}
impl Entry {
    fn active(&self) -> bool {
        self.state.upgrade().is_some_and(|s| s.active())
    }
    fn stop(&self) {
        if let Some(s) = self.state.upgrade() {
            s.stop();
        }
        self.root.stop();
    }
}
/// The pool is tied to one actual HostRuntime. Call maintain/close/close_all to release host slots;
/// Drop still revokes instances, but cannot borrow the external host to disconnect its records.
pub struct Pool {
    host: HostBinding,
    manager: Option<Weak<()>>,
    limits: Limits,
    next: u64,
    providers: BTreeMap<String, ManagedInstance>,
    sessions: BTreeMap<u64, Entry>,
    last_tick: u64,
}
fn disconnect(host: &mut HostRuntime, instance: &ManagedInstance) -> Result<()> {
    instance.stop();
    if host.connection_phase(instance.connection()).is_ok() {
        instance.close(host)?;
    }
    Ok(())
}
impl Pool {
    pub fn new(host: &HostRuntime, limits: Limits) -> Result<Self> {
        if limits.max_sessions == 0
            || limits.max_sessions > 16
            || limits.max_providers == 0
            || limits.max_providers > 64
            || limits.max_restarts > 8
        {
            return Err(Error::Limit);
        }
        Ok(Self {
            host: host.binding(),
            manager: None,
            limits,
            next: 1,
            providers: BTreeMap::new(),
            sessions: BTreeMap::new(),
            last_tick: 0,
        })
    }
    fn check_host(&self, host: &HostRuntime) -> Result<()> {
        if host.binding() != self.host {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
    fn check_manager(&self, manager: &Manager) -> Result<()> {
        if self
            .manager
            .as_ref()
            .is_some_and(|bound| !Weak::ptr_eq(bound, &manager.identity()))
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn entry(&self, session: &Session) -> Result<&Entry> {
        let e = self.sessions.get(&session.id).ok_or(Error::Denied)?;
        if !e
            .state
            .upgrade()
            .is_some_and(|s| Arc::ptr_eq(&s, &session.state))
            || e.root.connection().binding() != session.state.binding
        {
            return Err(Error::Denied);
        }
        Ok(e)
    }
    pub fn root(&self, session: &Session) -> Result<&ManagedInstance> {
        let e = self.entry(session)?;
        if !e.active() {
            return Err(Error::Denied);
        }
        Ok(&e.root)
    }
    pub fn provider(&self, id: &str) -> Option<&ManagedInstance> {
        self.providers.get(id)
    }
    pub fn usage(&self) -> Usage {
        Usage {
            sessions: self.sessions.len(),
            providers: self.providers.len(),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn grant_root(
        &mut self,
        host: &mut HostRuntime,
        session: &Session,
        kind: GrantKind,
        scope: &str,
        expires: u64,
        now: u64,
    ) -> Result<()> {
        self.check_host(host)?;
        self.root(session)?;
        host.grant(
            self.sessions
                .get_mut(&session.id)
                .ok_or(Error::Denied)?
                .root
                .parts_mut()
                .1,
            kind,
            scope,
            expires,
            now,
        )?;
        Ok(())
    }
    pub fn start(
        &mut self,
        manager: &mut Manager,
        host: &mut HostRuntime,
        root: &str,
        optional: &[OptionalDependency],
        revision: u64,
    ) -> Result<Session> {
        self.check_host(host)?;
        self.check_manager(manager)?;
        let plan = manager.activation_plan(root, optional, revision)?;
        self.maintain(manager, host)?;
        self.start_plan(manager, host, plan, None, 0, None)
    }
    fn start_plan(
        &mut self,
        manager: &mut Manager,
        host: &mut HostRuntime,
        plan: ActivationPlan,
        replace: Option<u64>,
        restarts: u32,
        last_attempt: Option<u64>,
    ) -> Result<Session> {
        if manager.revision() != plan.revision() {
            return Err(morrow_core::Error::RevisionConflict.into());
        }
        if self.sessions.len() >= self.limits.max_sessions && replace.is_none() {
            return Err(Error::Limit);
        }
        let missing: Vec<_> = plan
            .packages()
            .iter()
            .filter(|p| p.id != plan.root() && !self.providers.contains_key(&p.id))
            .collect();
        if self.providers.len() + missing.len() > self.limits.max_providers {
            return Err(Error::Limit);
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::Limit)?;
        let mut prepared: Vec<(String, ManagedInstance)> = Vec::new();
        let result = (|| -> Result<ManagedInstance> {
            for package in missing {
                prepared.push((package.id.clone(), manager.connect(&package.id, host)?));
            }
            Ok(manager.connect(plan.root(), host)?)
        })();
        let root = match result {
            Ok(root) => root,
            Err(error) => {
                for (_, p) in prepared.iter().rev() {
                    let _ = disconnect(host, p);
                }
                return Err(error);
            }
        };
        let revocation = match host.revocation(root.connection()) {
            Ok(v) => v,
            Err(error) => {
                let _ = disconnect(host, &root);
                for (_, p) in prepared.iter().rev() {
                    let _ = disconnect(host, p);
                }
                return Err(error.into());
            }
        };
        let state = Arc::new(State {
            live: AtomicBool::new(true),
            binding: root.connection().binding(),
            revocation,
            cancel: root.cancellation(),
        });
        for (id, p) in prepared {
            self.providers.insert(id, p);
        }
        self.manager = Some(manager.identity());
        self.sessions.insert(
            id,
            Entry {
                root,
                plan,
                state: Arc::downgrade(&state),
                closed: false,
                restarts,
                last_attempt,
            },
        );
        if let Some(old) = replace {
            self.remove_entry(host, old)?;
        }
        Ok(Session { id, state })
    }
    fn remove_entry(&mut self, host: &mut HostRuntime, id: u64) -> Result<()> {
        if let Some(mut e) = self.sessions.remove(&id) {
            e.stop();
            if !e.closed {
                disconnect(host, &e.root)?;
                e.closed = true;
            }
        }
        Ok(())
    }
    fn release_unused(&mut self, host: &mut HostRuntime) -> Result<()> {
        let wanted: BTreeSet<_> = self
            .sessions
            .values()
            .filter(|e| e.active())
            .flat_map(|e| {
                e.plan
                    .packages()
                    .iter()
                    .filter(move |p| p.id != e.plan.root())
                    .map(|p| p.id.clone())
            })
            .collect();
        let unused: Vec<_> = self
            .providers
            .keys()
            .filter(|id| !wanted.contains(*id))
            .cloned()
            .collect();
        for id in unused {
            if let Some(p) = self.providers.remove(&id) {
                disconnect(host, &p)?;
            }
        }
        Ok(())
    }
    /// Observe actual managed state, stop required consumers, then release only unshared providers.
    pub fn maintain(&mut self, manager: &Manager, host: &mut HostRuntime) -> Result<()> {
        self.check_host(host)?;
        self.check_manager(manager)?;
        let mut bad: BTreeSet<String> = self
            .providers
            .iter()
            .filter(|(_, p)| manager.validate_instance(host, p).is_err())
            .map(|(id, _)| id.clone())
            .collect();
        loop {
            let before = bad.len();
            for e in self.sessions.values().filter(|e| e.active()) {
                for (caller, provider) in e.plan.required_edges() {
                    if bad.contains(provider) {
                        bad.insert(caller.clone());
                    }
                }
            }
            if bad.len() == before {
                break;
            }
        }
        for id in &bad {
            if let Some(p) = self.providers.get(id) {
                p.stop();
            }
        }
        let mut abandoned = Vec::new();
        for (id, e) in &mut self.sessions {
            if !e.active()
                || manager.validate_instance(host, &e.root).is_err()
                || e.plan
                    .required()
                    .iter()
                    .any(|p| p != e.plan.root() && bad.contains(p))
            {
                e.stop();
                if !e.closed {
                    disconnect(host, &e.root)?;
                    e.closed = true;
                }
            }
            if e.state.strong_count() == 0 {
                abandoned.push(*id);
            }
        }
        for id in abandoned {
            self.remove_entry(host, id)?;
        }
        for id in bad {
            if let Some(p) = self.providers.remove(&id) {
                disconnect(host, &p)?;
            }
        }
        self.release_unused(host)
    }
    pub fn close(&mut self, host: &mut HostRuntime, session: &Session) -> Result<()> {
        self.check_host(host)?;
        self.entry(session)?;
        self.remove_entry(host, session.id)?;
        self.release_unused(host)
    }
    pub fn close_all(&mut self, host: &mut HostRuntime) -> Result<()> {
        self.check_host(host)?;
        let ids: Vec<_> = self.sessions.keys().copied().collect();
        for id in ids {
            self.remove_entry(host, id)?;
        }
        self.release_unused(host)
    }
    /// Explicit recovery creates a fresh root and restores no object grants; it never reruns input.
    pub fn restart(
        &mut self,
        manager: &mut Manager,
        host: &mut HostRuntime,
        session: &Session,
        revision: u64,
        now: u64,
    ) -> Result<Session> {
        self.check_host(host)?;
        self.check_manager(manager)?;
        let old = self.entry(session)?;
        let plan = manager.activation_plan(old.plan.root(), old.plan.optionals(), revision)?;
        if now < self.last_tick {
            return Err(Error::Denied);
        }
        self.maintain(manager, host)?;
        let old = self.entry(session)?;
        if old.active() {
            return Err(Error::Denied);
        }
        if old.restarts >= self.limits.max_restarts {
            return Err(Error::RetryBudget);
        }
        if old.last_attempt.is_some_and(|last| {
            last.checked_add(self.limits.retry_delay)
                .is_none_or(|ready| now < ready)
        }) {
            return Err(Error::Cooldown);
        }
        let attempts = old.restarts + 1;
        self.last_tick = now;
        let old = self.sessions.get_mut(&session.id).ok_or(Error::Denied)?;
        old.restarts = attempts;
        old.last_attempt = Some(now);
        self.start_plan(manager, host, plan, Some(session.id), attempts, Some(now))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        objects: &mut SharedObjects,
        session: &Session,
        input: &Invocation,
        context: Context<'_>,
        limits: GraphLimits,
        clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<RoutedOutput> {
        self.maintain(manager, host)?;
        let entry = self.entry(session)?;
        if !entry.active() {
            return Err(Error::Denied);
        }
        let providers: Vec<_> = entry
            .plan
            .packages()
            .iter()
            .filter(|p| p.id != entry.plan.root())
            .filter_map(|p| self.providers.get(&p.id))
            .collect();
        let mut failed = None;
        let result = dynamic_dependencies::run_graph_tracking(
            manager,
            host,
            objects,
            &entry.root,
            &providers,
            input,
            context,
            limits,
            clock,
            cancel,
            &mut failed,
        );
        if let Some(binding) = failed {
            if let Some(p) = self
                .providers
                .values()
                .find(|p| p.connection().binding() == binding)
            {
                p.stop();
            }
            if let Some(e) = self.sessions.get(&session.id)
                && e.root.connection().binding() == binding
            {
                e.stop();
            }
        }
        // Include externally observed revocation even when the task ended with Denied/cancel.
        self.maintain(manager, host)?;
        result.map_err(Error::Execution)
    }
}
