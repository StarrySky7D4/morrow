//! Explicit v8 factory whose actual scheduler threads are captured at creation.
//! Scope tokens cannot be built from a fingerprint, bool, JSON or empty table.
use std::{sync::{Arc,Weak},time::{Duration,Instant}};
use codex_exec_server::ExecServerRuntimeOptions;
use codex_http_client::HttpClientFactory;
use codex_sandboxing::{NativeSchedulerThreads,NativeSchedulerClosed};
use morrow_agent_session_exec_v1_r2::{Error,Result};
use tokio::runtime::Runtime;
use crate::{ProvisionedWindowsBackend,WindowsRunnerProvisioningSpec,NativeExecutionCompletion,ReviewedNativeCompletion};

pub struct NativeFactoryRetirement {
    backend:Weak<ProvisionedWindowsBackend>,runtime:Weak<Runtime>,
    threads:Arc<NativeSchedulerThreads>,attempted:bool,
    creation_started:bool,creation_succeeded:bool,
    // The caller installs this attempt before creation. A subsequent helper/
    // factory error retains its SAME runtime and original thread anchors here.
    failed_creation_runtime:Option<Arc<Runtime>>,
    asuser:Option<Arc<codex_sandboxing::AsUserFactoryBinding>>,
    failed_asuser_backend:Option<Arc<ProvisionedWindowsBackend>>,
}
pub struct NativeFactoryCompletion {backend:Weak<ProvisionedWindowsBackend>,_threads:NativeSchedulerClosed}
/// Still not an owner-finish or guest-setup cleanup permission.
pub struct RetiredExecutionCompletion {execution:NativeExecutionCompletion,_factory:NativeFactoryCompletion}
impl NativeFactoryRetirement {
    pub fn new()->Self {Self {backend:Weak::new(),runtime:Weak::new(),threads:NativeSchedulerThreads::new(),
        attempted:false,creation_started:false,creation_succeeded:false,failed_creation_runtime:None,asuser:None,failed_asuser_backend:None}}
    /// Precondition for finishing the protected owner while this SAME scheduler
    /// is staged. This is not scheduler shutdown or any cleanup capability.
    pub fn verify_staged_runtime_for_owner_finish(&self,runtime:&Arc<Runtime>)->Result<()> {
        if self.attempted || !self.creation_succeeded || self.failed_creation_runtime.is_some()
            || self.failed_asuser_backend.is_some() {
            return Err(Error::CommitUnknown);
        }
        let original=self.runtime.upgrade().ok_or(Error::CommitUnknown)?;
        if !Arc::ptr_eq(&original,runtime){return Err(Error::Denied);}
        Ok(())
    }
    /// Called after synchronous shutdown of this factory's original Runtime.
    /// Weak absence is ONLY a precondition; original native waits and checked
    /// thread closes below supply the actual scheduler completion proof.
    pub fn finish_after_shutdown(&mut self)->Result<NativeFactoryCompletion> {
        if self.attempted || !self.creation_succeeded || self.failed_creation_runtime.is_some()||self.failed_asuser_backend.is_some(){return Err(Error::CommitUnknown);}
        if self.backend.upgrade().is_some() || self.runtime.upgrade().is_some(){return Err(Error::Conflict);}
        self.attempted=true;
        let threads=self.threads.verify_native_shutdown(Instant::now()+Duration::from_secs(15))
            .map_err(|_|Error::CommitUnknown)?;
        // Only after the ORIGINAL backend/runtime are gone and original native
        // scheduler threads checked-closed may its real setup/R/S pins retire.
        self.asuser.take();
        Ok(NativeFactoryCompletion {backend:self.backend.clone(),_threads:threads})
    }
}
impl NativeExecutionCompletion {
    pub fn retire_factory(self,factory:NativeFactoryCompletion)->Result<RetiredExecutionCompletion> {
        if !self.is_original_factory(&factory.backend){return Err(Error::Denied);}
        Ok(RetiredExecutionCompletion {execution:self,_factory:factory})
    }
}
impl RetiredExecutionCompletion {
    pub fn take_asuser_origin_retirement(&mut self)->Option<codex_sandboxing::AsUserOriginRetirement> {
        self.execution.asuser_origin.take()
    }
    pub fn is_for_review(&self,review:&ReviewedNativeCompletion)->bool {self.execution.is_for_review(review)}
    pub fn is_for_owner(&self,owner:morrow_core::dispatch::HostBinding)->bool {self.execution.is_for_owner(owner)}
    pub fn runner_pid(&self)->u32 {self.execution.runner_pid()}
    pub fn runner_creation_filetime(&self)->u64 {self.execution.runner_creation_filetime()}
    pub fn runner_raw_dword(&self)->u32 {self.execution.runner_raw_dword()}
    pub fn startup_attempt(&self)->u64 {self.execution.startup_attempt()}
    pub fn terminal_close_ack_verified(&self)->bool {self.execution.terminal_close_ack_verified()}
}
impl ProvisionedWindowsBackend {
    pub fn production_with_asuser_native_close_runner(options:ExecServerRuntimeOptions,factory:HttpClientFactory,
        helper_sha256:[u8;32],original:Arc<codex_sandboxing::AsUserMaterialization>,retirement:&mut NativeFactoryRetirement)
        ->Result<(Arc<Self>,Arc<Runtime>)> {
        if retirement.creation_started||retirement.asuser.is_some(){return Err(Error::CommitUnknown);}
        let binding=codex_sandboxing::AsUserFactoryBinding::new(original);
        let spec=WindowsRunnerProvisioningSpec::from_locked_native_close_v8(binding.runner_view())?;
        retirement.asuser=Some(binding.clone());
        let (backend,runtime)=Self::production_with_native_close_runner(options,factory,helper_sha256,spec,retirement)?;
        retirement.failed_asuser_backend=Some(backend.clone());retirement.failed_creation_runtime=Some(runtime.clone());
        binding.bind_original_backend(&backend).map_err(|_|Error::CommitUnknown)?;
        retirement.failed_asuser_backend.take();retirement.failed_creation_runtime.take();
        Ok((backend,runtime))
    }
    /// Host-only explicit v8 selection; the ordinary constructor and supplied
    /// runtime route retain their existing v7/default behavior.
    pub fn production_with_native_close_runner(options:ExecServerRuntimeOptions,factory:HttpClientFactory,
        helper_sha256:[u8;32],spec:WindowsRunnerProvisioningSpec,retirement:&mut NativeFactoryRetirement)
        ->Result<(Arc<Self>,Arc<Runtime>)> {
        if retirement.creation_started{return Err(Error::CommitUnknown);}
        retirement.creation_started=true;
        spec.validate_native_close_metadata()?;
        let threads=retirement.threads.clone();
        let start=threads.clone();let stop=threads.clone();
        let runtime=Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all()
            .on_thread_start(move ||start.thread_start()).on_thread_stop(move ||stop.thread_stop())
            .build().map_err(|_|Error::Storage)?);
        retirement.failed_creation_runtime=Some(runtime.clone());
        retirement.runtime=Arc::downgrade(&runtime);
        let backend=Self::production_with_native_close_environment(options,factory,helper_sha256,runtime.clone(),spec)?;
        retirement.backend=Arc::downgrade(&backend);
        retirement.creation_succeeded=true;
        retirement.failed_creation_runtime.take();
        Ok((backend,runtime))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_finish_accepts_only_its_staged_live_runtime_without_minting_shutdown() {
        let runtime=Arc::new(tokio::runtime::Builder::new_current_thread().build().unwrap());
        let foreign=Arc::new(tokio::runtime::Builder::new_current_thread().build().unwrap());
        let mut original=NativeFactoryRetirement::new();
        assert!(original.verify_staged_runtime_for_owner_finish(&runtime).is_err());
        original.creation_started=true;original.creation_succeeded=true;
        original.runtime=Arc::downgrade(&runtime);
        original.verify_staged_runtime_for_owner_finish(&runtime).unwrap();
        assert!(matches!(original.verify_staged_runtime_for_owner_finish(&foreign),Err(Error::Denied)));
        // A live original is suitable for owner flush, but cannot prove shutdown.
        assert!(matches!(original.finish_after_shutdown(),Err(Error::Conflict)));
        assert!(!original.attempted);
        original.failed_creation_runtime=Some(runtime.clone());
        assert!(original.verify_staged_runtime_for_owner_finish(&runtime).is_err());
        original.failed_creation_runtime.take();original.attempted=true;
        assert!(original.verify_staged_runtime_for_owner_finish(&runtime).is_err());
    }
    #[test]
    fn unstarted_or_failed_factory_attempt_cannot_mint_shutdown_from_absent_weak_values() {
        let mut original=NativeFactoryRetirement::new();
        assert!(original.backend.upgrade().is_none());assert!(original.runtime.upgrade().is_none());
        assert!(matches!(original.finish_after_shutdown(),Err(Error::CommitUnknown)));
        original.creation_started=true;
        assert!(matches!(original.finish_after_shutdown(),Err(Error::CommitUnknown)));
        // No native tracker callback, shutdown, OS wait or capability was made.
        assert!(!original.creation_succeeded);assert!(!original.attempted);
    }
}
