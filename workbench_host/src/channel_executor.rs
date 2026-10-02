//! One native channel executor retaining its original host owner through failure.
//! Storage maintenance is independent of execution and never authorizes replay.
use morrow_core::task::Invocation;
use morrow_plugin_runtime::{
    channel::ChannelBroker, io_jobs::ManagedHostOwner, manager::ManagedInstance,
    package::TaskReport,
};
use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

pub(crate) struct Exit<O> {
    pub(crate) owner: O,
    pub(crate) report: Option<TaskReport>,
    pub(crate) error: String,
    pub(crate) repair: bool,
}
impl<O> Exit<O> {
    pub(crate) fn requires_cleanup(&self) -> bool {
        self.repair || !self.error.is_empty() || self.report.as_ref().is_none_or(failed_report)
    }
}

fn failed_report(report: &TaskReport) -> bool {
    report.failure.is_some() || report.execution.outcome.is_err() || report.output.is_none()
}

/// A failed native spawn has not run the invocation or maintenance hook.
/// The original owner is returned for explicit restoration, never reopened.
pub(crate) struct SpawnFailure<O> {
    pub(crate) owner: O,
    pub(crate) error: io::Error,
}

/// The trusted caller supplies its already validated original instance/broker
/// pair. Neither value may come from an untrusted invocation or replacement grant.
pub(crate) fn spawn<O, C>(
    owner: O,
    broker: Arc<ChannelBroker>,
    instance: Arc<ManagedInstance>,
    invocation: Invocation,
    clock: C,
    max_output_bytes: u32,
) -> Result<JoinHandle<Exit<O>>, SpawnFailure<O>>
where
    O: ManagedHostOwner,
    C: FnMut() -> u64 + Send + 'static,
{
    spawn_inner(
        owner,
        broker,
        instance,
        invocation,
        clock,
        max_output_bytes,
        None,
    )
}

/// Exercise real native creation failure without exhausting thread resources.
/// This only selects the actual Builder stack reservation; no result is mocked.
#[cfg(feature = "fault-injection")]
#[allow(dead_code)]
pub(crate) fn spawn_with_stack_size<O, C>(
    owner: O,
    broker: Arc<ChannelBroker>,
    instance: Arc<ManagedInstance>,
    invocation: Invocation,
    clock: C,
    max_output_bytes: u32,
    stack_bytes: usize,
) -> Result<JoinHandle<Exit<O>>, SpawnFailure<O>>
where
    O: ManagedHostOwner,
    C: FnMut() -> u64 + Send + 'static,
{
    spawn_inner(
        owner,
        broker,
        instance,
        invocation,
        clock,
        max_output_bytes,
        Some(stack_bytes),
    )
}

fn spawn_inner<O, C>(
    owner: O,
    broker: Arc<ChannelBroker>,
    instance: Arc<ManagedInstance>,
    invocation: Invocation,
    clock: C,
    max_output_bytes: u32,
    stack_bytes: Option<usize>,
) -> Result<JoinHandle<Exit<O>>, SpawnFailure<O>>
where
    O: ManagedHostOwner,
    C: FnMut() -> u64 + Send + 'static,
{
    let handoff = Arc::new(Mutex::new(Some(owner)));
    let child_handoff = handoff.clone();
    let stop_instance = instance.clone();
    let mut builder = thread::Builder::new().name("morrow-channel-executor".into());
    if let Some(stack_bytes) = stack_bytes {
        builder = builder.stack_size(stack_bytes);
    }
    match builder.spawn(move || {
        let owner = child_handoff
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .expect("original channel owner handoff");
        execute(
            owner,
            &broker,
            &instance,
            &invocation,
            clock,
            max_output_bytes,
        )
    }) {
        Ok(worker) => Ok(worker),
        Err(error) => {
            // Revoke before any fallible host restoration or resource observation.
            stop_instance.request_stop();
            let owner = handoff
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
                .expect("failed native spawn retains original channel owner");
            Err(SpawnFailure { owner, error })
        }
    }
}

fn execute<O: ManagedHostOwner>(
    mut owner: O,
    broker: &ChannelBroker,
    instance: &ManagedInstance,
    invocation: &Invocation,
    mut clock: impl FnMut() -> u64,
    max_output_bytes: u32,
) -> Exit<O> {
    let ran = catch_unwind(AssertUnwindSafe(|| {
        owner.with_managed_runtime(|manager, host| {
            broker.run_invocation(manager, host, instance, invocation, &mut clock)
        })
    }));
    let (report, mut error) = match ran {
        Ok(Some(report))
            if report
                .output
                .as_ref()
                .is_none_or(|output| output.bytes.len() <= max_output_bytes as usize) =>
        {
            // run_invocation already stops the authentic Control on a failed
            // TaskReport. Retain that exact owned report and its first cause.
            (Some(report), String::new())
        }
        Ok(Some(_)) => {
            instance.request_stop();
            (
                None,
                "channel output exceeds the private result bound".into(),
            )
        }
        Ok(None) => {
            instance.request_stop();
            (
                None,
                "original managed owner unavailable; outcome Unknown".into(),
            )
        }
        Err(_) => {
            instance.request_stop();
            (
                None,
                "channel executor panicked; business outcome Unknown".into(),
            )
        }
    };
    // A maintenance panic must not unwind away the sole owner. It is not a
    // successful seal, rollback, or permission to replay the completed task.
    let maintenance = catch_unwind(AssertUnwindSafe(|| owner.finish_io()));
    let repair = !matches!(&maintenance, Ok(Ok(())));
    if repair {
        instance.request_stop();
        // A retained failed TaskReport is already the primary diagnostic.
        // repair records maintenance failure independently, without replacing it.
        if error.is_empty() && !report.as_ref().is_some_and(failed_report) {
            error = if maintenance.is_err() {
                "original storage maintenance panicked; outcome Unknown"
            } else {
                "original storage maintenance failed; outcome Unknown"
            }
            .into();
        }
    }
    Exit {
        owner,
        report,
        error,
        repair,
    }
}
