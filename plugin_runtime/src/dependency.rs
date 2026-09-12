//! Host-routed, single-level dependency calls over already authorized immutable input.
//! Runtime bindings and outputs are local authority; descriptions are not transferable grants.
use crate::{
    Cancellation, Fault,
    package::PreparedPackage,
    shared_objects::{self, Lease, Mapping, SharedObjects, TransformRequest},
};
use morrow_core::{
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{InstancePhase, Revocation},
    shared_object::Descriptor,
    task::{PluginFailure, Transform},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Debug)]
pub enum Error {
    Core(morrow_core::Error),
    Shared(shared_objects::Error),
    Execution(Fault),
    Plugin(PluginFailure),
    Denied,
    Limit,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dependency: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<morrow_core::Error> for Error {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl From<shared_objects::Error> for Error {
    fn from(e: shared_objects::Error) -> Self {
        Self::Shared(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy)]
pub struct Endpoint<'a> {
    pub package: &'a PreparedPackage,
    pub connection: &'a Connection,
}
/// One explicit trusted routing approval, not guest-supplied permission or a dependency lockfile.
pub struct Spec<'a> {
    pub handler: &'a str,
    pub input_type: &'a str,
    pub output_type: &'a str,
    pub scope: &'a str,
    pub expires: u64,
}
struct PackageIdentity {
    id: String,
    version: String,
    digest: [u8; 32],
}
impl PackageIdentity {
    fn new(package: &PreparedPackage) -> Self {
        let package = package.package();
        Self {
            id: package.manifest().package_id.clone(),
            version: package.manifest().package_version.clone(),
            digest: package.digest(),
        }
    }
    fn matches(&self, endpoint: Endpoint<'_>) -> bool {
        let package = endpoint.package.package();
        self.id == package.manifest().package_id
            && self.version == package.manifest().package_version
            && self.digest == package.digest()
            && endpoint.connection.package_digest() == Some(self.digest)
    }
}
struct State {
    host: HostBinding,
    caller: ConnectionBinding,
    provider: ConnectionBinding,
    caller_package: PackageIdentity,
    provider_package: PackageIdentity,
    caller_revocation: Revocation,
    provider_revocation: Revocation,
    revoked: AtomicBool,
    last_tick: AtomicU64,
    expires: u64,
    handler: String,
    input_type: String,
    output_type: String,
    scope: String,
    max_input: usize,
}
impl State {
    fn check_liveness(&self, now: u64) -> Result<()> {
        if now >= self.expires
            || now < self.last_tick.load(Ordering::Acquire)
            || self.revoked.load(Ordering::Acquire)
            || self.caller_revocation.is_revoked()
            || self.provider_revocation.is_revoked()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn validate_liveness(&self, now: u64) -> Result<()> {
        self.check_liveness(now)?;
        self.last_tick
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |previous| {
                (now >= previous).then_some(now)
            })
            .map_err(|_| Error::Denied)?;
        Ok(())
    }
    fn check(&self, host: &HostRuntime, caller: &Connection, now: u64) -> Result<()> {
        if host.binding() != self.host
            || caller.binding() != self.caller
            || caller.package_digest() != Some(self.caller_package.digest)
            || host.connection_phase(caller) != Ok(InstancePhase::Ready)
            || host.binding_phase(self.provider) != Ok(InstancePhase::Ready)
        {
            return Err(Error::Denied);
        }
        self.check_liveness(now)
    }
    fn validate(&self, host: &HostRuntime, caller: &Connection, now: u64) -> Result<()> {
        self.check(host, caller, now)?;
        self.validate_liveness(now)
    }
}
/// Cannot be constructed outside this module. Bytes already copied out cannot be recalled;
/// subsequent content submission must call validate, then validate_liveness at final authorization.
/// The original input lease is checked before/after run only; its descriptor is correlation,
/// not a retained authority lease. Retiring input alone does not invalidate an issued output.
pub struct DependencyOutput {
    state: Arc<State>,
    input: Descriptor,
    bytes: Vec<u8>,
}
impl DependencyOutput {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn output_type(&self) -> &str {
        &self.state.output_type
    }
    pub fn input_descriptor(&self) -> &Descriptor {
        &self.input
    }
    pub fn validate(&self, host: &HostRuntime, caller: &Connection, now: u64) -> Result<()> {
        self.state.validate(host, caller, now)
    }
    /// No HostRuntime borrow: usable inside the core's final content authorization callback.
    pub fn validate_liveness(&self, now: u64) -> Result<()> {
        self.state.validate_liveness(now)
    }
}
/// Approval applies to exact live endpoints, one registered handler, scope and deadline.
/// Dropping or revoking the route invalidates all retained outputs; it does not undo commits.
pub struct Dependency {
    state: Arc<State>,
}
struct TemporaryLease<'a> {
    objects: &'a mut SharedObjects,
    lease: Lease,
    mapping: Option<Mapping>,
}
impl Drop for TemporaryLease<'_> {
    fn drop(&mut self) {
        // Revocation first; real mapping ownership is released after future access is denied.
        let _ = self.objects.revoke(&self.lease);
        self.mapping.take();
    }
}
impl Dependency {
    pub fn bind(
        host: &HostRuntime,
        caller: Endpoint<'_>,
        provider: Endpoint<'_>,
        spec: Spec<'_>,
        now: u64,
    ) -> Result<Self> {
        if spec.expires <= now
            || spec.scope.is_empty()
            || spec.scope.len() > 256
            || spec.scope.chars().any(char::is_control)
            || caller.connection.binding() == provider.connection.binding()
            || host.connection_phase(caller.connection) != Ok(InstancePhase::Ready)
            || host.connection_phase(provider.connection) != Ok(InstancePhase::Ready)
            || caller.connection.package_digest() != Some(caller.package.package().digest())
            || provider.connection.package_digest() != Some(provider.package.package().digest())
            || provider.package.package().manifest().guest_abi_version != 2
        {
            return Err(Error::Denied);
        }
        let registration = provider.package.package().transform_handler(&Transform {
            handler: spec.handler.into(),
            input_type: spec.input_type.into(),
            output_type: spec.output_type.into(),
            input: vec![],
        })?;
        Ok(Self {
            state: Arc::new(State {
                host: host.binding(),
                caller: caller.connection.binding(),
                provider: provider.connection.binding(),
                caller_package: PackageIdentity::new(caller.package),
                provider_package: PackageIdentity::new(provider.package),
                caller_revocation: host.revocation(caller.connection)?,
                provider_revocation: host.revocation(provider.connection)?,
                revoked: AtomicBool::new(false),
                last_tick: AtomicU64::new(now),
                expires: spec.expires,
                handler: spec.handler.into(),
                input_type: spec.input_type.into(),
                output_type: spec.output_type.into(),
                scope: spec.scope.into(),
                max_input: registration.max_input_bytes as usize,
            }),
        })
    }
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    fn check_endpoints(
        &self,
        host: &HostRuntime,
        caller: Endpoint<'_>,
        provider: Endpoint<'_>,
        now: u64,
    ) -> Result<()> {
        if !self.state.caller_package.matches(caller)
            || !self.state.provider_package.matches(provider)
            || provider.connection.binding() != self.state.provider
        {
            return Err(Error::Denied);
        }
        self.state.check(host, caller.connection, now)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        objects: &mut SharedObjects,
        host: &mut HostRuntime,
        caller: Endpoint<'_>,
        provider: Endpoint<'_>,
        input: &Mapping,
        task_id: &str,
        mut clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<DependencyOutput> {
        let admission = clock();
        self.check_endpoints(host, caller, provider, admission)?;
        objects.check_mapping_scope(
            host,
            caller.connection,
            input,
            &self.state.scope,
            admission,
        )?;
        if input.bytes().len() > self.state.max_input
            || input.bytes().len() > morrow_core::task::MAX_VALUE_BYTES
        {
            return Err(Error::Limit);
        }
        // Only after endpoint, live lease, scope, allocation and size checks may either clock advance.
        self.state.validate_liveness(admission)?;
        objects.validate_mapping(host, caller.connection, input, admission)?;
        // Recheck source access at the actual grant instant; admission alone cannot authorize
        // sharing a caller lease that expired while the trusted clock advanced.
        let sharing = clock();
        self.check_endpoints(host, caller, provider, sharing)?;
        objects.check_mapping_scope(host, caller.connection, input, &self.state.scope, sharing)?;
        let lease = objects.grant(
            host,
            provider.connection,
            input.descriptor(),
            &self.state.scope,
            self.state.expires,
            sharing,
        )?;
        let mut temporary = TemporaryLease {
            objects,
            lease,
            mapping: None,
        };
        temporary.mapping = Some(temporary.objects.map(
            host,
            provider.connection,
            &temporary.lease,
            clock(),
        )?);
        let report = temporary.objects.run_transform(
            host,
            provider.connection,
            provider.package,
            temporary.mapping.as_ref().expect("created mapping"),
            TransformRequest {
                task_id,
                handler: &self.state.handler,
                input_type: &self.state.input_type,
                output_type: &self.state.output_type,
            },
            &mut clock,
            cancel,
        )?;
        let completed = clock();
        self.check_endpoints(host, caller, provider, completed)?;
        temporary.objects.check_mapping_scope(
            host,
            caller.connection,
            input,
            &self.state.scope,
            completed,
        )?;
        self.state.validate_liveness(completed)?;
        temporary
            .objects
            .validate_mapping(host, caller.connection, input, completed)?;
        if report.execution.host_calls != 0 || report.response.is_some() {
            return Err(Error::Denied);
        }
        match report.execution.outcome {
            Ok(0) => {}
            Ok(_) => return Err(Error::Execution(Fault::TaskProtocol)),
            Err(e) => return Err(Error::Execution(e)),
        }
        if let Some(failure) = report.failure {
            return Err(Error::Plugin(failure));
        }
        let output = report.output.ok_or(Error::Execution(Fault::TaskProtocol))?;
        if output.type_id != self.state.output_type {
            return Err(Error::Denied);
        }
        let result = DependencyOutput {
            state: Arc::clone(&self.state),
            input: input.descriptor().clone(),
            bytes: output.bytes,
        };
        result.validate(host, caller.connection, completed)?;
        // Only the temporary provider lease/map are released; caller input ownership is unchanged.
        drop(temporary);
        Ok(result)
    }
}
impl Drop for Dependency {
    fn drop(&mut self) {
        self.revoke();
    }
}
