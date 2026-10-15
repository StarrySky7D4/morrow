//! Called only from the original checked normal-close sequence, after its
//! private operation-order state reaches JobDrained. No OS query or public fact
//! input is accepted here. Public ledger booleans cannot authorize this writer.
use super::*;
use codex_windows_sandbox::native_close_wire as wire;
fn rejected()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"v8 original native facts incomplete")}
fn close_fact(f:NativeHandleCloseFact)->wire::HandleCloseFact {
    match f {
        NativeHandleCloseFact::ClosedVerified=>wire::HandleCloseFact::ClosedVerifiedOnce,
        NativeHandleCloseFact::NotConfigured|NativeHandleCloseFact::NeverCreated=>wire::HandleCloseFact::NotApplicableAbsent,
        NativeHandleCloseFact::Open=>wire::HandleCloseFact::StillOpen,
        NativeHandleCloseFact::CloseDebtRetained{os_error,..}=>wire::HandleCloseFact::CloseDebtRetained{os_error},
        _=>wire::HandleCloseFact::NotProven,
    }
}
fn output(f:OutputStreamFactSnapshot)->wire::OutputStreamWorkerFact {
    wire::OutputStreamWorkerFact {
        pipe_eof:if f.eof_or_completion_observed && f.timely_eof_and_joined_verified {wire::PipeEofFact::CleanEofObserved}else{wire::PipeEofFact::NotProven},
        reader_join:if f.join_state==Some(WorkerJoinState::JoinedTimely) && f.timely_eof_and_joined_verified {
            wire::WorkerNativeJoinFact::JoinedTimelyClean}else{wire::WorkerNativeJoinFact::NotProven},
        handle_close:close_fact(f.reader_handle_close),
    }
}
impl RetainedRunnerContext {
    fn original_native_receipt(&mut self,used_conpty:bool)->io::Result<wire::ReportedNativeCloseFacts> {
        if self.native_shutdown_stage!=NativeShutdownStage::JobDrained {return Err(rejected());}
        let runtime=self.native_close_runtime.clone().ok_or_else(rejected)?;
        if !runtime.terminal_ack_received() || !runtime.physical_eof_seen(){return Err(rejected());}
        let f=self.refresh_native_facts_snapshot();
        if f.first_error.latch_lock!=LockObservationState::AcquiredClean || f.first_error.first_error_kind.is_some()
            || f.first_error.no_replay_latched || f.has_any_handle_close_debt || self.has_close_debt()
            || f.running_worker_count!=0 || f.late_reap_observed {return Err(rejected());}
        let ChildExitFactState::VerifiedExit(exit)=f.owner.exit else{return Err(rejected())};
        if !exit.wait_confirmed_signaled || exit.timed_out {return Err(rejected());}
        let identity=wire::VerifiedChildProcessIdentity::try_new(exit.process_id,exit.thread_id,exit.creation_time_100ns)
            .map_err(|_| rejected())?;
        let (witness,origin)=match f.job.active_zero_state {
            JobActiveZeroFactState::ActiveZeroVerified{witness,extra_member_origin}=>(witness,extra_member_origin),
            _=>return Err(rejected()),
        };
        if !f.job.job_present || !f.job.admission_verified || f.job.admitted_child_identity!=Some(ChildProcessIdentity {
            process_id:exit.process_id,thread_id:exit.thread_id,creation_time_100ns:exit.creation_time_100ns}) {return Err(rejected());}
        let input=f.input;
        if !input.timely_peer_eof_and_joined_verified || !input.retired_after_child_exit
            || !self.native_input_close.as_ref().is_some_and(OwnedWinHandle::is_closed_verified) {return Err(rejected());}
        let stdin=if f.stdin.initially_open==Some(false) {wire::StdinHandleCloseFact::InitiallyNotOpen}
            else if f.stdin.closed_once_verified && f.stdin.close_count==Some(1) && f.stdin.write_failed==Some(false) {
                wire::StdinHandleCloseFact::ClosedOnceVerified {write_disposition:wire::StdinWriteDisposition::AllWritesSucceeded}
            }else{wire::StdinHandleCloseFact::NotProven};
        let writer=f.control_and_writers;
        if !writer.business_exit_writer_joined_and_acked || writer.business_exit_ack_exit_code!=Some(exit.exit_code)
            || writer.frame_write_join_state!=Some(WorkerJoinState::JoinedTimely) || writer.frame_write_succeeded!=Some(true) {
            return Err(rejected());
        }
        let host_pending=wire::LeaseRetirementFact::OutOfRunnerScopePendingHostOwner;
        Ok(wire::ReportedNativeCloseFacts {
            child_exit:wire::ChildNativeExitFact::Witnessed {identity:identity.to_wire(),raw_exit_code_dword:exit.raw_exit_code,
                business_exit_code_i32:exit.exit_code,wait_on_original_handle:wire::OriginalProcessWaitProof::SignaledExitedOnOriginalHandle,
                timeout_disposition:wire::TimeoutDisposition::CompletedWithinBudget},
            io_streams_and_workers:wire::IoStreamsAndWorkersFacts {stdout:output(f.stdout),
                stderr:if f.stderr.configured {wire::StderrStreamWorkerFact::SeparateStream{stream:output(f.stderr)}}
                    else{wire::StderrStreamWorkerFact::MergedConptyOrAbsent{handle_close:wire::HandleCloseFact::NotApplicableAbsent}},
                input:wire::InputStreamWorkerFact {peer_eof:wire::InputPeerEofFact::CleanPostChildExitFrameBoundaryEof,
                    reader_join:wire::WorkerNativeJoinFact::JoinedTimelyClean,
                    termination_disposition:if input.explicit_terminate_received {wire::InputTerminationFact::ExplicitTerminateReceived}
                        else if input.terminated_on_disconnect {wire::InputTerminationFact::TerminatedOnDisconnect}
                        else{wire::InputTerminationFact::NoTerminationObserved}}},
            handles_close:wire::OriginalHandlesCloseFacts {
                conpty_closure:if writer.conpty_lock==LockObservationState::AcquiredClean && writer.conpty_active==Some(false) {
                    if used_conpty {wire::ConptyClosureFact::ClosedBeforeOutputJoinVerified}else{wire::ConptyClosureFact::NotUsedPipeMode}
                }else{wire::ConptyClosureFact::NotProven},stdin_closure:stdin,
                child_primary_thread_handle:close_fact(f.primary_thread_handle_close),child_process_handle:close_fact(f.owner.process_handle_close),
                close_ordering_after_io_joins:wire::StageOrderingFact::ProvenInStrictOrder},
            job_lifecycle:wire::OriginalJobLifecycleFacts {
                // One job is captured once in this one admitted context. This is
                // its per-attempt association id, never an OS HANDLE/address.
                admission:wire::JobAdmissionFact::AdmittedVerifiedOriginalJob {job_instance_id:runtime.attempt(),
                    child_membership_verified:wire::ObservationProof::ProvenOnOriginal},
                active_zero_drain:wire::JobActiveZeroDrainFact::DrainedToActiveZero{total_processes:witness.total_processes,
                    active_processes:witness.active_processes,total_terminated_processes:witness.total_terminated_processes,
                    ordering_after_child_handle_close:wire::StageOrderingFact::ProvenInStrictOrder},
                extra_member_origin:match origin {ExtraJobMemberOriginFact::SingleProcessOnlyVerified=>wire::ExtraJobMembersOriginFact::SingleProcessObservedOnly,
                    _=>wire::ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper},
                preserve_descendants:if f.job.preserve_descendants_state==JobPreserveDescendantsFactState::VerifiedNotPreserved {
                    wire::JobForbiddenOpFact::NotInvoked}else{wire::JobForbiddenOpFact::InvokedViolation},
                terminate_job:if f.job.terminate_invoked {wire::JobForbiddenOpFact::InvokedViolation}else{wire::JobForbiddenOpFact::NotInvoked}},
            writers:wire::WriterClosureFacts {business_exit_writer:wire::BusinessExitWriterFact::JoinedTimelyBeforeReceipt{exit_code_i32:exit.exit_code},
                prior_control_and_frame_writers:wire::PriorControlWritersFact::AllJoinedAndQuiesced{control_results_written:runtime.controls_written()?},
                sender_receipt_writer:wire::SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose},
            leases:wire::LeaseRetirementFacts {runner_input_and_stdin_lease:wire::LeaseRetirementFact::RetiredCleanly,
                input_lease:host_pending,keeper_lease:host_pending,quota_lease:host_pending,
                attempt_lease:wire::AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush{startup_attempt_id:runtime.attempt()}},
            debt_and_sticky_unknown:wire::DebtAndStickyUnknownFacts {close_debt:wire::CloseDebtFact::ZeroDebtProven,
                first_error:wire::FirstErrorFact::NoneObserved,poison_state:wire::PoisonStateFact::NoPoisonObserved,
                sticky_unknown_state:wire::StickyUnknownStateFact::Clear},
        })
    }
    pub(crate) fn send_native_close_receipt(&mut self,pipe:&Arc<StdMutex<File>>,deadline:Instant,used_conpty:bool)->io::Result<()> {
        if self.native_receipt_writer!=FinalReceiptWriterFactState::NotStartedPendingFutureReceiptSend {
            self.sticky_latch.record_first(StickyUnknownKind::InputControlWriteFailed,"native receipt writer already attempted; no replay");
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,"native receipt writer already attempted"));
        }
        let result:io::Result<()>=(|| {
        self.native_receipt_writer=FinalReceiptWriterFactState::PendingCurrentReceiptFlushAndClose;
        let facts=self.original_native_receipt(used_conpty)?;
        let runtime=self.native_close_runtime.clone().ok_or_else(rejected)?;
        if Instant::now()>=deadline || self.frame_write_worker.as_ref().is_some_and(RetainedWorker::has_retained_join_handle) {
            return Err(io::Error::new(io::ErrorKind::TimedOut,"native receipt writer unavailable"));
        }
        self.retained_writers[1]=Some(Arc::new(Arc::clone(pipe)));
        let pipe=Arc::clone(pipe);
        let worker=std::thread::Builder::new().name("v8-original-final-receipt-writer".into()).spawn(move || {
            let mut writer=acquire_mutex_bounded(&pipe,deadline,"native receipt original pipe")?;
            runtime.write_receipt(&mut *writer,facts)?;
            if Instant::now()>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"native receipt write late"));}
            Ok(())
        })?;
        self.frame_write_worker=Some(RetainedWorker::new("v8 final receipt writer",worker));
        self.frame_write_worker.as_mut().unwrap().join_bounded(deadline)?
            .as_ref().map(|_| ()).map_err(|e|e.raw_os_error().map(io::Error::from_raw_os_error)
                .unwrap_or_else(||io::Error::new(e.kind(),e.to_string())))
        })();
        self.native_receipt_writer=if result.is_ok() {FinalReceiptWriterFactState::JoinedTimelyAfterFlush}
            else{FinalReceiptWriterFactState::FailedOrRetained};
        self.refresh_native_facts_snapshot();result
    }
}
