//! Explicit unsupported result until the platform can guarantee the precondition.
use super::*;
use morrow_core::file_effect::ReplaceOutcome;

impl TargetBroker {
    /// Windows pathname replacement cannot currently provide this broker's
    /// expected-object condition. Retaining share=0 prevents our rename too;
    /// allowing DELETE sharing lets a competing replacement change the name
    /// while our retained original remains unchanged. A metadata/file-ID check
    /// followed by rename still has a race and is not an atomic condition.
    ///
    /// Validate the original live owner and exact selection first, then fail
    /// explicitly without claiming dispatch, reserving an execution job, opening
    /// a temporary file, or changing Prepared/staged content. The caller can
    /// retain or cancel its preparation but must not silently weaken the effect.
    pub fn replace(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<ReplaceOutcome> {
        self.replace_controlled(manager, host, instance, request, &mut LocalControl(clock))
    }
    pub fn replace_controlled(
        &mut self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &RequestRecord,
        control: &mut impl TargetControl,
    ) -> Result<ReplaceOutcome> {
        self.validate_request_controlled(manager, host, instance, request, control)?;
        if request.request().disposition != Disposition::Replace {
            return Err(Error::Mismatch);
        }
        Err(Error::UnsupportedConditionalReplacement)
    }
}
