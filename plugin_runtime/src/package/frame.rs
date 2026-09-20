//! Owned package execution only; response binding and authority remain in the worker.
use super::PreparedPackage;
use crate::continuation::{CallToken, Execution, Kind, PendingCall};
use crate::{Cancellation, Fault, Report, TaskRun};

pub(crate) struct FrameExecution {
    execution: Execution,
    allow_content: bool,
    denied_core: bool,
}

impl FrameExecution {
    /// Deliver the result of a host call to the pending call site.
    pub(crate) fn resume(
        &mut self,
        token: &CallToken,
        result: Result<Vec<u8>, ()>,
    ) -> Result<(), Fault> {
        self.execution.resume(token, result)
    }

    /// Finalize the frame; an unanswered import cannot become a successful result.
    ///
    /// If any `Core` call was denied while running in IO-only mode, the
    /// completed run is discarded in favour of a `TaskProtocol` fault:
    /// the protocol violation dominates any later cancellation or
    /// completion state.
    pub(crate) fn finish(self) -> TaskRun {
        let mut run = self.execution.finish();
        if self.denied_core {
            run.completion = None;
            run.report.outcome = Err(Fault::TaskProtocol);
        }
        run
    }
}

impl PreparedPackage {
    /// Allow core routing; the worker must still enforce actual content authority.
    pub(crate) fn start_service_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
    ) -> Result<FrameExecution, TaskRun> {
        self.start_frame(input, cancel, true)
    }

    /// Start the frame in IO-only mode: every `Core` call is denied.
    pub(super) fn start_io_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
    ) -> Result<FrameExecution, TaskRun> {
        self.start_frame(input, cancel, false)
    }

    fn start_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
        allow_content: bool,
    ) -> Result<FrameExecution, TaskRun> {
        if !self.runner.io_abi {
            return Err(TaskRun {
                report: Report {
                    outcome: Err(Fault::UnsupportedAbi),
                    host_calls: 0,
                    fuel_remaining: self.limits.fuel,
                },
                completion: None,
            });
        }

        match Execution::start(&self.runner, Some(input), cancel) {
            Ok(execution) => Ok(FrameExecution {
                execution,
                allow_content,
                denied_core: false,
            }),
            Err(fault) => Err(TaskRun {
                report: Report {
                    outcome: Err(fault),
                    host_calls: 0,
                    fuel_remaining: self.limits.fuel,
                },
                completion: None,
            }),
        }
    }
}

// Recheck every denied call; never unwrap a second cancellation-sensitive poll.
impl FrameExecution {
    pub(crate) fn pending(&mut self) -> Option<&PendingCall> {
        loop {
            let denied_token = {
                let call = self.execution.pending()?;
                if call.kind == Kind::Core && !self.allow_content {
                    Some(call.token.clone())
                } else {
                    None
                }
            };
            if let Some(token) = denied_token {
                self.denied_core = true;
                self.execution
                    .resume(&token, Err(()))
                    .expect("same pending call");
            } else {
                return self.execution.pending();
            }
        }
    }
}

#[cfg(test)]
mod tests;
