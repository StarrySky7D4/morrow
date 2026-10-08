//! Reviewed Wasm session/process routing on the original R2 ledger authority.
#![deny(unsafe_code)]
use morrow_agent_process_control_v1::{
    Capabilities as ProcessCapabilities, ReplyBody,
    host::{Binding, Budget, Handle, Host as ProcessHost, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Outcome, Reply, Request, Result,
    authority::{Admission, Capabilities as SessionCapabilities, SessionExecHost},
    safe_exec::ToolIdentity,
};
use morrow_core::{
    dispatch::{Connection, HostBinding, HostRuntime},
    plugin_package::Package,
};
use morrow_plugin_runtime::{Cancellation, Limits, Runner, TaskRun};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
pub mod catalog;
pub mod native_session;
mod managed;
mod session_exec;
mod package;
pub use managed::ManagedPreparedPackage;
pub use session_exec::{ManagedPreparedSessionExecPackage, PreparedSessionExecPackage};
pub use package::{AgentProcessPackage, Declaration, MAX_ARCHIVE_BYTES};

#[allow(clippy::too_many_arguments)]
fn route_frame(
    runtime: &mut HostRuntime,
    host: &SessionExecHost,
    approved: &Approved,
    processes: &mut ProcessHost,
    bytes: &[u8],
    mut clock: impl FnMut() -> u64,
    mut managed_live: impl FnMut(&HostRuntime) -> bool,
) -> std::result::Result<Vec<u8>, ()> {
    if !managed_live(runtime) {
        return Err(());
    }
    if let Ok(request) = Request::decode(bytes) {
        let reply = host
            .dispatch(
                runtime,
                &approved.connection,
                &approved.admission,
                bytes,
                &mut clock,
            )
            .map_err(|_| ())?;
        Reply::decode_for(&request, &reply).map_err(|_| ())?;
        if !managed_live(runtime) {
            return Err(());
        }
        return Ok(reply);
    }
    let request = morrow_agent_process_control_v1::Request::decode(bytes).map_err(|_| ())?;
    let clock = RefCell::new(&mut clock);
    let binding_seen = RefCell::new(None::<Binding>);
    let body = processes
        .dispatch(
            approved.binding,
            &request,
            || (**clock.borrow_mut())(),
            |binding| {
                *binding_seen.borrow_mut() = Some(binding.clone());
                approved
                    .processes
                    .get(&request.handle)
                    .is_some_and(|execution| {
                        managed_live(runtime)
                            && binding.connection == approved.binding
                            && binding.generation == approved.generation
                            && binding.session_id == execution.identity.session_id
                            && binding.operation_id == execution.identity.operation_id
                            && approved
                                .live_tool(runtime, host, execution, &mut **clock.borrow_mut())
                                .is_ok()
                            && managed_live(runtime)
                    })
            },
        )
        .unwrap_or_else(ReplyBody::Rejected);
    let success = !matches!(body, ReplyBody::Rejected(_));
    let reply = morrow_agent_process_control_v1::Reply::new(&request, body)
        .map_err(|_| ())?
        .encode()
        .map_err(|_| ())?;
    if success {
        let live = binding_seen.borrow().as_ref().is_some_and(|binding| {
            managed_live(runtime)
                && binding.connection == approved.binding
                && approved
                    .processes
                    .get(&request.handle)
                    .is_some_and(|execution| {
                        approved
                            .live_tool(runtime, host, execution, &mut **clock.borrow_mut())
                            .is_ok()
                            && managed_live(runtime)
                    })
        });
        if !live {
            if request.action.is_mutation() {
                let _ = processes.veto_delivery(&request);
            }
            let error = if request.action.is_mutation() {
                morrow_agent_process_control_v1::Error::Unknown
            } else {
                morrow_agent_process_control_v1::Error::Denied
            };
            return morrow_agent_process_control_v1::Reply::new(
                &request,
                ReplyBody::Rejected(error),
            )
            .map_err(|_| ())?
            .encode()
            .map_err(|_| ());
        }
    }
    Ok(reply)
}

/// Exact R2 wire dispatch on the original connection/admission; never falls back.
fn route_session_frame(
    runtime: &mut HostRuntime,
    host: &SessionExecHost,
    approved: &Approved,
    bytes: &[u8],
    mut clock: impl FnMut() -> u64,
    mut managed_live: impl FnMut(&HostRuntime) -> bool,
) -> std::result::Result<Vec<u8>, ()> {
    let request = Request::decode(bytes).map_err(|_| ())?;
    if runtime.binding() != approved.owner || !managed_live(runtime) {
        return Err(());
    }
    let reply = host
        .dispatch(
            runtime,
            &approved.connection,
            &approved.admission,
            bytes,
            &mut clock,
        )
        .map_err(|_| ())?;
    Reply::decode_for(&request, &reply).map_err(|_| ())?;
    if !managed_live(runtime) {
        return Err(());
    }
    Ok(reply)
}

fn random() -> Result<[u8; 32]> {
    let mut value = [0; 32];
    getrandom::fill(&mut value).map_err(|_| Error::Storage)?;
    if value == [0; 32] {
        return Err(Error::Storage);
    }
    Ok(value)
}
fn subset_session(a: SessionCapabilities, b: SessionCapabilities) -> bool {
    package::session_bits(a) & !package::session_bits(b) == 0
}
fn subset_process(a: ProcessCapabilities, b: ProcessCapabilities) -> bool {
    package::process_bits(a) & !package::process_bits(b) == 0
}

/// Opaque original authority. Neither serialized identifiers nor package metadata
/// can construct this approval or reconstitute it after host restart.
pub struct Approved {
    connection: Arc<Connection>,
    admission: Admission,
    owner: HostBinding,
    package: [u8; 32],
    binding: [u8; 32],
    generation: u64,
    sessions: Vec<String>,
    execution_domain: String,
    process: ProcessCapabilities,
    session: SessionCapabilities,
    created: u64,
    expires: u64,
    probes: AtomicU64,
    processes: BTreeMap<[u8; 32], ExecutionBinding>,
}
struct ExecutionBinding {
    identity: ToolIdentity,
    connection: Arc<Connection>,
    admission: Admission,
}
impl Approved {
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn shared_connection(&self) -> Arc<Connection> {
        self.connection.clone()
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    fn probe_id(&self) -> Result<String> {
        let sequence = self
            .probes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let prefix = self
            .binding
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(format!("process-review-{prefix}-{sequence}"))
    }
    fn live_tool(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        execution: &ExecutionBinding,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<()> {
        let now = clock();
        let expected = &execution.identity;
        if runtime.binding() != self.owner
            || now < self.created
            || now >= self.expires
            || expected.generation != self.generation
            || expected.execution_domain != self.execution_domain
            || !self.sessions.contains(&expected.session_id)
        {
            return Err(Error::Denied);
        }
        host.validate_started_tool(
            runtime,
            &execution.connection,
            &execution.admission,
            expected,
            &mut *clock,
        )?;
        let request = Request::new_for_generation(
            self.probe_id()?,
            self.generation,
            Action::Inspect {
                operation_id: expected.operation_id.clone(),
            },
        )?;
        let bytes = host.dispatch(
            runtime,
            &self.connection,
            &self.admission,
            request.raw(),
            &mut *clock,
        )?;
        if !matches!(
            Reply::decode_for(&request, &bytes)?.outcome,
            Outcome::Tool(_)
        ) {
            return Err(Error::Denied);
        }
        host.validate_started_tool(
            runtime,
            &execution.connection,
            &execution.admission,
            expected,
            clock,
        )
    }
}

pub struct PreparedPackage {
    package: AgentProcessPackage,
    runner: Runner,
    limits: Limits,
}
impl PreparedPackage {
    pub fn new(package: AgentProcessPackage, limits: Limits) -> Result<Self> {
        if limits.fuel == 0
            || limits.fuel > 100_000_000
            || limits.memory_bytes < 65536
            || limits.memory_bytes > 64 * 1024 * 1024
            || limits.host_calls > 1024
        {
            return Err(Error::Limit);
        }
        let budget = package
            .base
            .manifest()
            .budget
            .as_ref()
            .ok_or(Error::Invalid)?;
        let limits = Limits {
            fuel: limits.fuel.min(budget.fuel),
            memory_bytes: limits.memory_bytes.min(budget.memory_bytes as usize),
            host_calls: limits.host_calls.min(budget.host_calls),
        };
        let runner = Runner::new_agent_session_process_task(package.base.module(), limits)
            .map_err(|_| Error::Contract)?;
        Ok(Self {
            package,
            runner,
            limits,
        })
    }
    pub fn package(&self) -> &AgentProcessPackage {
        &self.package
    }
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        review_sha256: [u8; 32],
        session: SessionCapabilities,
        process: ProcessCapabilities,
        sessions: Vec<String>,
        expires: u64,
        now: u64,
    ) -> Result<Approved> {
        let d = &self.package.declaration;
        if review_sha256 != self.package.review_sha256()
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
        let connection = Arc::new(
            runtime
                .connect_package(&self.package.base)
                .map_err(|_| Error::Denied)?,
        );
        let admission = match host.admit(
            runtime,
            &connection,
            d.session,
            session,
            sessions.clone(),
            d.execution_domain.clone(),
            expires,
            now,
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = runtime.disconnect(&connection);
                return Err(error);
            }
        };
        Ok(Approved {
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
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn register_process(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        approved: &mut Approved,
        processes: &mut ProcessHost,
        identity: ToolIdentity,
        executor_connection: Arc<Connection>,
        executor_admission: &Admission,
        capabilities: ProcessCapabilities,
        budget: Budget,
        provider: Box<dyn ProcessProvider>,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Handle> {
        if approved.package != self.package.review_sha256()
            || !subset_process(capabilities, approved.process)
            || approved.processes.len() >= 128
        {
            return Err(Error::Denied);
        }
        let execution = ExecutionBinding {
            identity,
            connection: executor_connection,
            admission: executor_admission.clone(),
        };
        approved.live_tool(runtime, host, &execution, &mut clock)?;
        let now = clock();
        if now < approved.created || now >= approved.expires {
            return Err(Error::Denied);
        }
        let nonce = random()?;
        let handle = processes
            .register(
                Binding {
                    connection: approved.binding,
                    session_id: execution.identity.session_id.clone(),
                    operation_id: execution.identity.operation_id.clone(),
                    generation: execution.identity.generation,
                    created_at_ms: now,
                    expires_at_ms: approved.expires,
                },
                nonce,
                capabilities,
                budget,
                provider,
            )
            .map_err(|_| Error::Denied)?;
        approved.processes.insert(nonce, execution);
        if let Err(error) = approved.live_tool(
            runtime,
            host,
            approved.processes.get(&nonce).ok_or(Error::Denied)?,
            &mut clock,
        ) {
            let _ = processes.trusted_cleanup(handle);
            return Err(error);
        }
        Ok(handle)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        approved: &Approved,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<TaskRun> {
        self.run_guarded(
            runtime,
            host,
            approved,
            processes,
            input,
            &mut clock,
            cancel,
            |_| true,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn run_guarded(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        approved: &Approved,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
        mut managed_live: impl FnMut(&HostRuntime) -> bool,
    ) -> Result<TaskRun> {
        if approved.package != self.package.review_sha256() || runtime.binding() != approved.owner {
            return Err(Error::Denied);
        }
        let mut route = |bytes: &[u8]| {
            route_frame(
                runtime,
                host,
                approved,
                processes,
                bytes,
                &mut clock,
                &mut managed_live,
            )
        };
        Ok(self
            .runner
            .run_agent_session_process_task(input, &mut route, cancel))
    }
    /// The original complete owner performs maintenance before every imported frame.
    /// This borrows the same manager/Core; it is not an old IO package admission.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    fn run_owned_guarded<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        approved: &Approved,
        processes: &mut ProcessHost,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
        mut managed_live: impl FnMut(&morrow_plugin_runtime::manager::Manager, &HostRuntime) -> bool,
    ) -> Result<TaskRun> {
        if approved.package != self.package.review_sha256()
            || owner.runtime().binding() != approved.owner
        {
            return Err(Error::Denied);
        }
        let mut route = |bytes: &[u8]| -> std::result::Result<Vec<u8>, ()> {
            if owner.runtime().binding() != approved.owner {
                return Err(());
            }
            let _ = clock();
            let active = owner
                .with_managed_runtime(|manager, runtime| managed_live(manager, runtime))
                .unwrap_or(false);
            if !active {
                return Err(());
            }
            // The hook retains Storage's actual sealer and leases. A failed hook
            // traps this task; neither an earlier effect nor authority is replayed.
            owner.prepare_io().map_err(|_| ())?;
            if owner.runtime().binding() != approved.owner {
                return Err(());
            }
            let _ = clock();
            owner
                .with_managed_runtime(|manager, runtime| {
                    route_frame(
                        runtime,
                        host,
                        approved,
                        processes,
                        bytes,
                        &mut clock,
                        |runtime| managed_live(manager, runtime),
                    )
                })
                .ok_or(())?
        };
        Ok(self
            .runner
            .run_agent_session_process_task(input, &mut route, cancel))
    }
    /// Uses the combined import but admits only canonical R2 frames.
    /// No ProcessHost is created, bound or invoked on this route.
    #[allow(clippy::too_many_arguments)]
    fn run_session_guarded(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        approved: &Approved,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
        mut managed_live: impl FnMut(&HostRuntime) -> bool,
    ) -> Result<TaskRun> {
        if approved.package != self.package.review_sha256() || runtime.binding() != approved.owner {
            return Err(Error::Denied);
        }
        let mut route = |bytes: &[u8]| {
            route_session_frame(
                runtime,
                host,
                approved,
                bytes,
                &mut clock,
                &mut managed_live,
            )
        };
        Ok(self
            .runner
            .run_agent_session_process_task(input, &mut route, cancel))
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    fn run_session_owned_guarded<O: morrow_plugin_runtime::io_jobs::ManagedHostOwner>(
        &self,
        owner: &mut O,
        host: &SessionExecHost,
        approved: &Approved,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
        mut managed_live: impl FnMut(&morrow_plugin_runtime::manager::Manager, &HostRuntime) -> bool,
    ) -> Result<TaskRun> {
        if approved.package != self.package.review_sha256()
            || owner.runtime().binding() != approved.owner
        {
            return Err(Error::Denied);
        }
        let mut route = |bytes: &[u8]| -> std::result::Result<Vec<u8>, ()> {
            // Validate the schema before maintenance; a process frame cannot dispatch here.
            Request::decode(bytes).map_err(|_| ())?;
            if owner.runtime().binding() != approved.owner {
                return Err(());
            }
            let _ = clock();
            if !owner
                .with_managed_runtime(|m, r| managed_live(m, r))
                .unwrap_or(false)
            {
                return Err(());
            }
            owner.prepare_io().map_err(|_| ())?;
            if owner.runtime().binding() != approved.owner {
                return Err(());
            }
            let _ = clock();
            owner
                .with_managed_runtime(|manager, runtime| {
                    route_session_frame(runtime, host, approved, bytes, &mut clock, |runtime| {
                        managed_live(manager, runtime)
                    })
                })
                .ok_or(())?
        };
        Ok(self
            .runner
            .run_agent_session_process_task(input, &mut route, cancel))
    }

    pub fn revoke(
        &self,
        host: &SessionExecHost,
        approved: &Approved,
        processes: &mut ProcessHost,
    ) -> Result<()> {
        let revoked = host.revoke(&approved.admission);
        for nonce in approved.processes.keys() {
            let _ = processes.trusted_cleanup(Handle {
                nonce: *nonce,
                generation: approved.generation,
            });
        }
        revoked
    }
    /// Trusted cleanup discovery, including registrations denied after provider work.
    /// This does not deliver handles to Wasm or revive expired execution authority.
    pub fn owned_handles(&self, runtime: &HostRuntime, approved: &Approved) -> Result<Vec<Handle>> {
        if approved.package != self.package.review_sha256() || runtime.binding() != approved.owner {
            return Err(Error::Denied);
        }
        Ok(approved
            .processes
            .keys()
            .map(|nonce| Handle {
                nonce: *nonce,
                generation: approved.generation,
            })
            .collect())
    }
}
