//! A complete, stable source-token readback rejected before token derivation.
//! Only this retained native Attempt can close its actual LogonUser/caller/CWD
//! resources and mint a separate refusal. Partial output, failed observation,
//! deadline and close failure stay Unknown. No process/runner completion is made.
use super::*;

impl Attempt {
    pub(super) fn capture_source_policy_rejection(
        &mut self,
        source: TargetFacts,
        error: &io::Error,
    ) -> io::Result<LoginResult> {
        if self.phase != Phase::TokenHeld
            || self.site != Site::TargetPolicyGuard
            || self.first.is_some()
            || self.refusal_fact.is_some()
            || self.source_facts.is_some()
            || self.login_token.state != State::Held
            || self.calls[NativeCall::Login as usize] != 1
            || error.kind() != io::ErrorKind::PermissionDenied
            || error.raw_os_error().is_some()
        {
            return Err(bad());
        }
        let admission = self.admission.as_ref().ok_or_else(bad)?;
        let reason = admission.target.source_rejection(&source).ok_or_else(bad)?;
        if self.target_policy_rejection != Some(reason) {
            return Err(bad());
        }
        self.source_facts = Some(source);
        self.refusal_fact = Some(Failure {
            stage: Stage::Token,
            site: Site::TargetPolicyGuard,
            kind: error.kind(),
            win32: None,
            ntstatus: None,
            target_policy_rejection: Some(reason),
        });
        if !self.source_policy_refusal_base(0) {
            return Err(bad());
        }
        Ok(LoginResult::SourcePolicyRejected)
    }

    fn source_policy_refusal_base(&self, retirement: u8) -> bool {
        let Some(source) = self.source_facts.as_ref() else {
            return false;
        };
        let Some(admission) = self.admission.as_ref() else {
            return false;
        };
        let Some(reason) = admission.target.source_rejection(source) else {
            return false;
        };
        if self.first.is_some()
            || self.outcome.observation_lost
            || self.outcome.dispatch != crate::asuser_attempt_outcome::RunnerDispatch::NotAttempted
            || self.caller_attempt.is_some()
            || self.caller_refusal.is_some()
            || self.target_policy_rejection != Some(reason)
            || !self.refusal_fact.is_some_and(|fact| {
                fact.stage == Stage::Token
                    && fact.site == Site::TargetPolicyGuard
                    && fact.kind == io::ErrorKind::PermissionDenied
                    && fact.win32.is_none()
                    && fact.ntstatus.is_none()
                    && fact.target_policy_rejection == Some(reason)
            })
            || source.initial != source.final_stats
            || source.initial.token == 0
            || source.initial.auth == 0
            || source.initial.modified == 0
            || !source.initial.primary
            || self.thread_token.state != State::Rejected
            || self.thread_token.raw != 0
            || self.pi.hProcess != 0
            || self.pi.hThread != 0
            || self.pi.dwProcessId != 0
            || self.pi.dwThreadId != 0
            || self.logon.is_some()
            || self.target_statistics.is_some()
            || self.token_security.is_some()
            || self.token_sd_len != 0
            || self.launch_creation_identity.is_some()
            || self.default_reads != 0
            || self.default_acl_len != 0
            || self.post_default.is_some()
            || self.before_default_len != 0
            || self.runner_object_phase != RunnerObjectPhase::Unobserved
            || self.runner_object_status.is_some()
            || self.default_initialization.phase != DefaultPhase::Unobserved
            || self.default_initialization.status.is_some()
            || self.runner_before_len != 0
            || self.gui_closed
            || self.origin_retirement_taken
            || self.object_query.is_none()
            || !self.object_replies[0].as_ref().is_some_and(|reply| {
                reply.status >= 0 && reply.returned == 56 && reply.granted == Some(source.granted)
            })
            || self.object_replies[1..].iter().any(Option::is_some)
            || self.pipe_names.is_none()
            || self.caller_privileges.is_none()
            || !self.caller_windows[0]
                .as_ref()
                .is_some_and(CallerWindow::stable)
            || self.caller_windows[1..].iter().any(Option::is_some)
            || admission.generation != self.generation
            || !admission.caller.original_handles_closed()
            || !self
                .broker
                .as_ref()
                .is_some_and(|broker| broker.pristine_before_start())
        {
            return false;
        }
        let state = match retirement {
            0 => State::Held,
            1 => State::Closed,
            _ => return false,
        };
        for slot in [&self.login_token, &self.caller_token] {
            if slot.state != state
                || (retirement == 0 && matches!(slot.raw, 0 | -1))
                || (retirement == 1 && slot.raw != 0)
            {
                return false;
            }
        }
        if retirement == 0 && self.login_token.raw == self.caller_token.raw {
            return false;
        }
        // Two original caller queries, followed by exactly fifteen complete
        // source-token queries. No derive, DACL, create or transfer fits this.
        let mut expected = [0u8; NATIVE_CALL_COUNT];
        for (call, count) in [
            (NativeCall::ThreadToken, 1),
            (NativeCall::Login, 1),
            (NativeCall::TokenQuery, 17),
            (NativeCall::ObjectQuery, 1),
            (NativeCall::PrivilegeLookup, 3),
            (NativeCall::CallerToken, 1),
            (NativeCall::CallerWindow, 1),
            (NativeCall::Retirement, retirement),
        ] {
            expected[call as usize] = count;
        }
        self.calls == expected
            && self.slots().iter().all(|slot| {
                std::ptr::eq(*slot, &self.login_token)
                    || std::ptr::eq(*slot, &self.caller_token)
                    || std::ptr::eq(*slot, &self.thread_token)
                    || slot.state == State::Empty && slot.raw == 0
            })
    }

    pub(super) fn source_policy_refusal_resources_closed(&self) -> bool {
        self.source_policy_refusal_base(1)
            && self
                .command
                .as_ref()
                .is_some_and(|command| command.cwd_pins.checked_closed())
    }

    pub(super) fn close_known_source_policy_refusal(&mut self) -> io::Result<NativeCallerRefusal> {
        if self.phase != Phase::TokenHeld
            || self.refusal_anchor.is_some()
            || self.refusal_kind.is_some()
            || !self.source_policy_refusal_base(0)
            || !self
                .command
                .as_ref()
                .is_some_and(|command| command.cwd_pins.ready())
        {
            return Err(bad());
        }
        self.call(NativeCall::Retirement, 1, Stage::Retirement)?;
        let deadline = self.deadline.min(
            Instant::now()
                .checked_add(std::time::Duration::from_secs(1))
                .ok_or_else(bad)?,
        );
        self.refusal_close_budget(deadline)?;
        self.login_token.close()?;
        self.refusal_close_budget(deadline)?;
        self.caller_token.close()?;
        self.refusal_close_budget(deadline)?;
        self.command
            .as_mut()
            .ok_or_else(bad)?
            .cwd_pins
            .close_once(deadline)?;
        self.refusal_close_budget(deadline)?;
        if !self.source_policy_refusal_resources_closed() {
            return Err(bad());
        }
        let anchor = std::sync::Arc::new(());
        self.refusal_anchor = Some(anchor.clone());
        self.refusal_kind = Some(RefusalKind::SourcePolicy);
        self.phase = Phase::CallerRefused;
        Ok(NativeCallerRefusal {
            anchor,
            generation: self.generation,
            kind: RefusalKind::SourcePolicy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_admission_or_native_observations_cannot_authorize_source_policy_retirement() {
        let mut attempt =
            Attempt::reserved(37, Instant::now() + std::time::Duration::from_secs(30));
        attempt.phase = Phase::TokenHeld;
        attempt.site = Site::TargetPolicyGuard;
        attempt.target_policy_rejection = Some(crate::WindowsTargetPolicyRejection::UserNotManaged);
        for state in [
            State::Empty,
            State::InFlight,
            State::Held,
            State::Unknown,
            State::Closed,
        ] {
            attempt.login_token.state = state;
            attempt.login_token.raw = if state == State::Closed { 0 } else { 44 };
            assert!(attempt.close_known_source_policy_refusal().is_err());
            assert_eq!(attempt.calls[NativeCall::Retirement as usize], 0);
            assert!(!attempt.source_policy_refusal_resources_closed());
            assert!(attempt.refusal_anchor.is_none());
        }
        // These are model values, never handed to an OS close operation.
    }
}
