//! Guest-requested dependency calls routed through the current approved managed graph.
//! One level only: providers cannot themselves request dependencies in this profile.
use crate::{
    Cancellation, Fault,
    dependency::{Dependency, DependencyOutput, Endpoint, Error, Result},
    manager::{ManagedInstance, Manager},
    shared_objects::{Mapping, SharedObjects},
};
use morrow_core::{
    dependency_call::Request,
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{InstancePhase, Revocation},
    shared_object::Descriptor,
    task::Invocation,
};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};

/// Trusted workflow policy. Time uses the same monotonic clock as host grants.
pub struct Context<'a> {
    pub scope: &'a str,
    pub expires: u64,
    pub max_calls: u32,
}
impl<'a> Context<'a> {
    pub fn new(scope: &'a str, expires: u64) -> Self {
        Self {
            scope,
            expires,
            max_calls: 8,
        }
    }
}
/// Opaque proof for the caller's final transformed bytes, including *every* dependency used.
pub struct RoutedOutput {
    host: HostBinding,
    caller: ConnectionBinding,
    digest: [u8; 32],
    revocation: Revocation,
    cancel: Cancellation,
    expires: u64,
    last_tick: AtomicU64,
    output_type: String,
    bytes: Vec<u8>,
    // Dependency::drop revokes the output; retain the routes with their proofs.
    routes: Vec<(Dependency, DependencyOutput)>,
}
impl RoutedOutput {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn output_type(&self) -> &str {
        &self.output_type
    }
    pub fn dependency_calls(&self) -> usize {
        self.routes.len()
    }
    pub fn validate(&self, host: &HostRuntime, caller: &Connection, now: u64) -> Result<()> {
        if host.binding() != self.host
            || caller.binding() != self.caller
            || caller.package_digest() != Some(self.digest)
            || host.connection_phase(caller) != Ok(InstancePhase::Ready)
        {
            return Err(Error::Denied);
        }
        self.validate_liveness(now)?;
        for (_, output) in &self.routes {
            output.validate(host, caller, now)?;
        }
        Ok(())
    }
    pub fn validate_liveness(&self, now: u64) -> Result<()> {
        if self.revocation.is_revoked()
            || now >= self.expires
            || now < self.last_tick.load(Ordering::Acquire)
        {
            return Err(Error::Denied);
        }
        if let Some(fault) = self.cancel.fault() {
            return Err(Error::Execution(fault));
        }
        for (_, output) in &self.routes {
            output.validate_liveness(now)?;
        }
        self.last_tick
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                (now >= old).then_some(now)
            })
            .map_err(|_| Error::Denied)?;
        Ok(())
    }
}
struct PublishedInput<'a> {
    objects: &'a mut SharedObjects,
    descriptor: Descriptor,
    mapping: Option<Mapping>,
}
impl Drop for PublishedInput<'_> {
    fn drop(&mut self) {
        // Retire removes all permissions, including a caller grant whose map failed.
        let _ = self.objects.retire(&self.descriptor);
        self.mapping.take();
    }
}
fn endpoint(instance: &ManagedInstance) -> Endpoint<'_> {
    Endpoint {
        package: instance.package(),
        connection: instance.connection(),
    }
}

/// Hosts explicitly supply already connected provider instances. Guests supply only slot/input/ID.
/// Every failure poisons the enclosing pure transform; no partial output becomes a content intent.
#[allow(clippy::too_many_arguments)]
pub fn run(
    manager: &Manager,
    host: &mut HostRuntime,
    objects: &mut SharedObjects,
    caller: &ManagedInstance,
    providers: &[&ManagedInstance],
    input: &Invocation,
    context: Context<'_>,
    mut clock: impl FnMut() -> u64,
    cancel: Cancellation,
) -> Result<RoutedOutput> {
    if context.max_calls == 0
        || context.max_calls > 16
        || providers.len() > 16
        || context.scope.is_empty()
        || context.scope.len() > 256
        || context.scope.chars().any(char::is_control)
        || input.transform().is_none()
    {
        return Err(Error::Limit);
    }
    manager
        .validate_instance(host, caller)
        .map_err(|_| Error::Denied)?;
    let cancel = Cancellation::linked(cancel, caller.cancellation());
    if let Some(fault) = cancel.fault() {
        return Err(Error::Execution(fault));
    }
    let mut last_tick = clock();
    if context.expires <= last_tick {
        return Err(Error::Denied);
    }
    let revocation = host.revocation(caller.connection())?;
    let mut calls = BTreeSet::new();
    let mut routes = Vec::new();
    let mut callback_error = None;
    let prefix = input
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let report = caller.package().run_dependency_transform(
        input,
        &mut |bytes| {
            let result = (|| -> Result<Vec<u8>> {
                if let Some(fault) = cancel.fault() {
                    return Err(Error::Execution(fault));
                }
                manager
                    .validate_instance(host, caller)
                    .map_err(|_| Error::Denied)?;
                let request = Request::decode(bytes)?;
                if calls.len() >= context.max_calls as usize
                    || !calls.insert(request.call_id().to_owned())
                {
                    return Err(Error::Limit);
                }
                let lock = manager
                    .dependency(
                        &caller.package().package().manifest().package_id,
                        request.slot(),
                    )
                    .ok_or(Error::Denied)?;
                let mut matching = providers.iter().copied().filter(|p| {
                    p.package().package().manifest().package_id == lock.provider_id
                        && p.package().package().digest() == lock.provider_digest
                });
                let provider = matching.next().ok_or(Error::Denied)?;
                if matching.next().is_some()
                    || provider
                        .package()
                        .package()
                        .manifest()
                        .required_features
                        .iter()
                        .any(|f| f == morrow_core::plugin_package::DEPENDENCY_CALLS_FEATURE)
                {
                    return Err(Error::Denied);
                }
                let now = clock();
                if now < last_tick || now >= context.expires {
                    return Err(Error::Denied);
                }
                last_tick = now;
                let route = manager
                    .bind_locked_dependency(
                        host,
                        caller,
                        provider,
                        request.slot(),
                        context.scope,
                        context.expires,
                        now,
                    )
                    .map_err(|_| Error::Denied)?;
                let descriptor = objects.publish(
                    host,
                    caller.connection(),
                    context.scope,
                    request.input(),
                    now,
                )?;
                let mut temporary = PublishedInput {
                    objects,
                    descriptor,
                    mapping: None,
                };
                let lease = temporary.objects.grant(
                    host,
                    caller.connection(),
                    &temporary.descriptor,
                    context.scope,
                    context.expires,
                    clock(),
                )?;
                temporary.mapping = Some(temporary.objects.map(
                    host,
                    caller.connection(),
                    &lease,
                    clock(),
                )?);
                let output = route.run(
                    temporary.objects,
                    host,
                    endpoint(caller),
                    endpoint(provider),
                    temporary.mapping.as_ref().expect("created mapping"),
                    &format!("{prefix}-{}", calls.len()),
                    &mut clock,
                    Cancellation::linked(cancel.clone(), provider.cancellation()),
                )?;
                let now = clock();
                if now < last_tick || now >= context.expires {
                    return Err(Error::Denied);
                }
                last_tick = now;
                if let Some(fault) = cancel.fault() {
                    return Err(Error::Execution(fault));
                }
                output.validate(host, caller.connection(), now)?;
                let response = request.encode_response(output.output_type(), output.bytes())?;
                routes.push((route, output));
                Ok(response)
            })();
            result.map_err(|error| {
                callback_error = Some(error);
            })
        },
        cancel.clone(),
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    match report.execution.outcome {
        Ok(0) => (),
        Ok(_) => return Err(Error::Execution(Fault::TaskProtocol)),
        Err(e) => return Err(Error::Execution(e)),
    }
    if let Some(failure) = report.failure {
        return Err(Error::Plugin(failure));
    }
    manager
        .validate_instance(host, caller)
        .map_err(|_| Error::Denied)?;
    let completed = clock();
    if completed < last_tick {
        return Err(Error::Denied);
    }
    let value = report.output.ok_or(Error::Execution(Fault::TaskProtocol))?;
    let output = RoutedOutput {
        host: host.binding(),
        caller: caller.connection().binding(),
        digest: caller.package().package().digest(),
        revocation,
        cancel,
        expires: context.expires,
        last_tick: AtomicU64::new(last_tick),
        output_type: value.type_id,
        bytes: value.bytes,
        routes,
    };
    output.validate(host, caller.connection(), completed)?;
    Ok(output)
}
