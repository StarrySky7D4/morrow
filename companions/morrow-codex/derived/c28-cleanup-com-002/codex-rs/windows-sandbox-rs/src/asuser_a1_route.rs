//! Additive sealed carrier along the existing SAME MatchedRunnerArtifact Arc.
//! No wire/env/global latest-owner/alternate slot; P origin and real owned
//! setup/materializer/factory object associations precede the one admission.
use std::{any::Any,io,sync::{Arc,Weak,OnceLock,Mutex,atomic::{AtomicBool,AtomicU8,Ordering}}};
use crate::{MatchedRunnerArtifact,OwnedWindowsSandboxSetup};
use crate::asuser_a1_helper::parent_native::AsUserParentOrigin;
use crate::asuser_a1_policy::{Bundle,TargetPolicy,Sid};
fn bad()->io::Error{io::Error::new(io::ErrorKind::PermissionDenied,"A1 original call-chain binding rejected")}
/// Lower native closure only. Minted by THIS actual owner after its original
/// IO/ACK, SDK local retirement and T/GUI checked closes. Q still must retain
/// it until original owner/factory/setup normal finish; no public constructor.
pub struct AsUserOriginRetirement {
    pub(crate) origin:Arc<crate::asuser_a1_helper::parent_native::OriginGuard>,
    pub(crate) factory:Weak<dyn Any+Send+Sync>,
    pub(crate) startup:u64,
}
/// Diagnostic projection of an already consumed refusal capability. This value
/// is freely copyable and cannot authorize retirement or another invocation.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AsUserRefusalReason {MissingCallerPrivilege,LogonRejected(u32),SourcePolicyRejected(crate::WindowsTargetPolicyRejection)}
impl AsUserRefusalReason {
    pub fn label(self)->&'static str {match self {
        Self::MissingCallerPrivilege=>"KNOWN_MISSING_CALLER_PRIVILEGE",
        Self::LogonRejected(_)=>"KNOWN_LOGON_USER_REJECTED",
        Self::SourcePolicyRejected(_)=>"KNOWN_SOURCE_TARGET_POLICY_REJECTED",
    }}
    pub fn closed_label(self)->&'static str {match self {
        Self::MissingCallerPrivilege=>"KNOWN_MISSING_CALLER_PRIVILEGE_CLOSED_BEFORE_DISPATCH",
        Self::LogonRejected(_)=>"KNOWN_LOGON_USER_REJECTED_RESOURCES_CLOSED_BEFORE_DISPATCH",
        Self::SourcePolicyRejected(_)=>"KNOWN_SOURCE_TARGET_POLICY_REJECTED_RESOURCES_CLOSED_BEFORE_DISPATCH",
    }}
    pub fn win32_error(self)->Option<u32> {match self {Self::MissingCallerPrivilege|Self::SourcePolicyRejected(_)=>None,Self::LogonRejected(code)=>Some(code)}}
}
/// Actual pre-dispatch caller/logon refusal return, native closure and original
/// local registry retirement. The historical type name is kept for compatibility.
/// This value has no runner/terminal-ACK semantics and no setup cleanup right.
pub struct AsUserCallerRefusal {
    local:crate::elevated::checked_owner::LocalCallerRefusal,
    origin:Arc<crate::asuser_a1_helper::parent_native::OriginGuard>,
    factory:Weak<dyn Any+Send+Sync>,
    setup:Weak<OwnedWindowsSandboxSetup>,
}
pub struct AsUserOriginRefusalRetirement {
    pub(crate) origin:Arc<crate::asuser_a1_helper::parent_native::OriginGuard>,
    pub(crate) factory:Weak<dyn Any+Send+Sync>,
    pub(crate) setup:Weak<OwnedWindowsSandboxSetup>,
    pub(crate) startup:u64,
    reason:AsUserRefusalReason,
}
impl AsUserOriginRefusalRetirement {
    pub fn reason(&self)->AsUserRefusalReason {self.reason}
    #[doc(hidden)]
    pub fn is_for_original_setup(&self,setup:&Arc<OwnedWindowsSandboxSetup>)->bool {
        Weak::ptr_eq(&self.setup,&Arc::downgrade(setup))
    }
}
impl AsUserCallerRefusal {
    pub fn startup_attempt(&self)->u64 {self.local.startup()}
    #[doc(hidden)]
    pub fn is_for_original_backend<T:Any+Send+Sync>(&self,backend:&Arc<T>)->bool {
        let original:Arc<dyn Any+Send+Sync>=backend.clone();
        Weak::ptr_eq(&self.factory,&Arc::downgrade(&original))
    }
    #[doc(hidden)]
    pub fn retire_local(self)->io::Result<AsUserOriginRefusalRetirement> {
        self.local.require_original_retired()?;
        let startup=self.local.startup();
        let reason=self.local.reason()?;
        Ok(AsUserOriginRefusalRetirement {origin:self.origin,factory:self.factory,setup:self.setup,startup,reason})
    }
}

pub struct AsUserMaterialization {
    origin:Arc<AsUserParentOrigin>,setup:Arc<OwnedWindowsSandboxSetup>,
    s:Arc<MatchedRunnerArtifact>,r:Arc<MatchedRunnerArtifact>,original_materializer:Arc<()>,
}
impl AsUserMaterialization {
    /// Only live C code with the genuine consumed P inlet and actual setup
    /// object can call this. Q supplies its existing execution_instance Arc,
    /// original setup Arc, and original materialized R/S pins, never reacquires.
    #[doc(hidden)]
    pub fn from_originals(origin:Arc<AsUserParentOrigin>,setup:Arc<OwnedWindowsSandboxSetup>,
        s:Arc<MatchedRunnerArtifact>,r:Arc<MatchedRunnerArtifact>,materializer:Arc<()>)->io::Result<Arc<Self>> {
        if origin.is_session_attempt(){return Err(bad());}
        Self::from_bound_native_origin(origin,setup,s,r,materializer)
    }
    /// A distinct nominal attempt inlet. It cannot be obtained from serialized
    /// observations or supplied as a legacy original parent origin.
    #[doc(hidden)]
    pub fn from_parent_attempt(attempt:Arc<crate::AsUserParentAttempt>,setup:Arc<OwnedWindowsSandboxSetup>,
        s:Arc<MatchedRunnerArtifact>,r:Arc<MatchedRunnerArtifact>,materializer:Arc<()>)->io::Result<Arc<Self>> {
        Self::from_bound_native_origin(attempt.origin.clone(),setup,s,r,materializer)
    }
    fn from_bound_native_origin(origin:Arc<AsUserParentOrigin>,setup:Arc<OwnedWindowsSandboxSetup>,
        s:Arc<MatchedRunnerArtifact>,r:Arc<MatchedRunnerArtifact>,materializer:Arc<()>)->io::Result<Arc<Self>> {
        origin.guard.require_admitted(origin.pid,origin.creation)?;
        if s.sha256()!=setup.setup_exe_sha256()||s.verify_resolved(setup.setup_exe_path()).is_err()
            ||s.shares_original_pin(&r)||origin.c.shares_original_pin(&r)||origin.c.shares_original_pin(&s)
            ||r.sha256()==s.sha256(){return Err(bad());}
        let table=s.asuser_s_table()?;
        if table.c!=origin.c.sha256()||table.r!=r.sha256(){return Err(bad());}
        Ok(Arc::new(Self{origin,setup,s,r,original_materializer:materializer}))
    }
}
pub struct AsUserFactoryBinding {
    materialization:Arc<AsUserMaterialization>,
    backend:OnceLock<Weak<dyn Any+Send+Sync>>,claimed:AtomicBool,
    refusal:Mutex<Option<crate::elevated::checked_owner::LocalCallerRefusal>>,
    refusal_taken:AtomicBool,
    // Explicit qualification input only: 0 production, 1/2 logon armed/consumed,
    // 3 raced/rejected, 4/5 source policy, 6/7 after-helper stop armed/consumed. One shared
    // CAS excludes competing fault inputs; no value is a completion capability.
    start_qualification:AtomicU8,
}
impl AsUserFactoryBinding {
    pub(crate) fn original_setup(&self)->&OwnedWindowsSandboxSetup {&self.materialization.setup}
    #[doc(hidden)]
    pub fn new(original:Arc<AsUserMaterialization>)->Arc<Self> {
        Arc::new(Self{materialization:original,backend:OnceLock::new(),claimed:AtomicBool::new(false),
            refusal:Mutex::new(None),refusal_taken:AtomicBool::new(false),start_qualification:AtomicU8::new(0)})
    }
    /// The real SDK constructor supplies its returned backend Arc once. This
    /// Weak points at that concrete allocation and cannot prolong the factory.
    #[doc(hidden)]
    pub fn bind_original_backend<T:Any+Send+Sync>(&self,backend:&Arc<T>)->io::Result<()> {
        let actual:Arc<dyn Any+Send+Sync>=backend.clone();
        self.backend.set(Arc::downgrade(&actual)).map_err(|_|bad())
    }
    pub fn runner_view(self:&Arc<Self>)->Arc<MatchedRunnerArtifact> {
        self.materialization.r.asuser_view(self.clone())
    }
    pub(crate) fn claim_caller_refusal(&self,runner:&MatchedRunnerArtifact,generation:u64)->io::Result<()> {
        let original=&self.materialization;
        original.origin.guard.require_admitted(original.origin.pid,original.origin.creation)?;
        if self.start_qualification.load(Ordering::Acquire)!=0
            || !original.r.shares_original_pin(runner) || !runner.native_close_v8()
            || self.backend.get().and_then(Weak::upgrade).is_none()
            || self.claimed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() {return Err(bad());}
        // The original attempt is consumed even though its caller observation
        // denied dispatch. Another factory cannot reuse this P origin.
        original.origin.guard.claim_native_attempt(generation)?;
        Ok(())
    }
    /// Explicit disposable-VM harness fault, tied to this actual factory and
    /// its live original P inlet. It changes one submitted password only; the
    /// setup account, stored credential and native result predicates are intact.
    pub(crate) fn qualify_logon_rejection_for<T:Any+Send+Sync>(&self,backend:&Arc<T>)->io::Result<()> {
        self.arm_input_qualification_for(backend,1)
    }
    /// Denial-only qualification after real successful LogonUser/readback.
    /// Requiring managed_user == caller conflicts with the unchanged rule that
    /// the managed user must differ from caller, so it cannot admit any token.
    pub(crate) fn qualify_source_policy_rejection_for<T:Any+Send+Sync>(&self,backend:&Arc<T>)->io::Result<()> {
        self.arm_input_qualification_for(backend,4)
    }
    /// Stop-only disposable-VM fault on this original factory. It cannot create
    /// a clean refusal, a completion, a replacement owner or cleanup authority.
    pub(crate) fn qualify_after_helper_stop_for<T:Any+Send+Sync>(&self,backend:&Arc<T>)->io::Result<()> {
        self.arm_input_qualification_for(backend,6)
    }
    fn arm_input_qualification_for<T:Any+Send+Sync>(&self,backend:&Arc<T>,mode:u8)->io::Result<()> {
        if !matches!(mode,1|4|6){return Err(bad());}
        let actual:Arc<dyn Any+Send+Sync>=backend.clone();
        self.materialization.origin.guard.require_admitted(
            self.materialization.origin.pid,self.materialization.origin.creation)?;
        if !self.backend.get().is_some_and(|bound|Weak::ptr_eq(bound,&Arc::downgrade(&actual)))
            || self.claimed.load(Ordering::Acquire)
            || self.start_qualification.compare_exchange(0,mode,Ordering::AcqRel,Ordering::Acquire).is_err() {
            return Err(bad());
        }
        if self.claimed.load(Ordering::Acquire) {
            self.start_qualification.store(3,Ordering::Release);return Err(bad());
        }
        Ok(())
    }
    pub(crate) fn logon_password(&self,original:&str)->io::Result<Vec<u16>> {
        if !self.claimed.load(Ordering::Acquire){return Err(bad());}
        let text=match self.start_qualification.load(Ordering::Acquire) {
            0|5|7=>original,
            1=>{
                self.start_qualification.compare_exchange(1,2,Ordering::AcqRel,Ordering::Acquire).map_err(|_|bad())?;
                // Deterministically different, non-secret input. Nothing is
                // written to setup credentials, environment or diagnostics.
                if original=="Morrow-QA-Rejected-Logon-A" {"Morrow-QA-Rejected-Logon-B"}
                else {"Morrow-QA-Rejected-Logon-A"}
            },
            _=>return Err(bad()),
        };
        Ok(text.encode_utf16().chain(Some(0)).collect())
    }
    pub(crate) fn retain_caller_refusal(&self,original:crate::elevated::checked_owner::LocalCallerRefusal)->io::Result<()> {
        original.require_original_retired()?;
        let mut slot=self.refusal.try_lock().map_err(|_|bad())?;
        if !self.claimed.load(Ordering::Acquire) || self.refusal_taken.load(Ordering::Acquire) || slot.is_some(){return Err(bad());}
        *slot=Some(original);Ok(())
    }
    /// Same concrete factory allocation only. Exec-server may serialize its
    /// error, but no string, diagnostic snapshot or global latest-owner lookup
    /// participates in this sealed same-binding lane.
    #[doc(hidden)]
    pub fn take_caller_refusal_for<T:Any+Send+Sync>(&self,backend:&Arc<T>)->io::Result<Option<AsUserCallerRefusal>> {
        let actual:Arc<dyn Any+Send+Sync>=backend.clone();
        if !self.backend.get().is_some_and(|original|Weak::ptr_eq(original,&Arc::downgrade(&actual))) {return Err(bad());}
        let mut slot=self.refusal.try_lock().map_err(|_|bad())?;
        if self.refusal_taken.load(Ordering::Acquire){return Err(bad());}
        let Some(original)=slot.as_ref() else{return Ok(None);};
        original.require_original_retired()?;
        self.refusal_taken.store(true,Ordering::Release);
        let local=slot.take().ok_or_else(bad)?;
        Ok(Some(AsUserCallerRefusal {local,origin:self.materialization.origin.guard.clone(),
            factory:self.backend.get().ok_or_else(bad)?.clone(),setup:Arc::downgrade(&self.materialization.setup)}))
    }
    pub(crate) fn admission(&self,caller:crate::caller_prerequisites::AsUserCaller,generation:u64,runner:&MatchedRunnerArtifact)
        ->io::Result<Arc<crate::asuser_a1_native::Admission>> {
        let original=&self.materialization;
        original.origin.guard.require_admitted(original.origin.pid,original.origin.creation)?;
        if !original.r.shares_original_pin(runner)||!runner.native_close_v8()
            ||self.backend.get().and_then(Weak::upgrade).is_none()
            ||self.claimed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Err(bad());}
        original.origin.guard.claim_native_attempt(generation)?;
        let (caller_sid,auth,session,caller_token,caller_logon)=original.origin.caller_identity()?;
        caller.require_target_policy_caller(&caller_sid,auth,session)?;
        let managed=Sid::from_text(original.setup.offline_sid())?;
        let mut target=TargetPolicy{managed_user:managed,caller:Sid::from_bytes(&caller_sid)?,
            session,caller_auth:auth,caller_logon,caller_token,integrity_rid:0x2000};
        let mut stop_after_helper=false;
        match self.start_qualification.load(Ordering::Acquire) {
            0|1=>{},
            4=>{
                self.start_qualification.compare_exchange(4,5,Ordering::AcqRel,Ordering::Acquire).map_err(|_|bad())?;
                target.managed_user=target.caller;
            },
            6=>{
                self.start_qualification.compare_exchange(6,7,Ordering::AcqRel,Ordering::Acquire).map_err(|_|bad())?;
                stop_after_helper=true;
            },
            _=>return Err(bad()),
        }
        let bundle=Arc::new(Bundle::from_originals(original.origin.clone(),original.setup.clone(),original.s.clone(),
            original.r.clone(),original.original_materializer.clone(),self.backend.get().ok_or_else(bad)?.clone())?);
        Ok(Arc::new(crate::asuser_a1_native::Admission::bind(caller,bundle,target,generation,stop_after_helper)?))
    }
}
