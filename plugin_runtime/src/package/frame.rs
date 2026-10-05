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

    /// End a failed admitted directory call without replaying or re-entering the guest.
    pub(crate) fn abort(&mut self, fault: Fault) {
        self.execution.abort(fault);
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
        self.start_frame(input, cancel, true, false)
    }

    /// Start the frame in IO-only mode: every `Core` call is denied.
    pub(super) fn start_io_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
    ) -> Result<FrameExecution, TaskRun> {
        self.start_frame(input, cancel, false, false)
    }

    /// Only the explicitly negotiated mutation package uses this owned frame.
    /// The worker must still bind a live trusted permit and original owner.
    pub(crate) fn start_mutation_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
    ) -> Result<FrameExecution, TaskRun> {
        self.start_frame(input, cancel, false, true)
    }

    /// Only the negotiated independent directory import; Core remains denied.
    pub(crate) fn start_directory_frame(
        &self, input: &[u8], cancel: Cancellation,
    ) -> Result<FrameExecution, TaskRun> {
        let result = if input.len() <= crate::MAX_DIRECTORY_REQUEST_BYTES
            && self.runner.directory_abi
            && self.package.manifest().required_features.iter().any(|feature|
                feature == morrow_core::plugin_package::DIRECTORY_REQUEST_FEATURE)
        {
            Execution::start(&self.runner, Some(input), cancel)
        } else {
            Err(Fault::UnsupportedAbi)
        };
        result.map(|execution| FrameExecution {
            execution, allow_content: false, denied_core: false,
        }).map_err(|fault| TaskRun {
            report: Report { outcome: Err(fault), host_calls: 0,
                fuel_remaining: self.limits.fuel }, completion: None,
        })
    }
    fn start_frame(
        &self,
        input: &[u8],
        cancel: Cancellation,
        allow_content: bool,
        mutation: bool,
    ) -> Result<FrameExecution, TaskRun> {
        if mutation != self.package.mutation_enabled()
            || (mutation && !self.runner.mutation_abi)
            || (!mutation && !self.runner.io_abi)
        {
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
