//! Source047 retained A1 native allocation core.
//! No Drop closes. This module is not a launch authority: it requires the
//! original native P->C inlet and same materializer/factory pre-login admission.
//! The default/v7/v8 spawn path is deliberately not redirected into this core.
use std::{cell::UnsafeCell,ffi::c_void,io,mem,ptr,sync::atomic::{AtomicBool,Ordering},time::Instant};
use windows_sys::Win32::Foundation::{CloseHandle,GetLastError,GetHandleInformation,HANDLE,LocalFree,LUID};
use windows_sys::Win32::Security::{LogonUserW,GetTokenInformation,SetTokenInformation,LookupPrivilegeValueW,
    GetKernelObjectSecurity,SECURITY_ATTRIBUTES,
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
    TransferInFlight,Transferred,Retiring,Rejected,Closed,Unknown}
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
#[derive(Clone,Copy)]
struct Failure {stage:Stage,kind:io::ErrorKind,win32:Option<i32>,ntstatus:Option<i32>}
#[derive(Clone,Copy)]
struct ObjectReply {status:i32,returned:u32,granted:Option<u32>}
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
    phase:Phase,first:Option<Failure>,stage:Stage,deadline:Instant,generation:u64,
    token:Slot,thread_token:Slot,r_token:Slot,caller_token:Slot,post_thread_token:Slot,process_sd:Slot,thread_sd:Slot,
    process:Slot,thread:Slot,pi:PROCESS_INFORMATION,
    calls:[u8;13], // thread-open, login, token-query, SD-query, AsUser,
    // close-boundary, NtQueryObject, LookupPrivilegeValue, original-R-token-open,
    // original-C-token-open, original-C-readonly-window, post-API-thread-token
    object_replies:[Option<ObjectReply>;3],
    buffer:Box<[u64;BUFFER_BYTES/8]>,
    token_sd:Box<[u64;BUFFER_BYTES/8]>,token_sd_len:usize,
    token_security:Option<crate::asuser_a1_security::TokenLayout>,
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
    origin_retirement_taken:bool,
}
impl Attempt {
    pub(crate) fn reserved(generation:u64,deadline:Instant)->Self {
        Self{phase:Phase::Reserved,first:None,stage:Stage::Admission,deadline,generation,
            token:Slot::new(false),thread_token:Slot::new(false),r_token:Slot::new(false),caller_token:Slot::new(false),post_thread_token:Slot::new(false),process_sd:Slot::new(true),thread_sd:Slot::new(true),
            process:Slot::new(false),thread:Slot::new(false),pi:unsafe{mem::zeroed()},
            calls:[0;13],object_replies:[None;3],buffer:Box::new([0;BUFFER_BYTES/8]),token_sd:Box::new([0;BUFFER_BYTES/8]),
            token_sd_len:0,token_security:None,default_acl:Box::new([0;4096]),default_acl_len:0,
            default_initialization:DefaultInitialization::new(),default_reads:0,
            default_creation:Box::new([0;512]),post_default:None,
            before_default_acl:Box::new([0;4096]),before_default_len:0,
            pipe_slots:std::array::from_fn(|_|Slot::new(false)),pipe_sds:std::array::from_fn(|_|Slot::new(true)),
            pipe_names:None,gui_closed:false,
            target_statistics:None,caller_windows:std::array::from_fn(|_|None),caller_privileges:None,
            object_query:None,logon:None,admission:None,broker:None,command:None,
            caller_attempt:Some(crate::CallerPrerequisiteAttempt::new()),origin_retirement_taken:false}
    }
    fn fail(&mut self,e:&io::Error) {
        if self.first.is_none(){self.first=Some(Failure{stage:self.stage,kind:e.kind(),win32:e.raw_os_error(),
            ntstatus:e.get_ref().and_then(|inner|inner.downcast_ref::<NativeQueryFailure>()).map(|reply|reply.status)});}
        self.phase=Phase::Unknown;
    }
    pub(crate) fn consume_caller(&mut self)->io::Result<AsUserCaller> {
        if self.phase!=Phase::Reserved||self.admission.is_some(){return Err(bad());}
        // This cell is reserved in the actual blocking C native-start thread.
        // The thread-affine 046 box is never moved from H's interactive thread
        // and never reruns H's separate diagnostic observation.
        let original=self.caller_attempt.as_mut().ok_or_else(bad)?;
        if !original.reserve(){return Err(bad());}original.observe_once();
        let original=self.caller_attempt.take().ok_or_else(bad)?;
        match original.consume_for_asuser(){Ok(value)=>Ok(value),Err(original)=>{
            self.caller_attempt=Some(original);Err(bad())}}
    }
    fn checkpoint(&mut self,stage:Stage)->io::Result<()> {
        self.stage=stage;
        if self.first.is_some(){return Err(bad());}
        if Instant::now()>=self.deadline {
            let e=io::Error::new(io::ErrorKind::TimedOut,"A1 original budget expired");self.fail(&e);return Err(e);
        }
        Ok(())
    }
    fn call(&mut self,index:usize,limit:u8,stage:Stage)->io::Result<()> {
        self.checkpoint(stage)?;
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
        self.call(2,100,Stage::Token)?;
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
        self.call(10,3,Stage::Token)?;
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
        self.call(9,1,Stage::Token)?;self.caller_token.begin()?;
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),8,&mut self.caller_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.caller_token.capture(ok!=0,error)?;
        let mut flags=0;
        if unsafe{GetHandleInformation(self.caller_token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        if flags&1!=0{return Err(bad());}
        let mut wanted=[0u64;2];
        for (i,name) in ["SeIncreaseQuotaPrivilege","SeAssignPrimaryTokenPrivilege"].iter().enumerate() {
            self.call(7,4,Stage::Token)?;
            let name:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();let mut luid:LUID=unsafe{mem::zeroed()};
            if unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut luid)}==0{return Err(io::Error::last_os_error());}
            wanted[i]=(u64::from(luid.HighPart as u32)<<32)|u64::from(luid.LowPart);
        }
        if wanted[0]==wanted[1]{return Err(bad());}self.caller_privileges=Some(wanted);
        self.caller_window(0,a)
    }
    fn caller_after_api(&mut self,a:&Admission)->io::Result<()> {
        self.call(11,1,Stage::ThreadToken)?;self.post_thread_token.begin()?;
        let ok=unsafe{OpenThreadToken(GetCurrentThread(),8,1,&mut self.post_thread_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        let captured=self.post_thread_token.capture(ok!=0,error);
        if !(ok==0&&error==1008&&self.post_thread_token.state==State::Rejected&&self.post_thread_token.raw==0) {
            captured?;return Err(bad());
        }
        self.caller_window(2,a)
    }
    fn login_inner(&mut self,a:&Admission,user:&[u16],password:&[u16])->io::Result<()> {
        self.checkpoint(Stage::Admission)?;
        if self.phase!=Phase::Reserved||self.generation!=a.generation||self.calls[1]!=0
            || user.len()<2||user.len()>257||password.is_empty()||password.len()>1025
            || user.last()!=Some(&0)||password.last()!=Some(&0)
            || user[..user.len()-1].contains(&0)||password[..password.len()-1].contains(&0) {return Err(bad());}
        if !user.iter().copied().eq(a.bundle.target_username().encode_utf16().chain(Some(0))) {return Err(bad());}
        let (pid,creation)=Self::current_identity()?;
        a.caller.require_original(pid,creation)?;
        a.bundle.verify_originals(pid,creation)?;
        self.resolve_query()?;
        self.call(0,1,Stage::ThreadToken)?;self.thread_token.begin()?;
        let ok=unsafe{OpenThreadToken(GetCurrentThread(),8,1,&mut self.thread_token.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        let captured=self.thread_token.capture(ok!=0,error);
        if !(ok==0&&error==1008&&self.thread_token.state==State::Rejected&&self.thread_token.raw==0) {
            captured?;return Err(bad());
        }
        self.checkpoint(Stage::ThreadToken)?;
        self.prepare_caller(a)?;
        self.call(1,1,Stage::Login)?;self.token.begin()?;self.phase=Phase::LoginInFlight;
        // Exactly one INTERACTIVE/DEFAULT login; original output is preinstalled.
        let domain=[b'.' as u16,0];
        let ok=unsafe{LogonUserW(user.as_ptr(),domain.as_ptr(),password.as_ptr(),2,0,&mut self.token.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        self.token.capture(ok!=0,error)?;
        self.phase=Phase::TokenHeld;self.checkpoint(Stage::Login)?;
        let mut flags=0;
        if unsafe{GetHandleInformation(self.token.raw,&mut flags)}==0{return Err(io::Error::last_os_error());}
        if flags&1!=0{return Err(bad());} // no repair/SetHandleInformation on T
        let facts=self.target_facts()?;
        a.target.validate(&facts)?;
        self.capture_token_security(facts.integrity_rid,a.caller.integrity_rid()?)?;
        self.initialize_default(a,&facts)?;
        self.logon=Some(facts.logon);self.phase=Phase::TokenValidated;Ok(())
    }
    fn verify_target_before_create(&mut self)->io::Result<()> {
        let observed=self.statistics()?;
        if self.target_statistics!=Some(observed){return Err(bad());}
        self.call(3,4,Stage::TokenSecurity)?;
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
    fn initialize_default(&mut self,a:&Admission,before:&TargetFacts)->io::Result<()> {
        let original=self.observe_default()?;
        self.before_default_acl[..original.len()].copy_from_slice(&original);self.before_default_len=original.len();
        if self.statistics()?!=before.final_stats{return Err(bad());}
        let safe=super::asuser_a1_security::default_objects(&original,a.target.caller,before.logon).is_ok();
        if self.default_initialization.choose(safe,before.granted)? {
            let template=super::asuser_a1_security::default_template(a.target.caller,before.logon)?;
            let bytes=unsafe{std::slice::from_raw_parts_mut(self.default_creation.as_mut_ptr().cast::<u8>(),4096)};
            bytes[..template.len()].copy_from_slice(&template);
            let header=TOKEN_DEFAULT_DACL{DefaultDacl:self.default_creation.as_mut_ptr().cast()};
            self.call(12,1,Stage::TokenSecurity)?;
            let ok=unsafe{SetTokenInformation(self.token.raw,TokenDefaultDacl,
                (&header as *const TOKEN_DEFAULT_DACL).cast(),mem::size_of::<TOKEN_DEFAULT_DACL>() as u32)};
            let error=if ok==0{unsafe{GetLastError()}}else{0};self.default_initialization.returned(ok!=0,error)?;
        }
        self.checkpoint(Stage::TokenSecurity)?;
        let after=self.target_facts()?;a.target.validate(&after)?;
        let actual=self.observe_default()?;
        super::asuser_a1_security::default_objects(&actual,a.target.caller,before.logon)?;
        let actual_success=self.calls[12]==1&&self.default_initialization.phase==DefaultPhase::SetSucceeded
            &&self.default_initialization.status==Some(0);
        if self.statistics()?!=after.final_stats||!after.same_after_default_initialization(before,actual_success){return Err(bad());}
        self.default_acl[..actual.len()].copy_from_slice(&actual);self.default_acl_len=actual.len();
        self.target_statistics=Some(after.final_stats);self.post_default=Some(after);
        self.default_initialization.verified()
    }
    fn query(&mut self,class:i32)->io::Result<usize> {
        self.call(2,100,Stage::Token)?;self.buffer.fill(0);
        let cap=if matches!(class,TokenElevation|TokenSessionId|TokenElevationType|TokenHasRestrictions|
            TokenIsAppContainer|TokenUIAccess|TokenVirtualizationEnabled|TokenMandatoryPolicy){4}else{BUFFER_BYTES};
        let mut returned=0;
        let raw=if self.r_token.state==State::Held{self.r_token.raw}else{self.token.raw};
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
        self.call(6,3,Stage::Grant)?;
        let mut basic=[0u32;14];let mut returned=0;
        let raw=if self.r_token.state==State::Held{self.r_token.raw}else{self.token.raw};
        let status=unsafe{self.object_query.ok_or_else(bad)?(raw,0,basic.as_mut_ptr().cast(),56,&mut returned)};
        self.object_replies[usize::from(self.calls[6]-1)]=Some(ObjectReply{status,returned,
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
        self.call(7,5,Stage::Token)?;
        if unsafe{LookupPrivilegeValueW(ptr::null(),name.as_ptr(),&mut allowed)}==0{return Err(io::Error::last_os_error());}
        let len=self.query(TokenPrivileges)?;if len<4{return Err(bad());}
        let count=u32::from_le_bytes(self.bytes(4).try_into().unwrap()) as usize;
        let offset=mem::offset_of!(TOKEN_PRIVILEGES,Privileges);let stride=mem::size_of::<LUID_AND_ATTRIBUTES>();
        if count>64||offset.checked_add(count*stride).is_none_or(|v|v>len){return Err(bad());}
        let mut unsafe_privileges=0;
        for i in 0..count {
            let p=unsafe{ptr::read(self.bytes(len).as_ptr().add(offset+i*stride).cast::<LUID_AND_ATTRIBUTES>())};
            // A disabled powerful privilege is still present and potentially
            // enableable by the target; do not count only its enabled bit.
            if p.Luid.LowPart!=allowed.LowPart||p.Luid.HighPart!=allowed.HighPart||p.Attributes&4!=0 {unsafe_privileges+=1;}
        }
        let len=self.query(TokenOwner)?;if len<mem::size_of::<TOKEN_OWNER>(){return Err(bad());}
        let owner=unsafe{ptr::read(self.buffer.as_ptr().cast::<TOKEN_OWNER>())};
        let owner=self.sid(len,owner.Owner,mem::size_of::<TOKEN_OWNER>())?;
        let context=crate::asuser_a1_helper::retained_subject(raw)?.hash;
        let final_stats=self.statistics()?;
        Ok(TargetFacts{initial,final_stats,granted,user,owner,context,logon,logon_attributes,logon_count,
            session,integrity_rid,elevated,elevation_type,restricted,app_container,ui_access,
            virtualization,mandatory_policy,privilege_count:count as u16,unsafe_privileges,admin_membership})
    }
    fn capture_token_security(&mut self,minimum_integrity:u32,caller_integrity:u32)->io::Result<()> {
        self.call(3,3,Stage::TokenSecurity)?;
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
        let layout=super::asuser_a1_security::token_layout(bytes)?;
        if layout.integrity_rid<minimum_integrity||layout.integrity_rid>caller_integrity{return Err(bad());}
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
        self.checkpoint(Stage::Create)?;
        if self.phase!=Phase::TokenValidated||self.calls[4]!=0||self.generation!=a.generation{return Err(bad());}
        let logon=self.logon.ok_or_else(bad)?;
        self.checkpoint(Stage::ProcessSd)?;
        Self::descriptor(&mut self.process_sd,&policy::process_thread_sddl(a.target.caller,logon,false)?)?;
        self.checkpoint(Stage::ThreadSd)?;
        Self::descriptor(&mut self.thread_sd,&policy::process_thread_sddl(a.target.caller,logon,true)?)?;
        let attrs=|slot:&Slot|SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor:slot.raw as *mut c_void,bInheritHandle:0};
        let process_sa=attrs(&self.process_sd);let thread_sa=attrs(&self.thread_sd);
        self.caller_window(1,a)?;
        let command=self.command.as_mut().ok_or_else(bad)?;
        let (application,cmd,environment,cwd,desktop)=(command.application.as_ptr(),command.command.as_mut_ptr(),
            command.environment.as_mut_ptr(),command.cwd.as_ptr(),command.desktop.as_mut_ptr());
        let mut si:STARTUPINFOW=unsafe{mem::zeroed()};si.cb=mem::size_of::<STARTUPINFOW>() as u32;
        si.lpDesktop=desktop;
        self.call(4,1,Stage::Create)?;self.phase=Phase::CreateInFlight;
        self.process.begin()?;self.thread.begin()?;
        // No inheritance: R uses the existing by-name authenticated pipe
        // transport. Token, SD, keys, parent/control and GUI aliases are absent
        // from the runner's handle table. No profile, suspended start or repair.
        let ok=unsafe{CreateProcessAsUserW(self.token.raw,application,cmd,
            &process_sa,&thread_sa,0,0x08000000|0x00000400,environment,cwd,&si,&mut self.pi)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        // Capture BOTH raw outputs before any fallible operation, even rejection.
        self.process.raw=self.pi.hProcess;self.thread.raw=self.pi.hThread;
        let process=self.process.capture(ok!=0,error);let thread=self.thread.capture(ok!=0,error);
        if ok==0&&self.process.state==State::Rejected&&self.thread.state==State::Rejected {
            self.phase=Phase::Rejected;return Err(io::Error::from_raw_os_error(error as i32));
        }
        process?;thread?;
        if self.pi.dwProcessId==0||self.pi.dwThreadId==0||self.process.raw==self.thread.raw{return Err(bad());}
        self.phase=Phase::Created;self.checkpoint(Stage::Create)?;
        self.caller_after_api(a)
    }
    fn readback(&mut self,a:&Admission)->io::Result<()> {
        if self.phase!=Phase::Created{return Err(bad());}
        for thread in [false,true] {
            self.call(3,4,if thread{Stage::ThreadReadback}else{Stage::ProcessReadback})?;
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
        self.call(8,1,Stage::Token)?;
        self.r_token.begin()?;
        let ok=unsafe{OpenProcessToken(self.process.raw,0x00020008,&mut self.r_token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.r_token.capture(ok!=0,error)?;
        let actual_runner=self.target_facts()?;
        a.target.validate_runner(&actual_runner,self.logon.ok_or_else(bad)?,self.target_statistics.ok_or_else(bad)?.auth)?;
        let baseline=self.post_default.as_ref().ok_or_else(bad)?;
        if actual_runner.owner!=baseline.owner||actual_runner.context!=baseline.context{return Err(bad());}
        self.default_objects(a.target.caller,actual_runner.logon,true)?;
        if self.statistics()?!=actual_runner.final_stats{return Err(bad());}
        self.r_token.close()?;self.checkpoint(Stage::Token)?;
        Ok(())
    }
    pub(crate) fn outstanding(&self)->bool {
        ![&self.token,&self.thread_token,&self.r_token,&self.caller_token,&self.post_thread_token,
            &self.process_sd,&self.thread_sd,&self.process,&self.thread,&self.pipe_slots[0],&self.pipe_slots[1],
            &self.pipe_sds[0],&self.pipe_sds[1]].iter().all(|s|s.retired())
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
            ||!matches!(self.thread.state,State::Transferred)||self.calls[5]!=0||!self.gui_closed {return Err(bad());}
        // Creation budget is not reused after arbitrary business lifetime. This
        // is one finite normal-close budget, never a retry/new launch budget.
        self.deadline=Instant::now().checked_add(std::time::Duration::from_secs(1)).ok_or_else(bad)?;
        self.checkpoint(Stage::Retirement)?;
        self.calls[5]=1;
        self.phase=Phase::Retiring;
        for slot in &mut self.pipe_sds{slot.close()?;}
        self.checkpoint(Stage::Retirement)?;
        self.thread_sd.close()?;self.checkpoint(Stage::Retirement)?;
        self.process_sd.close()?;self.checkpoint(Stage::Retirement)?;
        self.thread_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.post_thread_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.caller_token.close()?;self.checkpoint(Stage::Retirement)?;
        self.token.close()?;self.checkpoint(Stage::Retirement)?;
        if self.outstanding(){return Err(bad());}self.phase=Phase::Closed;Ok(())
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
        if self.phase!=Phase::Reserved||self.pipe_names.is_some()||input==output||input.len()>160||output.len()>160
            ||![&input,&output].iter().all(|v|v.starts_with(r"\\.\pipe\")){return Err(bad());}
        self.pipe_names=Some((input,output));Ok(())
    }
    pub(crate) fn start(&mut self,command:Command,user:&[u16],password:&[u16],
        stage:impl FnOnce(&std::fs::File,&std::fs::File)->io::Result<()>)->io::Result<()> {
        let result=self.start_inner(command,user,password,stage);if let Err(error)=&result{self.fail(error);}result
    }
    fn start_inner(&mut self,command:Command,user:&[u16],password:&[u16],
        stage:impl FnOnce(&std::fs::File,&std::fs::File)->io::Result<()>)->io::Result<()> {
        if self.command.is_some(){return Err(bad());}
        // All UTF-16/env/CWD locks remain on this SAME original owner through
        // native rejection, cancellation, transfer failure, deadline or unwind.
        self.command=Some(command);
        let admission=self.admission.as_ref().cloned().ok_or_else(bad)?;
        self.login_inner(&admission,user,password)?;
        self.create_transport_pipes(&admission)?;
        {use std::os::windows::io::FromRawHandle;
            let write=mem::ManuallyDrop::new(unsafe{std::fs::File::from_raw_handle(self.pipe_slots[0].raw as _)});
            let read=mem::ManuallyDrop::new(unsafe{std::fs::File::from_raw_handle(self.pipe_slots[1].raw as _)});
            stage(&write,&read)?;}
        self.checkpoint(Stage::Broker)?;
        let command=self.command.as_mut().ok_or_else(bad)?;
        let desktop=self.broker.as_mut().ok_or_else(bad)?.start(self.logon.ok_or_else(bad)?,&mut command.environment)?;
        command.desktop=desktop.encode_utf16().chain(Some(0)).collect();
        self.checkpoint(Stage::Broker)?;
        self.verify_target_before_create()?;
        self.create_inner(&admission)?;
        self.readback(&admission)
    }
    fn create_transport_pipes(&mut self,a:&Admission)->io::Result<()> {
        let names=self.pipe_names.as_ref().ok_or_else(bad)?.clone();let logon=self.logon.ok_or_else(bad)?;
        for (index,name) in [names.0,names.1].into_iter().enumerate(){
            let text=format!("D:P(A;;GRGWRC;;;{})(A;;GRGW;;;{})(A;;RC;;;OW)",a.target.caller.text(),logon.text());
            Self::descriptor(&mut self.pipe_sds[index],&text)?;
            let sa=SECURITY_ATTRIBUTES{nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor:self.pipe_sds[index].raw as _,bInheritHandle:0};
            let name=name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            self.pipe_slots[index].begin()?;
            let raw=unsafe{windows_sys::Win32::System::Pipes::CreateNamedPipeW(name.as_ptr(),
                if index==0{2|0x80000}else{1|0x80000},8,1,65536,65536,0,&sa)};
            let error=if raw==0||raw == -1{unsafe{GetLastError()}}else{0};
            self.pipe_slots[index].raw=raw;self.pipe_slots[index].capture(raw!=0&&raw != -1,error)?;
            self.checkpoint(Stage::TokenSecurity)?;
        }Ok(())
    }
    pub(crate) fn transport_files(&mut self)->io::Result<(std::fs::File,std::fs::File)> {
        use std::os::windows::io::FromRawHandle;
        if self.phase!=Phase::Created||self.pipe_slots.iter().any(|v|v.state!=State::Held){return Err(bad());}
        let raw=[self.pipe_slots[0].raw,self.pipe_slots[1].raw];
        for slot in &mut self.pipe_slots{slot.raw=0;slot.state=State::Transferred;}
        Ok(unsafe{(std::fs::File::from_raw_handle(raw[0] as _),std::fs::File::from_raw_handle(raw[1] as _))})
    }
    pub(crate) fn desktop_name(&self)->io::Result<String> {
        if self.phase!=Phase::Created{return Err(bad());}
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
        if self.phase!=Phase::Created||self.process.state!=State::Held||self.thread.state!=State::Held{return Err(bad());}
        self.phase=Phase::TransferInFlight;
        install(self.process.raw,self.pi.dwProcessId,self.thread.raw,self.pi.dwThreadId)?;
        self.process.raw=0;self.thread.raw=0;
        self.process.state=State::Transferred;self.thread.state=State::Transferred;
        self.phase=Phase::Transferred;self.checkpoint(Stage::Transfer)
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
                if let Some(first)=original.first {
                    if let Some(code)=first.win32{return Err(io::Error::from_raw_os_error(code));}
                    if let Some(status)=first.ntstatus{return Err(io::Error::other(NativeQueryFailure{status,
                        returned:original.object_replies.iter().flatten().find(|v|v.status==status).map_or(0,|v|v.returned)}));}
                    return Err(io::Error::new(first.kind,"A1 original first failure retained"));
                }Err(error)
            },
            Err(_)=>{original.stage=Stage::Unwind;let error=io::Error::other("A1 original native unwind");
                original.fail(&error);Err(error)}
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
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
