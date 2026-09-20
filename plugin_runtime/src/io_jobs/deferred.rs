//! Owned dispatches keep the original owner available while transport waits.
use super::*;
use io_execution::{DispatchObservation, Error, Live};

#[derive(Clone)]
pub(super) struct Guard {
    pub control: Arc<Control>,
    pub validity: Option<service_history::Validity>,
    pub content: Option<ServiceContentAccess>,
}
impl Guard {
    pub(super) fn check(&self, live: &Live) -> io_execution::Result<()> {
        if let Some(validity) = &self.validity {
            validity.check()?;
        }
        self.control
            .authority
            .as_ref()
            .ok_or(Error::Denied)?
            .with_execution_time(|now| {
                if let Some(content) = &self.content {
                    content.check_at(now)?;
                }
                live.check_liveness(now)
            })
    }
}
pub(super) struct PendingDispatch {
    pub operation: String,
    pub guard: Guard,
    pub run: Box<dyn FnOnce() -> io_execution::Result<DispatchObservation> + Send>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn route<O: HostOwner>(
    router: &mut dyn BrokerRouter,
    call: u32,
    parsed: &Request,
    owner: &mut O,
    commands: &Receiver<owner_commands::Command<O>>,
    instance: &ManagedInstance,
    broker: &Broker,
    control: &Arc<Control>,
    lease: &Arc<IoJobLease>,
    cancel: &Cancellation,
    reserved: &mut u64,
    http_guards: &mut Vec<crate::http_io::HttpCallGuard>,
    validity: Option<&service_history::Validity>,
    content: Option<&ServiceContentAccess>,
) -> Result<Result<Vec<u8>, RouterFault>, JobError> {
    let (start, pending, used, expected, uncertain) = {
        let mut context = RouteContext {
            host: checked_runtime(owner, control.host)?,
            instance,
            broker,
            control,
            lease,
            cancel,
            request: parsed,
            reserved,
            used: false,
            expected: None,
            uncertain: false,
            deferred: None,
            http_guard: None,
            service_validity: validity,
            service_content: content,
        };
        let start = router.begin(&mut context, call, parsed);
        if let Some(guard) = context.http_guard.take() {
            http_guards.push(guard);
        }
        (
            start,
            context.deferred.take(),
            context.used,
            context.expected.take(),
            context.uncertain,
        )
    };
    let reply = match (start, pending) {
        (RouteStart::Deferred, Some(pending)) => {
            let PendingDispatch {
                operation,
                guard,
                run,
            } = pending;
            // Retire the live entry on every return/unwind, after the task has
            // joined. Durable Unknown/Observed evidence is never removed.
            struct Retire<'a>(&'a Broker, String);
            impl Drop for Retire<'_> {
                fn drop(&mut self) {
                    self.0.retire(&self.1);
                }
            }
            let _retire = Retire(broker, operation);
            let mut task = match transport_task::TransportTask::spawn(cancel.clone(), run) {
                Ok(task) => task,
                Err(_) => return Ok(Err(RouterFault::Unknown)),
            };
            let observation = loop {
                if let Some(result) = task.poll() {
                    break result;
                }
                // Only the reserved owner lane runs here; later guest jobs and
                // queued service updates keep their original FIFO ordering.
                if let Ok(command) = commands.try_recv() {
                    command.execute(owner, control)?;
                }
                thread::sleep(Duration::from_millis(2));
            };
            let result = match observation {
                Ok(Ok(observation)) => broker.complete_in_job(
                    observation,
                    checked_runtime(owner, control.host)?,
                    instance,
                    |live| guard.check(live),
                ),
                _ => Err(Error::OutcomeUnknown),
            };
            // The observation can be retained even when endpoint authority was
            // independently revoked. Retained evidence is not live delivery.
            match result {
                Ok(response) if http_guards.iter().all(|guard| guard.check().is_ok()) => {
                    Ok(response)
                }
                _ => Err(RouterFault::Unknown),
            }
        }
        (_, Some(pending)) => {
            broker.retire(&pending.operation);
            Err(RouterFault::Unknown)
        }
        (RouteStart::Deferred, None) => Err(if used {
            RouterFault::Unknown
        } else {
            RouterFault::Denied
        }),
        (RouteStart::Ready(reply), None) => {
            if uncertain || (expected.is_some() && reply.is_err()) {
                Err(RouterFault::Unknown)
            } else if reply.as_ref().is_ok_and(|v| expected.as_ref() != Some(v)) {
                Err(if used {
                    RouterFault::Unknown
                } else {
                    RouterFault::Denied
                })
            } else {
                reply
            }
        }
    };
    Ok(reply)
}
