//! Source047 retained A1 native allocation core.
//! No Drop closes. This module is not a launch authority: it requires the
//! original native P->C inlet and same materializer/factory pre-login admission.
//! The default/v7/v8 spawn path is deliberately not redirected into this core.
use crate::WindowsAsUserNativeSite as Site;
use std::{cell::UnsafeCell,ffi::c_void,io,mem,ptr,sync::atomic::{AtomicBool,Ordering},time::Instant};
use windows_sys::Win32::Foundation::{CloseHandle,GetLastError,GetHandleInformation,HANDLE,LocalFree,LUID};
use windows_sys::Win32::Security::{LogonUserW,CreateRestrictedToken,DuplicateTokenEx,SecurityImpersonation,TokenPrimary,GetTokenInformation,SetTokenInformation,LookupPrivilegeValueW,
    GetKernelObjectSecurity,SetKernelObjectSecurity,SECURITY_ATTRIBUTES,
    TokenUser,TokenStatistics,TokenGroups,TokenPrivileges,TokenSessionId,
    TokenIntegrityLevel,TokenElevation,TokenElevationType,TokenHasRestrictions,TokenDefaultDacl,TokenOwner,
    TokenIsAppContainer,TokenUIAccess,TokenVirtualizationEnabled,TokenMandatoryPolicy,
    TOKEN_USER,TOKEN_STATISTICS,TOKEN_GROUPS,SID_AND_ATTRIBUTES,TOKEN_PRIVILEGES,
    LUID_AND_ATTRIBUTES,TOKEN_MANDATORY_LABEL,TOKEN_DEFAULT_DACL,TOKEN_OWNER,TOKEN_ADJUST_DEFAULT};
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::System::Threading::{CreateProcessAsUserW,GetCurrentThread,
    GetCurrentProcess,GetProcessId,GetProcessTimes,OpenProcessToken,OpenThreadToken,PROCESS_INFORMATION,STARTUPINFOW};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW,GetProcAddress};
use super::asuser_a1_policy::{self as policy,Bundle,Command,Sid,Statistics,TargetFacts,TargetPolicy};
use super::caller_prerequisites::AsUserCaller;
#[path="asuser_logon_refusal.rs"]
mod logon_refusal;
#[path="asuser_source_policy_refusal.rs"]
mod source_policy_refusal;

/// Read the Windows installation directory from the native system query.
/// This returns path data only, never a startup, identity or cleanup capability.
/// It neither reads nor inherits the caller's environment or user profile.
pub fn native_system_windows_directory()->io::Result<String> {
    NativeSystemDirectory::read().map(NativeSystemDirectory::into_root)
}

pub(crate) const BUFFER_BYTES:usize=65536;
/// Opaque result of the fixed native system-directory query. No constructor
/// from caller env/argv/JSON or business SpawnRequest.environment exists.
pub(crate) struct NativeSystemDirectory {root:String}
impl NativeSystemDirectory {
    pub(crate) fn read()->io::Result<Self> {
        #[link(name="kernel32")]
        unsafe extern "system" {fn GetSystemWindowsDirectoryW(buffer:*mut u16,size:u32)->u32;}
        let mut buffer=[0u16;32768];
        let length=unsafe{GetSystemWindowsDirectoryW(buffer.as_mut_ptr(),buffer.len() as u32)} as usize;
        if length==0{return Err(io::Error::last_os_error());}
        if length>=buffer.len(){return Err(bad());}
        let root=String::from_utf16(&buffer[..length]).map_err(|_|bad())?;
        if root.ends_with('\\')||root.contains('\0')||root.len()<3{return Err(bad());}
        Ok(Self{root})
    }
    pub(crate) fn into_root(self)->String {self.root}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum Phase {Reserved,LoginInFlight,TokenHeld,TokenValidated,CreateInFlight,Created,
    TransferInFlight,Transferred,Retiring,Rejected,Closed,CallerRefused,CallerRefusalRetired,Unknown}
/// The refusal variant is an original resource capability, not an error-code
/// classification. Its matching closed caller allocation stays in this exact
/// cell; a later typed Logon refusal also requires checked query/CWD closure.
pub(crate) enum CallerAdmission {Ready(AsUserCaller),Refused(NativeCallerRefusal)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum RefusalKind {Caller,Logon,SourcePolicy}
pub(crate) struct NativeCallerRefusal {anchor:std::sync::Arc<()>,generation:u64,kind:RefusalKind}
pub(crate) enum NativeStart {Started,Refused(NativeCallerRefusal)}
enum LoginResult {Ready,Rejected,SourcePolicyRejected}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum State {Empty,InFlight,Held,Rejected,Transferred,Closed,Unknown,CloseFailed(u32)}
#[derive(Clone,Copy)]
struct Slot {raw:HANDLE,state:State,local:bool}
impl Slot {
    fn new(local:bool)->Self {Self{raw:0,state:State::Empty,local}}
    fn begin(&mut self)->io::Result<()> {
        if self.state!=State::Empty{return Err(bad());}self.state=State::InFlight;Ok(())
    }
    fn capture(&mut self,ok:bool,error:u32)->io::Result<()> {
        self.state=match (ok,self.raw) {
            (true,raw) if raw!=0&&raw != -1=>State::Held,
            (false,0)=>State::Rejected,_=>State::Unknown};
        if self.state==State::Held {Ok(())}else if !ok&&error!=0 {Err(io::Error::from_raw_os_error(error as i32))}
        else {Err(bad())}
    }
    fn close(&mut self)->io::Result<()> {
        match self.state {State::Empty|State::Rejected|State::Closed|State::Transferred=>return Ok(()),
            State::Held=>{},_=>return Err(bad())}
        self.state=State::InFlight;
        let ok=if self.local {unsafe{LocalFree(self.raw as *mut c_void).is_null()}}
            else {unsafe{CloseHandle(self.raw)!=0}};
        if ok {self.raw=0;self.state=State::Closed;Ok(())}
        else {let error=unsafe{GetLastError()};self.state=State::CloseFailed(error);Err(io::Error::from_raw_os_error(error as i32))}
    }
    fn retired(&self)->bool {matches!(self.state,State::Empty|State::Rejected|State::Closed|State::Transferred)}
}
fn bad()->io::Error {io::Error::new(io::ErrorKind::PermissionDenied,"A1 original native operation rejected")}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Stage {Admission,ThreadToken,Login,Grant,Token,TokenSecurity,Broker,ProcessSd,ThreadSd,Create,
    ProcessReadback,ThreadReadback,Transfer,Retirement,Unwind,Deadline}
// Distinct native operations must never share a once-only call reservation.
// In particular, deriving a token must not consume its default-DACL mutation.
#[derive(Clone,Copy)]
enum NativeCall {ThreadToken,Login,TokenQuery,SecurityDescriptor,Create,Retirement,ObjectQuery,
    PrivilegeLookup,RunnerToken,CallerToken,CallerWindow,PostThreadToken,DefaultInitialization,LaunchTokenDerivation,LaunchObjectCreation,RunnerObjectSecurity,RunnerObjectInitialization}
const NATIVE_CALL_COUNT:usize=NativeCall::RunnerObjectInitialization as usize+1;
#[derive(Clone,Copy)]
struct Failure {stage:Stage,site:Site,kind:io::ErrorKind,win32:Option<i32>,ntstatus:Option<i32>,
    target_policy_rejection:Option<crate::WindowsTargetPolicyRejection>}
#[derive(Clone,Copy)]
struct ObjectReply {status:i32,returned:u32,granted:Option<u32>}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum RunnerObjectPhase {Unobserved,InFlight,SetSucceeded,Verified,Unknown}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum DefaultPhase {Unobserved,SafeWithoutMutation,SetInFlight,SetSucceeded,Verified,Unknown}
struct DefaultInitialization {phase:DefaultPhase,status:Option<u32>}
impl DefaultInitialization {
    fn new()->Self {Self{phase:DefaultPhase::Unobserved,status:None}}
    fn choose(&mut self,safe:bool,actual_grant:u32)->io::Result<bool> {
        if self.phase!=DefaultPhase::Unobserved{return Err(bad());}
        if safe {self.phase=DefaultPhase::SafeWithoutMutation;return Ok(false);}
        if actual_grant&crate::asuser_a1_rights::TOKEN_INITIALIZATION
            !=crate::asuser_a1_rights::TOKEN_INITIALIZATION {self.phase=DefaultPhase::Unknown;return Err(bad());}
        self.phase=DefaultPhase::SetInFlight;Ok(true)
    }
    fn returned(&mut self,ok:bool,error:u32)->io::Result<()> {
        if self.phase!=DefaultPhase::SetInFlight||self.status.is_some(){return Err(bad());}
        self.status=Some(if ok{0}else{error});
        if ok{self.phase=DefaultPhase::SetSucceeded;Ok(())}else{self.phase=DefaultPhase::Unknown;
            Err(io::Error::from_raw_os_error(error as i32))}
    }
    fn verified(&mut self)->io::Result<()> {
        if !matches!(self.phase,DefaultPhase::SafeWithoutMutation|DefaultPhase::SetSucceeded){return Err(bad());}
        self.phase=DefaultPhase::Verified;Ok(())
    }
}
#[derive(Debug)]
struct NativeQueryFailure {status:i32,returned:u32}
impl std::fmt::Display for NativeQueryFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        write!(f,"A1 original NtQueryObject status {} returned {}",self.status,self.returned)
    }
}
impl std::error::Error for NativeQueryFailure {}
type QueryObject=unsafe extern "system" fn(HANDLE,u32,*mut c_void,u32,*mut u32)->i32;

#[derive(Clone)]
struct CallerWindow {
    initial:Statistics,final_stats:Statistics,context:[u8;32],user:Sid,
    integrity:u32,quota:u32,assign:u32,
    defaults:crate::asuser_a1_helper::TrustedDefaults,
}
impl CallerWindow {
    fn stable(&self)->bool {self.initial==self.final_stats&&self.initial.primary&&self.integrity==0x3000
        &&self.quota&4==0&&self.assign&4==0}
    fn before_api(&self,earlier:&Self)->bool {
        self.stable()&&earlier.stable()&&self.initial==earlier.initial&&self.context==earlier.context
            &&self.user==earlier.user&&self.quota==earlier.quota&&self.assign==earlier.assign&&self.defaults==earlier.defaults
    }
    fn restored_after_api(&self,before:&Self)->bool {
        // Documented temporary enable may change ModifiedId. Never presume
        // unchanged or changed: record both, compare the actual stable identity
        // and complete normalized security/privilege policy instead.
        self.stable()&&before.stable()&&self.initial.token==before.initial.token
            &&self.initial.auth==before.initial.auth&&self.context==before.context
            &&self.user==before.user&&self.integrity==before.integrity
            &&self.quota==before.quota&&self.assign==before.assign&&self.defaults==before.defaults
    }
}


/// Carries actual consumed caller observation AND native origin/call-chain
/// instance relation. No Clone/Deserialize or constructor from public facts.
pub(crate) struct Admission {
    caller:AsUserCaller,bundle:std::sync::Arc<Bundle>,target:TargetPolicy,generation:u64,
}
impl Admission {
    pub(crate) fn command(&self,input:&str,output:&str)->io::Result<Command> {
        Command::new(&self.bundle,input,output,policy::Environment::from_system(NativeSystemDirectory::read()?)?)
    }
    // Called only by the original factory after a native precreation origin has
    // been received. No safe public "trust me" origin constructor exists.
    pub(super) fn bind(caller:AsUserCaller,bundle:std::sync::Arc<Bundle>,
        target:TargetPolicy,generation:u64)->io::Result<Self> {
        bundle.verify_originals(caller.pid(),caller.creation())?;
        caller.require_target_policy_caller(target.caller.bytes(),target.caller_auth,target.session)?;
        if caller.integrity_rid()?!=0x3000||target.integrity_rid!=0x2000{return Err(bad());}
        if generation==0{return Err(bad());}
        Ok(Self{caller,bundle,target,generation})
    }
}
pub(crate) struct Attempt {
    outcome:crate::asuser_attempt_outcome::AttemptOutcome,phase:Phase,first:Option<Failure>,stage:Stage,site:Site,deadline:Instant,generation:u64,
    target_policy_rejection:Option<crate::WindowsTargetPolicyRejection>,
    login_token:Slot,source_facts:Option<TargetFacts>,restricted_token:Slot,token:Slot,thread_token:Slot,r_token:Slot,caller_token:Slot,post_thread_token:Slot,process_sd:Slot,thread_sd:Slot,
    process:Slot,thread:Slot,pi:PROCESS_INFORMATION,
    calls:[u8;NATIVE_CALL_COUNT],
    object_replies:[Option<ObjectReply>;5],
    buffer:Box<[u64;BUFFER_BYTES/8]>,
    token_sd:Box<[u64;BUFFER_BYTES/8]>,token_sd_len:usize,
    launch_creation:Box<[u64;512]>,launch_creation_identity:Option<(Sid,Sid)>,
    token_security:Option<crate::asuser_a1_security::TokenLayout>,
    runner_object_phase:RunnerObjectPhase,runner_object_status:Option<u32>,
    runner_creation:Box<[u64;512]>,runner_before:Box<[u64;512]>,runner_before_len:usize,
    // Original T's actual default-object template, separately captured from
    // its TOKEN OBJECT descriptor. Addendum1625 permits exactly the bounded
    // private-T default initialization below; the TOKEN OBJECT SD is read-only.
    default_acl:Box<[u8;4096]>,default_acl_len:usize,
    default_initialization:DefaultInitialization,default_reads:u8,
    default_creation:Box<[u64;512]>,post_default:Option<TargetFacts>,
    before_default_acl:Box<[u8;4096]>,before_default_len:usize,
    pipe_slots:[Slot;2],pipe_sds:[Slot;2],pipe_names:Option<(String,String)>,gui_closed:bool,
    target_statistics:Option<Statistics>,
    caller_windows:[Option<CallerWindow>;3],caller_privileges:Option<[u64;2]>,
    object_query:Option<QueryObject>,logon:Option<Sid>,admission:Option<std::sync::Arc<Admission>>,
    broker:Option<Box<crate::asuser_a1_helper::Broker>>,
    command:Option<Command>,
    caller_attempt:Option<Box<crate::CallerPrerequisiteAttempt>>,
    caller_refusal:Option<crate::caller_prerequisites::ClosedIneligibleCaller>,
    refusal_anchor:Option<std::sync::Arc<()>>,
    refusal_kind:Option<RefusalKind>,refusal_fact:Option<Failure>,
    origin_retirement_taken:bool,
}
impl Attempt {
    pub(crate) fn reserved(generation:u64,deadline:Instant)->Self {
        Self{outcome:crate::asuser_attempt_outcome::AttemptOutcome::reserved(),phase:Phase::Reserved,first:None,stage:Stage::Admission,site:Site::NotObserved,deadline,generation,
            target_policy_rejection:None,
            login_token:Slot::new(false),source_facts:None,restricted_token:Slot::new(false),token:Slot::new(false),thread_token:Slot::new(false),r_token:Slot::new(false),caller_token:Slot::new(false),post_thread_token:Slot::new(false),process_sd:Slot::new(true),thread_sd:Slot::new(true),
            process:Slot::new(false),thread:Slot::new(false),pi:unsafe{mem::zeroed()},
            calls:[0;NATIVE_CALL_COUNT],object_replies:[None;5],buffer:Box::new([0;BUFFER_BYTES/8]),token_sd:Box::new([0;BUFFER_BYTES/8]),
            token_sd_len:0,launch_creation:Box::new([0;512]),launch_creation_identity:None,token_security:None,runner_object_phase:RunnerObjectPhase::Unobserved,runner_object_status:None,
            runner_creation:Box::new([0;512]),runner_before:Box::new([0;512]),runner_before_len:0,default_acl:Box::new([0;4096]),default_acl_len:0,
            default_initialization:DefaultInitialization::new(),default_reads:0,
            default_creation:Box::new([0;512]),post_default:None,
            before_default_acl:Box::new([0;4096]),before_default_len:0,
            pipe_slots:std::array::from_fn(|_|Slot::new(false)),pipe_sds:std::array::from_fn(|_|Slot::new(true)),
            pipe_names:None,gui_closed:false,
            target_statistics:None,caller_windows:std::array::from_fn(|_|None),caller_privileges:None,
            object_query:None,logon:None,admission:None,broker:None,command:None,
            caller_attempt:Some(crate::CallerPrerequisiteAttempt::new()),caller_refusal:None,refusal_anchor:None,
            refusal_kind:None,refusal_fact:None,origin_retirement_taken:false}
    }
    fn observe_first_failure(&self, diagnostic:Option<&crate::WindowsStartDiagnostic>) {
        // An explicit returned LogonUser refusal is an expected typed outcome,
        // but remains the first native diagnostic even if its later close fails.
        let (Some(first),Some(diagnostic))=(self.refusal_fact.or(self.first),diagnostic) else{return;};
        use crate::WindowsAsUserNativeStage as Observed;
        let stage=match first.stage {
            Stage::Admission=>Observed::Admission,
            Stage::ThreadToken=>Observed::ThreadToken,
            Stage::Login=>Observed::Login,
            Stage::Grant=>Observed::Grant,
            Stage::Token=>Observed::Token,
            Stage::TokenSecurity=>Observed::TokenSecurity,
            Stage::Broker=>Observed::Broker,
            Stage::ProcessSd=>Observed::ProcessSd,
            Stage::ThreadSd=>Observed::ThreadSd,
            Stage::Create=>Observed::Create,
            Stage::ProcessReadback=>Observed::ProcessReadback,
            Stage::ThreadReadback=>Observed::ThreadReadback,
            Stage::Transfer=>Observed::Transfer,
            Stage::Retirement=>Observed::Retirement,
            Stage::Unwind=>Observed::Unwind,
            Stage::Deadline=>Observed::Deadline,
        };
        // No second OS query, GetLastError, formatting or ownership transfer.
        diagnostic.observe_asuser_native_failure_with_target_policy(stage,first.site,first.kind,first.win32,first.ntstatus,first.target_policy_rejection);
    }
    fn fail(&mut self,e:&io::Error) {
        if self.first.is_none(){self.first=Some(Failure{stage:self.stage,site:self.site,kind:e.kind(),win32:e.raw_os_error(),
            ntstatus:e.get_ref().and_then(|inner|inner.downcast_ref::<NativeQueryFailure>()).map(|reply|reply.status),
            target_policy_rejection:if self.site==Site::TargetPolicyGuard {self.target_policy_rejection}else{None}});}
        // Error reporting and physical ownership are independent. A denied
        // source token has no runner, but still owns its login/query handles.
        if matches!(self.stage,Stage::Unwind|Stage::Deadline)||self.site==Site::DeadlineGuard
            || self.command.as_ref().is_some_and(|command|command.cwd_pins.uncertain())
            || matches!(self.runner_object_phase,RunnerObjectPhase::InFlight|RunnerObjectPhase::SetSucceeded|RunnerObjectPhase::Unknown)
            || self.slots().iter().any(|slot|matches!(slot.state,State::InFlight|State::Unknown|State::CloseFailed(_))) {
            self.outcome.lost();
        }
        self.phase=Phase::Unknown;
    }
    pub(crate) fn consume_caller(&mut self)->io::Result<AsUserCaller> {
        if self.phase!=Phase::Reserved||self.admission.is_some()||self.caller_refusal.is_some(){return Err(bad());}
        // This cell is reserved in the actual blocking C native-start thread.
        // The thread-affine 046 box is never moved from H's interactive thread
        // and never reruns H's separate diagnostic observation.
        let original=self.caller_attempt.as_mut().ok_or_else(bad)?;
        if !original.reserve(){return Err(bad());}original.observe_once();
        let original=self.caller_attempt.take().ok_or_else(bad)?;
        match original.consume_for_asuser(){Ok(value)=>Ok(value),Err(original)=>{
            match original.consume_closed_ineligible() {
                Ok(closed)=>{
                    // Caller-local closure alone never releases an owner,
                    // registry charge, factory or setup. Keep this exact proof
                    // for the separate consume-only refusal transaction.
                    debug_assert!(closed.original_handles_closed());
                    self.caller_refusal=Some(closed);
                },
                Err(original)=>self.caller_attempt=Some(original),
            }
            Err(bad())}}
    }
    pub(crate) fn take_caller_admission(&mut self)->io::Result<CallerAdmission> {
        self.checkpoint(Stage::Admission)?;
        match self.consume_caller() {
            Ok(caller)=>Ok(CallerAdmission::Ready(caller)),
            Err(_) if self.phase==Phase::Reserved && self.refusal_anchor.is_none()
                && self.caller_refusal_resources_closed() && Instant::now()<self.deadline=>{
                let anchor=std::sync::Arc::new(());
                self.refusal_anchor=Some(anchor.clone());
                self.refusal_kind=Some(RefusalKind::Caller);
                self.phase=Phase::CallerRefused;
                Ok(CallerAdmission::Refused(NativeCallerRefusal{anchor,generation:self.generation,kind:RefusalKind::Caller}))
            },
            Err(error)=>Err(error),
        }
    }
    fn caller_refusal_resources_closed(&self)->bool {
        self.first.is_none() && self.refusal_fact.is_none() && !self.outcome.observation_lost
            && self.outcome.dispatch==crate::asuser_attempt_outcome::RunnerDispatch::NotAttempted
            && self.caller_attempt.is_none()
            && self.caller_refusal.as_ref().is_some_and(|proof|proof.original_handles_closed())
            && self.slots().iter().all(|slot|slot.state==State::Empty && slot.raw==0)
            && self.calls.iter().all(|count|*count==0)
            && self.pi.hProcess==0 && self.pi.hThread==0 && self.pi.dwProcessId==0 && self.pi.dwThreadId==0
            && self.admission.is_none() && self.command.is_none() && self.broker.is_none()
            && self.pipe_names.is_none() && self.runner_object_phase==RunnerObjectPhase::Unobserved
            && self.default_initialization.phase==DefaultPhase::Unobserved
    }
    pub(crate) fn require_caller_refusal(&self,proof:&NativeCallerRefusal)->io::Result<()> {
        if self.phase!=Phase::CallerRefused || self.generation!=proof.generation
            || self.refusal_kind!=Some(proof.kind)
            || !self.refusal_anchor.as_ref().is_some_and(|anchor|std::sync::Arc::ptr_eq(anchor,&proof.anchor))
            || !self.refusal_resources_closed(proof.kind) || Instant::now()>=self.deadline {return Err(bad());}
        Ok(())
    }
    fn refusal_resources_closed(&self,kind:RefusalKind)->bool {
        match kind {RefusalKind::Caller=>self.caller_refusal_resources_closed(),
            RefusalKind::Logon=>self.logon_refusal_resources_closed(),
            RefusalKind::SourcePolicy=>self.source_policy_refusal_resources_closed()}
    }
    /// Only metadata follows the caller's checked native closes. Called at the
    /// same-owner registry commit point, without native calls or resource Drop.
    pub(crate) fn commit_caller_refusal(&mut self) {
        self.phase=Phase::CallerRefusalRetired;
        self.outcome.closed();
    }
    pub(crate) fn caller_refusal_retired(&self,proof:&NativeCallerRefusal)->bool {
        self.phase==Phase::CallerRefusalRetired && self.generation==proof.generation
            && self.refusal_kind==Some(proof.kind)
            && self.refusal_anchor.as_ref().is_some_and(|anchor|std::sync::Arc::ptr_eq(anchor,&proof.anchor))
            && self.refusal_resources_closed(proof.kind)
            && self.outcome.resources==crate::asuser_attempt_outcome::LocalResources::Closed
    }
    pub(crate) fn refusal_reason(&self,proof:&NativeCallerRefusal)->io::Result<crate::AsUserRefusalReason> {
        if !self.caller_refusal_retired(proof){return Err(bad());}
        Ok(match proof.kind {
            RefusalKind::Caller=>crate::AsUserRefusalReason::MissingCallerPrivilege,
            RefusalKind::Logon=>crate::AsUserRefusalReason::LogonRejected(
                self.refusal_fact.and_then(|fact|fact.win32).ok_or_else(bad)? as u32),
            RefusalKind::SourcePolicy=>crate::AsUserRefusalReason::SourcePolicyRejected(
                self.refusal_fact.and_then(|fact|fact.target_policy_rejection).ok_or_else(bad)?),
        })
    }
    #[cfg(test)]
    pub(crate) fn caller_refusal_snapshot(&self)->Option<crate::caller_prerequisites::CallerPrerequisiteSnapshot> {
        self.caller_refusal.as_ref().map(|proof|proof.snapshot())
    }
    fn checkpoint(&mut self,stage:Stage)->io::Result<()> {
        self.stage=stage;
        if self.first.is_some(){return Err(bad());}
        if Instant::now()>=self.deadline {
            self.site=Site::DeadlineGuard;
            let e=io::Error::new(io::ErrorKind::TimedOut,"A1 original budget expired");self.fail(&e);return Err(e);
        }
        Ok(())
    }
    fn call(&mut self,operation:NativeCall,limit:u8,stage:Stage)->io::Result<()> {
        self.checkpoint(stage)?;
        let index=operation as usize;
        if self.calls[index]>=limit {let e=bad();self.fail(&e);return Err(e);}
        self.calls[index]+=1;Ok(())
    }
    fn current_identity()->io::Result<(u32,u64)> {
        let process=unsafe{GetCurrentProcess()};let pid=unsafe{GetProcessId(process)};
        let mut c=unsafe{mem::zeroed()};let mut e=c;let mut k=c;let mut u=c;
        if pid==0||unsafe{GetProcessTimes(process,&mut c,&mut e,&mut k,&mut u)}==0{return Err(io::Error::last_os_error());}
        Ok((pid,(u64::from(c.dwHighDateTime)<<32)|u64::from(c.dwLowDateTime)))
    }
    fn resolve_query(&mut self)->io::Result<()> {
        // Resolve once before login, with a borrowed existing module only.
        let name:[u16;10]=[110,116,100,108,108,46,100,108,108,0];
        let module=unsafe{GetModuleHandleW(name.as_ptr())};
        if module==0{return Err(io::Error::last_os_error());}
        let address=unsafe{GetProcAddress(module,b"NtQueryObject\0".as_ptr())}.ok_or_else(io::Error::last_os_error)?;
        self.object_query=Some(unsafe{mem::transmute::<unsafe extern "system" fn()->isize,QueryObject>(address)});Ok(())
    }
    fn caller_statistics(&mut self)->io::Result<Statistics> {
        self.call(NativeCall::TokenQuery,128,Stage::Token)?;
        if self.caller_token.state!=State::Held{return Err(bad());}
        let mut s:TOKEN_STATISTICS=unsafe{mem::zeroed()};let mut length=0;
        let ok=unsafe{GetTokenInformation(self.caller_token.raw,TokenStatistics,
            (&mut s as *mut TOKEN_STATISTICS).cast(),mem::size_of::<TOKEN_STATISTICS>() as u32,&mut length)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        if ok==0{return Err(io::Error::from_raw_os_error(error as i32));}
        if length as usize!=mem::size_of::<TOKEN_STATISTICS>(){return Err(bad());}
        let luid=|v:LUID|(u64::from(v.HighPart as u32)<<32)|u64::from(v.LowPart);
        Ok(Statistics{token:luid(s.TokenId),auth:luid(s.AuthenticationId),modified:luid(s.ModifiedId),primary:s.TokenType==1})
    }
    fn caller_window(&mut self,index:usize,a:&Admission)->io::Result<()> {
        self.call(NativeCall::CallerWindow,3,Stage::Token)?;
        if index>2||self.caller_windows[index].is_some(){return Err(bad());}
        let initial=self.caller_statistics()?;
        let subject=crate::asuser_a1_helper::retained_subject(self.caller_token.raw)?;
        let defaults=crate::asuser_a1_helper::trusted_defaults(self.caller_token.raw,&subject.sid)?;
        let final_stats=self.caller_statistics()?;
        let wanted=self.caller_privileges.ok_or_else(bad)?;
        let find=|luid:u64|->io::Result<u32> {
            let mut matches=subject.privileges.iter().filter(|(low,high,_)|
                ((u64::from(*high as u32)<<32)|u64::from(*low))==luid);
            let value=matches.next().map(|v|v.2).ok_or_else(bad)?;
            if matches.next().is_some(){return Err(bad());}Ok(value)
        };
        let window=CallerWindow{initial,final_stats,context:subject.hash,
            user:Sid::from_bytes(&subject.sid)?,integrity:subject.integrity,quota:find(wanted[0])?,assign:find(wanted[1])?,defaults};
        // Store the actual separate window even when it proves an unsupported
        // change. Never overwrite Source046 or invent a ModifiedId transition.
        self.caller_windows[index]=Some(window.clone());
        a.caller.require_primary_identity(initial.token,initial.auth,window.user.bytes())?;
        if window.defaults.logon!=a.target.caller_logon||window.initial.token!=a.target.caller_token{return Err(bad());}
        if !window.stable(){return Err(bad());}
        if index==1&&!window.before_api(self.caller_windows[0].as_ref().ok_or_else(bad)?){return Err(bad());}
        if index==2&&!window.restored_after_api(self.caller_windows[1].as_ref().ok_or_else(bad)?){return Err(bad());}
        self.checkpoint(Stage::Token)
    }
    fn prepare_caller(&mut self,a:&Admission)->io::Result<()> {
        self.call(NativeCall::CallerToken,1,Stage::Token)?;self.caller_token.begin()?;
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),8,&mut self.caller_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.caller_token.capture(ok!=0,error)?;
        let mut flags=0;
        if unsafe{GetHandleInformation(self.caller_token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        if flags&1!=0{return Err(bad());}
        let mut wanted=[0u64;2];
        for (i,name) in ["SeIncreaseQuotaPrivilege","SeAssignPrimaryTokenPrivilege"].iter().enumerate() {
            self.call(NativeCall::PrivilegeLookup,4,Stage::Token)?;
            let name:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();let mut luid:LUID=unsafe{mem::zeroed()};
            if unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut luid)}==0{return Err(io::Error::last_os_error());}
            wanted[i]=(u64::from(luid.HighPart as u32)<<32)|u64::from(luid.LowPart);
        }
        if wanted[0]==wanted[1]{return Err(bad());}self.caller_privileges=Some(wanted);
        self.caller_window(0,a)
    }
    fn caller_after_api(&mut self,a:&Admission)->io::Result<()> {
        self.call(NativeCall::PostThreadToken,1,Stage::ThreadToken)?;self.post_thread_token.begin()?;
        let ok=unsafe{OpenThreadToken(GetCurrentThread(),8,1,&mut self.post_thread_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        let captured=self.post_thread_token.capture(ok!=0,error);
        if !(ok==0&&error==1008&&self.post_thread_token.state==State::Rejected&&self.post_thread_token.raw==0) {
            captured?;return Err(bad());
        }
        self.caller_window(2,a)
    }
    fn login_inner(&mut self,a:&Admission,user:&[u16],password:&[u16])->io::Result<LoginResult> {
        self.checkpoint(Stage::Admission)?;
        if self.phase!=Phase::Reserved||self.generation!=a.generation||self.calls[NativeCall::Login as usize]!=0
            || user.len()<2||user.len()>257||password.is_empty()||password.len()>1025
            || user.last()!=Some(&0)||password.last()!=Some(&0)
            || user[..user.len()-1].contains(&0)||password[..password.len()-1].contains(&0) {return Err(bad());}
        if !user.iter().copied().eq(a.bundle.target_username().encode_utf16().chain(Some(0))) {return Err(bad());}
        self.site=Site::LoginCallerIdentity;
        let (pid,creation)=Self::current_identity()?;
        self.site=Site::LoginCallerBinding;
        a.caller.require_original(pid,creation)?;
        a.bundle.verify_originals(pid,creation)?;
        self.site=Site::ResolveObjectQuery;
        self.resolve_query()?;
        self.site=Site::ThreadTokenReservation;
        self.call(NativeCall::ThreadToken,1,Stage::ThreadToken)?;self.thread_token.begin()?;
        self.site=Site::OpenThreadTokenResult;
        let ok=unsafe{OpenThreadToken(GetCurrentThread(),8,1,&mut self.thread_token.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        let captured=self.thread_token.capture(ok!=0,error);
        if !(ok==0&&error==1008&&self.thread_token.state==State::Rejected&&self.thread_token.raw==0) {
            captured?;self.site=Site::ThreadTokenAbsenceGuard;return Err(bad());
        }
        self.checkpoint(Stage::ThreadToken)?;
        self.site=Site::PrepareCaller;
        self.prepare_caller(a)?;
        self.site=Site::LoginReservation;
        self.call(NativeCall::Login,1,Stage::Login)?;self.login_token.begin()?;self.phase=Phase::LoginInFlight;
        // Exactly one INTERACTIVE/DEFAULT login; original output is preinstalled.
        let domain=[b'.' as u16,0];
        self.site=Site::LogonUserResult;
        let ok=unsafe{LogonUserW(user.as_ptr(),domain.as_ptr(),password.as_ptr(),2,0,&mut self.login_token.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        if matches!(self.capture_login_result(ok!=0,error)?,LoginResult::Rejected) {
            return Ok(LoginResult::Rejected);
        }
        self.phase=Phase::TokenHeld;self.checkpoint(Stage::Login)?;
        let mut flags=0;
        self.site=Site::TokenHandleInformation;
        if unsafe{GetHandleInformation(self.login_token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        self.site=Site::TokenInheritanceGuard;
        if flags&1!=0{return Err(bad());} // no repair/SetHandleInformation on T
        self.site=Site::TargetFacts;
        let source=self.target_facts()?;
        self.site=Site::TargetPolicyGuard;
        if let Err(error)=a.target.validate_source(&source) {
            self.target_policy_rejection=a.target.source_rejection(&source);
            return self.capture_source_policy_rejection(source,&error);
        }
        self.source_facts=Some(source.clone());
        self.derive_launch_token(&source,a.target.caller)?;
        self.site=Site::TargetFacts;
        let facts=self.target_facts()?;
        self.site=Site::TargetPolicyGuard;
        if let Err(error)=a.target.validate_launch(&source,&facts) {
            self.target_policy_rejection=a.target.launch_rejection(&source,&facts);return Err(error);
        }
        self.site=Site::TokenSecurity;
        self.capture_token_security(facts.integrity_rid,a.caller.integrity_rid()?)?;
        self.site=Site::DefaultInitialization;
        self.initialize_default(&a.target,&facts)?;
        self.logon=Some(facts.logon);self.phase=Phase::TokenValidated;Ok(LoginResult::Ready)
    }
    /// One derivation, no fallback to the login token. Both outputs are in the
    /// original owner before any fallible work; failed/partial outputs stay owned.
    fn derive_launch_token(&mut self,source:&TargetFacts,caller:Sid)->io::Result<()> {
        self.site=Site::LaunchTokenDerivation;
        let subject=crate::asuser_a1_helper::retained_subject(self.login_token.raw)?;
        if subject.hash!=source.context||self.statistics()?!=source.final_stats{return Err(bad());}
        let name:Vec<u16>="SeChangeNotifyPrivilege".encode_utf16().chain(Some(0)).collect();
        let mut allowed:LUID=unsafe{mem::zeroed()};
        self.call(NativeCall::PrivilegeLookup,8,Stage::Token)?;
        if unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut allowed)}==0{return Err(io::Error::last_os_error());}
        let mut deleted=subject.privileges.iter().filter(|(low,high,_)|
            *low!=allowed.LowPart||*high!=allowed.HighPart).map(|(low,high,_)|
            LUID_AND_ATTRIBUTES{Luid:LUID{LowPart:*low,HighPart:*high},Attributes:0}).collect::<Vec<_>>();
        if deleted.len()!=usize::from(source.unsafe_privileges)||deleted.len()>64{return Err(bad());}
        self.call(NativeCall::LaunchTokenDerivation,1,Stage::Token)?;self.restricted_token.begin()?;
        // Explicit deletion, not DISABLE_MAX_PRIVILEGE; no SID filtering,
        // SANDBOX_INERT or LUA_TOKEN. The postcondition checks the actual result.
        let ok=unsafe{CreateRestrictedToken(self.login_token.raw,0,0,ptr::null(),
            deleted.len() as u32,deleted.as_mut_ptr(),0,ptr::null(),&mut self.restricted_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.restricted_token.capture(ok!=0,error)?;
        let mut flags=0;
        if unsafe{GetHandleInformation(self.restricted_token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        if flags&1!=0||self.restricted_token.raw==self.login_token.raw{return Err(bad());}
        self.create_launch_object(caller,source.logon)
    }
    fn create_launch_object(&mut self,caller:Sid,logon:Sid)->io::Result<()> {
        self.site=Site::LaunchObjectCreation;
        if self.restricted_token.state!=State::Held||self.launch_creation_identity.is_some(){return Err(bad());}
        let template=super::asuser_a1_security::launch_token_template(caller,logon)?;
        if template.len()>4096{return Err(bad());}
        let bytes=unsafe{std::slice::from_raw_parts_mut(self.launch_creation.as_mut_ptr().cast::<u8>(),4096)};
        bytes[..template.len()].copy_from_slice(&template);
        self.launch_creation_identity=Some((caller,logon));
        let attributes=SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor:self.launch_creation.as_mut_ptr().cast(),bInheritHandle:0};
        self.call(NativeCall::LaunchObjectCreation,1,Stage::Token)?;self.token.begin()?;
        // Same granted access as the private restricted intermediate. The new
        // object receives this exact creation DACL; neither existing token is
        // relabeled or has its security descriptor changed. No fallback token.
        let ok=unsafe{DuplicateTokenEx(self.restricted_token.raw,0,&attributes,
            SecurityImpersonation,TokenPrimary,&mut self.token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.token.capture(ok!=0,error)?;
        let mut flags=0;
        if unsafe{GetHandleInformation(self.token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        if flags&1!=0||self.token.raw==self.login_token.raw||self.token.raw==self.restricted_token.raw{return Err(bad());}
        Ok(())
    }
    /// Diagnostic-only edge on the original Err, using only the facts already
    /// captured by this Attempt. Return the SAME error; never change admission.
    fn validate_target_policy(&mut self,policy:&TargetPolicy,facts:&TargetFacts)->io::Result<()> {
        let result=policy.validate(facts);
        if result.is_err()&&self.target_policy_rejection.is_none() {
            self.target_policy_rejection=Some(policy.first_rejection(facts)
                .unwrap_or(crate::WindowsTargetPolicyRejection::UnclassifiedGuardRejection));
        }
        result
    }
    fn verify_target_before_create(&mut self)->io::Result<()> {
        let observed=self.statistics()?;
        if self.target_statistics!=Some(observed){return Err(bad());}
        self.call(NativeCall::SecurityDescriptor,4,Stage::TokenSecurity)?;
        self.buffer.fill(0);let mut length=0;
        if unsafe{GetKernelObjectSecurity(self.token.raw,0x15,self.buffer.as_mut_ptr().cast(),BUFFER_BYTES as u32,&mut length)}==0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize!=self.token_sd_len{return Err(bad());}
        let original=unsafe{std::slice::from_raw_parts(self.token_sd.as_ptr().cast::<u8>(),self.token_sd_len)};
        if original!=self.bytes(length as usize){return Err(bad());}
        let observed=self.statistics()?;
        if self.target_statistics!=Some(observed){return Err(bad());}
        let caller=self.admission.as_ref().ok_or_else(bad)?.target.caller;
        self.default_objects(caller,self.logon.ok_or_else(bad)?,true)?;
        let observed=self.statistics()?;
        if self.target_statistics!=Some(observed){return Err(bad());}
        self.checkpoint(Stage::TokenSecurity)
    }
    fn observe_default(&mut self)->io::Result<Vec<u8>> {
        if self.default_reads>=4{return Err(bad());}self.default_reads+=1;
        let length=self.query(TokenDefaultDacl)?;
        if length<mem::size_of::<TOKEN_DEFAULT_DACL>() {return Err(bad());}
        let header=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_DEFAULT_DACL>())};
        let offset=(header.DefaultDacl as usize).checked_sub(self.buffer.as_ptr() as usize).ok_or_else(bad)?;
        if offset<mem::size_of::<TOKEN_DEFAULT_DACL>()||offset%4!=0{return Err(bad());}
        let head=self.bytes(length).get(offset..offset.checked_add(8).ok_or_else(bad)?).ok_or_else(bad)?;
        let size=u16::from_le_bytes(head[2..4].try_into().map_err(|_|bad())?) as usize;
        let bytes=self.bytes(length).get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
        super::asuser_a1_security::default_layout(bytes)?;
        Ok(bytes.to_vec())
    }
    fn default_objects(&mut self,caller:Sid,logon:Sid,_compare:bool)->io::Result<()> {
        let bytes=self.observe_default()?;
        super::asuser_a1_security::default_objects(&bytes,caller,logon)?;
        if bytes!=self.default_acl[..self.default_acl_len]{return Err(bad());}
        self.checkpoint(Stage::TokenSecurity)
    }
    fn initialize_default(&mut self,policy:&TargetPolicy,before:&TargetFacts)->io::Result<()> {
        let original=self.observe_default()?;
        self.before_default_acl[..original.len()].copy_from_slice(&original);self.before_default_len=original.len();
        if self.statistics()?!=before.final_stats{return Err(bad());}
        let safe=super::asuser_a1_security::default_objects(&original,policy.caller,before.logon).is_ok();
        if self.default_initialization.choose(safe,before.granted)? {
            let template=super::asuser_a1_security::default_template(policy.caller,before.logon)?;
            let bytes=unsafe{std::slice::from_raw_parts_mut(self.default_creation.as_mut_ptr().cast::<u8>(),4096)};
            bytes[..template.len()].copy_from_slice(&template);
            let header=TOKEN_DEFAULT_DACL{DefaultDacl:self.default_creation.as_mut_ptr().cast()};
            self.call(NativeCall::DefaultInitialization,1,Stage::TokenSecurity)?;
            let ok=unsafe{SetTokenInformation(self.token.raw,TokenDefaultDacl,
                (&header as *const TOKEN_DEFAULT_DACL).cast(),mem::size_of::<TOKEN_DEFAULT_DACL>() as u32)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};self.default_initialization.returned(ok!=0,error)?;
        }
        self.checkpoint(Stage::TokenSecurity)?;
        let after=self.target_facts()?;
        policy.validate_launch(self.source_facts.as_ref().ok_or_else(bad)?,&after)?;
        let actual=self.observe_default()?;
        super::asuser_a1_security::default_objects(&actual,policy.caller,before.logon)?;
        let actual_success=self.calls[NativeCall::DefaultInitialization as usize]==1&&self.default_initialization.phase==DefaultPhase::SetSucceeded
            &&self.default_initialization.status==Some(0);
        if self.statistics()?!=after.final_stats||!after.same_after_default_initialization(before,actual_success){return Err(bad());}
        self.default_acl[..actual.len()].copy_from_slice(&actual);self.default_acl_len=actual.len();
        self.target_statistics=Some(after.final_stats);self.post_default=Some(after);
        self.default_initialization.verified()
    }
    fn query(&mut self,class:i32)->io::Result<usize> {
        self.call(NativeCall::TokenQuery,128,Stage::Token)?;self.buffer.fill(0);
        let cap=if matches!(class,TokenElevation|TokenSessionId|TokenElevationType|TokenHasRestrictions|
            TokenIsAppContainer|TokenUIAccess|TokenVirtualizationEnabled|TokenMandatoryPolicy){4}else{BUFFER_BYTES};
        let mut returned=0;
        let raw=if self.r_token.state==State::Held{self.r_token.raw}else if self.token.state==State::Held{self.token.raw}else{self.login_token.raw};
        let ok=unsafe{GetTokenInformation(raw,class,self.buffer.as_mut_ptr().cast(),cap as u32,&mut returned)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        if ok==0{return Err(io::Error::from_raw_os_error(error as i32));}
        if returned==0||returned as usize>cap{return Err(bad());}
        self.checkpoint(Stage::Token)?;Ok(returned as usize)
    }
    fn bytes(&self,len:usize)->&[u8] {unsafe{std::slice::from_raw_parts(self.buffer.as_ptr().cast(),len)}}
    fn scalar(&mut self,class:i32)->io::Result<u32> {
        let len=self.query(class)?;if len!=4{return Err(bad());}
        Ok(u32::from_le_bytes(self.bytes(4).try_into().map_err(|_|bad())?))
    }
    fn boolean(&mut self,class:i32)->io::Result<bool> {
        match self.scalar(class)? {0=>Ok(false),1=>Ok(true),_=>Err(bad())}
    }
    fn sid(&self,len:usize,p:*mut c_void,minimum:usize)->io::Result<Sid> {
        let offset=(p as usize).checked_sub(self.buffer.as_ptr() as usize).ok_or_else(bad)?;
        if offset<minimum||offset%4!=0{return Err(bad());}
        let b=self.bytes(len);let head=b.get(offset..offset+8).ok_or_else(bad)?;
        Sid::from_bytes(b.get(offset..offset+8+usize::from(head[1])*4).ok_or_else(bad)?)
    }
    fn statistics(&mut self)->io::Result<Statistics> {
        let len=self.query(TokenStatistics)?;
        if len!=mem::size_of::<TOKEN_STATISTICS>(){return Err(bad());}
        let s=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_STATISTICS>())};
        let luid=|v:LUID|(u64::from(v.HighPart as u32)<<32)|u64::from(v.LowPart);
        Ok(Statistics{token:luid(s.TokenId),auth:luid(s.AuthenticationId),modified:luid(s.ModifiedId),primary:s.TokenType==1})
    }
    fn target_facts(&mut self)->io::Result<TargetFacts> {
        self.call(NativeCall::ObjectQuery,5,Stage::Grant)?;
        let mut basic=[0u32;14];let mut returned=0;
        let raw=if self.r_token.state==State::Held{self.r_token.raw}else if self.token.state==State::Held{self.token.raw}else{self.login_token.raw};
        let status=unsafe{self.object_query.ok_or_else(bad)?(raw,0,basic.as_mut_ptr().cast(),56,&mut returned)};
        self.object_replies[usize::from(self.calls[NativeCall::ObjectQuery as usize]-1)]=Some(ObjectReply{status,returned,
            granted:(status>=0&&returned==56).then_some(basic[1])});
        if status<0{return Err(io::Error::other(NativeQueryFailure{status,returned}));}
        if returned!=56{return Err(bad());}
        let granted=basic[1];
        let initial=self.statistics()?;
        let len=self.query(TokenUser)?;
        if len<mem::size_of::<TOKEN_USER>(){return Err(bad());}
        let user=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_USER>())};
        let user=self.sid(len,user.User.Sid,mem::size_of::<TOKEN_USER>())?;
        let len=self.query(TokenGroups)?;if len<4{return Err(bad());}
        let count=u32::from_le_bytes(self.bytes(4).try_into().unwrap()) as usize;
        let offset=mem::offset_of!(TOKEN_GROUPS,Groups);let stride=mem::size_of::<SID_AND_ATTRIBUTES>();
        let end=offset.checked_add(count.checked_mul(stride).ok_or_else(bad)?).ok_or_else(bad)?;
        if count>256||end>len{return Err(bad());}
        let mut logon=None;let mut logon_count=0;let mut logon_attributes=0;let mut admin_membership=false;
        let admin=Sid::from_bytes(&[1,2,0,0,0,0,0,5,32,0,0,0,32,2,0,0])?;
        for index in 0..count {
            let group=unsafe{ptr::read(self.bytes(len).as_ptr().add(offset+index*stride).cast::<SID_AND_ATTRIBUTES>())};
            let sid=self.sid(len,group.Sid,end)?;
            // UAC deny-only Administrators is not a nonadmin managed account.
            // Observe the actual group SID, not a name or elevation boolean.
            if sid==admin{admin_membership=true;}
            if group.Attributes&0xc0000000!=0||sid.is_logon() {
                logon_count+=1;logon=Some(sid);logon_attributes=group.Attributes;
            }
        }
        let logon=logon.ok_or_else(bad)?;
        let len=self.query(TokenIntegrityLevel)?;
        if len<mem::size_of::<TOKEN_MANDATORY_LABEL>(){return Err(bad());}
        let label=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>())};
        let il=self.sid(len,label.Label.Sid,mem::size_of::<TOKEN_MANDATORY_LABEL>())?;
        let b=il.bytes();if b.len()!=12||b[..8]!=[1,1,0,0,0,0,0,16]{return Err(bad());}
        let integrity_rid=u32::from_le_bytes(b[8..12].try_into().unwrap());
        let session=self.scalar(TokenSessionId)?;let elevated=self.boolean(TokenElevation)?;
        let elevation_type=self.scalar(TokenElevationType)?;
        let restricted_len=self.query(TokenHasRestrictions)?;
        let restricted=match (restricted_len,self.bytes(restricted_len)) {
            (1,[0])|(4,[0,0,0,0])=>false,(1,[1])|(4,[1,0,0,0])=>true,_=>return Err(bad())};
        let app_container=self.boolean(TokenIsAppContainer)?;let ui_access=self.boolean(TokenUIAccess)?;
        let virtualization=self.boolean(TokenVirtualizationEnabled)?;
        let mandatory_policy=self.scalar(TokenMandatoryPolicy)?;
        let name:Vec<u16>="SeChangeNotifyPrivilege".encode_utf16().chain(Some(0)).collect();
        let mut allowed:LUID=unsafe{mem::zeroed()};
        self.call(NativeCall::PrivilegeLookup,8,Stage::Token)?;
        if unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut allowed)}==0{return Err(io::Error::last_os_error());}
        let len=self.query(TokenPrivileges)?;if len<4{return Err(bad());}
        let count=u32::from_le_bytes(self.bytes(4).try_into().unwrap()) as usize;
        let offset=mem::offset_of!(TOKEN_PRIVILEGES,Privileges);let stride=mem::size_of::<LUID_AND_ATTRIBUTES>();
        if count>64||offset.checked_add(count*stride).is_none_or(|v|v>len){return Err(bad());}
        let mut unsafe_privileges=0;let mut change_notify=None;
        for i in 0..count {
            let p=unsafe{ptr::read(self.bytes(len).as_ptr().add(offset+i*stride).cast::<LUID_AND_ATTRIBUTES>())};
            // A disabled powerful privilege is still present and potentially
            // enableable by the target; do not count only its enabled bit.
            if p.Luid.LowPart!=allowed.LowPart||p.Luid.HighPart!=allowed.HighPart||p.Attributes&4!=0 {unsafe_privileges+=1;}
            else if change_notify.replace(p.Attributes).is_some(){return Err(bad());}
        }
        let len=self.query(TokenOwner)?;if len<mem::size_of::<TOKEN_OWNER>(){return Err(bad());}
        let owner=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_OWNER>())};
        let owner=self.sid(len,owner.Owner,mem::size_of::<TOKEN_OWNER>())?;
        let subject=crate::asuser_a1_helper::retained_subject(raw)?;
        let context=subject.hash;let security_context=subject.security_context;
        let final_stats=self.statistics()?;
        Ok(TargetFacts{initial,final_stats,granted,user,owner,context,security_context,change_notify,logon,logon_attributes,logon_count,
            session,integrity_rid,elevated,elevation_type,restricted,app_container,ui_access,
            virtualization,mandatory_policy,privilege_count:count as u16,unsafe_privileges,admin_membership})
    }
    fn capture_token_security(&mut self,minimum_integrity:u32,caller_integrity:u32)->io::Result<()> {
        self.call(NativeCall::SecurityDescriptor,3,Stage::TokenSecurity)?;
        self.token_sd.fill(0);let mut len=0;
        // OWNER|DACL|LABEL. LABEL requests the actual mandatory label only; no
        // label write/repair or SeSecurityPrivilege enable is attempted.
        let ok=unsafe{GetKernelObjectSecurity(self.token.raw,0x15,self.token_sd.as_mut_ptr().cast(),BUFFER_BYTES as u32,&mut len)};
        if ok==0{return Err(io::Error::last_os_error());}
        if len<20||len as usize>BUFFER_BYTES{return Err(bad());}
        let bytes=unsafe{std::slice::from_raw_parts(self.token_sd.as_ptr().cast::<u8>(),len as usize)};
        // Readback storage is retained with T, not a hash-to-authority shortcut.
        // Policy for token-object reachability is part of original pre-login
        // admission; a token ACL alone cannot prove the inaccessible C handles.
        let (caller,logon)=self.launch_creation_identity.ok_or_else(bad)?;
        let layout=super::asuser_a1_security::launch_token(bytes,caller,logon,minimum_integrity,caller_integrity)?;
        self.token_security=Some(layout);
        self.token_sd_len=len as usize;self.checkpoint(Stage::TokenSecurity)
    }
    fn descriptor(slot:&mut Slot,text:&str)->io::Result<()> {
        let wide:Vec<u16>=text.encode_utf16().chain(Some(0)).collect();slot.begin()?;
        let mut raw=ptr::null_mut();
        let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(),1,&mut raw,ptr::null_mut())};
        let error=if ok==0{unsafe{GetLastError()}}else{0};slot.raw=raw as isize;slot.capture(ok!=0,error)
    }
    fn create_inner(&mut self,a:&Admission)->io::Result<()> {
        self.site=Site::CreateInputGuard;
        self.checkpoint(Stage::Create)?;
        if self.phase!=Phase::TokenValidated||self.calls[NativeCall::Create as usize]!=0||self.generation!=a.generation{return Err(bad());}
        let logon=self.logon.ok_or_else(bad)?;
        self.site=Site::ProcessDescriptor;
        self.checkpoint(Stage::ProcessSd)?;
        Self::descriptor(&mut self.process_sd,&policy::process_thread_sddl(a.target.caller,logon,false)?)?;
        self.site=Site::ThreadDescriptor;
        self.checkpoint(Stage::ThreadSd)?;
        Self::descriptor(&mut self.thread_sd,&policy::process_thread_sddl(a.target.caller,logon,true)?)?;
        let attrs=|slot:&Slot|SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor:slot.raw as *mut c_void,bInheritHandle:0};
        let process_sa=attrs(&self.process_sd);let thread_sa=attrs(&self.thread_sd);
        self.site=Site::CallerBeforeCreate;
        self.caller_window(1,a)?;
        self.site=Site::CreateCommandGuard;
        let command=self.command.as_mut().ok_or_else(bad)?;
        if !command.cwd_pins.ready(){return Err(bad());}
        let (application,cmd,environment,cwd,desktop)=(command.application.as_ptr(),command.command.as_mut_ptr(),
            command.environment.as_mut_ptr(),command.cwd.as_ptr(),command.desktop.as_mut_ptr());
        let mut si:STARTUPINFOW=unsafe{mem::zeroed()};si.cb=mem::size_of::<STARTUPINFOW>() as u32;
        si.lpDesktop=desktop;
        self.site=Site::CreateReservation;
        self.call(NativeCall::Create,1,Stage::Create)?;self.phase=Phase::CreateInFlight;self.outcome.create_entered();
        self.process.begin()?;self.thread.begin()?;
        // No inheritance: R uses the existing by-name authenticated pipe
        // transport. Token, SD, keys, parent/control and GUI aliases are absent
        // from the runner's handle table. No profile, suspended start or repair.
        self.site=Site::CreateProcessAsUserResult;
        let ok=unsafe{CreateProcessAsUserW(self.token.raw,application,cmd,
            &process_sa,&thread_sa,0,0x08000000|0x00000400,environment,cwd,&si,&mut self.pi)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        // Capture BOTH raw outputs before any fallible operation, even rejection.
        self.process.raw=self.pi.hProcess;self.thread.raw=self.pi.hThread;
        let process=self.process.capture(ok!=0,error);let thread=self.thread.capture(ok!=0,error);
        if ok==0&&self.process.state==State::Rejected&&self.thread.state==State::Rejected {
            self.phase=Phase::Rejected;self.outcome.create_rejected();return Err(io::Error::from_raw_os_error(error as i32));
        }
        process?;thread?;
        self.site=Site::CreateOutputIdentityGuard;
        if self.pi.dwProcessId==0||self.pi.dwThreadId==0||self.process.raw==self.thread.raw{return Err(bad());}
        self.phase=Phase::Created;self.checkpoint(Stage::Create)?;
        self.site=Site::CallerAfterCreate;
        self.caller_after_api(a)
    }
    fn readback(&mut self,a:&Admission)->io::Result<()> {
        if self.phase!=Phase::Created||self.runner_object_phase!=RunnerObjectPhase::Unobserved{return Err(bad());}
        for thread in [false,true] {
            self.call(NativeCall::SecurityDescriptor,4,if thread{Stage::ThreadReadback}else{Stage::ProcessReadback})?;
            self.buffer.fill(0);let mut length=0;
            let raw=if thread{self.thread.raw}else{self.process.raw};
            if unsafe{GetKernelObjectSecurity(raw,0x15,self.buffer.as_mut_ptr().cast(),BUFFER_BYTES as u32,&mut length)}==0 {
                return Err(io::Error::last_os_error());
            }
            if length<20||length as usize>BUFFER_BYTES{return Err(bad());}
            super::asuser_a1_security::creation(self.bytes(length as usize),a.target.caller,
                self.logon.ok_or_else(bad)?,thread,a.target.integrity_rid)?;
            self.checkpoint(if thread{Stage::ThreadReadback}else{Stage::ProcessReadback})?;
        }
        self.checkpoint(Stage::Token)?;
        self.call(NativeCall::RunnerToken,1,Stage::Token)?;
        self.r_token.begin()?;
        self.site=Site::RunnerTokenOpen;
        let ok=unsafe{OpenProcessToken(self.process.raw,crate::asuser_a1_rights::RUNNER_TOKEN_INITIALIZATION,&mut self.r_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.r_token.capture(ok!=0,error)?;
        let before=self.target_facts()?;
        let baseline=self.post_default.as_ref().ok_or_else(bad)?.clone();
        a.target.validate_runner_initialization(&before,&baseline)?;
        self.site=Site::RunnerTokenBefore;
        let old=self.runner_object_security(a,false,before.user)?;
        let template=super::asuser_a1_security::runner_token_template(a.target.caller,before.logon)?;
        if template.len()>4096||self.runner_object_phase!=RunnerObjectPhase::Unobserved{return Err(bad());}
        let output=unsafe{std::slice::from_raw_parts_mut(self.runner_creation.as_mut_ptr().cast::<u8>(),4096)};
        output[..template.len()].copy_from_slice(&template);
        self.site=Site::RunnerTokenInitialization;
        self.call(NativeCall::RunnerObjectInitialization,1,Stage::TokenSecurity)?;
        self.runner_object_phase=RunnerObjectPhase::InFlight;
        // Only the new actual R primary, never the input launch token. No
        // owner/label/default change; the existing kernel BA-query ACE stays.
        let ok=unsafe{SetKernelObjectSecurity(self.r_token.raw,0x80000004,self.runner_creation.as_mut_ptr().cast())};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.runner_object_status=Some(error);
        self.runner_object_phase=if ok!=0{RunnerObjectPhase::SetSucceeded}else{RunnerObjectPhase::Unknown};
        if ok==0{return Err(io::Error::from_raw_os_error(error as i32));}
        self.site=Site::RunnerTokenAfter;
        if self.runner_object_security(a,true,before.user)?!=old{return Err(bad());}
        let after=self.target_facts()?;
        a.target.validate_runner_initialization(&after,&baseline)?;
        if after!=before{return Err(bad());}
        self.default_objects(a.target.caller,after.logon,true)?;
        if self.statistics()?!=after.final_stats{return Err(bad());}
        // Re-read the input token's object descriptor via its original handle.
        // Its prelaunch descriptor is immutable despite the distinct R init.
        self.call(NativeCall::SecurityDescriptor,5,Stage::TokenSecurity)?;
        self.buffer.fill(0);let mut length=0;
        if unsafe{GetKernelObjectSecurity(self.token.raw,0x15,self.buffer.as_mut_ptr().cast(),BUFFER_BYTES as u32,&mut length)}==0 {
            return Err(io::Error::last_os_error());
        }
        let original=unsafe{std::slice::from_raw_parts(self.token_sd.as_ptr().cast::<u8>(),self.token_sd_len)};
        if length as usize!=self.token_sd_len||original!=self.bytes(length as usize){return Err(bad());}
        self.call(NativeCall::TokenQuery,128,Stage::TokenSecurity)?;
        let mut launch:TOKEN_STATISTICS=unsafe{mem::zeroed()};let mut returned=0;
        if unsafe{GetTokenInformation(self.token.raw,TokenStatistics,(&mut launch as *mut TOKEN_STATISTICS).cast(),
            mem::size_of::<TOKEN_STATISTICS>() as u32,&mut returned)}==0{return Err(io::Error::last_os_error());}
        let luid=|v:LUID|(u64::from(v.HighPart as u32)<<32)|u64::from(v.LowPart);
        let unchanged=Statistics{token:luid(launch.TokenId),auth:luid(launch.AuthenticationId),
            modified:luid(launch.ModifiedId),primary:launch.TokenType==1};
        if returned as usize!=mem::size_of::<TOKEN_STATISTICS>()||Some(unchanged)!=self.target_statistics{return Err(bad());}
        self.runner_object_phase=RunnerObjectPhase::Verified;
        self.site=Site::RunnerTokenClose;
        self.r_token.close()?;self.checkpoint(Stage::Token)?;
        Ok(())
    }
    fn runner_object_security(&mut self,a:&Admission,initialized:bool,user:Sid)->io::Result<super::asuser_a1_security::RunnerTokenReadback> {
        if self.r_token.state!=State::Held{return Err(bad());}
        self.call(NativeCall::RunnerObjectSecurity,2,Stage::TokenSecurity)?;
        self.buffer.fill(0);let mut length=0;
        // OWNER|GROUP|DACL|LABEL, from the actual original primary handle.
        if unsafe{GetKernelObjectSecurity(self.r_token.raw,0x17,self.buffer.as_mut_ptr().cast(),BUFFER_BYTES as u32,&mut length)}==0 {
            return Err(io::Error::last_os_error());
        }
        if length<20||length>4096{return Err(bad());}
        let facts=super::asuser_a1_security::runner_token_object(self.bytes(length as usize),a.target.caller,
            self.logon.ok_or_else(bad)?,user,initialized)?;
        if !initialized {
            if self.runner_before_len!=0{return Err(bad());}
            let before=unsafe{std::slice::from_raw_parts_mut(self.runner_before.as_mut_ptr().cast::<u8>(),4096)};
            before[..length as usize].copy_from_slice(self.bytes(length as usize));self.runner_before_len=length as usize;
        }
        Ok(facts)
    }
    fn slots(&self)->[&Slot;15] {
        [&self.login_token,&self.restricted_token,&self.token,&self.thread_token,&self.r_token,&self.caller_token,&self.post_thread_token,
            &self.process_sd,&self.thread_sd,&self.process,&self.thread,&self.pipe_slots[0],&self.pipe_slots[1],
            &self.pipe_sds[0],&self.pipe_sds[1]]
    }
    pub(crate) fn outstanding(&self)->bool {
        !self.slots().iter().all(|s|s.retired())
            || self.command.as_ref().is_some_and(|command|command.cwd_pins.outstanding())
            || matches!(self.phase,Phase::LoginInFlight|Phase::CreateInFlight|Phase::TransferInFlight|Phase::Unknown)
    }
    /// SDK/registry caller must already hold the real original native completion
    /// after forwarder joins and ACK-slot retirement. Never an A0 resource receipt.
    pub(crate) fn retire_after_native(&mut self)->io::Result<()> {
        self.stage=Stage::Retirement;
        let result=self.retire_inner();if let Err(error)=&result{self.fail(error);}result
    }
    fn retire_inner(&mut self)->io::Result<()> {
        if self.phase!=Phase::Transferred||!matches!(self.process.state,State::Transferred)
            ||!matches!(self.thread.state,State::Transferred)||self.calls[NativeCall::Retirement as usize]!=0||!self.gui_closed {return Err(bad());}
        // Creation budget is not reused after arbitrary business lifetime. This
        // is one finite normal-close budget, never a retry/new launch budget.
        self.deadline=Instant::now().checked_add(std::time::Duration::from_secs(1)).ok_or_else(bad)?;
        self.checkpoint(Stage::Retirement)?;
        self.calls[NativeCall::Retirement as usize]=1;
        self.phase=Phase::Retiring;
        for slot in &mut self.pipe_sds{slot.close()?;}
        self.checkpoint(Stage::Retirement)?;
        self.thread_sd.close()?;self.checkpoint(Stage::Retirement)?;
        self.process_sd.close()?;self.checkpoint(Stage::Retirement)?;
        self.thread_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.post_thread_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.caller_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.token.close()?;self.checkpoint(Stage::Retirement)?;
        self.restricted_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.login_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.command.as_mut().ok_or_else(bad)?.cwd_pins.close_once(self.deadline)?;
        self.checkpoint(Stage::Retirement)?;
        if self.outstanding(){return Err(bad());}self.phase=Phase::Closed;self.outcome.closed();Ok(())
    }
    pub(crate) fn native_adopted(&self)->bool {
        self.first.is_none()&&matches!(self.phase,Phase::Transferred|Phase::Closed)
    }
    pub(crate) fn take_origin_retirement(&mut self)->io::Result<crate::AsUserOriginRetirement> {
        if self.first.is_some()||self.phase!=Phase::Closed||self.outstanding()||self.origin_retirement_taken{return Err(bad());}
        let bundle=&self.admission.as_ref().ok_or_else(bad)?.bundle;
        let proof=bundle.origin_retirement(self.generation);self.origin_retirement_taken=true;Ok(proof)
    }
    pub(crate) fn bind_admission(&mut self,a:std::sync::Arc<Admission>,runner:&crate::MatchedRunnerArtifact)->io::Result<()> {
        self.site=Site::AdmissionBind;
        let result=self.bind_inner(a,runner);if let Err(error)=&result{self.fail(error);}result
    }
    fn bind_inner(&mut self,a:std::sync::Arc<Admission>,runner:&crate::MatchedRunnerArtifact)->io::Result<()> {
        self.checkpoint(Stage::Admission)?;
        if self.phase!=Phase::Reserved||self.admission.is_some()||a.generation!=self.generation
            ||!a.bundle.same_original_runner(runner){return Err(bad());}
        self.broker=Some(crate::asuser_a1_helper::Broker::new(std::sync::Arc::clone(&a.bundle),self.generation,self.deadline));
        self.admission=Some(a);Ok(())
    }
    pub(crate) fn set_pipe_names(&mut self,input:String,output:String)->io::Result<()> {
        self.site=Site::PipeNamesGuard;
        if self.phase!=Phase::Reserved||self.pipe_names.is_some()||input==output||input.len()>160||output.len()>160
            ||![&input,&output].iter().all(|v|v.starts_with(r"\\.\pipe\")){return Err(bad());}
        self.pipe_names=Some((input,output));Ok(())
    }
    pub(crate) fn start(&mut self,command:Command,user:&[u16],password:&[u16],
        stage:impl FnOnce(&std::fs::File,&std::fs::File)->io::Result<()>)->io::Result<NativeStart> {
        let result=self.start_inner(command,user,password,stage);if let Err(error)=&result{self.fail(error);}result
    }
    fn start_inner(&mut self,command:Command,user:&[u16],password:&[u16],
        stage:impl FnOnce(&std::fs::File,&std::fs::File)->io::Result<()>)->io::Result<NativeStart> {
        self.site=Site::StartCommandGuard;
        if self.command.is_some(){return Err(bad());}
        // All UTF-16/env/CWD locks remain on this SAME original owner through
        // native rejection, cancellation, transfer failure, deadline or unwind.
        self.command=Some(command);
        self.command.as_mut().ok_or_else(bad)?.pin_cwd_once(self.deadline)?;
        let admission=self.admission.as_ref().cloned().ok_or_else(bad)?;
        self.site=Site::LoginInputGuard;
        match self.login_inner(&admission,user,password) {
            Ok(LoginResult::Ready)=>{},
            Ok(LoginResult::Rejected)=>return self.close_known_logon_refusal().map(NativeStart::Refused),
            Ok(LoginResult::SourcePolicyRejected)=>return self.close_known_source_policy_refusal().map(NativeStart::Refused),
            Err(error)=>{
                self.fail(&error);
                self.close_rejected_login_handles();
                return Err(error);
            },
        }
        self.site=Site::TransportPipeInputs;
        self.create_transport_pipes(&admission)?;
        self.site=Site::StageOriginalTransport;
        {use std::os::windows::io::FromRawHandle;
            let write=mem::ManuallyDrop::new(unsafe{std::fs::File::from_raw_handle(self.pipe_slots[0].raw as _)});
            let read=mem::ManuallyDrop::new(unsafe{std::fs::File::from_raw_handle(self.pipe_slots[1].raw as _)});
            stage(&write,&read)?;}
        self.site=Site::BrokerInputs;
        self.checkpoint(Stage::Broker)?;
        let command=self.command.as_mut().ok_or_else(bad)?;
        self.site=Site::BrokerStart;
        let desktop=self.broker.as_mut().ok_or_else(bad)?.start(self.logon.ok_or_else(bad)?,&mut command.environment,&mut self.site)?;
        command.desktop=desktop.encode_utf16().chain(Some(0)).collect();
        self.checkpoint(Stage::Broker)?;
        self.site=Site::TargetBeforeCreate;
        self.verify_target_before_create()?;
        self.site=Site::CreateInputGuard;
        self.create_inner(&admission)?;
        self.site=Site::NativeReadback;
        self.readback(&admission)?;Ok(NativeStart::Started)
    }
    fn close_rejected_login_handles(&mut self) {
        use crate::asuser_attempt_outcome::RunnerDispatch;
        if self.outcome.observation_lost||self.outcome.dispatch!=RunnerDispatch::NotAttempted
            ||self.calls[NativeCall::Retirement as usize]!=0||self.calls[NativeCall::Create as usize]!=0
            ||self.slots().iter().any(|s|!matches!(s.state,State::Empty|State::Rejected|State::Held|State::Closed)) {
            return;
        }
        self.calls[NativeCall::Retirement as usize]=1; // one cleanup attempt, independent of the failed login budget
        let Some(deadline)=Instant::now().checked_add(std::time::Duration::from_secs(1)) else {
            self.outcome.lost();return;
        };
        for slot in [&mut self.token,&mut self.restricted_token,&mut self.login_token,&mut self.thread_token,&mut self.caller_token] {
            if Instant::now()>=deadline||slot.close().is_err() {self.outcome.lost();return;}
        }
        // Keep factory/account, path pins and original registry ownership. A
        // local HANDLE close is not a full transaction/native completion receipt.
        // The first failure and the no-replay phase remain unchanged.
    }
    fn create_transport_pipes(&mut self,a:&Admission)->io::Result<()> {
        self.site=Site::TransportPipeInputs;
        let names=self.pipe_names.as_ref().ok_or_else(bad)?.clone();let logon=self.logon.ok_or_else(bad)?;
        for (index,name) in [names.0,names.1].into_iter().enumerate(){
            let text=format!("D:P(A;;GRGWRC;;;{})(A;;GRGW;;;{})(A;;RC;;;OW)",a.target.caller.text(),logon.text());
            self.site=Site::TransportPipeDescriptor;
            Self::descriptor(&mut self.pipe_sds[index],&text)?;
            let sa=SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor:self.pipe_sds[index].raw as _,bInheritHandle:0};
            let name=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            self.site=Site::TransportPipeReservation;
            self.pipe_slots[index].begin()?;
            self.site=Site::CreateNamedPipeResult;
            let raw=unsafe{windows_sys::Win32::System::Pipes::CreateNamedPipeW(name.as_ptr(),
                if index==0{2|0x80000}else{1|0x80000},8,1,65536,65536,0,&sa)};
            let error=if raw==0||raw == -1{unsafe{GetLastError()}}else{0};
            self.pipe_slots[index].raw=raw;self.pipe_slots[index].capture(raw!=0&&raw != -1,error)?;
            self.site=Site::TransportPipeCheckpoint;
            self.checkpoint(Stage::TokenSecurity)?;
        }Ok(())
    }
    pub(crate) fn transport_files(&mut self)->io::Result<(std::fs::File,std::fs::File)> {
        use std::os::windows::io::FromRawHandle;
        if self.phase!=Phase::Created||!self.runner_initialization_closed()||self.pipe_slots.iter().any(|v|v.state!=State::Held){return Err(bad());}
        let raw=[self.pipe_slots[0].raw,self.pipe_slots[1].raw];
        for slot in &mut self.pipe_slots{slot.raw=0;slot.state=State::Transferred;}
        Ok(unsafe{(std::fs::File::from_raw_handle(raw[0] as _),std::fs::File::from_raw_handle(raw[1] as _))})
    }
    pub(crate) fn desktop_name(&self)->io::Result<String> {
        if self.phase!=Phase::Created||!self.runner_initialization_closed(){return Err(bad());}
        let desktop=&self.command.as_ref().ok_or_else(bad)?.desktop;
        let units=desktop.strip_suffix(&[0]).ok_or_else(bad)?;
        String::from_utf16(units).map_err(|_|bad())
    }
    pub(crate) fn close_gui_after_native(&mut self)->io::Result<()> {
        if self.first.is_some()||self.phase!=Phase::Transferred||self.gui_closed{return Err(bad());}
        self.broker.as_mut().ok_or_else(bad)?.retire_aliases()?;self.gui_closed=true;Ok(())
    }
    /// Move the originals only into the original pre-admitted owner. A transfer
    /// panic/error leaves TransferInFlight/Unknown, never an absence/close proof.
    /// Raw values are private borrowed call operands, never public observations.
    pub(crate) fn transfer_to_owner(&mut self,
        install:impl FnOnce(HANDLE,u32,HANDLE,u32)->io::Result<()>)->io::Result<()> {
        let result=self.transfer_inner(install);if let Err(error)=&result{self.fail(error);}result
    }
    fn transfer_inner(&mut self,
        install:impl FnOnce(HANDLE,u32,HANDLE,u32)->io::Result<()>)->io::Result<()> {
        self.checkpoint(Stage::Transfer)?;
        if self.phase!=Phase::Created||!self.runner_initialization_closed()
            ||self.process.state!=State::Held||self.thread.state!=State::Held{return Err(bad());}
        self.phase=Phase::TransferInFlight;
        install(self.process.raw,self.pi.dwProcessId,self.thread.raw,self.pi.dwThreadId)?;
        self.process.raw=0;self.thread.raw=0;
        self.process.state=State::Transferred;self.thread.state=State::Transferred;
        self.phase=Phase::Transferred;self.outcome.adopted();self.checkpoint(Stage::Transfer)
    }
    fn runner_initialization_closed(&self)->bool {
        self.first.is_none()&&self.runner_object_phase==RunnerObjectPhase::Verified
            &&self.runner_object_status==Some(0)&&self.calls[NativeCall::RunnerObjectInitialization as usize]==1
            &&self.runner_before_len>0&&self.r_token.state==State::Closed&&self.r_token.raw==0
    }
}
/// Preinstalled on the existing original CheckedRunnerOwner; never a new global
/// lane/registry/permit. No owner/Core/registry lock crosses native calls.
pub(crate) struct Cell {busy:AtomicBool,original:UnsafeCell<Attempt>}
unsafe impl Send for Cell {}
unsafe impl Sync for Cell {}
impl Cell {
    pub(crate) fn new(generation:u64,deadline:Instant)->Self {
        Self{busy:AtomicBool::new(false),original:UnsafeCell::new(Attempt::reserved(generation,deadline))}
    }
    pub(crate) fn with<T>(&self,f:impl FnOnce(&mut Attempt)->io::Result<T>)->io::Result<T> {
        self.with_observed(None,f)
    }
    pub(crate) fn with_observed<T>(&self,diagnostic:Option<&crate::WindowsStartDiagnostic>,
        f:impl FnOnce(&mut Attempt)->io::Result<T>)->io::Result<T> {
        if self.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() {
            return Err(io::Error::new(io::ErrorKind::WouldBlock,"A1 original cell borrowed"));
        }
        struct Borrow<'a>(&'a AtomicBool);
        impl Drop for Borrow<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
        let _borrow=Borrow(&self.busy);
        // Entire stable allocation stays in original owner even on panic.
        let original=unsafe{&mut *self.original.get()};
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(original))) {
            Ok(Ok(value))=>{
                original.observe_first_failure(diagnostic);
                if let Some(first)=original.first {
                    if let Some(code)=first.win32{return Err(io::Error::from_raw_os_error(code));}
                    if let Some(status)=first.ntstatus {
                        let returned=original.object_replies.iter().flatten().find(|v|v.status==status).map_or(0,|v|v.returned);
                        return Err(io::Error::other(NativeQueryFailure{status,returned}));
                    }
                    return Err(io::Error::new(first.kind,"A1 original failure retained; ignored error cannot become success"));
                }
                Ok(value)
            },Ok(Err(error))=>{
                original.fail(&error);
                original.observe_first_failure(diagnostic);
                if let Some(first)=original.first {
                    if let Some(code)=first.win32{return Err(io::Error::from_raw_os_error(code));}
                    if let Some(status)=first.ntstatus{return Err(io::Error::other(NativeQueryFailure{status,
                        returned:original.object_replies.iter().flatten().find(|v|v.status==status).map_or(0,|v|v.returned)}));}
                    return Err(io::Error::new(first.kind,"A1 original first failure retained"));
                }Err(error)
            },
            Err(_)=>{original.stage=Stage::Unwind;original.site=Site::Unwind;let error=io::Error::other("A1 original native unwind");
                original.fail(&error);original.observe_first_failure(diagnostic);Err(error)}
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn checkpoint_deadline_retains_resources_even_before_dispatch() {
        let mut attempt=Attempt::reserved(1,Instant::now());
        assert!(attempt.checkpoint(Stage::Token).is_err());
        assert!(attempt.outcome.observation_lost);
        attempt.close_rejected_login_handles();
        assert_eq!(attempt.calls[NativeCall::Retirement as usize],0);
        assert_eq!(attempt.outcome.resources,crate::asuser_attempt_outcome::LocalResources::RetainedUnknown);
    }
    #[test] fn token_derivation_and_default_initialization_have_independent_one_shot_budgets() {
        for repeated in [NativeCall::LaunchTokenDerivation,NativeCall::LaunchObjectCreation,NativeCall::DefaultInitialization,NativeCall::RunnerObjectInitialization] {
            let mut attempt=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(5));
            attempt.call(NativeCall::LaunchTokenDerivation,1,Stage::Token).unwrap();
            attempt.call(NativeCall::LaunchObjectCreation,1,Stage::Token).unwrap();
            attempt.call(NativeCall::DefaultInitialization,1,Stage::TokenSecurity).unwrap();
            attempt.call(NativeCall::RunnerObjectInitialization,1,Stage::TokenSecurity).unwrap();
            assert_eq!(attempt.calls[NativeCall::LaunchTokenDerivation as usize],1);
            assert_eq!(attempt.calls[NativeCall::DefaultInitialization as usize],1);
            assert!(attempt.call(repeated,1,Stage::Token).is_err());
            assert_eq!(attempt.calls[NativeCall::Create as usize],0);
        }
    }
    #[test] fn default_initialization_uses_actual_adjust_default_only_for_one_required_mutation() {
        let ordinary=crate::asuser_a1_rights::TOKEN_REQUIRED;
        let mut safe=DefaultInitialization::new();assert!(!safe.choose(true,ordinary).unwrap());
        assert_eq!(safe.status,None);safe.verified().unwrap();assert!(safe.choose(true,ordinary).is_err());
        let mut absent=DefaultInitialization::new();assert!(absent.choose(false,ordinary).is_err());
        assert_eq!(absent.phase,DefaultPhase::Unknown);assert_eq!(absent.status,None);
        let mut init=DefaultInitialization::new();
        assert!(init.choose(false,ordinary|TOKEN_ADJUST_DEFAULT).unwrap());
        init.returned(false,5).unwrap_err();assert_eq!(init.status,Some(5));
        assert!(init.choose(false,ordinary|TOKEN_ADJUST_DEFAULT).is_err());assert!(init.verified().is_err());
        let mut init=DefaultInitialization::new();assert!(init.choose(false,ordinary|TOKEN_ADJUST_DEFAULT).unwrap());
        init.returned(true,0).unwrap();init.verified().unwrap();assert_eq!(init.phase,DefaultPhase::Verified);
        assert!(init.returned(true,0).is_err());
    }
    #[test] fn runner_initialization_cannot_publish_until_its_original_write_handle_is_checked_closed() {
        let mut a=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(5));
        a.phase=Phase::Created;a.runner_object_phase=RunnerObjectPhase::Verified;
        a.runner_object_status=Some(0);a.calls[NativeCall::RunnerObjectInitialization as usize]=1;
        a.runner_before_len=20;a.r_token.raw=44;a.r_token.state=State::Held;
        assert!(!a.runner_initialization_closed());assert!(a.transport_files().is_err());
        assert!(a.desktop_name().is_err());
        assert!(a.transfer_inner(|_,_,_,_|panic!("must not publish")).is_err());
        // State fixture only: no OS call or fake handle close is performed.
        a.r_token.raw=0;a.r_token.state=State::Closed;assert!(a.runner_initialization_closed());
        a.runner_object_status=Some(5);assert!(!a.runner_initialization_closed());
        a.runner_object_status=Some(0);a.runner_before_len=0;assert!(!a.runner_initialization_closed());
    }
    #[test] fn uncertain_runner_initialization_keeps_original_owner_and_forbids_retry() {
        for phase in [RunnerObjectPhase::InFlight,RunnerObjectPhase::SetSucceeded,RunnerObjectPhase::Unknown] {
            let mut a=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(5));
            a.phase=Phase::Created;a.runner_object_phase=phase;
            a.r_token.raw=44;a.r_token.state=State::Held;
            a.calls[NativeCall::RunnerObjectInitialization as usize]=1;
            a.site=Site::RunnerTokenAfter;a.stage=Stage::TokenSecurity;
            a.fail(&io::Error::from_raw_os_error(5));
            assert!(a.outcome.observation_lost);assert_eq!(a.phase,Phase::Unknown);
            assert_eq!(a.r_token.raw,44);assert_eq!(a.r_token.state,State::Held);
            assert!(a.call(NativeCall::RunnerObjectInitialization,1,Stage::TokenSecurity).is_err());
            assert_eq!(a.calls[NativeCall::RunnerObjectInitialization as usize],1);
        }
    }
    #[test] fn failed_outputs_are_retained_unless_actual_rejection_has_canonical_null_output() {
        let mut slot=Slot::new(false);slot.begin().unwrap();slot.raw=44;
        assert!(slot.capture(false,5).is_err());assert_eq!(slot.raw,44);assert_eq!(slot.state,State::Unknown);
        assert!(!slot.retired());assert!(slot.begin().is_err());
        let mut slot=Slot::new(false);slot.begin().unwrap();slot.raw=-1;
        assert!(slot.capture(false,5).is_err());assert_eq!(slot.state,State::Unknown);
        let mut slot=Slot::new(false);slot.begin().unwrap();
        assert!(slot.capture(false,1314).is_err());assert_eq!(slot.state,State::Rejected);
        assert!(slot.retired());
    }
    #[test] fn original_transfer_failure_and_unwind_keep_both_originals_and_first_error_no_replay() {
        let cell=Cell::new(1,Instant::now()+std::time::Duration::from_secs(1));
        let result=cell.with(|a|{
            a.phase=Phase::Created;a.process.raw=44;a.process.state=State::Held;
            a.thread.raw=48;a.thread.state=State::Held;
            a.runner_object_phase=RunnerObjectPhase::Verified;a.runner_object_status=Some(0);
            a.calls[NativeCall::RunnerObjectInitialization as usize]=1;a.runner_before_len=20;
            a.r_token.state=State::Closed;
            a.pi.dwProcessId=4;a.pi.dwThreadId=8;
            a.transfer_to_owner(|p,pid,t,tid|{
                assert_eq!((p,t),(44,48));assert_eq!((pid,tid),(4,8));
                Err(io::Error::from_raw_os_error(123))
            })
        });
        assert_eq!(result.unwrap_err().raw_os_error(),Some(123));
        // Direct test-only inspection; no selector, original HANDLE API or close.
        let a=unsafe{&*cell.original.get()};
        assert_eq!(a.phase,Phase::Unknown);assert_eq!((a.process.raw,a.thread.raw),(44,48));
        assert_eq!(a.first.unwrap().win32,Some(123));assert!(!a.native_adopted());assert!(a.outstanding());
        assert!(cell.with(|a|a.transfer_to_owner(|_,_,_,_|panic!("replay"))).is_err());
        assert_eq!(unsafe{&*cell.original.get()}.first.unwrap().win32,Some(123));
        let cell=Cell::new(2,Instant::now()+std::time::Duration::from_secs(1));
        assert!(cell.with::<()>(|a|{a.token.raw=77;a.token.state=State::Held;panic!("scripted unwind")}).is_err());
        let a=unsafe{&*cell.original.get()};assert_eq!(a.token.raw,77);assert_eq!(a.phase,Phase::Unknown);
        assert_eq!(a.first.unwrap().stage,Stage::Unwind);
    }
    #[test] fn busy_cell_cannot_report_clean_and_unstarted_cannot_retire_or_fake_native_adoption() {
        let cell=Cell::new(3,Instant::now()+std::time::Duration::from_secs(1));
        cell.busy.store(true,Ordering::Release);
        assert_eq!(cell.with(|_|Ok(())).unwrap_err().kind(),io::ErrorKind::WouldBlock);
        cell.busy.store(false,Ordering::Release);
        let a=unsafe{&*cell.original.get()};assert_eq!(a.phase,Phase::Reserved);assert!(!a.native_adopted());
        assert!(cell.with(|a|a.retire_after_native()).is_err());
        let a=unsafe{&*cell.original.get()};assert_eq!(a.phase,Phase::Unknown);assert!(!a.native_adopted());
    }
}

#[cfg(test)]
mod caller_window_tests {
    use super::*;
    fn window()->CallerWindow {
        let user=Sid::from_bytes(&[1,1,0,0,0,0,0,5,21,0,0,0]).unwrap();
        let stats=Statistics{token:11,auth:22,modified:33,primary:true};
        CallerWindow{initial:stats,final_stats:stats,context:[7;32],user,integrity:0x3000,quota:0,assign:0,
            defaults:crate::asuser_a1_helper::TrustedDefaults{owner:user,
                logon:Sid::from_bytes(&[1,3,0,0,0,0,0,5,5,0,0,0,97,0,0,0,101,0,0,0]).unwrap(),acl:vec![2,0,8,0,0,0,0,0]}}
    }
    #[test] fn caller_sampling_windows_do_not_presume_modified_id_across_temporary_enable() {
        let before=window();let mut after=window();
        assert!(after.restored_after_api(&before));
        after.initial.modified=44;after.final_stats=after.initial;
        assert!(after.restored_after_api(&before));
        assert!(!after.before_api(&before)); // no selected API occurred between those windows
        after.final_stats.modified=45;assert!(!after.restored_after_api(&before));
    }
    #[test] fn actual_unrestored_policy_or_foreign_primary_token_never_becomes_normal_completion() {
        let before=window();let mut after=window();after.quota=2;
        assert!(!after.restored_after_api(&before));
        let mut after=window();after.context=[8;32];assert!(!after.restored_after_api(&before));
        let mut after=window();after.initial.token=99;after.final_stats=after.initial;
        assert!(!after.restored_after_api(&before));
        let mut after=window();after.initial.auth=99;after.final_stats=after.initial;
        assert!(!after.restored_after_api(&before));
    }
}

#[cfg(test)]
mod first_failure_projection_tests2659 {
    use super::*;
    use crate::{WindowsAsUserNativeStage as S,WindowsStartDiagnostic,
        WindowsStartError as E,WindowsStartStage};
    fn cell()->Cell {Cell::new(2659,Instant::now()+std::time::Duration::from_secs(1))}
    #[test]
    fn same_original_internal_denial_is_recorded_once_without_inventing_os_access_denied() {
        let cell=cell();let d=WindowsStartDiagnostic::default();let mut calls=0;
        d.mark(WindowsStartStage::AsUserAdmissionBind);
        let result=cell.with_observed::<()>(Some(&d),|a|{
            calls+=1;a.stage=Stage::Admission;a.site=Site::LoginInputGuard;Err(bad())
        });
        assert_eq!(calls,1);assert_eq!(result.unwrap_err().kind(),io::ErrorKind::PermissionDenied);
        let saved=d.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(saved.owner_stage,S::Admission);assert_eq!(saved.error,E::IoAccessDenied);
        assert_eq!(saved.site,Site::LoginInputGuard);
        assert_eq!((saved.win32,saved.ntstatus),(None,None));
        let a=unsafe{&*cell.original.get()};
        assert_eq!(a.phase,Phase::Unknown);assert_eq!(a.first.unwrap().win32,None);
        assert_eq!(a.first.unwrap().stage,Stage::Admission);assert!(!a.native_adopted());
    }
    #[test]
    fn original_raw_first_error_survives_later_error_and_ignored_error_return_paths() {
        let cell=cell();let d=WindowsStartDiagnostic::default();
        let error=cell.with_observed::<()>(Some(&d),|a|{
            a.stage=Stage::Create;a.site=Site::CreateProcessAsUserResult;a.fail(&io::Error::from_raw_os_error(1314));
            a.stage=Stage::Retirement;a.site=Site::CallerAfterCreate;Err(io::Error::from_raw_os_error(5))
        }).unwrap_err();
        assert_eq!(error.raw_os_error(),Some(1314));
        let saved=d.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(saved.owner_stage,S::Create);assert_eq!(saved.win32,Some(1314));
        assert_eq!(saved.site,Site::CreateProcessAsUserResult);
        assert_eq!(saved.ntstatus,None);
        // An ignored original error still cannot become success; no OS call here.
        assert_eq!(cell.with_observed(Some(&d),|_|Ok(())).unwrap_err().raw_os_error(),Some(1314));
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,Some(saved));
        assert_eq!(unsafe{&*cell.original.get()}.first.unwrap().stage,Stage::Create);
    }
    #[test]
    fn saved_signed_ntstatus_is_preserved_without_win32_substitution_or_native_query() {
        let cell=cell();let d=WindowsStartDiagnostic::default();let status=-1073741790i32;
        let error=cell.with_observed::<()>(Some(&d),|a|{
            a.stage=Stage::Token;a.site=Site::TargetFacts;
            a.object_replies[0]=Some(ObjectReply{status,returned:12,granted:None});
            Err(io::Error::other(NativeQueryFailure{status,returned:12}))
        }).unwrap_err();
        assert_eq!(error.raw_os_error(),None);
        let reply=error.get_ref().unwrap().downcast_ref::<NativeQueryFailure>().unwrap();
        assert_eq!((reply.status,reply.returned),(status,12));
        let saved=d.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(saved.owner_stage,S::Token);assert_eq!(saved.error,E::IoOther);
        assert_eq!((saved.win32,saved.ntstatus),(None,Some(status)));
    }
    #[test]
    fn busy_original_cell_does_not_call_callback_or_fabricate_first_failure() {
        let cell=cell();let d=WindowsStartDiagnostic::default();
        cell.busy.store(true,Ordering::Release);
        let result=cell.with_observed(Some(&d),|_|->io::Result<()> {panic!("borrow must reject before callback")});
        assert_eq!(result.unwrap_err().kind(),io::ErrorKind::WouldBlock);
        cell.busy.store(false,Ordering::Release);
        assert_eq!(d.snapshot().unwrap().current.asuser_native_failure,None);
        assert!(unsafe{&*cell.original.get()}.first.is_none());
        assert_eq!(unsafe{&*cell.original.get()}.phase,Phase::Reserved);
    }
    #[test]
    fn unwind_preserves_original_slots_and_first_stage_without_closing_or_replaying() {
        let cell=cell();let d=WindowsStartDiagnostic::default();
        let result=cell.with_observed::<()>(Some(&d),|a|{
            a.token.raw=77;a.token.state=State::Held;
            std::panic::resume_unwind(Box::new("synthetic projection unwind"));
        });
        assert_eq!(result.unwrap_err().kind(),io::ErrorKind::Other);
        let a=unsafe{&*cell.original.get()};
        assert_eq!(a.phase,Phase::Unknown);assert_eq!(a.token.raw,77);assert_eq!(a.token.state,State::Held);
        assert_eq!(a.first.unwrap().stage,Stage::Unwind);
        let saved=d.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(saved.owner_stage,S::Unwind);assert_eq!(saved.site,Site::Unwind);
        assert_eq!((saved.win32,saved.ntstatus),(None,None));assert!(!a.native_adopted());
    }
}

#[cfg(test)]
mod target_policy_observation_tests2740 {
    use super::*;
    use crate::{WindowsStartDiagnostic,WindowsTargetPolicyRejection as R};
    fn input()->(TargetPolicy,TargetFacts) {
        let sid=|v:&str|Sid::from_text(v).unwrap();
        let user=sid("S-1-5-21-1-2-3-1001");let caller=sid("S-1-5-21-1-2-3-1002");
        let stats=Statistics{token:1,auth:2,modified:3,primary:true};
        (TargetPolicy{managed_user:user,caller,session:1,caller_auth:4,
            caller_logon:sid("S-1-5-5-9-10"),caller_token:5,integrity_rid:0x2000},
         TargetFacts{initial:stats,final_stats:stats,granted:policy::TOKEN_REQUIRED,user,owner:user,
            context:[0;32],security_context:[0;32],change_notify:Some(3),logon:sid("S-1-5-5-11-12"),logon_attributes:0xc0000004,logon_count:1,
            session:1,integrity_rid:0x2000,elevated:false,elevation_type:1,restricted:false,
            app_container:false,ui_access:false,virtualization:false,mandatory_policy:1,
            privilege_count:1,unsafe_privileges:0,admin_membership:false})
    }
    #[test] fn same_original_guard_err_has_one_reason_no_raw_os_code_or_resource_operation() {
        let (policy,mut facts)=input();facts.virtualization=true;
        let cell=Cell::new(2740,Instant::now()+std::time::Duration::from_secs(1));
        let diagnostic=WindowsStartDiagnostic::default();
        let error=cell.with_observed::<()>(Some(&diagnostic),|a|{
            a.stage=Stage::Token;a.site=Site::TargetPolicyGuard;
            a.token.raw=77;a.token.state=State::Held;
            let error=a.validate_target_policy(&policy,&facts).unwrap_err();
            assert_eq!(error.to_string(),"A1 original security admission rejected");
            assert_eq!(error.raw_os_error(),None);Err(error)
        }).unwrap_err();
        assert_eq!(error.kind(),io::ErrorKind::PermissionDenied);
        assert_eq!(error.raw_os_error(),None);
        let observed=diagnostic.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(observed.target_policy_rejection,Some(R::Virtualization));
        assert_eq!((observed.win32,observed.ntstatus),(None,None));
        let a=unsafe{&*cell.original.get()};
        assert_eq!(a.phase,Phase::Unknown);assert_eq!(a.token.raw,77);
        assert_eq!(a.token.state,State::Held);assert_eq!(a.calls,[0;NATIVE_CALL_COUNT]);
        assert_eq!(a.first.unwrap().target_policy_rejection,Some(R::Virtualization));
        assert!(!a.native_adopted());
    }
    #[test] fn accepted_facts_do_not_manufacture_a_rejection_or_native_success() {
        let (policy,facts)=input();let mut a=Attempt::reserved(2740,Instant::now());
        assert!(a.validate_target_policy(&policy,&facts).is_ok());
        assert_eq!(a.target_policy_rejection,None);assert!(a.first.is_none());
        assert_eq!(a.phase,Phase::Reserved);assert_eq!(a.calls,[0;NATIVE_CALL_COUNT]);
        assert!(!a.native_adopted());
    }
    #[test] fn later_target_classification_cannot_relabel_an_earlier_first_failure() {
        let (policy,mut facts)=input();facts.admin_membership=true;
        let mut a=Attempt::reserved(2740,Instant::now());
        a.stage=Stage::Token;a.site=Site::TargetFacts;
        a.fail(&io::Error::from_raw_os_error(5));
        a.site=Site::TargetPolicyGuard;
        let error=a.validate_target_policy(&policy,&facts).unwrap_err();a.fail(&error);
        let diagnostic=WindowsStartDiagnostic::default();a.observe_first_failure(Some(&diagnostic));
        let first=diagnostic.snapshot().unwrap().current.asuser_native_failure.unwrap();
        assert_eq!(first.site,Site::TargetFacts);assert_eq!(first.win32,Some(5));
        assert_eq!(first.target_policy_rejection,None);assert_eq!(a.phase,Phase::Unknown);
    }
}

/// L0 real-OS contract test. Explicit opt-in on a disposable elevated Windows
/// runner only. It creates/deletes one fresh standard account and never changes
/// LSA rights, starts a business runner, or claims L1/L2/full-47 qualification.
#[cfg(test)]
mod login_launch_contract_windows {
    use super::*;
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NetUserAdd,NetUserDel,NetLocalGroupAddMembers,LOCALGROUP_MEMBERS_INFO_3,
        USER_INFO_1,USER_PRIV_USER,UF_SCRIPT,UF_DONT_EXPIRE_PASSWD};
    #[test]
    #[ignore = "disposable elevated Windows account fixture; run via structural acceptance"]
    fn fresh_standard_account_produces_minimal_launch_token() {
        assert_eq!(std::env::var("MORROW_DISPOSABLE_WINDOWS_TEST").as_deref(),Ok("1"));
        let suffix={use rand::{RngCore,SeedableRng};rand::rngs::SmallRng::from_entropy().next_u64()};
        let name=format!("ml0_{suffix:016x}");
        let password=format!("M0rrow!{suffix:016x}Aa9");
        let wide=|s:&str|s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let username=wide(&name);let secret=wide(&password);
        let info=USER_INFO_1{usri1_name:username.as_ptr() as _,usri1_password:secret.as_ptr() as _,
            usri1_password_age:0,usri1_priv:USER_PRIV_USER,usri1_home_dir:ptr::null_mut(),
            usri1_comment:ptr::null_mut(),usri1_flags:UF_SCRIPT|UF_DONT_EXPIRE_PASSWD,usri1_script_path:ptr::null_mut()};
        assert_eq!(unsafe{NetUserAdd(ptr::null(),1,(&info as *const USER_INFO_1).cast(),ptr::null_mut())},0);
        struct Account(Vec<u16>,bool);
        impl Drop for Account {fn drop(&mut self){if !self.1 {unsafe{NetUserDel(ptr::null(),self.0.as_ptr());}}}}
        let mut account=Account(username,false);
        let mut attempt=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(30));
        // This fixture borrows no production admission, owner, claim or origin.
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            // Match production's explicit built-in Users membership, including
            // localized Windows installations. Never grant an LSA privilege.
            let sid=Sid::from_text("S-1-5-32-545").unwrap();
            let mut group=[0u16;256];let mut domain=[0u16;256];
            let mut group_len=group.len() as u32;let mut domain_len=domain.len() as u32;let mut kind=0;
            assert_ne!(unsafe{windows_sys::Win32::Security::LookupAccountSidW(ptr::null(),
                sid.bytes().as_ptr().cast_mut().cast(),group.as_mut_ptr(),&mut group_len,
                domain.as_mut_ptr(),&mut domain_len,&mut kind)},0,"resolve built-in Users");
            assert_eq!(kind,windows_sys::Win32::Security::SidTypeAlias);
            assert!((group_len as usize)<group.len());group[group_len as usize]=0;
            let member=LOCALGROUP_MEMBERS_INFO_3{lgrmi3_domainandname:account.0.as_ptr() as _};
            let status=unsafe{NetLocalGroupAddMembers(ptr::null(),group.as_ptr(),3,
                (&member as *const LOCALGROUP_MEMBERS_INFO_3).cast(),1)};
            assert!(status==0||status==windows_sys::Win32::Foundation::ERROR_MEMBER_IN_ALIAS,
                "fixture Users membership failed: {status}");
            attempt.resolve_query().unwrap();
            attempt.r_token.begin().unwrap();
            let ok=unsafe{OpenProcessToken(GetCurrentProcess(),0x20008,&mut attempt.r_token.raw)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};attempt.r_token.capture(ok!=0,error).unwrap();
            let caller=attempt.target_facts().unwrap();attempt.r_token.close().unwrap();
            attempt.login_token.begin().unwrap();let domain=wide(".");
            let ok=unsafe{LogonUserW(account.0.as_ptr(),domain.as_ptr(),secret.as_ptr(),2,0,&mut attempt.login_token.raw)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};attempt.login_token.capture(ok!=0,error).unwrap();
            let source=attempt.target_facts().unwrap();
            let policy=TargetPolicy{managed_user:Sid::from_bytes(&crate::resolve_sid(&name).unwrap()).unwrap(),
                caller:caller.user,session:caller.session,caller_auth:caller.initial.auth,
                caller_logon:caller.logon,caller_token:caller.initial.token,integrity_rid:0x2000};
            assert_eq!(policy.source_rejection(&source),None,"source token policy");
            attempt.source_facts=Some(source.clone());
            attempt.derive_launch_token(&source,caller.user).unwrap();
            let launch=attempt.target_facts().unwrap();
            assert_eq!(policy.launch_rejection(&source,&launch),None,"derived token policy");
            attempt.capture_token_security(launch.integrity_rid,caller.integrity_rid)
                .expect("actual launch token object descriptor must satisfy admission");
            attempt.initialize_default(&policy,&launch).expect("launch default DACL initialization");
            assert_eq!(attempt.default_initialization.phase,DefaultPhase::Verified);
            assert_eq!(launch.unsafe_privileges,0);
            assert!(launch.privilege_count<=1);
            assert_eq!(launch.change_notify,source.change_notify);
            assert_eq!(attempt.calls[NativeCall::Create as usize],0); // no CreateProcessAsUser
            assert_eq!(attempt.calls[NativeCall::LaunchTokenDerivation as usize],1);
        }));
        for slot in [&mut attempt.r_token,&mut attempt.token,&mut attempt.restricted_token,&mut attempt.login_token] {
            slot.close().expect("fixture token close must be observed");
        }
        assert_eq!(unsafe{NetUserDel(ptr::null(),account.0.as_ptr())},0);
        account.1=true;
        if let Err(error)=result {std::panic::resume_unwind(error);}
    }
}

/// Opt-in OS representation regression. This creates only a current-user cmd
/// that exits immediately. It borrows no C28 admission, never logs on another
/// account or changes LSA, and cannot establish original runner qualification.
#[cfg(test)]
mod creation_descriptor_windows {
    use super::*;
    use windows_sys::Win32::System::Threading::{CreateProcessW,WaitForSingleObject,
        GetExitCodeProcess,GetExitCodeThread};

    #[test]
    #[ignore = "real current-user process fixture; explicit local Windows opt-in"]
    fn current_user_child_has_exact_original_process_and_thread_descriptors() {
        assert_eq!(std::env::var("MORROW_LOCAL_DESCRIPTOR_TEST").as_deref(),Ok("1"));
        let mut attempt=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(30));
        attempt.resolve_query().unwrap();attempt.r_token.begin().unwrap();
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),0x20008,&mut attempt.r_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        attempt.r_token.capture(ok!=0,error).unwrap();
        let facts=attempt.target_facts();attempt.r_token.close().unwrap();let facts=facts.unwrap();
        for (slot,thread) in [(&mut attempt.process_sd,false),(&mut attempt.thread_sd,true)] {
            Attempt::descriptor(slot,&policy::process_thread_sddl(facts.user,facts.logon,thread).unwrap()).unwrap();
        }
        let attrs=|slot:&Slot|SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor:slot.raw as *mut c_void,bInheritHandle:0};
        let process_sa=attrs(&attempt.process_sd);let thread_sa=attrs(&attempt.thread_sd);
        let system=NativeSystemDirectory::read().unwrap().into_root();
        let image=format!("{system}\\System32\\cmd.exe");
        let wide=|s:&str|s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let application=wide(&image);let mut command=wide(&format!("\"{image}\" /d /q /c exit 0"));
        let mut si:STARTUPINFOW=unsafe{mem::zeroed()};si.cb=mem::size_of::<STARTUPINFOW>() as u32;
        attempt.process.begin().unwrap();attempt.thread.begin().unwrap();
        let ok=unsafe{CreateProcessW(application.as_ptr(),command.as_mut_ptr(),&process_sa,&thread_sa,
            0,0x08000000,ptr::null(),ptr::null(),&si,&mut attempt.pi)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        attempt.process.raw=attempt.pi.hProcess;attempt.thread.raw=attempt.pi.hThread;
        let process=attempt.process.capture(ok!=0,error);let thread=attempt.thread.capture(ok!=0,error);
        if process.is_err()||thread.is_err() {
            if attempt.process.state==State::Rejected&&attempt.thread.state==State::Rejected {
                attempt.thread_sd.close().unwrap();attempt.process_sd.close().unwrap();
                panic!("fixture CreateProcess rejected: {error}");
            }
            retain("noncanonical creation outputs");
        }
        // Preserve the first assertion while still observing normal completion
        // of this disposable fixture through its original handles. No kill/PID
        // lookup or production Unknown retirement is introduced here.
        let validation=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            for thread in [false,true] {
                attempt.buffer.fill(0);let mut length=0;
                let raw=if thread{attempt.thread.raw}else{attempt.process.raw};
                assert_ne!(unsafe{GetKernelObjectSecurity(raw,0x15,attempt.buffer.as_mut_ptr().cast(),
                    BUFFER_BYTES as u32,&mut length)},0,"original descriptor read");
                assert!((20..=BUFFER_BYTES).contains(&(length as usize)));
                let observed=crate::asuser_a1_security::creation(attempt.bytes(length as usize),
                    facts.user,facts.logon,thread,0x2000).expect("exact original process/thread policy");
                assert_eq!(observed.integrity_rid,0x2000);
            }
        }));
        for raw in [attempt.process.raw,attempt.thread.raw] {
            if unsafe{WaitForSingleObject(raw,10_000)}!=0 {retain("original fixture did not signal");}
        }
        let mut process_code=0;let mut thread_code=0;
        if unsafe{GetExitCodeProcess(attempt.process.raw,&mut process_code)}==0
            ||unsafe{GetExitCodeThread(attempt.thread.raw,&mut thread_code)}==0 {retain("original exit query failed");}
        for slot in [&mut attempt.thread,&mut attempt.process,&mut attempt.thread_sd,&mut attempt.process_sd] {
            if slot.close().is_err(){retain("original fixture close failed");}
        }
        assert!(!attempt.outstanding());assert_eq!((process_code,thread_code),(0,0));
        if let Err(error)=validation{std::panic::resume_unwind(error);}
    }
    fn retain(reason:&str)->! {
        eprintln!("DESCRIPTOR_FIXTURE_UNKNOWN_NO_RETRY: {reason}");
        loop{std::thread::park_timeout(std::time::Duration::from_secs(1));}
    }
}

#[cfg(test)]
mod launch_object_tests {
    use super::*;
    #[test]
    fn intermediate_unknown_and_partial_final_output_keep_the_original_charge() {
        for partial_final in [false,true] {
            let mut a=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(2));
            a.restricted_token.begin().unwrap();
            a.restricted_token.raw=44;
            a.restricted_token.capture(true,0).unwrap();
            if partial_final {
                a.token.begin().unwrap();a.token.raw=48;
                assert!(a.token.capture(false,5).is_err());
            } else {a.restricted_token.state=State::InFlight;}
            a.fail(&io::Error::from_raw_os_error(5));
            a.close_rejected_login_handles();
            assert_eq!(a.calls[NativeCall::Retirement as usize],0);
            assert_eq!(a.restricted_token.raw,44);assert!(a.outcome.observation_lost);
            assert!(a.outstanding());assert_eq!(a.phase,Phase::Unknown);
            assert_eq!(a.calls[NativeCall::Create as usize],0);
        }
    }
    // Current-process read handle and two NEW token objects only. This does not
    // log on, mutate accounts/existing ACLs, impersonate or start any process.
    #[test]
    fn owned_creation_uses_production_descriptor_and_preserves_source_and_grant() {
        owned_creation(crate::asuser_a1_rights::TOKEN_INITIALIZATION);
    }
    #[test]
    fn owned_creation_preserves_full_logon_style_grant_without_gifting_it_to_runner() {
        // Newly duplicated fixture token only; the existing process token is
        // opened for QUERY|DUPLICATE|READ_CONTROL and never mutated.
        owned_creation(0x000f01ff);
    }
    fn owned_creation(requested:u32) {
        let mut a=Attempt::reserved(1,Instant::now()+std::time::Duration::from_secs(10));
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            a.resolve_query().unwrap();a.login_token.begin().unwrap();
            let ok=unsafe{OpenProcessToken(GetCurrentProcess(),0x2000a,&mut a.login_token.raw)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};a.login_token.capture(ok!=0,error).unwrap();
            let source=a.target_facts().unwrap();
            let before=crate::asuser_a1_helper::retained_subject(a.login_token.raw).unwrap();
            a.restricted_token.begin().unwrap();
            let ok=unsafe{DuplicateTokenEx(a.login_token.raw,requested,
                ptr::null(),SecurityImpersonation,TokenPrimary,&mut a.restricted_token.raw)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};a.restricted_token.capture(ok!=0,error).unwrap();
            let grant=|h| {
                let mut b=[0u32;14];let mut n=0;
                assert!(unsafe{a.object_query.unwrap()(h,0,b.as_mut_ptr().cast(),56,&mut n)}>=0);
                assert_eq!(n,56);b[1]
            };
            let expected_grant=grant(a.restricted_token.raw);
            a.create_launch_object(source.user,source.logon).unwrap();
            let final_facts=a.target_facts().unwrap();
            assert_eq!(final_facts.granted,expected_grant);
            assert_eq!(final_facts.context,source.context);
            assert_eq!(final_facts.security_context,source.security_context);
            assert_eq!(final_facts.owner,source.owner);assert_eq!(final_facts.user,source.user);
            assert_eq!(final_facts.initial.auth,source.initial.auth);
            assert_ne!(final_facts.initial.token,source.initial.token);
            a.capture_token_security(source.integrity_rid,source.integrity_rid).unwrap();
            let bytes=unsafe{std::slice::from_raw_parts(a.token_sd.as_ptr().cast::<u8>(),a.token_sd_len)};
            for (offset,mask) in [(0usize,0xe008bu32),(1,0x0000000a),(2,0x00020000)] {
                let mut changed=bytes.to_vec();
                let mut at=u32::from_le_bytes(changed[16..20].try_into().unwrap()) as usize+8;
                for _ in 0..offset{at+=u16::from_le_bytes(changed[at+2..at+4].try_into().unwrap()) as usize;}
                assert_eq!(u32::from_le_bytes(changed[at+4..at+8].try_into().unwrap()),mask);
                changed[at+4..at+8].copy_from_slice(&(mask|0x20).to_le_bytes());
                assert!(crate::asuser_a1_security::launch_token(&changed,source.user,source.logon,
                    source.integrity_rid,source.integrity_rid).is_err());
            }
            let after=crate::asuser_a1_helper::retained_subject(a.login_token.raw).unwrap();
            assert_eq!(before.hash,after.hash);assert_eq!(before.security_context,after.security_context);
            assert_eq!(a.calls[NativeCall::LaunchObjectCreation as usize],1);
            assert!(a.create_launch_object(source.user,source.logon).is_err());
            assert_eq!(a.calls[NativeCall::LaunchObjectCreation as usize],1);
            assert_eq!(a.calls[NativeCall::Create as usize],0);
            assert_eq!(a.calls[NativeCall::DefaultInitialization as usize],0);
        }));
        for slot in [&mut a.token,&mut a.restricted_token,&mut a.login_token] {
            slot.close().expect("checked fixture close on original owner");
        }
        assert!(a.slots().iter().all(|s|s.retired()));
        if let Err(error)=result{std::panic::resume_unwind(error);}
    }
}
