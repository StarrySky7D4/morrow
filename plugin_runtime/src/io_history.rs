//! Fresh, single-operation approval for a bodyless historical HTTP query.
//! Original command pins select a fact; they never restore dispatch authority.
use crate::{
    io_binding::{self, IoBinding},
    io_execution,
    manager::{ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::HostRuntime,
    io::{self, Action, OperationOutcome, Request, Response, Status},
    io_intent::{Command, Record},
    plugin_package::io::IoCapability,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct State {
    binding: IoBinding,
    expected: Command,
    revoked: AtomicBool,
    live: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}
/// Not serializable and deliberately without Debug. A trusted host issues this
/// approval for one exact original command on one freshly approved managed owner.
/// Clones share revocation. No resource/job reservation or byte refund is created.
#[derive(Clone)]
pub struct OperationHistoryGrant {
    state: Arc<State>,
}
impl OperationHistoryGrant {
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        expected: Command,
        now: u64,
    ) -> io_binding::Result<Self> {
        let package = instance.package().package();
        if expected.capability != IoCapability::HttpRequest
            || expected.protocol_sha256 != io::schema_digest()
            || expected.subject != package.manifest().package_id
            || expected.package_sha256 != package.digest()
            || binding.is_mutation_history()
        {
            return Err(io_binding::Error::Denied);
        }
        if expected.request_bytes > io::MAX_FRAME_BYTES as u64
            || expected.response_limit > io::MAX_FRAME_BYTES as u64
        {
            return Err(io_binding::Error::Limit);
        }
        // Validates every original command field. Its old approval SHA is only
        // a historical equality pin, never an actual current grant.
        Record::prepared(expected.clone()).map_err(|_| io_binding::Error::Denied)?;
        binding.preflight_capability(manager, host, instance, IoCapability::HttpRequest, now)?;
        Ok(Self {
            state: Arc::new(State {
                binding: binding.duplicate(),
                expected,
                revoked: AtomicBool::new(false),
                live: None,
            }),
        })
    }
    /// Restrict a newly issued, unshared grant. This trusted callback must be
    /// bounded, pure and non-reentrant. A failed probe permanently revokes it.
    pub fn with_live_guard(
        mut self,
        live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> io_binding::Result<Self> {
        let state = Arc::get_mut(&mut self.state).ok_or(io_binding::Error::Denied)?;
        if state.live.is_some() || state.revoked.load(Ordering::Acquire) {
            return Err(io_binding::Error::Denied);
        }
        if !live() {
            state.revoked.store(true, Ordering::Release);
            return Err(io_binding::Error::Denied);
        }
        state.live = Some(Arc::new(live));
        Ok(self)
    }
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    /// Owner/capability compatibility only: no clock sample, historical lookup
    /// or inference of another operation's read authority.
    pub(crate) fn validate_binding(&self, binding: &IoBinding) -> io_binding::Result<()> {
        self.state.binding.validate_same_owner(binding)?;
        self.state.binding.require_capability(IoCapability::HttpRequest)?;
        binding.require_capability(IoCapability::HttpRequest)?;
        Ok(())
    }
    fn check_revocation(&self) -> io_execution::Result<()> {
        if self.state.revoked.load(Ordering::Acquire) {
            return Err(io_execution::Error::Denied);
        }
        if let Some(live) = &self.state.live
            && !live()
        {
            self.state.revoked.store(true, Ordering::Release);
            return Err(io_execution::Error::Denied);
        }
        if self.state.revoked.load(Ordering::Acquire) {
            return Err(io_execution::Error::Denied);
        }
        Ok(())
    }
    /// The caller owns the original managed clock. Every delivery, including
    /// Ready/read, must sample it afresh; this grant never chooses or renews time.
    pub(crate) fn check_at(&self, now: u64) -> io_execution::Result<()> {
        self.check_revocation()?;
        self.state.binding.check_liveness(now)?;
        self.check_revocation()
    }
    /// Read only this grant's exact original operation. The Store verifies any
    /// required original frames; no historical body or command fields are handed
    /// to the guest. A reconciliation-only record is not invented completion.
    ///
    /// Same-now checks here catch synchronous revocation, but are not a second
    /// fresh clock observation. The runtime MUST resample its original authority
    /// clock after this method and at final Ready/read delivery.
    pub(crate) fn read(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: &Request,
        now: u64,
    ) -> io_execution::Result<Vec<u8>> {
        self.state.binding.validate_owner(host, instance)?;
        let Action::QueryOperation { operation_id } = request.action() else {
            return Err(io_execution::Error::Denied);
        };
        let package = instance.package().package();
        if operation_id.as_slice() != self.state.expected.operation_id.as_bytes()
            || self.state.expected.subject != package.manifest().package_id
            || self.state.expected.package_sha256 != package.digest()
        {
            return Err(io_execution::Error::Denied);
        }
        self.check_at(now)?;
        let observed = host
            .store_local()
            .lookup_http_operation_status(&self.state.expected);
        // Missing/error information is also private. Check before exposing any
        // historical status, even though runtime must still resample its clock.
        self.check_at(now)?;
        let outcome = match observed {
            Ok(Some(outcome)) => outcome,
            Ok(None) => OperationOutcome { status: Status::NotFound, http_status: 0 },
            Err(morrow_core::Error::OperationConflict | morrow_core::Error::RevisionConflict) => {
                OperationOutcome { status: Status::Conflict, http_status: 0 }
            }
            Err(morrow_core::Error::Integrity | morrow_core::Error::EvidenceUnavailable) => {
                OperationOutcome { status: Status::EvidenceUnavailable, http_status: 0 }
            }
            Err(other) => return Err(io_execution::storage(other)),
        };
        Response::encode_operation(request, &outcome).map_err(io_execution::storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limits, io_binding::Usage};
    use morrow_core::{
        plugin_package::{Package, catalog::Catalog, io as manifest_io, registry::Registry},
        store::{EventBudget, Store},
    };
    use std::collections::BTreeSet;

    const ID: &str = "org.example.operation.history";
    struct Fixture {
        manager: Manager,
        host: HostRuntime,
        instance: ManagedInstance,
        binding: IoBinding,
        _dir: tempfile::TempDir,
    }
    impl Fixture {
        fn new(approved: BTreeSet<IoCapability>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let wasm = wat::parse_str(
                r#"(module (memory (export "memory") 1)
                (func (export "morrow_run") (result i32) i32.const 0))"#,
            ).unwrap();
            let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
            let mut declaration = manifest_io::declaration(
                vec![IoCapability::FileRead, IoCapability::HttpRequest],
                vec!["api.invoke".into()],
            );
            let budget = declaration.budget.as_mut().unwrap();
            budget.max_resources = 1;
            budget.max_jobs = 1;
            budget.max_bytes = 8192;
            budget.max_job_bytes = 4096;
            budget.max_duration_ms = 50;
            manifest.io_declaration = Some(declaration);
            manifest.required_features.push(manifest_io::FEATURE.into());
            let package = Package::build(manifest, &wasm).unwrap();
            let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
            catalog.install(&package).unwrap();
            let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
            let mut manager = Manager::new(registry, Limits::default());
            manager.select(&package, manager.revision()).unwrap();
            manager.approve_io(ID, package.digest(), approved.clone(), manager.revision()).unwrap();
            manager.set_enabled(ID, package.digest(), true, manager.revision()).unwrap();
            let mut host = HostRuntime::new(
                Store::open(&dir.path().join("synthetic.db"), EventBudget::default()).unwrap(),
            ).unwrap();
            let instance = manager.connect(ID, &mut host).unwrap();
            let binding = manager.bind_io(
                &host, &instance, package.digest(), manager.revision(), &approved, 40, 1,
            ).unwrap();
            Self { manager, host, instance, binding, _dir: dir }
        }
        fn http() -> Self {
            Self::new(BTreeSet::from([IoCapability::FileRead, IoCapability::HttpRequest]))
        }
        fn command(&self) -> Command {
            Command {
                operation_id: "original-operation".into(),
                subject: ID.into(),
                package_sha256: self.instance.package().package().digest(),
                capability: IoCapability::HttpRequest,
                protocol_sha256: io::schema_digest(),
                request_sha256: [2; 32],
                approval_sha256: [0xa5; 32],
                target_sha256: [4; 32],
                request_bytes: 64,
                response_limit: 1024,
            }
        }
        fn issue(&self, expected: Command) -> io_binding::Result<OperationHistoryGrant> {
            OperationHistoryGrant::issue(
                &self.manager, &self.host, &self.instance, &self.binding, expected, 1,
            )
        }
    }
    #[test]
    fn issue_requires_current_http_approval_and_all_original_command_pins() {
        let f = Fixture::http();
        for field in 0..7 {
            let mut expected = f.command();
            match field {
                0 => expected.subject = "another-plugin".into(),
                1 => expected.package_sha256 = [9; 32],
                2 => expected.capability = IoCapability::FileRead,
                3 => expected.protocol_sha256 = [9; 32],
                4 => expected.approval_sha256 = [0; 32],
                5 => expected.operation_id = "invalid/operation".into(),
                _ => expected.request_sha256 = [0; 32],
            }
            assert_eq!(f.issue(expected).err(), Some(io_binding::Error::Denied));
        }
        let denied = Fixture::new(BTreeSet::from([IoCapability::FileRead]));
        assert_eq!(denied.issue(denied.command()).err(), Some(io_binding::Error::Denied));
        assert_eq!(f.binding.usage(), Usage::default());
    }
    #[test]
    fn original_frame_ceiling_is_not_widened_by_a_history_grant() {
        let f = Fixture::http();
        for response in [false, true] {
            let mut expected = f.command();
            if response {
                expected.response_limit = io::MAX_FRAME_BYTES as u64 + 1;
            } else {
                expected.request_bytes = io::MAX_FRAME_BYTES as u64 + 1;
            }
            assert_eq!(f.issue(expected).err(), Some(io_binding::Error::Limit));
        }
        assert_eq!(f.binding.usage(), Usage::default());
    }
    #[test]
    fn grant_and_clones_never_reserve_resources_or_refund_cumulative_bytes() {
        let f = Fixture::http();
        let held = f.binding.admit(
            &f.manager, &f.host, &f.instance, IoCapability::HttpRequest, 0, 3, 1,
        ).unwrap();
        let grant = f.issue(f.command()).unwrap();
        let copy = grant.clone();
        assert_eq!(grant.state.expected.approval_sha256, [0xa5; 32]);
        assert_eq!(f.binding.usage(), Usage { resources: 0, jobs: 1, bytes: 3 });
        grant.revoke();
        assert_eq!(copy.check_at(2), Err(io_execution::Error::Denied));
        drop(grant);
        drop(copy);
        drop(held);
        assert_eq!(f.binding.usage(), Usage { resources: 0, jobs: 0, bytes: 3 });
    }
    #[test]
    fn binding_compatibility_rejects_foreign_owner_and_capability_subset_without_clock() {
        let f = Fixture::http();
        let foreign = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        assert_eq!(grant.validate_binding(&foreign.binding), Err(io_binding::Error::Denied));
        let read_only = f.manager.bind_io(
            &f.host, &f.instance, f.instance.package().package().digest(),
            f.manager.revision(), &BTreeSet::from([IoCapability::FileRead]), 40, 1,
        ).unwrap();
        assert_eq!(grant.validate_binding(&read_only), Err(io_binding::Error::Denied));
        grant.validate_binding(&f.binding).unwrap();
        grant.check_at(2).unwrap();
    }
    #[test]
    fn wrong_operation_request_and_owner_do_not_sample_legitimate_clock() {
        let mut f = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        let another = Request::query_operation(7, b"another-operation").unwrap();
        assert_eq!(grant.read(&f.host, &f.instance, &another, 0), Err(io_execution::Error::Denied));
        let close = Request::encode_cancel(8, &[0; 32]).unwrap();
        assert_eq!(grant.read(&f.host, &f.instance, &close, 0), Err(io_execution::Error::Denied));
        let other = f.manager.connect(ID, &mut f.host).unwrap();
        let original = Request::query_operation(9, b"original-operation").unwrap();
        assert_eq!(grant.read(&f.host, &other, &original, 0), Err(io_execution::Error::Denied));
        grant.check_at(2).unwrap();
        assert_eq!(f.binding.usage(), Usage::default());
    }
    #[test]
    fn missing_original_operation_stays_missing_and_writes_nothing() {
        let f = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        let before = f.host.store_local().pending(0, 100).unwrap();
        let request = Request::query_operation(7, b"original-operation").unwrap();
        let reply = grant.read(&f.host, &f.instance, &request, 2).unwrap();
        assert_eq!(
            Response::decode_operation(&request, &reply).unwrap(),
            OperationOutcome { status: Status::NotFound, http_status: 0 },
        );
        assert_eq!(f.host.store_local().pending(0, 100).unwrap(), before);
        assert_eq!(f.binding.usage(), Usage::default());
    }
    #[test]
    fn shared_live_guard_failure_permanently_revokes_without_accounting_changes() {
        let f = Fixture::http();
        let failed = Arc::new(AtomicBool::new(false));
        let probe = failed.clone();
        let grant = f.issue(f.command()).unwrap()
            .with_live_guard(move || !probe.load(Ordering::Acquire)).unwrap();
        let copy = grant.clone();
        grant.check_at(2).unwrap();
        failed.store(true, Ordering::Release);
        assert_eq!(copy.check_at(3), Err(io_execution::Error::Denied));
        failed.store(false, Ordering::Release);
        assert_eq!(grant.check_at(4), Err(io_execution::Error::Denied));
        assert_eq!(f.binding.usage(), Usage::default());
    }
    #[test]
    fn live_guard_attachment_cannot_widen_a_shared_existing_or_revoked_grant() {
        let f = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        assert_eq!(grant.clone().with_live_guard(|| true).err(), Some(io_binding::Error::Denied));
        grant.check_at(1).unwrap();
        let guarded = f.issue(f.command()).unwrap().with_live_guard(|| true).unwrap();
        assert_eq!(guarded.with_live_guard(|| true).err(), Some(io_binding::Error::Denied));
        grant.revoke();
        assert_eq!(grant.with_live_guard(|| true).err(), Some(io_binding::Error::Denied));
        assert_eq!(f.issue(f.command()).unwrap().with_live_guard(|| false).err(), Some(io_binding::Error::Denied));
    }
    #[test]
    fn expiration_and_disconnect_do_not_restore_historical_approval() {
        let mut f = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        assert_eq!(grant.check_at(100), Err(io_execution::Error::Expired));
        f.instance.close(&mut f.host).unwrap();
        assert_eq!(grant.check_at(100), Err(io_execution::Error::Denied));
    }
    #[test]
    fn manager_drop_revokes_grant_even_if_current_binding_survives() {
        let f = Fixture::http();
        let grant = f.issue(f.command()).unwrap();
        drop(f.manager);
        assert_eq!(grant.check_at(2), Err(io_execution::Error::Denied));
    }
}
