//! Guest-requested dependency calls routed through the current approved managed graph.
//! Explicit graph execution has bounded depth/total work and rejects active-package reentry.
use crate::{
    Cancellation, Fault,
    dependency::{Dependency, DependencyOutput, Endpoint, Error, Result},
    manager::{ManagedInstance, Manager},
    package::TaskReport,
    shared_objects::{self, Mapping, SharedObjects},
};
use morrow_core::{
    dependency_call::Request,
    dispatch::{Connection, ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{InstancePhase, Revocation},
    shared_object::Descriptor,
    task::{Invocation, Transform},
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
    node_cancellations: Vec<Cancellation>,
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
            output.validate_host(host, now)?;
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
        for cancel in &self.node_cancellations {
            if let Some(fault) = cancel.fault() {
                return Err(Error::Execution(fault));
            }
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

/// Explicit trusted graph budget. Depth counts edges from the root; total calls span all nodes.
#[derive(Clone, Copy, Debug)]
pub struct GraphLimits {
    pub max_depth: u32,
    pub max_total_calls: u32,
}
impl Default for GraphLimits {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_total_calls: 16,
        }
    }
}
/// Preserve the single-level test.30 contract, including rejecting dependency-enabled providers.
#[allow(clippy::too_many_arguments)]
pub fn run(
    manager: &Manager,
    host: &mut HostRuntime,
    objects: &mut SharedObjects,
    caller: &ManagedInstance,
    providers: &[&ManagedInstance],
    input: &Invocation,
    context: Context<'_>,
    clock: impl FnMut() -> u64,
    cancel: Cancellation,
) -> Result<RoutedOutput> {
    let limits = GraphLimits {
        max_depth: 1,
        max_total_calls: context.max_calls,
    };
    execute(
        manager, host, objects, caller, providers, input, context, limits, false, clock, cancel,
    )
}
/// The host opts into multi-level execution. No guest can change graph budgets or select identities.
#[allow(clippy::too_many_arguments)]
pub fn run_graph(
    manager: &Manager,
    host: &mut HostRuntime,
    objects: &mut SharedObjects,
    caller: &ManagedInstance,
    providers: &[&ManagedInstance],
    input: &Invocation,
    context: Context<'_>,
    limits: GraphLimits,
    clock: impl FnMut() -> u64,
    cancel: Cancellation,
) -> Result<RoutedOutput> {
    execute(
        manager, host, objects, caller, providers, input, context, limits, true, clock, cancel,
    )
}
fn dynamic(instance: &ManagedInstance) -> bool {
    instance
        .package()
        .package()
        .manifest()
        .required_features
        .iter()
        .any(|f| f == morrow_core::plugin_package::DEPENDENCY_CALLS_FEATURE)
}
struct Execution<'a> {
    manager: &'a Manager,
    providers: &'a [&'a ManagedInstance],
    context: Context<'a>,
    limits: GraphLimits,
    graph: bool,
    active: Vec<String>,
    node_cancellations: Vec<Cancellation>,
    total: u32,
    last_tick: u64,
    routes: Vec<(Dependency, DependencyOutput)>,
}
impl Execution<'_> {
    fn tick(&mut self, now: u64) -> Result<()> {
        if now < self.last_tick || now >= self.context.expires {
            return Err(Error::Denied);
        }
        self.last_tick = now;
        Ok(())
    }
    fn node(
        &mut self,
        host: &mut HostRuntime,
        objects: &mut SharedObjects,
        caller: &ManagedInstance,
        input: &Invocation,
        clock: &mut dyn FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<TaskReport> {
        self.manager
            .validate_instance(host, caller)
            .map_err(|_| Error::Denied)?;
        if let Some(fault) = cancel.fault() {
            return Err(Error::Execution(fault));
        }
        let id = &caller.package().package().manifest().package_id;
        if self.active.iter().any(|p| p == id) {
            return Err(Error::Denied);
        }
        if self.active.len() > self.limits.max_depth as usize {
            return Err(Error::Limit);
        }
        self.tick(clock())?;
        self.active.push(id.clone());
        self.node_cancellations.push(cancel.clone());
        // Always unwind the active path on ordinary failures; a panic unwinds the whole task.
        let result = self.node_body(host, objects, caller, input, clock, cancel);
        self.active.pop();
        result
    }
    fn node_body(
        &mut self,
        host: &mut HostRuntime,
        objects: &mut SharedObjects,
        caller: &ManagedInstance,
        input: &Invocation,
        clock: &mut dyn FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<TaskReport> {
        let mut calls = BTreeSet::new();
        let mut callback_error = None;
        let prefix = input
            .digest()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let report = if dynamic(caller) {
            caller.package().run_dependency_transform(
                input,
                &mut |bytes| {
                    let result = (|| -> Result<Vec<u8>> {
                        if let Some(fault) = cancel.fault() {
                            return Err(Error::Execution(fault));
                        }
                        self.manager
                            .validate_instance(host, caller)
                            .map_err(|_| Error::Denied)?;
                        if self.total >= self.limits.max_total_calls
                            || calls.len() >= self.context.max_calls as usize
                        {
                            return Err(Error::Limit);
                        }
                        self.total += 1;
                        let request = Request::decode(bytes)?;
                        if !calls.insert(request.call_id().to_owned()) {
                            return Err(Error::Limit);
                        }
                        let lock = self
                            .manager
                            .dependency(
                                &caller.package().package().manifest().package_id,
                                request.slot(),
                            )
                            .ok_or(Error::Denied)?;
                        let mut matching = self.providers.iter().copied().filter(|p| {
                            p.package().package().manifest().package_id == lock.provider_id
                                && p.package().package().digest() == lock.provider_digest
                        });
                        let provider = matching.next().ok_or(Error::Denied)?;
                        if matching.next().is_some() || (!self.graph && dynamic(provider)) {
                            return Err(Error::Denied);
                        }
                        // A synchronous wait on an active package is a cycle even through another instance.
                        if self.active.iter().any(|id| id == &lock.provider_id) {
                            return Err(Error::Denied);
                        }
                        if self.active.len() > self.limits.max_depth as usize {
                            return Err(Error::Limit);
                        }
                        let now = clock();
                        self.tick(now)?;
                        let route = self
                            .manager
                            .bind_locked_dependency(
                                host,
                                caller,
                                provider,
                                request.slot(),
                                self.context.scope,
                                self.context.expires,
                                now,
                            )
                            .map_err(|_| Error::Denied)?;
                        let descriptor = objects.publish(
                            host,
                            caller.connection(),
                            self.context.scope,
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
                            self.context.scope,
                            self.context.expires,
                            clock(),
                        )?;
                        temporary.mapping = Some(temporary.objects.map(
                            host,
                            caller.connection(),
                            &lease,
                            clock(),
                        )?);
                        let task_id = format!("{prefix}-{}", calls.len());
                        let child_cancel =
                            Cancellation::linked(cancel.clone(), provider.cancellation());
                        let mut child_error = None;
                        let result = route.run_with_executor(
                            temporary.objects,
                            host,
                            endpoint(caller),
                            endpoint(provider),
                            temporary.mapping.as_ref().expect("created mapping"),
                            &task_id,
                            &mut *clock,
                            child_cancel,
                            |objects, host, mapping, request, clock, cancel| {
                                let invocation = Invocation::new_transform(
                                    request.task_id,
                                    Transform {
                                        handler: request.handler.into(),
                                        input_type: request.input_type.into(),
                                        output_type: request.output_type.into(),
                                        input: mapping.bytes().to_vec(),
                                    },
                                )?;
                                self.node(host, objects, provider, &invocation, clock, cancel)
                                    .map_err(|error| {
                                        child_error = Some(error);
                                        shared_objects::Error::Denied
                                    })
                            },
                        );
                        if let Some(error) = child_error {
                            return Err(error);
                        }
                        let output = result?;
                        let now = clock();
                        self.tick(now)?;
                        if let Some(fault) = cancel.fault() {
                            return Err(Error::Execution(fault));
                        }
                        output.validate(host, caller.connection(), now)?;
                        let response =
                            request.encode_response(output.output_type(), output.bytes())?;
                        self.routes.push((route, output));
                        Ok(response)
                    })();
                    result.map_err(|error| {
                        callback_error = Some(error);
                    })
                },
                cancel.clone(),
            )
        } else {
            caller.package().run_task(
                host,
                caller.connection(),
                input,
                &mut *clock,
                cancel.clone(),
            )
        };
        if let Some(error) = callback_error {
            return Err(error);
        }
        match &report.execution.outcome {
            Ok(0) => (),
            Ok(_) => return Err(Error::Execution(Fault::TaskProtocol)),
            Err(e) => return Err(Error::Execution(e.clone())),
        }
        if report.response.is_some() || report.failure.is_some() || report.output.is_none() {
            if let Some(failure) = report.failure {
                return Err(Error::Plugin(failure));
            }
            return Err(Error::Execution(Fault::TaskProtocol));
        }
        self.manager
            .validate_instance(host, caller)
            .map_err(|_| Error::Denied)?;
        if let Some(fault) = cancel.fault() {
            return Err(Error::Execution(fault));
        }
        self.tick(clock())?;
        // No stale sibling/deeper proof may be returned to an upstream guest after revocation.
        for (_, output) in &self.routes {
            output.validate_host(host, self.last_tick)?;
        }
        Ok(report)
    }
}
#[allow(clippy::too_many_arguments)]
fn execute(
    manager: &Manager,
    host: &mut HostRuntime,
    objects: &mut SharedObjects,
    caller: &ManagedInstance,
    providers: &[&ManagedInstance],
    input: &Invocation,
    context: Context<'_>,
    limits: GraphLimits,
    graph: bool,
    mut clock: impl FnMut() -> u64,
    cancel: Cancellation,
) -> Result<RoutedOutput> {
    if context.max_calls == 0
        || context.max_calls > 16
        || providers.len() > 64
        || (!graph && providers.len() > 16)
        || limits.max_depth == 0
        || limits.max_depth > 8
        || limits.max_total_calls == 0
        || limits.max_total_calls > 64
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
    if !dynamic(caller) {
        return Err(Error::Execution(Fault::UnsupportedAbi));
    }
    let cancel = Cancellation::linked(cancel, caller.cancellation());
    if let Some(fault) = cancel.fault() {
        return Err(Error::Execution(fault));
    }
    let now = clock();
    if now >= context.expires {
        return Err(Error::Denied);
    }
    let revocation = host.revocation(caller.connection())?;
    let mut execution = Execution {
        manager,
        providers,
        context,
        limits,
        graph,
        active: Vec::new(),
        node_cancellations: Vec::new(),
        total: 0,
        last_tick: now,
        routes: Vec::new(),
    };
    let report = execution.node(host, objects, caller, input, &mut clock, cancel.clone())?;
    let value = report.output.ok_or(Error::Execution(Fault::TaskProtocol))?;
    let completed = clock();
    execution.tick(completed)?;
    let output = RoutedOutput {
        host: host.binding(),
        caller: caller.connection().binding(),
        digest: caller.package().package().digest(),
        revocation,
        cancel,
        node_cancellations: execution.node_cancellations,
        expires: execution.context.expires,
        last_tick: AtomicU64::new(completed),
        output_type: value.type_id,
        bytes: value.bytes,
        routes: execution.routes,
    };
    output.validate(host, caller.connection(), completed)?;
    Ok(output)
}
