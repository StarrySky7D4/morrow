//! Consume-only original host completion. This is deliberately neither Clone,
//! Serialize nor publicly constructible; protocol/observation values cannot
//! manufacture it. Outer SDK/factory retirement is a separate obligation.
use std::{io,sync::Arc,time::Instant};
use super::checked_owner::{CheckedRunnerOwner,CheckedFinalizationOutcome};
use super::native_close_runtime::RuntimeRunnerIdentity;

pub struct NativeHostCompletion {
    pub(super) runner:RuntimeRunnerIdentity,
    pub(super) attempt:u64,
    pub(super) raw_dword:u32,
    pub(super) owner:Arc<CheckedRunnerOwner>,
    terminal_close_ack_verified:bool,
}
impl std::fmt::Debug for NativeHostCompletion {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        f.write_str("NativeHostCompletion([SEALED_ORIGINAL_OWNER])")
    }
}
impl NativeHostCompletion {
    pub fn runner_identity(&self)->RuntimeRunnerIdentity {self.runner}
    pub fn startup_attempt(&self)->u64 {self.attempt}
    pub fn runner_raw_dword(&self)->u32 {self.raw_dword}
    pub fn terminal_close_ack_verified(&self)->bool {self.terminal_close_ack_verified}
    /// Consume the local native obligation only. This does not claim SDK quota,
    /// formal/owner/factory or cleanup completion. The original owner remains
    /// retained if checked local registry retirement fails.
    pub fn retire_local(self)->io::Result<()> {self.owner.retire_native_local_slot()}
}

impl CheckedRunnerOwner {
    pub(crate) fn take_native_host_completion(self:&Arc<Self>,deadline:Instant)->io::Result<NativeHostCompletion> {
        let outcome=self.finalize(deadline)?;
        let CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{native_exit_code,..}=outcome
            else{return Err(io::Error::other("original v8 native completion unavailable"));};
        if self.is_sticky_unknown() || self.is_no_replay_sealed() {
            return Err(io::Error::other("original v8 completion sealed Unknown"));
        }
        self.close_owned_native_desktop()?;
        if self.native_completion_taken.swap(true,std::sync::atomic::Ordering::AcqRel) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,"original completion already consumed"));
        }
        let runtime=self.native_close_runtime().ok_or_else(||io::Error::other("original v8 binding absent"))?;
        let terminal_close_ack_verified=runtime.terminal_ack_written();
        if !terminal_close_ack_verified{
            self.record_runtime_lifecycle_failure();return Err(io::Error::other("original terminal ACK not proven"));
        }
        Ok(NativeHostCompletion {runner:runtime.runner_identity(),attempt:runtime.attempt(),
            raw_dword:native_exit_code,owner:Arc::clone(self),terminal_close_ack_verified})
    }
}
