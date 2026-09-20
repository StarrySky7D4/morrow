//! Private execution state. No host reference or callback survives a guest step.
//! This is not an authorization layer or a public asynchronous plugin API.
use super::*;
use wasmi::{Memory, TypedResumableCall, TypedResumableCallHostTrap, Val};

const YIELD: i32 = 0x4d52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Core,
    Dependency,
    Io,
}

#[derive(Clone)]
pub(super) struct CallToken {
    session: Arc<()>,
    sequence: u32,
}

pub(super) struct PendingCall {
    pub token: CallToken,
    pub kind: Kind,
    pub bytes: Vec<u8>,
    memory: Memory,
    output: usize,
    capacity: usize,
}

pub(super) fn suspend(
    state: &mut State,
    kind: Kind,
    bytes: Vec<u8>,
    memory: Memory,
    output: usize,
    capacity: usize,
) -> wasmi::Error {
    if state.pending.is_some() {
        return trap(state, Fault::Trap);
    }
    state.pending = Some(PendingCall {
        token: CallToken {
            session: state.session.clone(),
            sequence: state.calls,
        },
        kind,
        bytes,
        memory,
        output,
        capacity,
    });
    wasmi::Error::i32_exit(YIELD)
}

pub(super) struct Execution {
    store: Store<State>,
    continuation: Option<TypedResumableCallHostTrap<i32>>,
    outcome: Option<Result<i32, Fault>>,
}

impl Execution {
    pub fn start(
        runner: &Runner,
        input: Option<&[u8]>,
        cancel: Cancellation,
    ) -> Result<Self, Fault> {
        if runner.task_abi != input.is_some() {
            return Err(Fault::UnsupportedAbi);
        }
        if input.is_some_and(|b| b.is_empty() || b.len() > MAX_TASK_BYTES) {
            return Err(Fault::Limits);
        }
        if let Some(fault) = cancel.fault() {
            return Err(fault);
        }
        let state = State {
            task: input.map(|input| TaskState {
                input: input.to_vec(),
                read: false,
                completion: None,
            }),
            dependency: runner.dependency_abi,
            io: runner.io_abi,
            pending: None,
            session: Arc::new(()),
            limits: StoreLimitsBuilder::new()
                .memory_size(runner.limits.memory_bytes)
                .memories(1)
                .tables(1)
                .table_elements(4096)
                .instances(1)
                .trap_on_grow_failure(true)
                .build(),
            cancel,
            calls: 0,
            max_calls: runner.limits.host_calls,
            stopped: None,
        };
        let mut store = Store::new(&runner.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(runner.limits.fuel).expect("fuel enabled");
        let mut linker = Linker::new(&runner.engine);
        linker
            .func_wrap("morrow_v1", "exchange", host_exchange)
            .expect("one fixed import");
        if runner.task_abi {
            linker
                .func_wrap("morrow_task_v1", "read_input", task_read)
                .expect("task input import");
            linker
                .func_wrap("morrow_task_v1", "complete", task_complete)
                .expect("task result import");
        }
        if runner.dependency_abi {
            linker
                .func_wrap("morrow_dependency_v1", "call", dependency_call)
                .expect("dependency call import");
        }
        if runner.io_abi {
            linker
                .func_wrap("morrow_io_v1", "call", io_call)
                .expect("io call import");
        }
        let step = (|| {
            let instance = linker
                .instantiate_and_start(&mut store, &runner.module)
                .map_err(|_| Fault::Limits)?;
            let func = instance
                .get_typed_func::<(), i32>(&store, "morrow_run")
                .map_err(|_| Fault::UnsupportedAbi)?;
            func.call_resumable(&mut store, ()).map_err(map_trap)
        })();
        let mut execution = Self {
            store,
            continuation: None,
            outcome: None,
        };
        execution.accept(step);
        Ok(execution)
    }

    fn terminate(&mut self, outcome: Result<i32, Fault>) {
        self.continuation = None;
        self.store.data_mut().pending = None;
        self.outcome = Some(outcome);
    }

    // Preserve the original import-boundary fault over a later cancellation,
    // matching the synchronous import's `trap(state, fault)` precedence.
    fn stop(&mut self, fault: Fault) {
        if self.store.data().stopped.is_none() {
            self.store.data_mut().stopped = Some(fault.clone());
        }
        self.terminate(Err(fault));
    }

    fn accept(&mut self, step: Result<TypedResumableCall<i32>, Fault>) {
        if let Some(fault) = self
            .store
            .data()
            .stopped
            .clone()
            .or_else(|| self.store.data().cancel.fault())
        {
            self.terminate(Err(fault));
            return;
        }
        match step {
            Ok(TypedResumableCall::Finished(value)) => self.terminate(Ok(value)),
            Ok(TypedResumableCall::HostTrap(trap))
                if trap.host_error().i32_exit_status() == Some(YIELD)
                    && self.store.data().pending.is_some() =>
            {
                self.continuation = Some(trap);
            }
            Ok(TypedResumableCall::OutOfFuel(_)) => self.terminate(Err(Fault::Limits)),
            Ok(_) => self.terminate(Err(Fault::Trap)),
            Err(fault) => self.terminate(Err(fault)),
        }
    }

    /// Cancellation also gates the interval between yielding and invoking a callback.
    pub fn pending(&mut self) -> Option<&PendingCall> {
        if let Some(fault) = self.store.data().cancel.fault() {
            self.stop(fault);
        }
        self.store.data().pending.as_ref()
    }

    /// The token is process-local correlation, not a grant. The broker must check
    /// real authority before dispatch and delivery. Mismatched replies consume nothing.
    pub fn resume(
        &mut self,
        token: &CallToken,
        response: Result<Vec<u8>, ()>,
    ) -> Result<(), Fault> {
        let Some(pending) = self.store.data().pending.as_ref() else {
            return Err(Fault::TaskProtocol);
        };
        if !Arc::ptr_eq(&token.session, &pending.token.session)
            || token.sequence != pending.token.sequence
        {
            return Err(Fault::TaskProtocol);
        }
        if let Some(fault) = self.store.data().cancel.fault() {
            self.stop(fault);
            return Ok(());
        }
        let pending = self
            .store
            .data_mut()
            .pending
            .take()
            .expect("checked pending");
        let continuation = self.continuation.take().expect("pending continuation");
        let returned = match response {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= pending.capacity => {
                if pending
                    .memory
                    .write(&mut self.store, pending.output, &bytes)
                    .is_err()
                {
                    self.terminate(Err(Fault::Trap));
                    return Ok(());
                }
                bytes.len() as i32
            }
            Err(()) if pending.kind != Kind::Dependency => -1,
            _ => {
                self.stop(if pending.kind == Kind::Dependency {
                    Fault::TaskProtocol
                } else {
                    Fault::Trap
                });
                return Ok(());
            }
        };
        let step = continuation
            .resume(&mut self.store, &[Val::I32(returned)])
            .map_err(map_trap);
        self.accept(step);
        Ok(())
    }

    pub fn finish(mut self) -> TaskRun {
        let outcome = self
            .store
            .data()
            .stopped
            .clone()
            .or_else(|| self.store.data().cancel.fault())
            .map_or_else(
                || self.outcome.take().unwrap_or(Err(Fault::TaskProtocol)),
                Err,
            );
        let outcome = if self.store.data().task.is_some()
            && outcome.is_ok()
            && (outcome != Ok(0)
                || self
                    .store
                    .data()
                    .task
                    .as_ref()
                    .is_none_or(|t| t.completion.is_none()))
        {
            Err(Fault::TaskProtocol)
        } else {
            outcome
        };
        let completion = if outcome.is_ok() {
            self.store
                .data_mut()
                .task
                .as_mut()
                .and_then(|t| t.completion.take())
        } else {
            None
        };
        TaskRun {
            report: Report {
                outcome,
                host_calls: self.store.data().calls,
                fuel_remaining: self.store.get_fuel().unwrap_or(0),
            },
            completion,
        }
    }
}

fn map_trap(error: wasmi::Error) -> Fault {
    if error.as_trap_code() == Some(wasmi::TrapCode::OutOfFuel) {
        Fault::Limits
    } else {
        Fault::Trap
    }
}

#[cfg(test)]
mod tests;
