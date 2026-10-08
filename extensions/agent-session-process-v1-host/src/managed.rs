//! Fixed profile bridge. Base selection and complete wrapper review are separate decisions.
use super::*;
use morrow_agent_process_control_v1::host::HostIdentity;
use morrow_plugin_runtime::manager::{ManagedAgentSessionProcessInstance, Manager};

/// Owns the original manager connection and its separate live R2 wrapper admission.
/// Persisted wrapper approval, workbench routing and protected owner integration are separate.
pub struct ManagedPreparedPackage {
    prepared: PreparedPackage,
    instance: ManagedAgentSessionProcessInstance,
    approved: Approved,
    revision: u64,
    process_host: RefCell<Option<HostIdentity>>,
}

impl PreparedPackage {
    /// Crate-private binding: accepts only an original validated managed instance, never a raw ID.
    #[allow(clippy::too_many_arguments)]
    fn approve_managed_connection(
        &self,
        manager: &Manager,
        instance: &ManagedAgentSessionProcessInstance,
        runtime: &HostRuntime,
        host: &SessionExecHost,
        revision: u64,
        review_sha256: [u8; 32],
        session: SessionCapabilities,
        process: ProcessCapabilities,
        sessions: Vec<String>,
        expires: u64,
        now: u64,
    ) -> Result<Approved> {
        manager
            .validate_agent_session_process(instance, runtime, revision)
            .map_err(|_| Error::Denied)?;
        let connection = instance.shared_connection();
        let ceilings = instance.limits();
        let d = &self.package.declaration;
        if instance.package().digest() != self.package.base.digest()
            || connection.package_digest() != Some(self.package.base.digest())
            || self.limits.fuel > ceilings.fuel
            || self.limits.memory_bytes > ceilings.memory_bytes
            || self.limits.host_calls > ceilings.host_calls
            || review_sha256 != self.package.review_sha256()
            || !session.session_read
            || !subset_session(session, d.session)
            || !subset_process(process, d.process)
            || sessions.is_empty()
            || sessions.iter().any(|s| !d.sessions.contains(s))
            || sessions
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != sessions.len()
        {
            return Err(Error::Denied);
        }
        let binding = random()?;
        let generation = host.generation(runtime)?;
        let admission = host.admit(
            runtime,
            &connection,
            d.session,
            session,
            sessions.clone(),
            d.execution_domain.clone(),
            expires,
            now,
        )?;
        let approved = Approved {
            connection,
            admission,
            owner: runtime.binding(),
            package: review_sha256,
            binding,
            generation,
            sessions,
            execution_domain: d.execution_domain.clone(),
            process,
            session,
            created: now,
            expires,
            probes: AtomicU64::new(1),
            processes: BTreeMap::new(),
        };
        if manager
            .validate_agent_session_process(instance, runtime, revision)
            .is_err()
        {
            let _ = host.revoke(&approved.admission);
            return Err(Error::Denied);
        }
        Ok(approved)
    }
}

impl ManagedPreparedPackage {
    /// Resolve the selected base under the original manager, then explicitly review the wrapper.
    /// All subsequent errors stop and disconnect that same connection; no second one is created.
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        package: AgentProcessPackage,
        manager: &mut Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        expected_registry_revision: u64,
        review_sha256: [u8; 32],
        session: SessionCapabilities,
        process: ProcessCapabilities,
        sessions: Vec<String>,
        expires: u64,
        now: u64,
    ) -> Result<Self> {
        let id = package.base.manifest().package_id.clone();
        if manager
            .selection(&id)
            .is_none_or(|s| s.digest != package.base.digest())
        {
            return Err(Error::Denied);
        }
        let instance = manager
            .connect_agent_session_process(&id, runtime, expected_registry_revision)
            .map_err(|_| Error::Denied)?;
        let prepared = match PreparedPackage::new(package, instance.limits()) {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = instance.close(runtime);
                return Err(error);
            }
        };
        let approved = match prepared.approve_managed_connection(
            manager,
            &instance,
            runtime,
            host,
            expected_registry_revision,
            review_sha256,
            session,
            process,
            sessions,
            expires,
            now,
        ) {
            Ok(approved) => approved,
            Err(error) => {
                let _ = instance.close(runtime);
                return Err(error);
            }
        };
        Ok(Self {
            prepared,
            instance,
            approved,
            revision: expected_registry_revision,
            process_host: RefCell::new(None),
        })
    }
    pub fn package(&self) -> &AgentProcessPackage {
        self.prepared.package()
    }
    pub fn instance(&self) -> &ManagedAgentSessionProcessInstance {
        &self.instance
    }
    pub fn shared_connection(&self) -> Arc<Connection> {
        self.approved.shared_connection()
    }
    pub fn limits(&self) -> Limits {
        self.prepared.limits
    }
    pub fn generation(&self) -> u64 {
        self.approved.generation
    }
    pub fn session_capabilities(&self) -> SessionCapabilities {
        self.approved.session
    }
    pub fn revocation(&self, runtime: &HostRuntime) -> Result<morrow_core::lifecycle::Revocation> {
        if runtime.binding() != self.approved.owner {
            return Err(Error::Denied);
        }
        runtime
            .revocation(&self.shared_connection())
            .map_err(|_| Error::Denied)
    }
    /// Pure read of the exact original record under the still-live approved scope.
    /// This works after Claim and does not execute or replay that mutation.
    pub fn validate_tool_observation(
        &self,
        manager: &Manager,
        runtime: &HostRuntime,
        host: &SessionExecHost,
        operation: &str,
        proposal_sha256: [u8; 32],
        now: u64,
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.validate(manager, runtime)?;
        let approved = &self.approved;
        if runtime.binding() != approved.owner
            || !approved.session.session_read
            || !approved.session.propose
            || now < approved.created
            || now >= approved.expires
            || host.generation(runtime)? != approved.generation
        {
            return Err(Error::Denied);
        }
        let observation = host.inspect_tool_record(runtime, operation)?;
        let identity = &observation.identity;
        if identity.operation_id != operation
            || identity.proposal_sha256 != proposal_sha256
            || identity.generation != approved.generation
            || identity.execution_domain != approved.execution_domain
            || !approved.sessions.contains(&identity.session_id)
        {
            return Err(Error::Denied);
        }
        host.inspect_tool_observation(
            runtime,
            &approved.connection,
            &approved.admission,
            operation,
            now,
        )?;
        self.validate(manager, runtime)?;
        // A concurrent fact/record change is not silently substituted into delivery.
        if host.inspect_tool_record(runtime, operation)? != observation {
            return Err(Error::Denied);
        }
        host.inspect_tool_observation(
            runtime,
            &approved.connection,
            &approved.admission,
            operation,
            now,
        )?;
        self.validate(manager, runtime)?;
        Ok(observation)
    }

    fn validate(&self, manager: &Manager, runtime: &HostRuntime) -> Result<()> {
        manager
            .validate_agent_session_process(&self.instance, runtime, self.revision)
            .map_err(|_| Error::Denied)
    }
    fn bind_process_host(&self, host: &ProcessHost) -> Result<()> {
        let mut original = self.process_host.borrow_mut();
        match original.as_ref() {
            Some(identity) if !identity.matches(host) => Err(Error::Denied),
            Some(_) => Ok(()),
            None => {
                *original = Some(host.identity());
                Ok(())
            }
        }
    }
    /// No task replay. Every imported frame revalidates the original managed instance.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.validate(manager, runtime)?;
        self.bind_process_host(processes)?;
        let mut run = self.prepared.run_guarded(
            runtime,
            host,
            &self.approved,
            processes,
            input,
            || {
                let now = clock();
                if !self.instance.is_active() {
                    self.instance.request_stop();
                }
                now
            },
            self.instance.cancellation(),
            |runtime| self.validate(manager, runtime).is_ok(),
        )?;
        if self.validate(manager, runtime).is_err() {
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        Ok(run)
    }
    /// Runs with maintenance on the same complete owner at each import.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub fn run_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        owner
            .with_managed_runtime(|manager, runtime| self.validate(manager, runtime))
            .ok_or(Error::Denied)??;
        self.bind_process_host(processes)?;
        let mut run = self.prepared.run_owned_guarded(
            owner,
            host,
            &self.approved,
            processes,
            input,
            || {
                let now = clock();
                if !self.instance.is_active() {
                    self.instance.request_stop();
                }
                now
            },
            self.instance.cancellation(),
            |manager, runtime| self.validate(manager, runtime).is_ok(),
        )?;
        let active = owner
            .with_managed_runtime(|manager, runtime| self.validate(manager, runtime).is_ok())
            .unwrap_or(false);
        if !active {
            self.instance.request_stop();
            if let Some(bytes) = &mut run.completion {
                bytes.fill(0);
            }
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        Ok(run)
    }
    fn ensure_session_only(&self) -> Result<()> {
        if package::process_bits(self.approved.process) != 0
            || !self.approved.processes.is_empty()
            || self.process_host.borrow().is_some()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn validate_native_session(&self, manager: &Manager,
        runtime: &HostRuntime, host: &SessionExecHost, now: u64) -> Result<()> {
        self.validate(manager, runtime)?;
        self.ensure_session_only()?;
        let a = &self.approved;
        if !a.session.session_read || !a.session.session_write || a.session.propose
            || a.session.execute || a.session.retire || runtime.binding() != a.owner
            || now < a.created || now >= a.expires || host.generation(runtime)? != a.generation
            || !host.admission_live_at(&a.admission, now) {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn open_native_session(&self, manager: &Manager, runtime: &HostRuntime,
        host: Arc<SessionExecHost>, now: u64,
        live: Arc<dyn Fn() -> bool + Send + Sync>) -> Result<(native_session::NativeSessionLease, Result<()>)> {
        self.validate_native_session(manager, runtime, &host, now)?;
        let a = &self.approved;
        Ok(native_session::NativeSessionLease::new(runtime, host, a.connection.clone(),
            a.generation, self.prepared.package.declaration.session, a.sessions.clone(),
            a.execution_domain.clone(), now, a.expires, live))
    }
    /// Closes only an unbound, zero-process-capability combined session lease.
    /// A lease with process ownership must use its original ProcessHost cleanup.
    pub fn close_session(&self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        self.request_stop();
        self.ensure_session_only()?;
        if runtime.binding() != self.approved.owner {
            return Err(Error::Denied);
        }
        let revoked = host.revoke(&self.approved.admission);
        let closed = self.instance.close(runtime).map_err(|_| Error::Denied);
        revoked.and(closed)
    }

    /// Combined profile, canonical R2-only route without a process capability host.
    pub fn run_session(
        &self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        self.validate(manager, runtime)?;
        self.ensure_session_only()?;
        let run = self.prepared.run_session_guarded(
            runtime,
            host,
            &self.approved,
            input,
            || {
                let now = clock();
                if !self.instance.is_active() {
                    self.instance.request_stop();
                }
                now
            },
            self.instance.cancellation(),
            |runtime| self.validate(manager, runtime).is_ok(),
        )?;
        Ok(self.finish_session_run(run, self.validate(manager, runtime).is_ok()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_session_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<TaskRun> {
        owner
            .with_managed_runtime(|m, r| self.validate(m, r))
            .ok_or(Error::Denied)??;
        self.ensure_session_only()?;
        let run = self.prepared.run_session_owned_guarded(
            owner,
            host,
            &self.approved,
            input,
            || {
                let now = clock();
                if !self.instance.is_active() {
                    self.instance.request_stop();
                }
                now
            },
            self.instance.cancellation(),
            |m, r| self.validate(m, r).is_ok(),
        )?;
        let active = owner
            .with_managed_runtime(|m, r| self.validate(m, r).is_ok())
            .unwrap_or(false);
        Ok(self.finish_session_run(run, active))
    }
    fn finish_session_run(&self, mut run: TaskRun, active: bool) -> TaskRun {
        if !active {
            self.instance.request_stop();
            if let Some(bytes) = &mut run.completion {
                bytes.fill(0);
            }
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        run
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_process(
        &mut self,
        manager: &Manager,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
        identity: ToolIdentity,
        executor_connection: Arc<Connection>,
        executor_admission: &Admission,
        capabilities: ProcessCapabilities,
        budget: Budget,
        provider: Box<dyn ProcessProvider>,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Handle> {
        self.validate(manager, runtime)?;
        self.bind_process_host(processes)?;
        let handle = self.prepared.register_process(
            runtime,
            host,
            &mut self.approved,
            processes,
            identity,
            executor_connection,
            executor_admission,
            capabilities,
            budget,
            provider,
            || {
                let now = clock();
                if !self.instance.is_active() {
                    self.instance.request_stop();
                }
                now
            },
        )?;
        if self.validate(manager, runtime).is_err() {
            let _ = processes.trusted_cleanup(handle);
            return Err(Error::Denied);
        }
        Ok(handle)
    }
    /// Cleanup discovery remains available after stop; it grants no guest access.
    pub fn owned_handles(&self, runtime: &HostRuntime) -> Result<Vec<Handle>> {
        self.prepared.owned_handles(runtime, &self.approved)
    }
    pub fn request_stop(&self) {
        self.instance.request_stop();
    }
    /// Request trusted provider cleanup and release the original bounded Core connection.
    /// Actual exit/EOF reconciliation and ProcessHost::finish remain the trusted owner's duty.
    pub fn close(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        processes: &mut ProcessHost,
    ) -> Result<()> {
        self.request_stop();
        if runtime.binding() != self.approved.owner {
            return Err(Error::Denied);
        }
        let same_process_host = self
            .process_host
            .borrow()
            .as_ref()
            .is_none_or(|identity| identity.matches(processes));
        let revoked = if same_process_host {
            self.prepared.revoke(host, &self.approved, processes)
        } else {
            let _ = host.revoke(&self.approved.admission);
            Err(Error::Denied)
        };
        let closed = self.instance.close(runtime).map_err(|_| Error::Denied);
        revoked.and(closed)
    }
}
