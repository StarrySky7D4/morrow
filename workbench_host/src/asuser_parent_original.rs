//! Source047 existing-P adapter for the reviewed retaining native-pipe API.
//! P's fixed owner installs this allocation BEFORE effects. This path never
//! enters Tokio, ControllerLease, legacy JobChild, kill, or owning raw facades.
use morrow_codex_session_exec_windows_v1::{ParentCClosed,ParentChildIdentity,ParentCreate,
    ParentNativeExit,ParentOriginalProcess,ParentExitPredicate,ParentExitFailure};
use morrow_native_pipe_win::job::{Job,RetainedJobOwner,RetainedChildStdio,RetainedChildIdentity,
    RetainedKernelObservations,RetainedDisposition,RetainedPhase,OuterLifecycleEvidence,
    prepare_trusted_host_environment};
use std::{io,os::windows::io::BorrowedHandle,time::Instant};

#[derive(Clone,Copy)]
struct FirstFailure {kind:io::ErrorKind,win32:Option<i32>}
fn bad()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"A1 original P retained-owner binding rejected")}
fn remaining(deadline:Instant)->io::Result<()> {
    if Instant::now()>=deadline {Err(io::Error::new(io::ErrorKind::TimedOut,"A1 original P budget expired"))}else{Ok(())}
}
fn retain_first(first:&mut Option<FirstFailure>,error:&io::Error) {
    if first.is_none(){*first=Some(FirstFailure{kind:error.kind(),win32:error.raw_os_error()});}
}
fn native_identity(actual:RetainedChildIdentity)->io::Result<ParentChildIdentity> {
    if actual.pid==0||actual.primary_thread_id==0||actual.creation_filetime==0
        ||!actual.job_member_confirmed||!actual.process_noninherited_confirmed||!actual.thread_noninherited_confirmed{return Err(bad());}
    Ok(ParentChildIdentity{pid:actual.pid,thread_id:actual.primary_thread_id,creation_filetime:actual.creation_filetime})
}
fn observed_exit(original:ParentChildIdentity,actual:RetainedKernelObservations)->io::Result<ParentNativeExit> {
    use ParentExitPredicate::*;
    for (passed,predicate) in [
        (actual.verified_pid==Some(original.pid)&&actual.verified_tid==Some(original.thread_id)
            &&actual.creation_filetime==Some(original.creation_filetime),RetainedIdentity),
        (actual.job_member_confirmed,RetainedJobMembership),
        (actual.process_noninherited_confirmed&&actual.thread_noninherited_confirmed,RetainedNoninheritance),
        (actual.process_signaled,ProcessSignal),(actual.thread_signaled,PrimaryThreadSignal),
        (actual.last_active_processes.is_some(),JobActiveMissing),
        (actual.last_active_processes==Some(0),JobStillActive),
        (actual.post_exit_job_empty_observed,PostExitJobEmpty),
    ] {if !passed{return Err(ParentExitFailure::error(predicate));}}
    Ok(ParentNativeExit{identity:original,process_signaled:actual.process_signaled,thread_signaled:actual.thread_signaled,
        process_dword:actual.exit_code.ok_or_else(||ParentExitFailure::error(ParentExitPredicate::ProcessExitMissing))?,job_active:actual.last_active_processes.ok_or_else(||ParentExitFailure::error(ParentExitPredicate::JobActiveMissing))?,
        // No substitute zeros: N must query these through the SAME original
        // hThread/Job borrows before minting its consume-only ParentCClosed.
        thread_dword:None,job_terminated:None,job_total:None})
}

/// Actual read-only state from THIS retaining owner, not an external approval
/// or a capability constructor. The same decision is exercised by pure cases.
struct SameLifecycle {
    phase:RetainedPhase,disposition:RetainedDisposition,first_error:bool,no_replay:bool,
    create_calls:u8,resume_calls:u8,close_calls:u32,unresolved:bool,
}
impl SameLifecycle {
    fn from_original(owner:&RetainedJobOwner)->Self {
        let calls=owner.call_budgets();
        Self{phase:owner.phase(),disposition:owner.disposition(),first_error:owner.first_error().is_some(),
            no_replay:owner.no_replay(),create_calls:calls.create_process_calls,resume_calls:calls.resume_thread_calls,
            close_calls:calls.close_handle_calls,unresolved:owner.has_unresolved_debt()}
    }
    fn active(&self)->bool {
        // no_replay becomes TRUE before the first actual CreateProcess call,
        // including success. It prohibits another creation; it is NOT an error
        // flag for borrowing/resuming/observing this same original lifecycle.
        self.no_replay&&!self.first_error&&self.create_calls==1&&self.close_calls==0&&self.unresolved
            &&self.disposition==RetainedDisposition::ActivePending
            &&match self.phase {RetainedPhase::CreatedSuspendedValidated=>self.resume_calls==0,
                RetainedPhase::Running|RetainedPhase::ChildExitedAwaitingRetirement=>self.resume_calls==1,_=>false}
    }
    fn retired(&self)->bool {
        self.no_replay&&!self.first_error&&self.create_calls==1&&self.resume_calls==1&&self.close_calls==3
            &&!self.unresolved&&self.phase==RetainedPhase::Retired&&self.disposition==RetainedDisposition::RetiredCleanly
    }
}

struct Original {
    owner:Option<RetainedJobOwner>,first:Option<FirstFailure>,identity:Option<ParentChildIdentity>,
    observed:Option<ParentNativeExit>,create_attempted:bool,resume_attempted:bool,
    observe_attempted:bool,close_attempted:bool,retired:bool,
}
impl Original {
    fn allocation_only()->Self {Self{owner:None,first:None,identity:None,observed:None,
        create_attempted:false,resume_attempted:false,observe_attempted:false,close_attempted:false,retired:false}}
    fn finish<T>(&mut self,result:io::Result<T>)->io::Result<T> {
        if let Err(error)=&result{retain_first(&mut self.first,error);}result
    }
    fn ready(&mut self)->io::Result<()> {
        if let Some(first)=self.first {
            return Err(match first.win32 {Some(code)=>io::Error::from_raw_os_error(code),
                None=>io::Error::new(first.kind,"A1 original P first failure retained; no replay")});
        }
        let result=if !self.create_attempted||self.identity.is_none()||self.retired
            ||self.owner.as_ref().is_none_or(|owner|!SameLifecycle::from_original(owner).active()) {Err(bad())}else{Ok(())};
        self.finish(result)
    }
}
impl ParentOriginalProcess for Original {
    fn create_suspended(&mut self,input:ParentCreate<'_>)->io::Result<ParentChildIdentity> {
        if self.create_attempted||self.owner.is_some()||self.first.is_some(){return self.finish(Err(bad()));}
        self.create_attempted=true;
        let result=(|| {
            remaining(input.deadline)?;
            // The opaque native builder accepts exactly these three caller
            // keys and adds its existing token-derived LOCALAPPDATA. It never
            // takes ambient P environment or caller profile/marker overrides.
            if input.environment.len()!=3||!["SystemRoot","WINDIR","COMSPEC"].iter()
                .all(|name|input.environment.contains_key(*name)){return Err(bad());}
            let prepared=prepare_trusted_host_environment(input.environment)?;
            remaining(input.deadline)?;
            // No C exists during Job::new. Transfer immediately, before every
            // child-creation effect: ordinary Job Drop cannot kill an active C.
            self.owner=Some(Job::new()?.into_retained_owner());
            remaining(input.deadline)?;
            let owner=self.owner.as_mut().ok_or_else(bad)?;
            // One combined pass consumes three library observation units:
            // process wait/DWORD, primary-thread signal, original Job query.
            owner.set_max_observation_polls(3)?;
            let stdio=RetainedChildStdio::with_extra_inherited(input.stdio[0],input.stdio[1],input.stdio[2],input.extra_inherited)?;
            remaining(input.deadline)?;
            let actual=owner.create_suspended_prepared_host_with_stdio(input.application,input.arguments,input.cwd,prepared,stdio)?;
            let identity=native_identity(actual)?;self.identity=Some(identity);
            remaining(input.deadline)?;Ok(identity)
        })();self.finish(result)
    }
    fn original_process(&mut self)->io::Result<BorrowedHandle<'_>> {
        self.ready()?;let first=&mut self.first;
        match self.owner.as_mut().ok_or_else(bad)?.borrow_original_process_handle() {
            Ok(original)=>Ok(original.as_borrowed_handle()),Err(error)=>{retain_first(first,&error);Err(error)}
        }
    }
    fn original_primary_thread(&mut self)->io::Result<BorrowedHandle<'_>> {
        self.ready()?;let first=&mut self.first;
        match self.owner.as_mut().ok_or_else(bad)?.borrow_original_primary_thread_handle() {
            Ok(original)=>Ok(original.as_borrowed_handle()),Err(error)=>{retain_first(first,&error);Err(error)}
        }
    }
    fn original_job(&mut self)->io::Result<BorrowedHandle<'_>> {
        self.ready()?;let first=&mut self.first;
        match self.owner.as_mut().ok_or_else(bad)?.borrow_original_job_handle() {
            Ok(original)=>Ok(original.as_borrowed_handle()),Err(error)=>{retain_first(first,&error);Err(error)}
        }
    }
    fn begin_bootstrap_duplication(&mut self,source:BorrowedHandle<'_>)->io::Result<usize> {
        self.ready()?;
        let result=self.owner.as_mut().ok_or_else(bad)?.begin_bootstrap_borrowed_handle_duplication(source);self.finish(result)
    }
    fn complete_bootstrap_duplication(&mut self,slot:usize,outcome:io::Result<()>)->io::Result<()> {
        self.ready()?;
        let result=self.owner.as_mut().ok_or_else(bad)?.complete_bootstrap_handle_duplication(slot,outcome);self.finish(result)
    }
    fn resume_original_once(&mut self)->io::Result<()> {
        self.ready()?;if self.resume_attempted{return self.finish(Err(bad()));}self.resume_attempted=true;
        let result=self.owner.as_mut().ok_or_else(bad)?.resume_primary_thread();self.finish(result)
    }
    fn observe_original_exit(&mut self,deadline:Instant)->io::Result<ParentNativeExit> {
        self.ready()?;
        if !self.resume_attempted||self.observe_attempted{return self.finish(Err(bad()));}self.observe_attempted=true;
        let result=(|| {
            remaining(deadline)?;
            let actual=self.owner.as_mut().ok_or_else(bad)?.poll_lifecycle_observations()?;
            let value=observed_exit(self.identity.ok_or_else(bad)?,actual)?;
            self.observed=Some(value);remaining(deadline)?;Ok(value)
        })();self.finish(result)
    }
    fn close_after_natural_c(&mut self,proof:ParentCClosed)->io::Result<()> {
        self.ready()?;if self.close_attempted{return self.finish(Err(bad()));}self.close_attempted=true;
        let result=(|| {
            let original=self.identity.ok_or_else(bad)?;
            if proof.identity()!=original||self.observed.is_none_or(|value|value.identity!=original||value.process_dword!=0){return Err(bad());}
            remaining(proof.deadline())?;
            let owner=self.owner.as_mut().ok_or_else(bad)?;
            // This opaque proof can ONLY follow N's actual original C/thread0,
            // Job query, full fixed C47/9-only normal exit, record delivery and
            // checked close of ALL P aliases, including inherited file guards.
            owner.confirm_borrowed_stdio_discharged()?;
            owner.confirm_bootstrap_duplications_discharged()?;
            owner.record_outer_lifecycle_evidence(OuterLifecycleEvidence{
                // Borrowed mode owns no P reader threads. This covers only its
                // actual P aliases. ROOT's outer EOF/native-reader joins still
                // occur AFTER P0 and are never asserted by this local proof.
                stdio_reaped_and_closed:true,bootstrap_handles_discharged:true,
                protocol_ack_and_sdk_quiescent:true,prebirth_locks_retained_until_exit:true})?;
            if !owner.can_retire_handles(){return Err(bad());}
            remaining(proof.deadline())?;
            let receipt=owner.retire_handles()?;
            if native_identity(receipt.identity)?!=original||receipt.exit_code!=0
                ||!SameLifecycle::from_original(owner).retired(){return Err(bad());}
            remaining(proof.deadline())?;self.retired=true;Ok(())
        })();self.finish(result)
    }
}

/// Construction itself has no native, environment, IO, slot or registry effect.
pub(super) fn allocation_only()->Box<dyn ParentOriginalProcess> {Box::new(Original::allocation_only())}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn normal_first_created_no_replay_owner_allows_same_lifecycle_but_fault_and_retirement_do_not() {
        let mut actual=SameLifecycle{phase:RetainedPhase::CreatedSuspendedValidated,
            disposition:RetainedDisposition::ActivePending,first_error:false,no_replay:true,
            create_calls:1,resume_calls:0,close_calls:0,unresolved:true};
        assert!(actual.active());assert!(!actual.retired());
        actual.phase=RetainedPhase::Running;actual.resume_calls=1;assert!(actual.active());
        actual.phase=RetainedPhase::ChildExitedAwaitingRetirement;assert!(actual.active());
        // Even though every error also keeps noReplay=true, the same active
        // admission rejects actual fault/first-error and mismatched call state.
        actual.first_error=true;assert!(!actual.active());actual.first_error=false;
        actual.disposition=RetainedDisposition::UnknownRetained;assert!(!actual.active());
        actual.disposition=RetainedDisposition::ActivePending;actual.phase=RetainedPhase::FaultedRetained;assert!(!actual.active());
        actual.phase=RetainedPhase::Running;actual.create_calls=2;assert!(!actual.active());
        actual.create_calls=1;actual.no_replay=false;assert!(!actual.active());actual.no_replay=true;
        actual.phase=RetainedPhase::Retired;actual.disposition=RetainedDisposition::RetiredCleanly;
        actual.unresolved=false;actual.close_calls=3;assert!(!actual.active());assert!(actual.retired());
        actual.first_error=true;assert!(!actual.retired());actual.first_error=false;
        actual.unresolved=true;assert!(!actual.retired());actual.unresolved=false;
        actual.close_calls=2;assert!(!actual.retired());
        // A clean retired owner never re-enters active borrow/resume/create.
        assert!(!actual.active());
    }
    #[test] fn original_adapter_is_allocation_only_and_first_failure_prevents_later_native_use() {
        let mut original=Original::allocation_only();assert!(original.owner.is_none());assert!(original.identity.is_none());
        assert!(original.first.is_none());assert!(!original.create_attempted);assert!(!original.resume_attempted);
        assert!(original.original_process().is_err());assert!(original.owner.is_none());
        assert!(original.resume_original_once().is_err());assert!(!original.resume_attempted);
        let mut first=None;retain_first(&mut first,&io::Error::from_raw_os_error(123));
        retain_first(&mut first,&io::Error::from_raw_os_error(5));assert_eq!(first.unwrap().win32,Some(123));
    }
    #[test] fn native_kernel_observations_require_same_instance_and_never_substitute_logical_exit_or_zero() {
        let identity=ParentChildIdentity{pid:11,thread_id:13,creation_filetime:17};
        let actual=RetainedKernelObservations{verified_pid:Some(11),verified_tid:Some(13),creation_filetime:Some(17),
            job_member_confirmed:true,process_noninherited_confirmed:true,thread_noninherited_confirmed:true,
            process_signaled:true,exit_code:Some(0xc0000142),thread_signaled:true,last_active_processes:Some(0),post_exit_job_empty_observed:true};
        let value=observed_exit(identity,actual).unwrap();assert_eq!(value.process_dword,0xc0000142);
        assert_eq!((value.thread_dword,value.job_terminated,value.job_total),(None,None,None));
        for field in 0..11 {let mut changed=actual;match field {
            0=>changed.verified_pid=Some(12),1=>changed.verified_tid=Some(14),2=>changed.creation_filetime=Some(18),
            3=>changed.process_signaled=false,4=>changed.exit_code=None,5=>changed.thread_signaled=false,
            6=>changed.last_active_processes=None,7=>changed.last_active_processes=Some(1),
            8=>changed.post_exit_job_empty_observed=false,9=>changed.job_member_confirmed=false,
            _=>changed.thread_noninherited_confirmed=false}
            let error=observed_exit(identity,changed).err().unwrap();
            let predicate=error.get_ref().unwrap().downcast_ref::<ParentExitFailure>().unwrap().predicate();
            use ParentExitPredicate::*;
            assert_eq!(predicate,match field {0|1|2=>RetainedIdentity,3=>ProcessSignal,4=>ProcessExitMissing,
                5=>PrimaryThreadSignal,6=>JobActiveMissing,7=>JobStillActive,8=>PostExitJobEmpty,
                9=>RetainedJobMembership,_=>RetainedNoninheritance});
        }
        let mut real_259=actual;real_259.exit_code=Some(259);
        assert_eq!(observed_exit(identity,real_259).unwrap().process_dword,259);
    }
}
