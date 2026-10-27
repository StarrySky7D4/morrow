//! Pure proposal execution has no Store, live connection, callbacks or grants.
use super::PreparedPackage;
use crate::{Cancellation, Fault, Report, continuation::Execution};
use morrow_blob_upload_proposal_v1::Proposal;
use morrow_core::task::{Invocation, PluginFailure, TransformResult};

#[derive(Debug)]
pub struct ProposalReport {
    pub execution: Report,
    /// Untrusted plugin-produced metadata, never an operation approval.
    pub proposal: Option<Proposal>,
    pub failure: Option<PluginFailure>,
}
impl PreparedPackage {
    /// Invoke a declared same-package planner with bounded fuel/memory and no
    /// host authority. The product must run this away from the UI event thread
    /// and bind its result to the original package/revision/input and user review.
    pub fn run_blob_upload_proposal(
        &self,
        input: &Invocation,
        cancel: Cancellation,
    ) -> ProposalReport {
        let fault = |fault| ProposalReport {
            execution: Report {
                outcome: Err(fault),
                host_calls: 0,
                fuel_remaining: self.limits.fuel,
            },
            proposal: None,
            failure: None,
        };
        let registration = input
            .transform()
            .and_then(|t| self.package.blob_upload_proposal_handler(t).ok());
        let Some((registration, _mapping)) = registration else {
            return fault(Fault::TaskProtocol);
        };
        if !self.runner.blob_upload_abi {
            return fault(Fault::UnsupportedAbi);
        }
        let mut execution = match Execution::start(&self.runner, Some(input.bytes()), cancel) {
            Ok(execution) => execution,
            Err(error) => return fault(error),
        };
        // Never resume an upload or any other effect import, even if a guest
        // plans to ignore a refusal and then report a successful proposal.
        if execution.pending().is_some() {
            execution.abort(Fault::TaskProtocol);
        }
        let run = execution.finish();
        let mut report = ProposalReport {
            execution: run.report,
            proposal: None,
            failure: None,
        };
        if report.execution.outcome.is_ok() {
            match run
                .completion
                .as_ref()
                .and_then(|bytes| input.verify_transform_result(bytes).ok())
            {
                Some(TransformResult::Output(output))
                    if output.bytes.len() <= registration.max_output_bytes as usize =>
                {
                    match Proposal::decode(&output.bytes) {
                        Ok(proposal)
                            if self
                                .package
                                .blob_upload_declaration()
                                .is_some_and(|d| proposal.byte_length() <= d.max_object_bytes) =>
                        {
                            report.proposal = Some(proposal)
                        }
                        _ => report.execution.outcome = Err(Fault::TaskProtocol),
                    }
                }
                Some(TransformResult::Failure(failure)) => report.failure = Some(failure),
                _ => report.execution.outcome = Err(Fault::TaskProtocol),
            }
        }
        report
    }
}
