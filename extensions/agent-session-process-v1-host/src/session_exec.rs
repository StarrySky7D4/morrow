//! Explicit original R2 import profile. It never falls back to the process profile.
use crate::*;
use morrow_plugin_runtime::manager::{Manager, ManagedAgentSessionExecInstance};

pub struct PreparedSessionExecPackage {
    package: AgentProcessPackage,
    runner: Runner,
    limits: Limits,
}
impl PreparedSessionExecPackage {
    pub fn new(package: AgentProcessPackage, limits: Limits) -> Result<Self> {
        if limits.fuel == 0 || limits.fuel > 100_000_000
            || limits.memory_bytes < 65536 || limits.memory_bytes > 64 * 1024 * 1024
            || limits.host_calls > 1024 { return Err(Error::Limit); }
        let budget = package.base().manifest().budget.as_ref().ok_or(Error::Invalid)?;
        let limits = Limits { fuel: limits.fuel.min(budget.fuel),
            memory_bytes: limits.memory_bytes.min(budget.memory_bytes as usize),
            host_calls: limits.host_calls.min(budget.host_calls) };
        let runner = Runner::new_agent_session_exec_task(package.base().module(), limits)
            .map_err(|_| Error::Contract)?;
        Ok(Self { package, runner, limits })
    }
    pub fn package(&self) -> &AgentProcessPackage { &self.package }
}

/// One original managed connection, admitted separately under full-wrapper ceilings.
pub struct ManagedPreparedSessionExecPackage {
    prepared: PreparedSessionExecPackage,
    instance: ManagedAgentSessionExecInstance,
    admission: Admission,
    revision: u64,
    generation: u64,
    owner: HostBinding,
    capabilities: SessionCapabilities,
}
impl ManagedPreparedSessionExecPackage {
    #[allow(clippy::too_many_arguments)]
    pub fn connect(package: AgentProcessPackage, manager: &mut Manager,
        runtime: &mut HostRuntime, host: &SessionExecHost, revision: u64,
        review_sha256: [u8; 32], session: SessionCapabilities, sessions: Vec<String>,
        expires: u64, now: u64) -> Result<Self> {
        let declaration = package.declaration();
        if package.review_sha256() != review_sha256 || !session.session_read
            || !subset_session(session, declaration.session) || sessions.is_empty()
            || sessions.windows(2).any(|s| s[0] >= s[1])
            || sessions.iter().any(|s| !declaration.sessions.contains(s)) {
            return Err(Error::Denied);
        }
        let id = package.base().manifest().package_id.clone();
        if manager.selection(&id).is_none_or(|s| s.digest != package.base_sha256()) {
            return Err(Error::Denied);
        }
        let instance = manager.connect_agent_session_exec(&id, runtime, revision)
            .map_err(|_| Error::Denied)?;
        let result = (|| {
            let prepared = PreparedSessionExecPackage::new(package, instance.limits())?;
            manager.validate_agent_session_exec(&instance, runtime, revision)
                .map_err(|_| Error::Denied)?;
            if instance.package().digest() != prepared.package.base_sha256()
                || instance.shared_connection().package_digest() != Some(prepared.package.base_sha256()) {
                return Err(Error::Denied);
            }
            let generation = host.generation(runtime)?;
            let admission = host.admit(runtime, &instance.shared_connection(),
                prepared.package.declaration().session, session, sessions,
                prepared.package.declaration().execution_domain.clone(), expires, now)?;
            if manager.validate_agent_session_exec(&instance, runtime, revision).is_err() {
                let _ = host.revoke(&admission);
                return Err(Error::Denied);
            }
            Ok((prepared, admission, generation))
        })();
        match result {
            Ok((prepared, admission, generation)) => Ok(Self {
                prepared, instance, admission, revision, generation, owner: runtime.binding(),
                capabilities: session }),
            Err(error) => { let _ = instance.close(runtime); Err(error) }
        }
    }
    pub fn package(&self) -> &AgentProcessPackage { self.prepared.package() }
    pub fn instance(&self) -> &ManagedAgentSessionExecInstance { &self.instance }
    pub fn generation(&self) -> u64 { self.generation }
    pub fn limits(&self) -> Limits { self.prepared.limits }
    pub fn shared_connection(&self) -> Arc<Connection> { self.instance.shared_connection() }
    pub fn session_capabilities(&self) -> SessionCapabilities { self.capabilities }
    fn validate(&self, manager: &Manager, runtime: &HostRuntime) -> Result<()> {
        if runtime.binding() != self.owner { return Err(Error::Denied); }
        manager.validate_agent_session_exec(&self.instance, runtime, self.revision)
            .map_err(|_| Error::Denied)
    }
    pub fn request_stop(&self) { self.instance.request_stop(); }
    pub fn revocation(&self, runtime: &HostRuntime) -> Result<morrow_core::lifecycle::Revocation> {
        if runtime.binding() != self.owner { return Err(Error::Denied); }
        runtime.revocation(&self.shared_connection()).map_err(|_| Error::Denied)
    }
    /// Pure original read admission check, valid after Claim without replaying it.
    pub fn validate_tool_observation(&self, manager: &Manager, runtime: &HostRuntime,
        host: &SessionExecHost, operation: &str, proposal_sha256: [u8; 32], now: u64)
        -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.validate(manager, runtime)?;
        if !self.capabilities.session_read || !self.capabilities.propose
            || host.generation(runtime)? != self.generation { return Err(Error::Denied); }
        let observation = host.inspect_tool_record(runtime, operation)?;
        let identity = &observation.identity;
        if identity.operation_id != operation || identity.proposal_sha256 != proposal_sha256
            || identity.generation != self.generation
            || identity.execution_domain != self.prepared.package.declaration().execution_domain
            || !self.prepared.package.declaration().sessions.contains(&identity.session_id) {
            return Err(Error::Denied);
        }
        let _ = host.inspect_tool_observation(runtime, &self.shared_connection(),
            &self.admission, operation, now)?;
        self.validate(manager, runtime)?;
        Ok(observation)
    }
    fn finish(&self, mut run: TaskRun, active: bool) -> TaskRun {
        if !active {
            self.request_stop();
            if let Some(bytes) = &mut run.completion { bytes.fill(0); }
            run.completion = None;
            if run.report.outcome.is_ok() {
                run.report.outcome = Err(morrow_plugin_runtime::Fault::InactiveConnection);
            }
        }
        run
    }
    /// Dispatch exactly the original canonical R2 frame. No process imports are accepted.
    pub fn run(&self, manager: &Manager, runtime: &mut HostRuntime,
        host: &SessionExecHost, input: &[u8], mut clock: impl FnMut() -> u64) -> Result<TaskRun> {
        self.validate(manager, runtime)?;
        let mut route = |bytes: &[u8]| -> std::result::Result<Vec<u8>, ()> {
            self.validate(manager, runtime).map_err(|_| ())?;
            let request = Request::decode(bytes).map_err(|_| ())?;
            let raw = host.dispatch(runtime, &self.shared_connection(), &self.admission, bytes, || {
                let now = clock();
                if !self.instance.is_active() { self.request_stop(); }
                now
            }).map_err(|_| ())?;
            Reply::decode_for(&request, &raw).map_err(|_| ())?;
            self.validate(manager, runtime).map_err(|_| ())?;
            Ok(raw)
        };
        let run = self.prepared.runner.run_agent_session_exec_task(input, &mut route,
            self.instance.cancellation());
        Ok(self.finish(run, self.validate(manager, runtime).is_ok()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_owned<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(&self,
        owner: &mut O, host: &SessionExecHost, input: &[u8], mut clock: impl FnMut() -> u64)
        -> Result<TaskRun> {
        owner.with_managed_runtime(|m, r| self.validate(m, r)).ok_or(Error::Denied)??;
        let mut route = |bytes: &[u8]| -> std::result::Result<Vec<u8>, ()> {
            owner.with_managed_runtime(|m, r| self.validate(m, r)).ok_or(())?.map_err(|_| ())?;
            owner.prepare_io().map_err(|_| ())?;
            owner.with_managed_runtime(|manager, runtime| {
                self.validate(manager, runtime).map_err(|_| ())?;
                let request = Request::decode(bytes).map_err(|_| ())?;
                let raw = host.dispatch(runtime, &self.shared_connection(), &self.admission, bytes, || {
                    let now = clock();
                    if !self.instance.is_active() { self.request_stop(); }
                    now
                }).map_err(|_| ())?;
                Reply::decode_for(&request, &raw).map_err(|_| ())?;
                self.validate(manager, runtime).map_err(|_| ())?;
                Ok(raw)
            }).ok_or(())?
        };
        let run = self.prepared.runner.run_agent_session_exec_task(input, &mut route,
            self.instance.cancellation());
        let active = owner.with_managed_runtime(|m, r| self.validate(m, r).is_ok()).unwrap_or(false);
        Ok(self.finish(run, active))
    }
    pub fn close(&self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        self.request_stop();
        if runtime.binding() != self.owner { return Err(Error::Denied); }
        let revoked = host.revoke(&self.admission);
        let closed = self.instance.close(runtime).map_err(|_| Error::Denied);
        revoked.and(closed)
    }
}
