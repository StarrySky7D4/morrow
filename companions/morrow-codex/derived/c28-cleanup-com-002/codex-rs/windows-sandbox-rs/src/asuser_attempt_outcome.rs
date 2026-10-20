//! Independent observations. None of these values is a release capability.
//! In particular, runner absence says nothing about setup/account side effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunnerDispatch { NotAttempted, Rejected, Unconfirmed, Adopted }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalResources { Owned, Closed, RetainedUnknown }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttemptOutcome {
    pub dispatch: RunnerDispatch,
    pub resources: LocalResources,
    pub observation_lost: bool,
}
impl AttemptOutcome {
    pub(crate) fn reserved() -> Self {
        Self { dispatch: RunnerDispatch::NotAttempted,
            resources: LocalResources::Owned, observation_lost: false }
    }
    pub(crate) fn create_entered(&mut self) { self.dispatch = RunnerDispatch::Unconfirmed; }
    pub(crate) fn create_rejected(&mut self) {
        if !self.observation_lost && self.dispatch == RunnerDispatch::Unconfirmed {
            self.dispatch = RunnerDispatch::Rejected;
        }
    }
    pub(crate) fn adopted(&mut self) {
        if !self.observation_lost { self.dispatch = RunnerDispatch::Adopted; }
    }
    pub(crate) fn lost(&mut self) {
        self.observation_lost = true;
        self.resources = LocalResources::RetainedUnknown;
    }
    pub(crate) fn closed(&mut self) {
        if !self.observation_lost { self.resources = LocalResources::Closed; }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn predispatch_denial_does_not_claim_resources_are_closed() {
        let mut state=AttemptOutcome::reserved();
        assert_eq!(state.dispatch,RunnerDispatch::NotAttempted);
        assert_eq!(state.resources,LocalResources::Owned);
        state.closed();assert_eq!(state.resources,LocalResources::Closed);
    }
    #[test] fn unknown_is_not_rewritten_by_late_rejection_or_close() {
        let mut state=AttemptOutcome::reserved();state.create_entered();state.lost();
        state.create_rejected();state.adopted();state.closed();
        assert_eq!(state.dispatch,RunnerDispatch::Unconfirmed);
        assert_eq!(state.resources,LocalResources::RetainedUnknown);
    }
    #[test] fn os_rejection_and_completed_native_ownership_are_distinct() {
        let mut state=AttemptOutcome::reserved();state.create_entered();state.create_rejected();
        assert_eq!(state.dispatch,RunnerDispatch::Rejected);
        assert_eq!(state.resources,LocalResources::Owned);
        let mut live=AttemptOutcome::reserved();live.create_entered();live.adopted();
        assert_eq!(live.dispatch,RunnerDispatch::Adopted);
        assert_ne!(live,state);
    }
}
