//! Trusted native preparation against the existing protected owner.
//! No runtime reference, Store, protected key, or replacement owner escapes.
//! The preparation surface has no OS start operation and no guest wire lane.
use super::{AgentContext, SealedProposalSpec, sealed_proposal::SealedProposalLease};
use crate::{Result, now, platform::Instant};
use morrow_agent_session_exec_v1_r2::{
    authority::{Admission, Capabilities, SessionExecHost},
    safe_exec::ToolReview,
};
use morrow_codex_session_exec_windows_v1::{
    BorrowedNativeResources, BorrowedWindowsExecutionPort, ProvisionedWindowsBackend,
};
use morrow_core::dispatch::{Connection, HostRuntime};
use std::sync::Arc;

// The sole trusted-host executor and a separately approved sealed guest lease
// have independent original admissions. No guest borrows this executor.
const MAX_PREPARED_CONNECTIONS: usize = 1;

/// Opaque original preparation result. A callback cannot substitute an
/// unrelated admission or executor connection after this constructor.
pub struct PreparedAgentContext {
    identity: [u8; 32],
}

/// Correlation only. The approved package stays in original preparation debt;
/// dropping this token does not discard its connection or retire its grant.
pub struct PreparedNativeSessionPackage {
    identity: [u8; 32],
}

struct PreparedConnection {
    connection: Arc<Connection>,
    admission: Option<Admission>,
    revoked: bool,
    disconnected: bool,
}

#[cfg(test)]
#[path = "native_session_preparation_tests.rs"]
mod native_session_preparation_tests;

#[cfg(test)]
mod tests {
    // Ordinary synthetic SQLite only. These tests never construct a protected
    // Workbench, native backend, process, VM, DPAPI provider, or sandbox account.
    use super::*;
    use morrow_agent_session_exec_v1_r2::{Action, Outcome, Reply, Request};
    use morrow_core::{
        lifecycle::InstancePhase,
        store::{EventBudget, Store},
    };

    fn ordinary() -> (tempfile::TempDir, HostRuntime) {
        let temp = tempfile::tempdir().unwrap();
        let runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        (temp, runtime)
    }
    #[test]
    fn ordinary_preparation_enforces_single_connection_and_cleans_exact_owner() {
        let (_temp, mut runtime) = ordinary();
        let binding = runtime.binding();
        let mut preparation = AgentPreparation::new(&mut runtime, Instant::now());
        let connection = preparation.connect().unwrap();
        assert!(preparation.connect().is_err());
        let mut debt = preparation.into_debt(None);
        debt.cleanup(&mut runtime).unwrap();
        assert_eq!(runtime.binding(), binding);
        assert_ne!(
            runtime.connection_phase(&connection).ok(),
            Some(InstancePhase::Ready)
        );
    }
    #[test]
    fn ordinary_preparation_rejects_foreign_host_without_replacing_runtime() {
        let (_temp, mut runtime) = ordinary();
        let (_foreign_temp, mut foreign) = ordinary();
        let original_binding = runtime.binding();
        let foreign_host = SessionExecHost::new(&mut foreign).unwrap();
        let mut preparation = AgentPreparation::new(&mut runtime, Instant::now());
        let own_host = preparation.session_host().unwrap();
        assert!(preparation.generation(&foreign_host).is_err());
        assert!(preparation.generation(&own_host).is_ok());
        let mut debt = preparation.into_debt(None);
        debt.cleanup(&mut runtime).unwrap();
        assert_eq!(runtime.binding(), original_binding);
    }
    #[test]
    fn ordinary_finish_preserves_proposer_until_explicit_original_cleanup() {
        let (_temp, mut runtime) = ordinary();
        let mut preparation = AgentPreparation::new(&mut runtime, Instant::now());
        let host = preparation.session_host().unwrap();
        let connection = preparation.connect().unwrap();
        let caps = Capabilities {
            session_read: true,
            session_write: true,
            propose: true,
            execute: true,
            retire: false,
        };
        let expires = preparation.expires_after(30_000).unwrap();
        let admission = preparation
            .admit(
                &host,
                &connection,
                caps,
                caps,
                vec!["synthetic-session".into()],
                "synthetic-domain".into(),
                expires,
            )
            .unwrap();
        let prepared = preparation.context(Default::default(), None).unwrap();
        assert!(preparation.context(Default::default(), None).is_err());
        let context = preparation.release_context(prepared).unwrap();
        let request = Request::new_for_generation(
            "synthetic-create",
            preparation.generation(&host).unwrap(),
            Action::Create {
                session_id: "synthetic-session".into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        let raw = preparation
            .dispatch(&host, &connection, &admission, request.raw())
            .unwrap();
        assert!(matches!(
            Reply::decode_for(&request, &raw).unwrap().outcome,
            Outcome::Session(_)
        ));
        let mut debt = preparation.into_debt(Some(context));
        debt.cleanup(&mut runtime).unwrap();
        assert_ne!(
            runtime.connection_phase(&connection).ok(),
            Some(InstancePhase::Ready)
        );
        assert!(debt.retained_context.is_none());
        // A cleanup call does not replay Create or erase its durable session.
        debt.cleanup(&mut runtime).unwrap();
    }
    #[test]
    fn ordinary_preparation_uses_bounded_monotonic_workbench_clock() {
        let (_temp, mut runtime) = ordinary();
        let start = Instant::now();
        let preparation = AgentPreparation::new(&mut runtime, start);
        assert!(preparation.expires_after(0).is_err());
        assert!(preparation.expires_after(60_001).is_err());
        let now = preparation.now();
        let expires = preparation.expires_after(60_000).unwrap();
        assert!(expires >= now + 60_000);
    }
    #[test]
    fn ordinary_dropped_prepared_token_retains_exact_context_for_cleanup() {
        let (_temp, mut runtime) = ordinary();
        let binding = runtime.binding();
        let mut preparation = AgentPreparation::new(&mut runtime, Instant::now());
        let host = preparation.session_host().unwrap();
        let connection = preparation.connect().unwrap();
        let caps = Capabilities {
            session_read: true,
            session_write: true,
            propose: false,
            execute: false,
            retire: false,
        };
        let expires = preparation.expires_after(30_000).unwrap();
        preparation
            .admit(
                &host,
                &connection,
                caps,
                caps,
                vec!["synthetic-session".into()],
                "synthetic-domain".into(),
                expires,
            )
            .unwrap();
        let token = preparation.context(Default::default(), None).unwrap();
        drop(token);
        // A trusted callback may cancel after construction. Dropping its opaque
        // token must not lose the actual context or independent lease cleanup.
        let mut debt = preparation.into_debt(None);
        assert!(debt.retained_context.is_some());
        debt.cleanup(&mut runtime).unwrap();
        assert_eq!(runtime.binding(), binding);
        assert!(debt.retained_context.is_none());
        assert_ne!(
            runtime.connection_phase(&connection).ok(),
            Some(InstancePhase::Ready)
        );
    }
}

/// Outstanding original connection cleanup; never represents business rollback.
pub(crate) struct AgentPreparationDebt {
    host: Option<Arc<SessionExecHost>>,
    connections: Vec<PreparedConnection>,
    retained_context: Option<AgentContext>,
    sealed_proposal: Option<SealedProposalLease>,
    package: Option<morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage>,
    native_session_package: Option<morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage>,
}
impl AgentPreparationDebt {
    pub(crate) fn for_context(mut context: AgentContext) -> Self {
        context.original_authority();
        if let Some(ticket) = &context.lifecycle {
            let _ = ticket.cleanup();
        }
        let retired = context
            .lifecycle
            .as_ref()
            .is_some_and(|t| t.authority_is_retired());
        Self {
            host: Some(context.host.clone()),
            connections: vec![PreparedConnection {
                connection: context.executor_connection.clone(),
                admission: Some(context.executor_admission.clone()),
                revoked: retired,
                disconnected: retired,
            }],
            retained_context: Some(context),
            sealed_proposal: None,
            package: None,
            native_session_package: None,
        }
    }
    pub(crate) fn for_package(
        context: AgentContext,
        package: morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage,
    ) -> Self {
        let mut debt = Self::for_context(context);
        debt.package = Some(package);
        debt
    }
    pub(crate) fn cleanup(&mut self, runtime: &mut HostRuntime) -> Result<()> {
        if let Some(host) = &self.host {
            host.generation(runtime)
                .map_err(|_| "foreign preparation cleanup owner")?;
        }
        if let Some(context) = &self.retained_context
            && !context
                .resources
                .is_clean()
                .map_err(|e| format!("preparation facts: {e:?}"))?
        {
            return Err("preparation resources remain charged; no execution replay".into());
        }
        if let Some(context) = &mut self.retained_context {
            context
                .close_sealed(runtime)
                .map_err(|e| format!("preparation sealed cleanup: {e:?}"))?;
        }
        if let Some(lease) = &self.sealed_proposal {
            lease.request_stop();
            lease.close(
                runtime,
                self.host.as_ref().ok_or("preparation host unavailable")?,
            )?;
            self.sealed_proposal = None;
        }
        if let Some(package) = &self.native_session_package {
            package.request_stop();
            package.close_session(
                runtime, self.host.as_ref().ok_or("preparation host unavailable")?,
            )?;
            self.native_session_package = None;
        }
        if let Some(package) = &self.package {
            let context = self
                .retained_context
                .as_mut()
                .ok_or("package cleanup context unavailable")?;
            package.request_stop();
            package.close(runtime, &context.host, &mut context.processes)?;
            self.package = None;
        }
        for record in &mut self.connections {
            if !record.revoked {
                if let Some(admission) = &record.admission {
                    self.host
                        .as_ref()
                        .ok_or("preparation host unavailable")?
                        .revoke(admission)
                        .map_err(|e| format!("preparation admission cleanup: {e:?}"))?;
                }
                record.revoked = true;
            }
            if !record.disconnected {
                runtime
                    .disconnect(&record.connection)
                    .map_err(|e| format!("preparation connection cleanup: {e:?}"))?;
                record.disconnected = true;
            }
        }
        if let Some(context) = &self.retained_context
            && let Some(ticket) = &context.lifecycle
        {
            ticket.authority_retired(runtime)?;
        }
        if let Some(context) = self.retained_context.take()
            && let Err(context) = context.dispose_on_original(runtime)
        {
            self.retained_context = Some(*context);
            return Err("preparation context cleanup remains charged".into());
        }
        Ok(())
    }
}

/// A bounded synchronous trusted-host view. None of its methods starts a process.
/// Durable session/proposal changes are preserved even when preparation fails.
pub struct AgentPreparation<'a> {
    runtime: &'a mut HostRuntime,
    start: Instant,
    debt: AgentPreparationDebt,
    context_attempted: bool,
    catalog: Option<(
        &'a mut morrow_plugin_runtime::manager::Manager,
        &'a mut crate::agent_catalog::Slot,
    )>,
    reserved: Vec<String>,
    sealed_attempted: bool,
    prepared_identity: Option<[u8; 32]>,
    native_package_attempted: bool,
    native_package_identity: Option<[u8; 32]>,
    ledger: Arc<super::context_lifecycle::ContextLifecycleLedger>,
}
impl<'a> AgentPreparation<'a> {
    pub(crate) fn new(runtime: &'a mut HostRuntime, start: Instant) -> Self {
        let ledger = super::context_lifecycle::ContextLifecycleLedger::new(runtime.binding());
        Self {
            ledger,
            runtime,
            start,
            context_attempted: false,
            catalog: None,
            reserved: Vec::new(),
            sealed_attempted: false,
            prepared_identity: None,
            native_package_attempted: false,
            native_package_identity: None,
            debt: AgentPreparationDebt {
                host: None,
                connections: Vec::new(),
                retained_context: None,
                sealed_proposal: None,
                package: None,
            native_session_package: None,
            },
        }
    }
    pub(crate) fn with_catalog(
        runtime: &'a mut HostRuntime,
        start: Instant,
        manager: Option<&'a mut morrow_plugin_runtime::manager::Manager>,
        catalog: Option<&'a mut crate::agent_catalog::Slot>,
        reserved: Vec<String>,
        host: Arc<SessionExecHost>,
        ledger: Arc<super::context_lifecycle::ContextLifecycleLedger>,
    ) -> Self {
        let mut value = Self::new(runtime, start);
        value.catalog = manager.zip(catalog);
        value.reserved = reserved;
        value.debt.host = Some(host);
        value.ledger = ledger;
        value
    }
    /// Prepare exactly one separately reviewed original proposal guest. This
    /// does not run Wasm, propose, claim, or start any OS process.
    pub fn prepare_sealed_proposal(
        &mut self,
        spec: SealedProposalSpec,
        expires: u64,
    ) -> Result<()> {
        if self.context_attempted || self.sealed_attempted {
            return Err("sealed preparation already attempted".into());
        }
        self.sealed_attempted = true;
        let host = self.session_host()?;
        let time = self.now();
        if expires <= time
            || expires - time > 60_000
            || spec.generation() != self.generation(&host)?
        {
            return Err("sealed preparation clock or original generation mismatch".into());
        }
        let (manager, slot) = self
            .catalog
            .as_mut()
            .ok_or("original catalog preparation unavailable")?;
        let catalog = slot.opened(self.reserved.clone())?;
        match SealedProposalLease::prepare(
            manager,
            catalog,
            self.runtime,
            &host,
            spec,
            expires,
            time,
        ) {
            Ok(lease) => {
                self.debt.sealed_proposal = Some(lease);
                Ok(())
            }
            Err(failure) => {
                self.debt.sealed_proposal = failure.lease;
                Err(failure.error)
            }
        }
    }
    pub fn now(&self) -> u64 {
        now(self.start)
    }
    pub fn expires_after(&self, millis: u64) -> Result<u64> {
        if millis == 0 || millis > 60_000 {
            return Err("preparation lifetime bounds".into());
        }
        self.now()
            .checked_add(millis)
            .ok_or_else(|| "preparation lifetime overflow".into())
    }
    pub fn session_host(&mut self) -> Result<Arc<SessionExecHost>> {
        if self.debt.host.is_none() {
            self.debt.host = Some(Arc::new(
                SessionExecHost::new(self.runtime)
                    .map_err(|e| format!("original session host: {e:?}"))?,
            ));
        }
        Ok(self
            .debt
            .host
            .as_ref()
            .ok_or("preparation host unavailable")?
            .clone())
    }
    fn check_host(&self, host: &SessionExecHost) -> Result<()> {
        if self
            .debt
            .host
            .as_ref()
            .is_some_and(|own| std::ptr::eq(own.as_ref(), host))
        {
            Ok(())
        } else {
            Err("foreign preparation session host".into())
        }
    }
    fn index(&self, connection: &Connection) -> Result<usize> {
        self.debt
            .connections
            .iter()
            .position(|r| r.connection.binding() == connection.binding())
            .ok_or_else(|| "foreign preparation connection".into())
    }
    pub fn resources(&self) -> Result<Arc<BorrowedNativeResources>> {
        BorrowedNativeResources::for_owner(self.runtime)
            .map_err(|e| format!("original native resources: {e:?}").into())
    }
    pub fn bind_port(
        &self,
        host: Arc<SessionExecHost>,
        resources: Arc<BorrowedNativeResources>,
        backend: Arc<ProvisionedWindowsBackend>,
    ) -> Result<BorrowedWindowsExecutionPort> {
        self.check_host(&host)?;
        if !backend.is_production() || resources.owner_binding() != self.runtime.binding() {
            return Err("production backend or original resource binding required".into());
        }
        BorrowedWindowsExecutionPort::new(self.runtime, host, resources, backend)
            .map_err(|e| format!("original production port: {e:?}").into())
    }
    pub fn connect(&mut self) -> Result<Arc<Connection>> {
        if self.debt.connections.len() >= MAX_PREPARED_CONNECTIONS {
            return Err("preparation connection budget".into());
        }
        let connection = Arc::new(
            self.runtime
                .connect()
                .map_err(|e| format!("original preparation connection: {e:?}"))?,
        );
        self.debt.connections.push(PreparedConnection {
            connection: connection.clone(),
            admission: None,
            revoked: false,
            disconnected: false,
        });
        Ok(connection)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        &mut self,
        host: &SessionExecHost,
        connection: &Connection,
        declared: Capabilities,
        approved: Capabilities,
        sessions: Vec<String>,
        domain: String,
        expires: u64,
    ) -> Result<Admission> {
        self.check_host(host)?;
        let index = self.index(connection)?;
        if self.debt.connections[index].admission.is_some() {
            return Err("one tracked admission per preparation connection".into());
        }
        let admission = host
            .admit(
                self.runtime,
                connection,
                declared,
                approved,
                sessions,
                domain,
                expires,
                self.now(),
            )
            .map_err(|e| format!("original preparation admission: {e:?}"))?;
        self.debt.connections[index].admission = Some(admission.clone());
        Ok(admission)
    }
    pub fn generation(&self, host: &SessionExecHost) -> Result<u64> {
        self.check_host(host)?;
        host.generation(self.runtime)
            .map_err(|e| format!("original generation: {e:?}").into())
    }
    pub fn dispatch(
        &mut self,
        host: &SessionExecHost,
        connection: &Connection,
        admission: &Admission,
        bytes: &[u8],
    ) -> Result<Vec<u8>> {
        self.check_host(host)?;
        self.index(connection)?;
        let start = self.start;
        host.dispatch(self.runtime, connection, admission, bytes, || now(start))
            .map_err(|e| format!("original preparation request: {e:?}").into())
    }
    pub fn review_tool(&self, host: &SessionExecHost, operation: &str) -> Result<ToolReview> {
        self.check_host(host)?;
        host.review_tool(self.runtime, operation, self.now())
            .map_err(|e| format!("original tool review: {e:?}").into())
    }
    pub fn approve(
        &mut self,
        host: &SessionExecHost,
        connection: &Connection,
        executor: &Admission,
        operation: &str,
        proposal_digest: [u8; 32],
        intent_digest: [u8; 32],
    ) -> Result<[u8; 32]> {
        self.check_host(host)?;
        self.index(connection)?;
        let time = self.now();
        host.approve(
            self.runtime,
            connection,
            executor,
            operation,
            proposal_digest,
            intent_digest,
            time,
        )
        .map_err(|e| format!("original tool approval: {e:?}").into())
    }
    /// One original catalog connection with fresh ceiling/selection/revision
    /// checks. The actual package is retained before any further validation.
    /// Failure and dropped tokens never permit reminting or silent cleanup.
    pub fn prepare_native_session_package(
        &mut self,
        id: &str,
        full_sha256: [u8; 32],
        expected: morrow_agent_catalog_admin_v1::Revisions,
        expires: u64,
    ) -> Result<PreparedNativeSessionPackage> {
        if self.context_attempted || self.native_package_attempted {
            return Err("native session package preparation already attempted; no replay".into());
        }
        self.native_package_attempted = true;
        let host = self.session_host()?;
        let time = self.now();
        if expires <= time || expires - time > 60_000 {
            return Err("native session package lifetime bounds".into());
        }
        let mut identity = [0; 32];
        getrandom::fill(&mut identity)?;
        if identity == [0; 32] {
            return Err("native session package identity unavailable".into());
        }
        let (manager, slot) = self.catalog.as_mut()
            .ok_or("original catalog preparation unavailable")?;
        let package = slot.connect(manager, self.runtime, &host, id, full_sha256,
            expected, expires, time, self.reserved.clone())?;
        self.debt.native_session_package = Some(package);
        self.debt.native_session_package.as_ref().expect("retained above")
            .validate_native_session_package(manager, self.runtime, &host, time)?;
        self.native_package_identity = Some(identity);
        Ok(PreparedNativeSessionPackage { identity })
    }
    pub fn context_with_prepared_native_session(
        &mut self,
        package: PreparedNativeSessionPackage,
        processes: morrow_agent_process_control_v1::host::Host,
        port: Option<BorrowedWindowsExecutionPort>,
    ) -> Result<PreparedAgentContext> {
        self.context_impl(None, Some(package), processes, port)
    }
    pub fn context(
        &mut self,
        processes: morrow_agent_process_control_v1::host::Host,
        port: Option<BorrowedWindowsExecutionPort>,
    ) -> Result<PreparedAgentContext> {
        self.context_impl(None, None, processes, port)
    }
    /// Attach an independently approved session-only bridge before the original
    /// lifecycle ticket is registered. The primary process package is still
    /// connected by start_agent; neither package borrows the executor grant.
    /// Rejection preserves the caller's package and the exact cleanup context.
    pub fn context_with_native_session_package(
        &mut self,
        package: &mut Option<morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage>,
        processes: morrow_agent_process_control_v1::host::Host,
        port: Option<BorrowedWindowsExecutionPort>,
    ) -> Result<PreparedAgentContext> {
        self.context_impl(Some(package), None, processes, port)
    }
    fn context_impl(
        &mut self,
        package: Option<&mut Option<morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage>>,
        prepared: Option<PreparedNativeSessionPackage>,
        processes: morrow_agent_process_control_v1::host::Host,
        port: Option<BorrowedWindowsExecutionPort>,
    ) -> Result<PreparedAgentContext> {
        if self.context_attempted {
            return Err("context construction already attempted; no replay".into());
        }
        self.context_attempted = true;
        let mut identity = [0; 32];
        getrandom::fill(&mut identity)?;
        if identity == [0; 32] {
            return Err("prepared context identity unavailable".into());
        }
        if self.debt.connections.len() != 1 {
            return Err("one original preparation connection required".into());
        }
        let host = self
            .debt
            .host
            .as_ref()
            .ok_or("original preparation host required")?
            .clone();
        let record = &self.debt.connections[0];
        let connection = record.connection.clone();
        let admission = record
            .admission
            .as_ref()
            .ok_or("original tracked admission required")?
            .clone();
        if port
            .as_ref()
            .is_some_and(|p| !p.is_production() || p.owner_binding() != self.runtime.binding())
        {
            return Err("foreign or non-production context port".into());
        }
        let resources = self.resources()?;
        match AgentContext::new(host, processes, resources, port, connection, admission) {
            Ok(mut context) => {
                if let Some(package) = package {
                    let time = self.now();
                    let attached = match self.catalog.as_ref() {
                        Some((manager, _)) => context.attach_native_session_package(
                            manager, self.runtime, package, time,
                        ).map_err(|error| format!("native session preparation: {error:?}").into()),
                        None => Err("original catalog preparation unavailable".into()),
                    };
                    if let Err(error) = attached {
                        self.debt.retained_context = Some(context);
                        return Err(error);
                    }
                }
                if let Some(prepared) = prepared {
                    let time = self.now();
                    let attached: Result<()> = if self.native_package_identity != Some(prepared.identity) {
                        Err("foreign or consumed native session preparation token".into())
                    } else {
                        match self.catalog.as_ref() {
                            Some((manager, _)) => context.attach_native_session_package(
                                manager, self.runtime, &mut self.debt.native_session_package, time,
                            ).map_err(|error| format!("native session preparation: {error:?}").into()),
                            None => Err("original catalog preparation unavailable".into()),
                        }
                    };
                    if let Err(error) = attached {
                        self.debt.retained_context = Some(context);
                        return Err(error);
                    }
                    self.native_package_identity = None;
                }
                if let Err(error) = self.ledger.register(&mut context) {
                    self.debt.retained_context = Some(context);
                    return Err(error);
                }
                context.sealed_proposal = self.debt.sealed_proposal.take();
                self.debt.retained_context = Some(context);
                self.prepared_identity = Some(identity);
                Ok(PreparedAgentContext { identity })
            }
            Err(failure) => {
                self.debt.retained_context = Some(failure.context);
                Err(
                    "original context construction failed; exact preparation retained for cleanup"
                        .into(),
                )
            }
        }
    }
    pub(crate) fn finish(&self, context: &AgentContext) -> Result<()> {
        if self.debt.native_session_package.is_some() || self.native_package_identity.is_some() {
            return Err("untransferred native session package remains charged; explicit cleanup required".into());
        }
        self.check_host(&context.host)?;
        let keep = self.index(&context.executor_connection)?;
        if self.debt.connections.len() != 1
            || keep != 0
            || self.debt.connections[keep].admission.is_none()
            || context.resources.owner_binding() != self.runtime.binding()
            || !context
                .resources
                .is_clean()
                .map_err(|e| format!("preparation facts: {e:?}"))?
            || context
                .port()
                .is_some_and(|p| !p.is_production() || p.owner_binding() != self.runtime.binding())
        {
            return Err("prepared context is not clean and production-bound".into());
        }
        // The executor and separate sealed proposer transfer intact. Neither
        // a completed proposal task nor this callback revokes the proposer.
        Ok(())
    }
    pub(super) fn release_context(
        &mut self,
        prepared: PreparedAgentContext,
    ) -> Result<AgentContext> {
        if self.prepared_identity != Some(prepared.identity) {
            return Err("foreign prepared context token".into());
        }
        let context = self
            .debt
            .retained_context
            .as_ref()
            .ok_or("prepared context retained state unavailable")?;
        self.finish(context)?;
        if let Some(ticket) = &context.lifecycle {
            ticket.publish(context)?;
        }
        self.prepared_identity = None;
        self.debt
            .retained_context
            .take()
            .ok_or_else(|| "prepared context unavailable".into())
    }
    pub(crate) fn into_debt(mut self, context: Option<AgentContext>) -> AgentPreparationDebt {
        if context.is_some() {
            self.debt.retained_context = context;
        }
        if let Some(context) = &self.debt.retained_context
            && let Some(ticket) = &context.lifecycle
        {
            let _ = ticket.cleanup();
        }
        self.debt
    }
}
