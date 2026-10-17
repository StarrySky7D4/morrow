//! Additive sealed carrier along the existing SAME MatchedRunnerArtifact Arc.
//! No wire/env/global latest-owner/alternate slot; P origin and real owned
//! setup/materializer/factory object associations precede the one admission.
use std::{any::Any,io,sync::{Arc,Weak,OnceLock,atomic::{AtomicBool,Ordering}}};
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
}
impl AsUserFactoryBinding {
    pub(crate) fn original_setup(&self)->&OwnedWindowsSandboxSetup {&self.materialization.setup}
    #[doc(hidden)]
    pub fn new(original:Arc<AsUserMaterialization>)->Arc<Self> {
        Arc::new(Self{materialization:original,backend:OnceLock::new(),claimed:AtomicBool::new(false)})
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
        let target=TargetPolicy{managed_user:managed,caller:Sid::from_bytes(&caller_sid)?,
            session,caller_auth:auth,caller_logon,caller_token,integrity_rid:0x2000};
        let bundle=Arc::new(Bundle::from_originals(original.origin.clone(),original.setup.clone(),original.s.clone(),
            original.r.clone(),original.original_materializer.clone(),self.backend.get().ok_or_else(bad)?.clone())?);
        Ok(Arc::new(crate::asuser_a1_native::Admission::bind(caller,bundle,target,generation)?))
    }
}
