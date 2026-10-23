//! Exact returned LogonUser(FALSE, nonzero error, NULL output) before transport
//! or helper creation. No diagnosis, missing process or arbitrary error creates
//! a refusal capability. The same retained cell must close its actual resources.
use super::*;

impl Attempt {
    pub(super) fn capture_login_result(&mut self,ok:bool,error:u32)->io::Result<LoginResult> {
        if self.phase!=Phase::LoginInFlight || self.login_token.state!=State::InFlight
            || self.calls[NativeCall::Login as usize]!=1 || self.first.is_some()
            || self.refusal_fact.is_some() {return Err(bad());}
        let captured=self.login_token.capture(ok,error);
        if !ok && error!=0 && self.login_token.raw==0 && self.login_token.state==State::Rejected {
            let original=io::Error::from_raw_os_error(error as i32);
            self.refusal_fact=Some(Failure {stage:Stage::Login,site:Site::LogonUserResult,
                kind:original.kind(),win32:Some(error as i32),ntstatus:None,target_policy_rejection:None});
            return Ok(LoginResult::Rejected);
        }
        captured?;Ok(LoginResult::Ready)
    }

    fn logon_refusal_base(&self,retirement:u8)->bool {
        if self.first.is_some() || self.outcome.observation_lost
            || self.outcome.dispatch!=crate::asuser_attempt_outcome::RunnerDispatch::NotAttempted
            || self.caller_attempt.is_some() || self.caller_refusal.is_some()
            || !self.refusal_fact.is_some_and(|fact|fact.stage==Stage::Login
                && fact.site==Site::LogonUserResult && fact.win32.is_some_and(|code|code!=0)
                && fact.ntstatus.is_none() && fact.target_policy_rejection.is_none())
            || self.login_token.state!=State::Rejected || self.login_token.raw!=0
            || self.thread_token.state!=State::Rejected || self.thread_token.raw!=0
            || self.pi.hProcess!=0 || self.pi.hThread!=0 || self.pi.dwProcessId!=0 || self.pi.dwThreadId!=0
            || self.logon.is_some() || self.source_facts.is_some() || self.target_statistics.is_some()
            || self.token_security.is_some() || self.token_sd_len!=0 || self.launch_creation_identity.is_some()
            || self.target_policy_rejection.is_some() || self.default_reads!=0 || self.default_acl_len!=0
            || self.post_default.is_some() || self.before_default_len!=0
            || self.runner_object_phase!=RunnerObjectPhase::Unobserved || self.runner_object_status.is_some()
            || self.default_initialization.phase!=DefaultPhase::Unobserved || self.default_initialization.status.is_some()
            || self.runner_before_len!=0 || self.gui_closed || self.origin_retirement_taken
            || self.object_query.is_none() || self.object_replies.iter().any(Option::is_some)
            || self.pipe_names.is_none() || self.caller_privileges.is_none()
            || !self.caller_windows[0].as_ref().is_some_and(CallerWindow::stable)
            || self.caller_windows[1..].iter().any(Option::is_some)
            || !self.admission.as_ref().is_some_and(|a|a.generation==self.generation&&a.caller.original_handles_closed())
            || !self.broker.as_ref().is_some_and(|broker|broker.pristine_before_start()) {
            return false;
        }
        // The exact pre-Logon call sequence is frozen. In particular no derive,
        // DACL, token/process readback, create, transfer or helper call fits it.
        let mut expected=[0u8;NATIVE_CALL_COUNT];
        for (call,count) in [(NativeCall::ThreadToken,1),(NativeCall::Login,1),
            (NativeCall::TokenQuery,2),(NativeCall::PrivilegeLookup,2),
            (NativeCall::CallerToken,1),(NativeCall::CallerWindow,1),
            (NativeCall::Retirement,retirement)] {expected[call as usize]=count;}
        self.calls==expected && self.slots().iter().all(|slot| {
            std::ptr::eq(*slot,&self.login_token) || std::ptr::eq(*slot,&self.thread_token)
                || std::ptr::eq(*slot,&self.caller_token) || slot.state==State::Empty&&slot.raw==0
        })
    }

    pub(super) fn logon_refusal_resources_closed(&self)->bool {
        self.logon_refusal_base(1) && self.caller_token.state==State::Closed && self.caller_token.raw==0
            && self.command.as_ref().is_some_and(|command|command.cwd_pins.checked_closed())
    }

    pub(super) fn refusal_close_budget(&mut self,deadline:Instant)->io::Result<()> {
        if Instant::now()>=deadline {
            self.outcome.lost();
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        Ok(())
    }

    pub(super) fn close_known_logon_refusal(&mut self)->io::Result<NativeCallerRefusal> {
        if self.phase!=Phase::LoginInFlight || self.refusal_anchor.is_some() || self.refusal_kind.is_some()
            || !self.logon_refusal_base(0) || self.caller_token.state!=State::Held
            || self.caller_token.raw==0 || self.caller_token.raw == -1
            || !self.command.as_ref().is_some_and(|command|command.cwd_pins.ready()) {return Err(bad());}
        self.call(NativeCall::Retirement,1,Stage::Retirement)?;
        let deadline=self.deadline.min(Instant::now().checked_add(std::time::Duration::from_secs(1)).ok_or_else(bad)?);
        self.refusal_close_budget(deadline)?;
        self.caller_token.close()?;
        self.refusal_close_budget(deadline)?;
        self.command.as_mut().ok_or_else(bad)?.cwd_pins.close_once(deadline)?;
        self.refusal_close_budget(deadline)?;
        if !self.logon_refusal_resources_closed() {return Err(bad());}
        let anchor=std::sync::Arc::new(());
        self.refusal_anchor=Some(anchor.clone());self.refusal_kind=Some(RefusalKind::Logon);
        self.phase=Phase::CallerRefused;
        Ok(NativeCallerRefusal {anchor,generation:self.generation,kind:RefusalKind::Logon})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reserved()->Attempt {
        let mut a=Attempt::reserved(17,Instant::now()+std::time::Duration::from_secs(30));
        a.phase=Phase::LoginInFlight;a.stage=Stage::Login;a.site=Site::LogonUserResult;
        a.calls[NativeCall::Login as usize]=1;a.login_token.begin().unwrap();a
    }
    #[test]
    fn only_exact_false_nonzero_error_null_output_is_a_logon_rejection() {
        for (ok,error,raw,rejected) in [(false,1326,0,true),(false,1326,44,false),
            (false,1326,-1,false),(false,0,0,false),(true,0,0,false),(true,0,44,false)] {
            let mut a=reserved();a.login_token.raw=raw;
            let result=a.capture_login_result(ok,error);
            assert_eq!(matches!(result,Ok(LoginResult::Rejected)),rejected);
            assert_eq!(a.refusal_fact.is_some(),rejected);
            assert!(a.refusal_anchor.is_none());
            // A scalar result or absent admission never authorizes cleanup or
            // retirement. No native close runs against these model handles.
            assert!(a.close_known_logon_refusal().is_err());
            assert_eq!(a.calls[NativeCall::Retirement as usize],0);
            assert!(!a.logon_refusal_resources_closed());
        }
    }
    #[test]
    fn original_login_result_cannot_be_recaptured_or_promote_existing_unknown() {
        let mut a=reserved();assert!(matches!(a.capture_login_result(false,1326),Ok(LoginResult::Rejected)));
        assert!(a.capture_login_result(false,1326).is_err());
        a.fail(&io::Error::from(io::ErrorKind::TimedOut));
        assert_eq!(a.phase,Phase::Unknown);assert!(a.close_known_logon_refusal().is_err());
        assert_eq!(a.refusal_fact.unwrap().win32,Some(1326));
        assert_eq!(a.calls[NativeCall::Retirement as usize],0);
    }
    #[test]
    fn uncertain_partial_login_output_keeps_original_raw_and_no_refusal() {
        let mut a=reserved();a.login_token.raw=44;
        let error=match a.capture_login_result(false,1326){Err(error)=>error,_=>panic!("partial output accepted")};
        a.fail(&error);
        assert_eq!(a.login_token.raw,44);assert_eq!(a.login_token.state,State::Unknown);
        assert!(a.outcome.observation_lost);assert!(a.refusal_fact.is_none());
        assert!(a.close_known_logon_refusal().is_err());assert_eq!(a.calls[NativeCall::Retirement as usize],0);
    }
    #[test]
    fn diagnostic_preserves_original_logon_refusal_ahead_of_later_close_failure() {
        let mut a=reserved();assert!(matches!(a.capture_login_result(false,1326),Ok(LoginResult::Rejected)));
        a.stage=Stage::Retirement;a.fail(&io::Error::from_raw_os_error(6));
        assert_eq!(a.refusal_fact.unwrap().win32,Some(1326));assert_eq!(a.first.unwrap().win32,Some(6));
        assert_eq!(a.phase,Phase::Unknown);assert!(!a.logon_refusal_resources_closed());
        assert!(a.close_known_logon_refusal().is_err());
    }
    #[test]
    fn expired_refusal_close_budget_is_sticky_resource_uncertainty() {
        let mut a=reserved();assert!(a.refusal_close_budget(Instant::now()).is_err());
        assert!(a.outcome.observation_lost);
        assert_eq!(a.outcome.resources,crate::asuser_attempt_outcome::LocalResources::RetainedUnknown);
        assert!(a.close_known_logon_refusal().is_err());
    }
    #[test]
    #[ignore="explicit isolated Windows VM native LogonUser negative qualification"]
    fn actual_test_name_logon_rejection_has_no_handle_and_cannot_mint_full_refusal() {
        assert_eq!(std::env::var("MORROW_DISPOSABLE_WINDOWS_TEST").as_deref(),Ok("1"));
        let mut a=reserved();
        let name=format!("mwabs-{:08x}",std::process::id());
        let user:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();
        let password:Vec<u16>="qualification-not-a-secret".encode_utf16().chain(Some(0)).collect();
        let domain=[b'.' as u16,0];
        let ok=unsafe{LogonUserW(user.as_ptr(),domain.as_ptr(),password.as_ptr(),2,0,&mut a.login_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        assert!(matches!(a.capture_login_result(ok!=0,error),Ok(LoginResult::Rejected)));
        assert_eq!(a.login_token.raw,0);assert_ne!(error,0);
        assert!(a.close_known_logon_refusal().is_err());
        assert!(a.refusal_anchor.is_none());assert_eq!(a.calls[NativeCall::Retirement as usize],0);
    }
}
