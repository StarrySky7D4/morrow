//! Trusted native registry-to-runtime entry. Changes revoke before durable publication.
//! A successful operation past its final authorization boundary is never promised rollback.
use crate::{
    Cancellation, Fault, Limits, Report,
    dependency::{Dependency, Endpoint, Spec},
    io_binding::{IoBinding, IoContext, ServiceRunBudget},
    package::{PreparedPackage, TaskReport},
};
use morrow_core::{
    Error,
    dispatch::{Connection, ConnectionBinding, HostRuntime},
    lifecycle::{GrantKind, Revocation},
    plugin_package::{
        Package,
        io::IoCapability,
        registry::{LockedDependency, Registry, Selection},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Weak},
};

const MAX_INSTANCES: usize = 128;
#[derive(Debug)]
pub enum ManagerError {
    Core(Error),
    Prepare(Fault),
    Dependency(crate::dependency::Error),
    Io(crate::io_binding::Error),
}
impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core(e) => write!(f, "{e}"),
            Self::Prepare(e) => write!(f, "{e:?}"),
            Self::Dependency(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for ManagerError {}
impl From<Error> for ManagerError {
    fn from(value: Error) -> Self {
        Self::Core(value)
    }
}
impl From<Fault> for ManagerError {
    fn from(value: Fault) -> Self {
        Self::Prepare(value)
    }
}
pub type Result<T> = std::result::Result<T, ManagerError>;
/// An explicit host request to include one already approved optional slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalDependency {
    pub caller: String,
    pub slot: String,
}
#[derive(Clone, Debug)]
pub struct PlannedPackage {
    pub id: String,
    pub digest: [u8; 32],
}
/// Read-only exact-selection snapshot, not permission to connect or restore object grants.
/// All execution paths must revalidate the current manager before consuming the plan.
#[derive(Clone)]
pub struct ActivationPlan {
    root: String,
    revision: u64,
    packages: Vec<PlannedPackage>,
    required: BTreeSet<String>,
    required_edges: Vec<(String, String)>,
    optionals: Vec<OptionalDependency>,
}
impl ActivationPlan {
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Providers precede dependents along required edges only, with deterministic ID tie-breaking.
    pub fn packages(&self) -> &[PlannedPackage] {
        &self.packages
    }
    /// Root and packages reachable from it using required edges exclusively.
    pub fn required(&self) -> &BTreeSet<String> {
        &self.required
    }
    /// (caller, provider) required edges among all selected plan nodes, including optional subtrees.
    pub fn required_edges(&self) -> &[(String, String)] {
        &self.required_edges
    }
    pub fn optionals(&self) -> &[OptionalDependency] {
        &self.optionals
    }
}
pub(crate) struct Control {
    binding: ConnectionBinding,
    pub(crate) io: Option<Arc<IoContext>>,
    revocation: Revocation,
    cancel: Cancellation,
}
impl Control {
    pub(crate) fn active(&self) -> bool {
        !self.revocation.is_revoked() && self.cancel.fault().is_none()
    }
    fn stop(&self) {
        // Host read and commit boundaries see revocation before cooperative guest cancellation.
        self.revocation.revoke();
        self.cancel.cancel();
    }
}
/// Owns preparation and the approved connection. This stays valid only while its control is live.
/// References are trusted host adapters, never guest-facing administrative APIs.
pub struct ManagedInstance {
    package: PreparedPackage,
    connection: Connection,
    control: Arc<Control>,
}
impl ManagedInstance {
    pub(crate) fn io_control(&self) -> &Arc<Control> {
        &self.control
    }
    fn binding_matches(&self) -> bool {
        self.connection.binding() == self.control.binding
    }
    pub fn package(&self) -> &PreparedPackage {
        &self.package
    }
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    /// Allows trusted object grant setup without separating package and connection ownership.
    pub fn parts_mut(&mut self) -> (&PreparedPackage, &mut Connection) {
        (&self.package, &mut self.connection)
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.control.cancel.clone()
    }
    pub fn stop(&self) {
        self.control.stop();
    }
    /// Release the host's bounded instance record. Stop still applies if a wrong host is supplied.
    pub fn close(&self, host: &mut HostRuntime) -> morrow_core::Result<()> {
        self.stop();
        if !self.binding_matches() {
            return Err(Error::Invalid("replaced managed connection"));
        }
        host.disconnect(&self.connection)
    }
    pub fn run(&self, host: &mut HostRuntime, clock: impl FnMut() -> u64) -> Report {
        if !self.binding_matches() {
            return Report {
                outcome: Err(Fault::PackageBinding),
                host_calls: 0,
                fuel_remaining: self.package.limits().fuel,
            };
        }
        self.package
            .run(host, &self.connection, clock, self.control.cancel.clone())
    }
    pub fn run_task(
        &self,
        host: &mut HostRuntime,
        input: &morrow_core::task::Invocation,
        clock: impl FnMut() -> u64,
    ) -> TaskReport {
        if !self.binding_matches() {
            return TaskReport {
                execution: Report {
                    outcome: Err(Fault::PackageBinding),
                    host_calls: 0,
                    fuel_remaining: self.package.limits().fuel,
                },
                response: None,
                output: None,
                failure: None,
            };
        }
        self.package.run_task(
            host,
            &self.connection,
            input,
            clock,
            self.control.cancel.clone(),
        )
    }
}
impl Drop for ManagedInstance {
    fn drop(&mut self) {
        self.stop();
    }
}
/// One manager owns registry mutation. No mutable registry or unchecked connection factory escapes.
/// Existing low-level trusted APIs remain available; this is not isolation from the trusted host.
pub struct Manager {
    identity: Arc<()>,
    registry: Registry,
    limits: Limits,
    instances: BTreeMap<String, Vec<Weak<Control>>>,
}
impl Manager {
    pub fn new(registry: Registry, limits: Limits) -> Self {
        Self {
            identity: Arc::new(()),
            registry,
            limits,
            instances: BTreeMap::new(),
        }
    }
    pub(crate) fn identity(&self) -> Weak<()> {
        Arc::downgrade(&self.identity)
    }
    pub fn revision(&self) -> u64 {
        self.registry.revision()
    }
    pub fn selection(&self, id: &str) -> Option<&Selection> {
        self.registry.selection(id)
    }
    pub fn selections(&self) -> impl Iterator<Item = &Selection> {
        self.registry.selections()
    }
    /// Plan only: validates approved immutable selections without connecting, enabling or writing.
    /// Optional requests are followed only after their caller becomes reachable from the root.
    pub fn activation_plan(
        &self,
        root: &str,
        optional: &[OptionalDependency],
        expected_revision: u64,
    ) -> Result<ActivationPlan> {
        self.check_revision(expected_revision)?;
        let valid_identity = |value: &str| {
            !value.is_empty()
                && value.len() <= 256
                && !value
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '/' | ':' | '\\'))
        };
        if !valid_identity(root) {
            return Err(Error::Invalid("activation root").into());
        }
        // At most 64 packages, each declaring at most 16 slots; reject before copying requests.
        if optional.len() > 64 * morrow_core::plugin_package::MAX_DEPENDENCIES {
            return Err(Error::Limit.into());
        }
        let mut requests = BTreeSet::new();
        for request in optional {
            if !valid_identity(&request.caller)
                || !valid_identity(&request.slot)
                || !requests.insert((request.caller.clone(), request.slot.clone()))
            {
                return Err(Error::Invalid("invalid or duplicate optional dependency").into());
            }
        }
        let mut pending = BTreeSet::from([root.to_owned()]);
        let mut packages = BTreeMap::new();
        let mut edges = BTreeSet::new();
        let mut followed = BTreeSet::new();
        while let Some(id) = pending.pop_first() {
            if packages.contains_key(&id) {
                continue;
            }
            if packages.len() >= 64 {
                return Err(Error::Limit.into());
            }
            let (package, selection) = self.registry.resolve_enabled(&id)?;
            if package.manifest().guest_abi_version != 2 {
                return Err(Error::UnsupportedVersion.into());
            }
            packages.insert(id.clone(), selection.digest);
            for declaration in &package.manifest().dependencies {
                let key = (id.clone(), declaration.slot.clone());
                if requests.contains(&key) {
                    if !declaration.optional {
                        return Err(Error::Invalid("explicit slot is not optional").into());
                    }
                    followed.insert(key);
                } else if declaration.optional {
                    continue;
                }
                let (lock, provider, provider_selection) =
                    self.registry.resolve_dependency(&id, &declaration.slot)?;
                // resolve_dependency checks both current digests, declarations and provider closure.
                if lock.caller_digest != package.digest()
                    || lock.provider_digest != provider.digest()
                    || lock.provider_digest != provider_selection.digest
                {
                    return Err(Error::RevisionConflict.into());
                }
                if !declaration.optional {
                    edges.insert((id.clone(), lock.provider_id.clone()));
                }
                if !packages.contains_key(&lock.provider_id) {
                    pending.insert(lock.provider_id);
                }
            }
            if packages.len() + pending.len() > 64 {
                return Err(Error::Limit.into());
            }
        }
        if followed != requests {
            return Err(Error::Invalid("optional slot caller unreachable or undeclared").into());
        }
        let mut required = BTreeSet::from([root.to_owned()]);
        let mut frontier = vec![root.to_owned()];
        while let Some(caller) = frontier.pop() {
            for (_, provider) in edges.iter().filter(|(from, _)| from == &caller) {
                if required.insert(provider.clone()) {
                    frontier.push(provider.clone());
                }
            }
        }
        let mut remaining: BTreeMap<String, usize> =
            packages.keys().map(|id| (id.clone(), 0)).collect();
        let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (caller, provider) in &edges {
            *remaining.get_mut(caller).expect("included caller") += 1;
            dependents
                .entry(provider.clone())
                .or_default()
                .insert(caller.clone());
        }
        let mut ready: BTreeSet<_> = remaining
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut ordered = Vec::new();
        while let Some(id) = ready.pop_first() {
            ordered.push(PlannedPackage {
                id: id.clone(),
                digest: packages[&id],
            });
            if let Some(callers) = dependents.get(&id) {
                for caller in callers {
                    let count = remaining.get_mut(caller).expect("included dependent");
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(caller.clone());
                    }
                }
            }
        }
        if ordered.len() != packages.len() {
            return Err(Error::Invalid("required activation cycle").into());
        }
        Ok(ActivationPlan {
            root: root.into(),
            revision: expected_revision,
            packages: ordered,
            required,
            required_edges: edges.into_iter().collect(),
            optionals: optional.to_vec(),
        })
    }
    fn check_revision(&self, revision: u64) -> Result<()> {
        if revision != self.revision() {
            return Err(Error::RevisionConflict.into());
        }
        Ok(())
    }
    fn checked_selection(&self, id: &str, digest: [u8; 32], revision: u64) -> Result<&Selection> {
        self.check_revision(revision)?;
        let selection = self.selection(id).ok_or(Error::NotFound)?;
        if selection.digest != digest {
            return Err(Error::RevisionConflict.into());
        }
        Ok(selection)
    }
    fn revoke(&mut self, id: &str) {
        if let Some(controls) = self.instances.remove(id) {
            for control in controls.into_iter().filter_map(|w| w.upgrade()) {
                control.stop();
            }
        }
    }
    fn revoke_required_tree(&mut self, id: &str) {
        // Snapshot the old graph before registry mutation removes or replaces its edges.
        let callers = self.registry.required_dependents(id);
        for caller in callers {
            self.revoke(&caller);
        }
        self.revoke(id);
    }
    fn owns(&self, instance: &ManagedInstance) -> bool {
        instance.binding_matches()
            && self
                .instances
                .get(&instance.package.package().manifest().package_id)
                .is_some_and(|controls| {
                    controls
                        .iter()
                        .filter_map(Weak::upgrade)
                        .any(|control| Arc::ptr_eq(&control, &instance.control))
                })
    }
    pub(crate) fn validate_instance(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        if !self.owns(instance)
            || host.connection_phase(instance.connection())
                != Ok(morrow_core::lifecycle::InstancePhase::Ready)
        {
            return Err(Error::Invalid("inactive managed instance").into());
        }
        let (package, _) = self
            .registry
            .resolve_enabled(&instance.package.package().manifest().package_id)?;
        if package.digest() != instance.package.package().digest() {
            return Err(Error::Invalid("stale managed package").into());
        }
        Ok(())
    }
    fn prune(&mut self) -> usize {
        self.instances.retain(|_, controls| {
            controls.retain(|w| w.strong_count() != 0);
            !controls.is_empty()
        });
        self.instances.values().map(Vec::len).sum()
    }
    /// A validated package supplies the ID before revocation; Registry re-loads the installed digest.
    /// Failed validation/publication can leave old instances stopped while durable selection is intact.
    pub fn select(&mut self, package: &Package, revision: u64) -> Result<()> {
        self.check_revision(revision)?;
        let id = &package.manifest().package_id;
        if self
            .selection(id)
            .is_some_and(|s| s.digest == package.digest())
        {
            return Ok(());
        }
        self.revoke_required_tree(id);
        self.registry.select(package.digest(), revision)?;
        Ok(())
    }
    pub fn approve(
        &mut self,
        id: &str,
        digest: [u8; 32],
        approved: BTreeSet<GrantKind>,
        revision: u64,
    ) -> Result<()> {
        let selection = self.checked_selection(id, digest, revision)?;
        if selection.approved == approved {
            return Ok(());
        }
        // Even widening approvals requires a fresh connection, never mutating a live ceiling.
        self.revoke_required_tree(id);
        self.registry.approve(id, digest, approved, revision)?;
        Ok(())
    }
    /// Validate before revoking; even broader approval needs a new actual connection.
    pub fn approve_io(
        &mut self,
        id: &str,
        digest: [u8; 32],
        approved: BTreeSet<IoCapability>,
        revision: u64,
    ) -> Result<()> {
        self.registry
            .validate_io_approval(id, digest, &approved, revision)?;
        if self
            .selection(id)
            .is_some_and(|s| s.approved_io == approved)
        {
            return Ok(());
        }
        self.revoke_required_tree(id);
        self.registry.approve_io(id, digest, approved, revision)?;
        Ok(())
    }
    /// Bind a previously approved ceiling to this exact live host/instance. This does
    /// not grant an origin, filesystem path, credential or listener address.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_io(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        expected_digest: [u8; 32],
        expected_revision: u64,
        requested: &BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
    ) -> Result<IoBinding> {
        self.bind_io_kind(
            host,
            instance,
            expected_digest,
            expected_revision,
            requested,
            expires,
            now,
            false,
            None,
        )
    }
    /// Explicit trusted-host approval of one fixed service run on this exact instance.
    /// The package profile is only a ceiling. This does not grant a listener address,
    /// remote principal, content access or outbound endpoint, nor restore a persisted approval.
    /// Rebinding/renewal on the same instance is rejected even after all handles are dropped.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_service_run(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        expected_digest: [u8; 32],
        expected_revision: u64,
        requested: &BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
    ) -> Result<IoBinding> {
        if !requested.contains(&IoCapability::HttpListen)
            || !requested.contains(&IoCapability::HttpPublish)
        {
            return Err(Error::Invalid("service run requires listen and publish").into());
        }
        self.bind_io_kind(
            host,
            instance,
            expected_digest,
            expected_revision,
            requested,
            expires,
            now,
            true,
            None,
        )
    }
    /// Explicit one-shot run approval with cumulative task and byte ceilings.
    /// Only packages declaring the budget feature are accepted; no implicit defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_budgeted_service_run(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        expected_digest: [u8; 32],
        expected_revision: u64,
        requested: &BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
        budget: ServiceRunBudget,
    ) -> Result<IoBinding> {
        if !requested.contains(&IoCapability::HttpListen)
            || !requested.contains(&IoCapability::HttpPublish)
        {
            return Err(Error::Invalid("service run requires listen and publish").into());
        }
        self.bind_io_kind(
            host,
            instance,
            expected_digest,
            expected_revision,
            requested,
            expires,
            now,
            true,
            Some(budget),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn bind_io_kind(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        expected_digest: [u8; 32],
        expected_revision: u64,
        requested: &BTreeSet<IoCapability>,
        expires: u64,
        now: u64,
        service_run: bool,
        run_budget: Option<ServiceRunBudget>,
    ) -> Result<IoBinding> {
        let package = instance.package.package();
        let selection = self.checked_selection(
            &package.manifest().package_id,
            expected_digest,
            expected_revision,
        )?;
        self.validate_instance(host, instance)?;
        if !instance.control.active()
            || package.digest() != expected_digest
            || instance.connection().package_digest() != Some(expected_digest)
            || requested.is_empty()
            || !requested.is_subset(package.io_capabilities())
            || !requested.is_subset(&selection.approved_io)
        {
            return Err(Error::Invalid("unapproved IO binding").into());
        }
        IoBinding::new(
            self,
            host,
            instance,
            requested.clone(),
            expires,
            now,
            service_run,
            run_budget,
        )
        .map_err(ManagerError::Io)
    }
    pub fn set_enabled(
        &mut self,
        id: &str,
        digest: [u8; 32],
        enabled: bool,
        revision: u64,
    ) -> Result<()> {
        let selection = self.checked_selection(id, digest, revision)?;
        if selection.enabled == enabled {
            return Ok(());
        }
        self.revoke_required_tree(id);
        self.registry.set_enabled(id, digest, enabled, revision)?;
        Ok(())
    }
    pub fn remove(&mut self, id: &str, revision: u64) -> Result<()> {
        self.check_revision(revision)?;
        if self.selection(id).is_none() {
            return Err(Error::NotFound.into());
        }
        self.revoke_required_tree(id);
        self.registry.remove(id, revision)?;
        Ok(())
    }
    pub fn dependency(&self, caller: &str, slot: &str) -> Option<&LockedDependency> {
        self.registry.dependency(caller, slot)
    }
    /// Explicit host approval of an immutable provider for a declared caller slot.
    /// Approval stores no scope, expiry, process identity or reusable execution authority.
    #[allow(clippy::too_many_arguments)]
    pub fn approve_dependency(
        &mut self,
        caller: &str,
        caller_digest: [u8; 32],
        slot: &str,
        provider: &str,
        provider_digest: [u8; 32],
        revision: u64,
    ) -> Result<()> {
        self.checked_selection(caller, caller_digest, revision)?;
        self.checked_selection(provider, provider_digest, revision)?;
        if self.registry.dependency(caller, slot).is_some_and(|lock| {
            lock.caller_digest == caller_digest
                && lock.provider_id == provider
                && lock.provider_digest == provider_digest
        }) {
            return Ok(());
        }
        self.revoke_required_tree(caller);
        self.registry.approve_dependency(
            caller,
            caller_digest,
            slot,
            provider,
            provider_digest,
            revision,
        )?;
        Ok(())
    }
    pub fn remove_dependency(&mut self, caller: &str, slot: &str, revision: u64) -> Result<()> {
        self.check_revision(revision)?;
        if self.registry.dependency(caller, slot).is_none() {
            return Err(Error::NotFound.into());
        }
        self.revoke_required_tree(caller);
        self.registry.remove_dependency(caller, slot, revision)?;
        Ok(())
    }
    /// Restored records select immutable code only. A fresh live route requires actual managed
    /// endpoints from this manager and a new trusted scope/deadline in the current host.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_locked_dependency(
        &self,
        host: &HostRuntime,
        caller: &ManagedInstance,
        provider: &ManagedInstance,
        slot: &str,
        scope: &str,
        expires: u64,
        now: u64,
    ) -> Result<Dependency> {
        if !self.owns(caller) || !self.owns(provider) {
            return Err(Error::Invalid("foreign managed dependency instance").into());
        }
        let caller_package = caller.package.package();
        let provider_package = provider.package.package();
        let (lock, _, _) = self
            .registry
            .resolve_dependency(&caller_package.manifest().package_id, slot)?;
        if lock.caller_digest != caller_package.digest()
            || lock.provider_digest != provider_package.digest()
            || lock.provider_id != provider_package.manifest().package_id
        {
            return Err(Error::RevisionConflict.into());
        }
        let required = caller_package.dependency(slot)?;
        Dependency::bind(
            host,
            Endpoint {
                package: &caller.package,
                connection: &caller.connection,
            },
            Endpoint {
                package: &provider.package,
                connection: &provider.connection,
            },
            Spec {
                handler: &required.handler,
                input_type: &required.input_type,
                output_type: &required.output_type,
                scope,
                expires,
            },
            now,
        )
        .map_err(ManagerError::Dependency)
    }
    /// Validate current enabled state and package bytes, then bind the stored approved ceiling.
    pub fn connect(&mut self, id: &str, host: &mut HostRuntime) -> Result<ManagedInstance> {
        if self.prune() >= MAX_INSTANCES {
            return Err(Error::Limit.into());
        }
        let (package, selection) = self.registry.resolve_enabled(id)?;
        let package = PreparedPackage::new(package, self.limits)?;
        let connection = package.connect_approved(host, &selection.approved)?;
        let revocation = match host.revocation(&connection) {
            Ok(v) => v,
            Err(e) => {
                let _ = host.disconnect(&connection);
                return Err(e.into());
            }
        };
        let control = Arc::new(Control {
            binding: connection.binding(),
            io: package.package().io_declaration().and_then(|d| {
                d.budget.as_ref().map(|b| {
                    Arc::new(IoContext::new(
                        b,
                        d.service_run.as_ref().map(|run| run.max_duration_ms),
                        d.service_run
                            .as_ref()
                            .and_then(|run| run.budget.as_ref())
                            .map(|budget| ServiceRunBudget {
                                max_jobs: budget.max_jobs,
                                max_bytes: budget.max_bytes,
                            }),
                    ))
                })
            }),
            revocation,
            cancel: Cancellation::default(),
        });
        self.instances
            .entry(id.into())
            .or_default()
            .push(Arc::downgrade(&control));
        Ok(ManagedInstance {
            package,
            connection,
            control,
        })
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        for controls in self.instances.values() {
            for control in controls.iter().filter_map(Weak::upgrade) {
                control.stop();
            }
        }
    }
}
