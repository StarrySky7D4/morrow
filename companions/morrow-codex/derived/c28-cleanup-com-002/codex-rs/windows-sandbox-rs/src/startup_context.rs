//! Bounded, same-original startup queries. Never changes an ACL or execution identity.
//! Raw query handles live only in the invocation's preinstalled desktop resource.
//! No Drop close, PID reopen, impersonation, new logon, wait, worker or replay.
use super::{
    ContextFact as F, ContextUnavailable as U, QueryHandleClose, StartupAccessResult,
    StartupObjectObservation, StartupTokenRelations, WindowsStartupContext,
    ContextApiStage as Stage, ContextFailure, PrimaryTokenReadResults,
    TokenAllocationObservation, TokenOpenObservation, TokenOpenPurpose, TokenOutputAllocation,
    AuthzAllocation as A, AuthzRetirement as R, AuthzResourceObservation,
    AuthzDaclProjection, AuthzObjectObservation, AuthzObjectState as O, AuthzObjectMapping as M,
};
use std::{io, mem, ptr};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, ERROR_INSUFFICIENT_BUFFER};
use windows_sys::Win32::Security::{
    AccessCheck, DuplicateToken, GetTokenInformation, GetUserObjectSecurity, IsValidAcl, IsValidSecurityDescriptor,
    IsValidSid, SecurityIdentification, TokenGroups, TokenHasRestrictions, TokenIsAppContainer,
    TokenSessionId, TokenUser, GENERIC_MAPPING, PRIVILEGE_SET, SID_AND_ATTRIBUTES, TOKEN_GROUPS,
    TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_DUPLICATE, TOKEN_USER,
};
use windows_sys::Win32::System::Threading::OpenProcessToken;
use windows_sys::Win32::Security::Authorization::{
    AuthzInitializeResourceManager, AuthzInitializeContextFromToken, AuthzAccessCheck,
    AuthzFreeContext, AuthzFreeResourceManager, AUTHZ_ACCESS_REQUEST, AUTHZ_ACCESS_REPLY,
    AUTHZ_RM_FLAG_NO_AUDIT, AUTHZ_RM_FLAG_NO_CENTRAL_ACCESS_POLICIES,
};
use windows_sys::Win32::System::Threading::{WaitForSingleObject,GetExitCodeProcess};
use windows_sys::Win32::System::StationsAndDesktops::{
    GetUserObjectInformationW, UOI_NAME, UOI_FLAGS, USEROBJECTFLAGS,
};

const TOKEN_CAP: usize = 65536;
const SECURITY_CAP: usize = 65536;
const NAME_CAP_UNITS: usize = 512;
const GROUP_CAP: usize = 256;
const PRIVILEGE_CAP: usize = 4096;
const MAXIMUM_ALLOWED_MASK: u32 = 0x02000000;
const LOGON_GROUP: u32 = 0xc0000000;
const READ_CONTROL_MASK: u32 = 0x20000;
const STANDARD_REQUIRED: u32 = 0xf0000;

/// Only borrowed from the same anchored original. Zero wait, no PID reopen;
/// full DWORD is read only after signal (including genuine exit259).
pub(crate) fn sample_runner(process:HANDLE,startup_id:u64,pid:u32,creation:u64)
    ->crate::WindowsStartRunnerObservation {
    use crate::WindowsStartRunnerState as S;
    let wait=unsafe{WaitForSingleObject(process,0)};
    let state=match wait {
        0=>{
            let mut raw_dword=0;
            if unsafe{GetExitCodeProcess(process,&mut raw_dword)}!=0 {S::Exited{raw_dword}}
            else {S::SignaledExitQueryFailed{os_class:crate::WindowsOsError::from_code(unsafe{GetLastError()})}}
        }
        258=>S::RunningAtObservation,
        u32::MAX=>S::WaitFailed{os_class:crate::WindowsOsError::from_code(unsafe{GetLastError()})},
        wait_result=>S::UnexpectedWait{wait_result},
    };
    crate::WindowsStartRunnerObservation {startup_id,runner_pid:pid,
        runner_creation_filetime_100ns:creation,state}
}

/// No Debug: private names/SIDs/descriptors never enter a public formatter.
pub(crate) struct Buffer { words: Vec<u64>, len: usize }
impl Buffer {
    fn new(bytes: usize, cap: usize) -> Result<Self, U> {
        if bytes == 0 || bytes > cap { return Err(U::BoundExceeded); }
        Ok(Self { words: vec![0; bytes.div_ceil(8)], len: bytes })
    }
    fn raw(&mut self) -> *mut std::ffi::c_void { self.words.as_mut_ptr().cast() }
    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.words.as_ptr().cast(), self.len) }
    }
    fn bytes_mut(&mut self)->&mut [u8] {
        unsafe{std::slice::from_raw_parts_mut(self.words.as_mut_ptr().cast(),self.len)}
    }
}
fn last() -> U { U::Win32 { code: unsafe { GetLastError() } } }
fn fact<T>(r: Result<T, U>) -> F<T> {
    match r { Ok(value) => F::Observed(value), Err(error) => F::NotObserved(error) }
}
fn upstream(stage:Stage,error:U)->U {
    let reason=match error {
        U::Upstream{..}=>return error,
        U::NotAttempted=>ContextFailure::NotAttempted,
        U::NotApplicable=>ContextFailure::NotApplicable,
        U::OriginalUnavailable=>ContextFailure::OriginalUnavailable,
        U::Busy=>ContextFailure::Busy,
        U::Poisoned=>ContextFailure::Poisoned,
        U::BoundExceeded=>ContextFailure::BoundExceeded,
        U::InvalidBuffer=>ContextFailure::InvalidBuffer,
        U::Win32{code}=>ContextFailure::Win32{code},
    };
    U::Upstream{stage,reason}
}
fn compare(a: &Result<Vec<u8>, U>, a_stage:Stage, b: &Result<Vec<u8>, U>, b_stage:Stage) -> F<bool> {
    match (a, b) {
        (Ok(a), Ok(b)) => F::Observed(a == b),
        (Err(e), _) => F::NotObserved(upstream(a_stage,*e)),
        (_, Err(e)) => F::NotObserved(upstream(b_stage,*e)),
    }
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, U> {
    let end=offset.checked_add(4).ok_or(U::InvalidBuffer)?;
    Ok(u32::from_le_bytes(bytes.get(offset..end).ok_or(U::InvalidBuffer)?
        .try_into().map_err(|_| U::InvalidBuffer)?))
}
fn sid_at(bytes: &[u8], offset: usize) -> Result<Vec<u8>, U> {
    if offset % 4 != 0 { return Err(U::InvalidBuffer); }
    let head=bytes.get(offset..offset.checked_add(8).ok_or(U::InvalidBuffer)?).ok_or(U::InvalidBuffer)?;
    if head[0] != 1 || head[1] > 15 { return Err(U::InvalidBuffer); }
    let len=8 + head[1] as usize * 4;
    let sid=bytes.get(offset..offset.checked_add(len).ok_or(U::InvalidBuffer)?).ok_or(U::InvalidBuffer)?;
    let mut aligned=[0u32;17];
    unsafe { ptr::copy_nonoverlapping(sid.as_ptr(),aligned.as_mut_ptr().cast(),len); }
    if unsafe { IsValidSid(aligned.as_mut_ptr().cast()) } == 0 { return Err(U::InvalidBuffer); }
    Ok(sid.to_vec())
}
fn pointed_sid(buffer: &Buffer, sid: *mut std::ffi::c_void) -> Result<Vec<u8>, U> {
    let base=buffer.bytes().as_ptr() as usize;
    let offset=(sid as usize).checked_sub(base).ok_or(U::InvalidBuffer)?;
    sid_at(buffer.bytes(), offset)
}
fn token_buffer(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Buffer, U> {
    let mut needed=0;
    let ok=unsafe { GetTokenInformation(token, class, ptr::null_mut(), 0, &mut needed) };
    if ok != 0 { return Err(U::InvalidBuffer); }
    let error=last();
    if error != (U::Win32 { code: ERROR_INSUFFICIENT_BUFFER }) { return Err(error); }
    let mut buffer=Buffer::new(needed as usize, TOKEN_CAP)?;
    let mut returned=0;
    if unsafe { GetTokenInformation(token, class, buffer.raw(), needed, &mut returned) } == 0 {
        return Err(last()); // No growth loop/retry if the original size changed.
    }
    if returned == 0 || returned > needed { return Err(U::InvalidBuffer); }
    buffer.len=returned as usize;
    Ok(buffer)
}
fn token_scalar(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<u32, U> {
    let b=token_buffer(token, class)?;
    if b.len != 4 { return Err(U::InvalidBuffer); }
    u32_at(b.bytes(), 0)
}
fn token_restrictions_flag(token:HANDLE)->Result<bool,U> {
    let returned=token_buffer(token,TokenHasRestrictions)?;
    // Buffer::bytes is bounded by the API's validated returned length, not
    // its allocation size. Do not read/zero-extend unreturned DWORD padding.
    parse_restrictions_flag(TokenHasRestrictions,returned.bytes())
}
fn parse_restrictions_flag(class:TOKEN_INFORMATION_CLASS,returned:&[u8])->Result<bool,U> {
    // Class-specific compatibility: observed one-byte form plus documented
    // DWORD, both zero/nonzero. Never broaden SessionId/AppContainer/UOI.
    // https://learn.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-token_information_class
    if class!=TokenHasRestrictions {return Err(U::InvalidBuffer);}
    match returned.len() {
        1=>Ok(returned[0]!=0),
        4=>u32_at(returned,0).map(|value|value!=0),
        _=>Err(U::InvalidBuffer),
    }
}
fn token_user(token: HANDLE) -> Result<Vec<u8>, U> {
    let b=token_buffer(token, TokenUser)?;
    if b.len < mem::size_of::<TOKEN_USER>() { return Err(U::InvalidBuffer); }
    let value=unsafe { ptr::read(b.words.as_ptr().cast::<TOKEN_USER>()) };
    if (value.User.Sid as usize).checked_sub(b.bytes().as_ptr() as usize)
        .ok_or(U::InvalidBuffer)? < mem::size_of::<TOKEN_USER>() {return Err(U::InvalidBuffer);}
    pointed_sid(&b, value.User.Sid)
}
fn token_logon(token: HANDLE) -> Result<(Vec<u8>, u32), U> {
    let b=token_buffer(token, TokenGroups)?;
    let count=u32_at(b.bytes(),0)? as usize;
    let offset=mem::offset_of!(TOKEN_GROUPS, Groups);
    let array_end=group_array_end(count,b.len)?;
    let mut result=None;
    for index in 0..count {
        let entry=unsafe { ptr::read(b.bytes().as_ptr().add(offset + index * mem::size_of::<SID_AND_ATTRIBUTES>())
            .cast::<SID_AND_ATTRIBUTES>()) };
        // Check every returned pointer/SID before considering group attributes.
        if (entry.Sid as usize).checked_sub(b.bytes().as_ptr() as usize)
            .ok_or(U::InvalidBuffer)? < array_end {return Err(U::InvalidBuffer);}
        let sid=pointed_sid(&b,entry.Sid)?;
        if entry.Attributes & LOGON_GROUP == LOGON_GROUP {
            if result.is_some() { return Err(U::InvalidBuffer); }
            result=Some((sid,entry.Attributes));
        }
    }
    result.ok_or(U::OriginalUnavailable)
}
fn group_array_end(count:usize,bytes:usize)->Result<usize,U> {
    if count>GROUP_CAP {return Err(U::BoundExceeded);}
    let end=mem::offset_of!(TOKEN_GROUPS,Groups).checked_add(
        count.checked_mul(mem::size_of::<SID_AND_ATTRIBUTES>()).ok_or(U::InvalidBuffer)?)
        .ok_or(U::InvalidBuffer)?;
    if end>bytes {return Err(U::InvalidBuffer);}
    Ok(end)
}
pub(crate) struct PrivateTokenFacts {
    user: Result<Vec<u8>, U>,
    logon: Result<(Vec<u8>,u32), U>,
    session: Result<u32,U>,
    restricted: Result<bool,U>,
    app_container: Result<bool,U>,
}
impl PrivateTokenFacts {
    fn unavailable(reason: U)->Self { Self {user:Err(reason),logon:Err(reason),session:Err(reason),
        restricted:Err(reason),app_container:Err(reason)} }
    pub(crate) fn read(token: HANDLE)->Self { Self {
        user:token_user(token),logon:token_logon(token),session:token_scalar(token,TokenSessionId),
        restricted:token_restrictions_flag(token),
        app_container:token_scalar(token,TokenIsAppContainer).and_then(bool_scalar),
    }}
    fn read_results(&self)->PrimaryTokenReadResults {
        fn result<T>(value:&Result<T,U>)->F<()> {fact(value.as_ref().map(|_|()).map_err(|e|*e))}
        PrimaryTokenReadResults {user:result(&self.user),logon_groups:result(&self.logon),
            session:result(&self.session),restrictions:result(&self.restricted),
            app_container:result(&self.app_container)}
    }
    fn first_unavailable(&self)->Option<U> {
        for (stage,error) in [
            (Stage::QueryUser,self.user.as_ref().err()),
            (Stage::QueryLogonGroups,self.logon.as_ref().err()),
            (Stage::QuerySession,self.session.as_ref().err()),
            (Stage::QueryRestrictions,self.restricted.as_ref().err()),
            (Stage::QueryAppContainer,self.app_container.as_ref().err()),
        ] {
            if let Some(error)=error {return Some(upstream(stage,*error));}
        }
        None
    }
}
fn bool_scalar(value:u32)->Result<bool,U> {
    match value {0=>Ok(false),1=>Ok(true),_=>Err(U::InvalidBuffer)}
}

#[derive(Default)]
struct QuerySlot { raw: HANDLE, close_attempted: bool, close_error: Option<u32>, closed: bool }
impl QuerySlot {
    fn allocation_observation(&self,ok:i32,error:u32)->TokenAllocationObservation {
        let allocation=if self.raw==0 {TokenOutputAllocation::NotAllocated}
            else if ok!=0 && self.raw != -1 {TokenOutputAllocation::AllocatedOriginal}
            else {TokenOutputAllocation::UncertainOutput};
        TokenAllocationObservation {api_returned_success:ok!=0,allocation,
            win32_error:if ok==0 {Some(error)}else{None}}
    }
    fn open_for(&mut self,process:HANDLE,purpose:TokenOpenPurpose,mask:u32)
        ->(TokenOpenObservation,Result<(),U>) {
        let ok=unsafe{OpenProcessToken(process,mask,&mut self.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        let observation=TokenOpenObservation {purpose,requested_mask:mask,
            result:F::Observed(self.allocation_observation(ok,error))};
        let result=self.captured(ok,error);
        (observation,result)
    }
    fn close_native(&mut self)->io::Result<()> {
        self.close_with(|raw| {
            if unsafe{CloseHandle(raw)}!=0 {Ok(())}else{Err(unsafe{GetLastError()})}
        })
    }
    fn captured(&mut self, ok:i32, error:u32)->Result<(),U> {
        if ok==0 || self.raw==0 || self.raw == -1 {
            if self.raw!=0 {
                // An unsuccessful/invalid allocation is not authority to close
                // a possibly undefined output value. Retain uncertainty instead.
                self.close_attempted=true;self.close_error=Some(if ok==0 {error}else{6});
            }
            return Err(if ok==0 {U::Win32{code:error}}else{U::InvalidBuffer});
        }
        Ok(())
    }
    fn state(&self)->QueryHandleClose {
        if let Some(code)=self.close_error { QueryHandleClose::CloseFailed{code} }
        else if self.closed { QueryHandleClose::ClosedVerified }
        else if self.raw!=0 { QueryHandleClose::Held }
        else { QueryHandleClose::NotAllocated }
    }
    fn close_with(&mut self, close:impl FnOnce(HANDLE)->Result<(),u32>)->io::Result<()> {
        if self.raw==0 {return Ok(());}
        if self.close_attempted {
            return Err(self.close_error.map(|e|io::Error::from_raw_os_error(e as i32))
                .unwrap_or_else(||io::Error::other("original diagnostic token close uncertain")));
        }
        self.close_attempted=true; // Unwind cannot retry or silently lose the original.
        match close(self.raw) {
            Ok(())=>{self.raw=0;self.closed=true;Ok(())}
            Err(code)=>{self.close_error=Some(code);Err(io::Error::from_raw_os_error(code as i32))}
        }
    }
}
// Opaque Authz allocations are not kernel handles. They never use CloseHandle,
// QuerySlot's INVALID_HANDLE_VALUE rule, Drop, caching or a global owner lookup.
#[derive(Default)]
struct AuthzSlot { raw:isize, observation:AuthzResourceObservation }
impl AuthzSlot {
    fn allocate_with(&mut self,call:impl FnOnce(&mut isize)->(i32,u32))->Result<(),U> {
        if self.observation.allocation!=A::NotAttempted {return Err(U::OriginalUnavailable);}
        self.observation.allocation=A::InFlight;
        self.observation.retirement=R::Uncertain; // Installed before calling into native code.
        let (ok,error)=call(&mut self.raw);
        self.observation.initialization_error=if ok==0 {error}else{0};
        let (allocation,retirement)=match (ok!=0,self.raw!=0) {
            (true,true)=>(A::Held,R::Held),
            (true,false)=>(A::InvalidSuccessOutput,R::Uncertain),
            (false,true)=>(A::FailedUncertainOutput,R::Uncertain),
            (false,false)=>(A::FailedNoOutput,R::NotAllocated),
        };
        self.observation.allocation=allocation;self.observation.retirement=retirement;
        if ok==0 {Err(U::Win32{code:error})}
        else if allocation!=A::Held {Err(U::InvalidBuffer)}else{Ok(())}
    }
    fn unresolved(&self)->bool {
        !matches!(self.observation.retirement,R::NotAllocated|R::FreedVerified)
    }
    fn retained_error(&self)->io::Error {
        if self.observation.retirement==R::Failed {
            io::Error::from_raw_os_error(self.observation.retirement_error as i32)
        }else if self.observation.allocation==A::FailedUncertainOutput {
            io::Error::from_raw_os_error(self.observation.initialization_error as i32)
        }else{io::Error::other("original Authz resource uncertain; no replay")}
    }
    fn free_with(&mut self,free:impl FnOnce(isize)->Result<(),u32>)->io::Result<()> {
        if !self.unresolved() {return Ok(());}
        if self.observation.allocation!=A::Held || self.observation.retirement!=R::Held {
            return Err(self.retained_error()); // Includes in-flight unwind and previous failure.
        }
        self.observation.retirement=R::InFlight;
        match free(self.raw) {
            Ok(())=>{self.raw=0;self.observation.retirement=R::FreedVerified;Ok(())},
            Err(error)=>{self.observation.retirement_error=error;
                self.observation.retirement=R::Failed;Err(self.retained_error())},
        }
    }
}
#[derive(Default)]
struct AuthzManagerSlot(AuthzSlot);
#[derive(Default)]
struct AuthzContextSlot(AuthzSlot);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum AuthzResourceKind { Context, ResourceManager }
#[derive(Default)]
struct AuthzResources {
    manager:AuthzManagerSlot, context:AuthzContextSlot,
    station:AuthzObjectObservation, desktop:AuthzObjectObservation,
}
impl AuthzResources {
    fn observation(&self)->AuthzDaclProjection {
        AuthzDaclProjection::from_observations(self.manager.0.observation,
            self.context.0.observation,self.station,self.desktop)
    }
    fn unresolved(&self)->bool {self.context.0.unresolved() || self.manager.0.unresolved()}
    fn close_with(&mut self,mut free:impl FnMut(AuthzResourceKind,isize)->Result<(),u32>)->io::Result<()> {
        if matches!(self.station.state,O::InFlight) || matches!(self.desktop.state,O::InFlight) {
            // An unwound evaluator has no completed return witness. Retain all
            // dependencies; do not treat ordinary held values as free authority.
            if self.context.0.observation.retirement==R::Held {
                self.context.0.observation.retirement=R::Uncertain;
            }
            if self.manager.0.observation.retirement==R::Held {
                self.manager.0.observation.retirement=R::BlockedByContext;
            }
            return Err(io::Error::other("original Authz evaluation in flight; no replay"));
        }
        if let Err(error)=self.context.0.free_with(|raw|free(AuthzResourceKind::Context,raw)) {
            if self.manager.0.observation.retirement==R::Held {
                self.manager.0.observation.retirement=R::BlockedByContext;
            }
            return Err(error); // Never free a manager still referenced by a retained context.
        }
        self.manager.0.free_with(|raw|free(AuthzResourceKind::ResourceManager,raw))
    }
    fn free_native(kind:AuthzResourceKind,raw:isize)->Result<(),u32> {
        let ok=unsafe{match kind {
            AuthzResourceKind::Context=>AuthzFreeContext(raw),
            AuthzResourceKind::ResourceManager=>AuthzFreeResourceManager(raw),
        }};
        if ok!=0 {Ok(())}else{Err(unsafe{GetLastError()})}
    }
    fn initialize(&mut self,query:HANDLE)->Result<(),O> {
        // https://learn.microsoft.com/windows/win32/api/authz/nf-authz-authzinitializeresourcemanager
        self.manager.0.allocate_with(|raw| {
            let ok=unsafe{AuthzInitializeResourceManager(
                AUTHZ_RM_FLAG_NO_AUDIT|AUTHZ_RM_FLAG_NO_CENTRAL_ACCESS_POLICIES,
                None,None,None,ptr::null(),raw)};
            (ok,if ok==0 {unsafe{GetLastError()}}else{0})
        }).map_err(|_|O::ResourceManagerUnavailable)?;
        let manager=self.manager.0.raw;
        self.context.0.allocate_with(|raw| {
            let identifier=windows_sys::Win32::Foundation::LUID {LowPart:0,HighPart:0};
            let ok=unsafe{AuthzInitializeContextFromToken(0,query,manager,
                ptr::null(),identifier,ptr::null(),raw)};
            (ok,if ok==0 {unsafe{GetLastError()}}else{0})
        }).map_err(|_|O::ContextUnavailable)
    }
    fn observe(&mut self,query:HANDLE,primary:Result<(),U>,runner:&PrivateTokenFacts,
        station:&PreparedObject,desktop:&PreparedObject) {
        let ready=authz_prerequisite(primary,runner).and_then(|()|self.initialize(query));
        match ready {
            Err(state)=>{self.station.state=state;self.desktop.state=state;},
            Ok(())=>{
                authz_object_once(self.context.0.raw,station,true,&mut self.station);
                authz_object_once(self.context.0.raw,desktop,false,&mut self.desktop);
            },
        }
    }
}
fn authz_prerequisite(primary:Result<(),U>,runner:&PrivateTokenFacts)->Result<(),O> {
    if primary.is_err() || runner.first_unavailable().is_some() {return Err(O::PrimaryUnavailable);}
    if runner.restricted!=Ok(false) {return Err(O::RestrictedToken);}
    if runner.app_container!=Ok(false) {return Err(O::AppContainer);}
    Ok(())
}
#[derive(Default)]
pub(crate) struct StartupQueries {
    attempted: bool,
    primary: QuerySlot,
    duplicate_source: QuerySlot,
    impersonation: QuerySlot,
    authz:AuthzResources,
}
impl StartupQueries {
    pub(crate) fn has_attempted(&self)->bool {self.attempted}
    #[cfg(test)]
    pub(crate) fn retained_close_debt_fixture()->Self {
        // Labelled scalar-only negative fixture, never operated as a HANDLE.
        Self {attempted:true,primary:QuerySlot{raw:1234,close_attempted:true,
            close_error:Some(5),closed:false},duplicate_source:QuerySlot::default(),
            impersonation:QuerySlot::default(),authz:AuthzResources::default()}
    }
    #[cfg(test)]
    pub(crate) fn retained_close_debt_for_slot_fixture(index:usize)->Self {
        let mut queries=Self::default();queries.attempted=true;
        let slot=match index {0=>&mut queries.primary,1=>&mut queries.duplicate_source,
            2=>&mut queries.impersonation,_=>panic!("invalid labelled slot index")};
        *slot=QuerySlot{raw:1234,close_attempted:true,close_error:Some(5),closed:false};
        queries
    }
    pub(crate) fn unresolved(&self)->bool {
        self.primary.raw!=0 || self.duplicate_source.raw!=0 || self.impersonation.raw!=0 || self.authz.unresolved()
    }
    pub(crate) fn states(&self)->[QueryHandleClose;2] {[self.primary.state(),self.impersonation.state()]}
    pub(crate) fn all_states(&self)->[QueryHandleClose;3] {
        [self.primary.state(),self.duplicate_source.state(),self.impersonation.state()]
    }
    pub(crate) fn close_once(&mut self)->io::Result<()> {
        // Independent exact originals each get at most one close, even if the first fails.
        let a=self.impersonation.close_native();
        let b=self.duplicate_source.close_native();
        let c=self.close_primary_after_authz();
        a.and(b).and(c)
    }
    pub(crate) fn authz_observation(&self)->AuthzDaclProjection {self.authz.observation()}
    fn close_primary_after_authz(&mut self)->io::Result<()> {
        self.close_primary_with(AuthzResources::free_native,|raw| {
            if unsafe{CloseHandle(raw)}!=0 {Ok(())}else{Err(unsafe{GetLastError()})}
        })
    }
    fn close_primary_with(&mut self,free:impl FnMut(AuthzResourceKind,isize)->Result<(),u32>,
        query_close:impl FnOnce(HANDLE)->Result<(),u32>)->io::Result<()> {
        self.authz.close_with(free)?;
        self.primary.close_with(query_close) // Debt keeps the actual QUERY Held, not CloseFailed.
    }
    pub(crate) fn observe(&mut self, process:HANDLE, caller:&PrivateTokenFacts, intended_sid:&[u8],
        station:HANDLE, station_name:&[u16], desktop:HANDLE, desktop_name:&[u16],
        out:&mut WindowsStartupContext)
    {
        if self.attempted {return;} // No automatic repeat, including unavailable reads.
        self.attempted=true;
        out.query_status=F::Observed(());
        // One QUERY purpose only. The independently registered DUP source may
        // be opened later, never as a fallback for a failed QUERY.
        let (opened,primary_result)=self.primary.open_for(process,TokenOpenPurpose::PrimaryQuery,TOKEN_QUERY);
        out.query_stages.primary_open=opened;
        let runner=match primary_result {
            Ok(())=>PrivateTokenFacts::read(self.primary.raw),
            Err(reason)=>PrivateTokenFacts::unavailable(upstream(Stage::QueryPrimaryOpen,reason)),
        };
        out.query_stages.primary_reads=runner.read_results();
        out.token=token_relations(&runner,caller,intended_sid);
        // One original SD per object, retained only as finite ordinary memory.
        // Authz maps a private copy; legacy AccessCheck later receives the unchanged original.
        let station=prepare_object(station,station_name,true,&runner);
        let desktop=prepare_object(desktop,desktop_name,false,&runner);
        self.authz.observe(self.primary.raw,primary_result,&runner,&station,&desktop);
        // Copy facts, then checked-close QUERY before allocating another
        // purpose. Slot tombstones are NEVER reused or overwritten.
        let query_close=self.close_primary_after_authz();
        let prerequisite=duplicate_prerequisite(primary_result,&runner,&query_close);
        let token=match prerequisite {
            Ok(())=>{
                let (opened,result)=self.duplicate_source.open_for(
                    process,TokenOpenPurpose::DuplicationSource,TOKEN_DUPLICATE);
                match record_duplicate_source(out,opened,result) {
                    Ok(())=>{
                        let ok=unsafe{DuplicateToken(self.duplicate_source.raw,SecurityIdentification,&mut self.impersonation.raw)};
                        let error=if ok==0 {unsafe{GetLastError()}}else{0};
                        out.query_stages.impersonation_duplicate=F::Observed(
                            self.impersonation.allocation_observation(ok,error));
                        self.impersonation.captured(ok,error)
                            .map(|()|self.impersonation.raw).map_err(|e|upstream(Stage::DuplicateToken,e))
                    }
                    Err(reason)=>Err(reason),
                }
            }
            Err(reason)=>{
                out.query_stages.duplicate_source_open=TokenOpenObservation {
                    purpose:TokenOpenPurpose::DuplicationSource,requested_mask:0,
                    result:F::NotObserved(reason)};
                out.query_stages.impersonation_duplicate=F::NotObserved(reason);
                Err(reason)
            }
        };
        out.station=finish_object(station,true,token);
        out.desktop=finish_object(desktop,false,token);
        // Never expose raw handles, SID bytes or object names in the cloneable history.
        let _close_result=self.close_once();
        out.query_handle_closes=self.states(); // Owner checks unresolved separately, sealing debt.
        out.query_slot_closes=self.all_states();
        out.authz_dacl=self.authz.observation();
    }
}

fn record_duplicate_source(out:&mut WindowsStartupContext,opened:TokenOpenObservation,result:Result<(),U>)
    ->Result<(),U> {
    out.query_stages.duplicate_source_open=opened;
    result.map_err(|error|{
        let reason=upstream(Stage::DuplicateSourceOpen,error);
        out.query_stages.impersonation_duplicate=F::NotObserved(reason);
        reason
    })
}
fn duplicate_prerequisite(primary:Result<(),U>,runner:&PrivateTokenFacts,closed:&io::Result<()>)->Result<(),U> {
    primary.map_err(|e|upstream(Stage::QueryPrimaryOpen,e))?;
    if let Some(reason)=runner.first_unavailable() {return Err(reason);}
    closed.as_ref().map_err(|error|upstream(Stage::QueryPrimaryClose,
        match error.raw_os_error() {Some(code)=>U::Win32{code:code as u32},None=>U::OriginalUnavailable}))?;
    Ok(())
}
fn token_relations(runner:&PrivateTokenFacts,caller:&PrivateTokenFacts,intended_sid:&[u8])->StartupTokenRelations {
    let intended=if intended_sid.len()<=68 {sid_at(intended_sid,0)}else{Err(U::BoundExceeded)};
    let caller_logon=caller.logon.as_ref().map(|v|v.0.clone()).map_err(|e|*e);
    let runner_logon=runner.logon.as_ref().map(|v|v.0.clone()).map_err(|e|*e);
    StartupTokenRelations {
        user_equals_intended_sandbox:compare(&runner.user,Stage::QueryUser,&intended,Stage::IntendedSid),
        user_equals_caller_user:compare(&runner.user,Stage::QueryUser,&caller.user,Stage::CallerUser),
        logon_sid_equals_caller_logon_sid:compare(&runner_logon,Stage::QueryLogonGroups,&caller_logon,Stage::CallerLogonGroups),
        runner_logon_group_attributes:fact(runner.logon.as_ref().map(|v|v.1)
            .map_err(|e|upstream(Stage::QueryLogonGroups,*e))),
        session_equals_caller_session:fact(match (&runner.session,&caller.session) {
            (Ok(a),Ok(b))=>Ok(a==b),
            (Err(e),_)=>Err(upstream(Stage::QuerySession,*e)),
            (_,Err(e))=>Err(upstream(Stage::CallerSession,*e))}),
        has_restrictions:fact(runner.restricted.map_err(|e|upstream(Stage::QueryRestrictions,e))),
        is_app_container:fact(runner.app_container.map_err(|e|upstream(Stage::QueryAppContainer,e))),
    }
}

pub(crate) fn object_name(object:HANDLE)->Result<Vec<u16>,U> {
    let mut name=[0u16;NAME_CAP_UNITS];
    let mut returned=0;
    if unsafe{GetUserObjectInformationW(object,UOI_NAME,name.as_mut_ptr().cast(),
        (NAME_CAP_UNITS*2) as u32,&mut returned)}==0 {return Err(last());}
    if returned<4 || returned as usize>NAME_CAP_UNITS*2 || returned%2!=0 {return Err(U::InvalidBuffer);}
    let units=returned as usize/2;
    if name[units-1]!=0 || name[..units-1].contains(&0) {return Err(U::InvalidBuffer);}
    if String::from_utf16(&name[..units-1]).is_err(){return Err(U::InvalidBuffer);}
    Ok(name[..units].to_vec())
}
fn object_visible(object:HANDLE)->Result<bool,U> {
    let mut flags:USEROBJECTFLAGS=unsafe{mem::zeroed()};
    let mut returned=0;
    if unsafe{GetUserObjectInformationW(object,UOI_FLAGS,(&mut flags as *mut USEROBJECTFLAGS).cast(),
        mem::size_of::<USEROBJECTFLAGS>() as u32,&mut returned)}==0 {return Err(last());}
    if returned as usize!=mem::size_of::<USEROBJECTFLAGS>() {return Err(U::InvalidBuffer);}
    Ok(flags.dwFlags & 1 != 0) // WSF_VISIBLE; no activation or switching.
}
fn desktop_input(object:HANDLE)->Result<bool,U> {
    let mut value=0u32;let mut returned=0;
    if unsafe{GetUserObjectInformationW(object,6,(&mut value as *mut u32).cast(),4,&mut returned)}==0 {
        return Err(last());
    }
    if returned!=4 {return Err(U::InvalidBuffer);}
    bool_scalar(value) // UOI_IO, never read actual user input or switch desktop.
}
fn descriptor(object:HANDLE)->Result<Buffer,U> {
    let mut requested=0x1|0x2|0x4; // OWNER/GROUP/DACL only; never SACL.
    let mut needed=0;
    let ok=unsafe{GetUserObjectSecurity(object,&mut requested,ptr::null_mut(),0,&mut needed)};
    if ok!=0 {return Err(U::InvalidBuffer);}
    let error=last();
    if error!=(U::Win32{code:ERROR_INSUFFICIENT_BUFFER}) {return Err(error);}
    let mut b=Buffer::new(needed as usize,SECURITY_CAP)?;
    let mut returned=0;
    if unsafe{GetUserObjectSecurity(object,&mut requested,b.raw(),needed,&mut returned)}==0 {return Err(last());}
    if returned<20 || returned>needed {return Err(U::InvalidBuffer);}
    b.len=returned as usize;
    validate_descriptor(&b)?;
    Ok(b)
}
fn validate_descriptor(b:&Buffer)->Result<(),U> {
    let bytes=b.bytes();
    if bytes.len()<20 || bytes[0]!=1 {return Err(U::InvalidBuffer);}
    let control=u16::from_le_bytes([bytes[2],bytes[3]]);
    if control&0x8000==0 || u32_at(bytes,12)?!=0 {return Err(U::InvalidBuffer);} // self-relative, no SACL
    for field in [4,8] {
        let offset=u32_at(bytes,field)? as usize;
        if offset<20 {return Err(U::InvalidBuffer);}
        sid_at(bytes,offset)?;
    }
    let offset=u32_at(bytes,16)? as usize;
    if offset!=0 {
        if offset<20 || offset%4!=0 || offset.checked_add(8).ok_or(U::InvalidBuffer)?>bytes.len() {
            return Err(U::InvalidBuffer);
        }
        let size=u16::from_le_bytes([bytes[offset+2],bytes[offset+3]]) as usize;
        let count=u16::from_le_bytes([bytes[offset+4],bytes[offset+5]]) as usize;
        let end=offset.checked_add(size).ok_or(U::InvalidBuffer)?;
        if size<8 || end>bytes.len() || count>1024 {return Err(U::BoundExceeded);}
        let mut cursor=offset+8;
        for _ in 0..count {
            if cursor.checked_add(4).ok_or(U::InvalidBuffer)?>end {return Err(U::InvalidBuffer);}
            let size=u16::from_le_bytes([bytes[cursor+2],bytes[cursor+3]]) as usize;
            if size<4 || size%4!=0 {return Err(U::InvalidBuffer);}
            let ace_end=cursor.checked_add(size).ok_or(U::InvalidBuffer)?;
            if ace_end>end {return Err(U::InvalidBuffer);}
            // Bound the SID inside its own ACE, including object GUID layouts,
            // before handing a descriptor to the Win32 validators/AccessCheck.
            let sid_offset=match bytes[cursor] {
                0..=3 | 9..=10 | 13..=14 | 17..=21 => cursor+8,
                5..=8 | 11..=12 | 15..=16 => {
                    if size<12 {return Err(U::InvalidBuffer);}
                    let flags=u32_at(bytes,cursor+8)?;
                    if flags & !3 !=0 {return Err(U::InvalidBuffer);}
                    cursor+12+(if flags&1!=0 {16}else{0})+(if flags&2!=0 {16}else{0})
                }
                // Compound/unknown layouts are explicitly unobserved, not
                // manually merged into an effective access result.
                _=>return Err(U::InvalidBuffer),
            };
            sid_at(&bytes[..ace_end],sid_offset)?;
            cursor=ace_end;
        }
        if unsafe{IsValidAcl(bytes.as_ptr().add(offset).cast())}==0 {return Err(U::InvalidBuffer);}
    }
    if unsafe{IsValidSecurityDescriptor(bytes.as_ptr().cast_mut().cast())}==0 {return Err(U::InvalidBuffer);}
    Ok(())
}
fn mapping(station:bool,visible:bool)->GENERIC_MAPPING {
    if station {
        // MS window-station mapping: noninteractive excludes screen/write-attributes.
        GENERIC_MAPPING {
            GenericRead:READ_CONTROL_MASK|0x1|0x100|0x2|if visible{0x200}else{0},
            GenericWrite:READ_CONTROL_MASK|0x4|0x8|if visible{0x10}else{0},
            GenericExecute:READ_CONTROL_MASK|0x20|0x40,
            GenericAll:STANDARD_REQUIRED|if visible{0x37f}else{0x16f},
        }
    }else{
        GENERIC_MAPPING {GenericRead:READ_CONTROL_MASK|0x40|0x1,
            GenericWrite:READ_CONTROL_MASK|0x2|0x4|0x8|0x10|0x20|0x80,
            GenericExecute:READ_CONTROL_MASK|0x100,GenericAll:STANDARD_REQUIRED|0x1ff}
    }
}
fn access(b:&mut Buffer,token:HANDLE,station:bool,visible:bool,invoked:&mut bool)->Result<StartupAccessResult,U> {
    let mut map=mapping(station,visible);
    let mut privileges=Buffer::new(PRIVILEGE_CAP,PRIVILEGE_CAP)?;
    let mut size=PRIVILEGE_CAP as u32;
    let mut granted=0;
    let mut status=0;
    *invoked=true; // Set only immediately before the actual AccessCheck call.
    if unsafe{AccessCheck(b.raw(),token,MAXIMUM_ALLOWED_MASK,&mut map,
        privileges.raw().cast::<PRIVILEGE_SET>(),&mut size,&mut granted,&mut status)}==0 {
        return Err(last()); // Insufficient privilege buffer stays explicitly unavailable.
    }
    let denial_win32_error=if status==0 {Some(unsafe{GetLastError()})}else{None};
    validate_privileges(privileges.bytes(),size as usize)?;
    if status!=0 && status!=1 {return Err(U::InvalidBuffer);}
    Ok(StartupAccessResult {access_status:status!=0,granted_mask:granted,denial_win32_error})
}
fn validate_privileges(bytes:&[u8],returned:usize)->Result<(),U> {
    if returned>PRIVILEGE_CAP || returned>bytes.len() || returned<8 {return Err(U::InvalidBuffer);}
    let count=u32_at(bytes,0)? as usize;
    if count>256 {return Err(U::BoundExceeded);}
    if 8usize.checked_add(count.checked_mul(12).ok_or(U::InvalidBuffer)?)
        .ok_or(U::InvalidBuffer)?>returned {return Err(U::InvalidBuffer);}
    Ok(())
}
struct PreparedObject {
    out:StartupObjectObservation, descriptor:Result<Buffer,U>, visible:Result<bool,U>,
}
fn prepare_object(object:HANDLE,expected_name:&[u16],station:bool,
    runner:&PrivateTokenFacts)->PreparedObject {
    let name=fact(object_name(object).map(|name|name==expected_name));
    // UOI_FLAGS has no desktop visibility fact. Do not label a desktop hook flag as visible.
    let visible=if station {object_visible(object)} else {Err(U::NotApplicable)};
    let mut out=StartupObjectObservation {name_equals_creation_object:name,visible:fact(visible),
        receiving_user_input:if station {F::NotObserved(U::NotApplicable)}else{fact(desktop_input(object))},
        ..StartupObjectObservation::default()};
    let descriptor=descriptor(object);
    match &descriptor {
        Err(error)=>{
            let error=*error;
            let stage=if station {Stage::StationDescriptor}else{Stage::DesktopDescriptor};
            out.descriptor_read=F::NotObserved(error);
            out.descriptor_owner_equals_runner_user=F::NotObserved(upstream(stage,error));
            out.descriptor_group_equals_runner_logon_sid=F::NotObserved(upstream(stage,error));
            out.dacl_present=F::NotObserved(upstream(stage,error));
            out.dacl_null=F::NotObserved(upstream(stage,error));
            out.access=F::NotObserved(upstream(stage,error));
        }
        Ok(b)=>{
            out.descriptor_read=F::Observed(());
            let owner=u32_at(b.bytes(),4).and_then(|v|sid_at(b.bytes(),v as usize));
            let group=u32_at(b.bytes(),8).and_then(|v|sid_at(b.bytes(),v as usize));
            let logon=runner.logon.as_ref().map(|v|v.0.clone()).map_err(|e|*e);
            let stage=if station {Stage::StationDescriptor}else{Stage::DesktopDescriptor};
            out.descriptor_owner_equals_runner_user=compare(&owner,stage,&runner.user,Stage::QueryUser);
            out.descriptor_group_equals_runner_logon_sid=compare(&group,stage,&logon,Stage::QueryLogonGroups);
            out.dacl_present=F::Observed(b.bytes()[2]&4!=0);
            out.dacl_null=fact(u32_at(b.bytes(),16).map(|v|v==0));
        }
    }
    PreparedObject {out,descriptor,visible}
}
fn finish_object(prepared:PreparedObject,station:bool,token:Result<HANDLE,U>)->StartupObjectObservation {
    let PreparedObject {mut out,mut descriptor,visible}=prepared;
    if let Ok(b)=&mut descriptor {
        out.access=fact(token.map_err(|e|upstream(Stage::DuplicateToken,e)).and_then(|t| {
            let visible=if station {visible.map_err(|e|upstream(Stage::StationVisibility,e))?}else{false};
            access(b,t,station,visible,&mut out.access_check_invoked)
        }));
    }
    out
}
#[cfg(test)]
fn object_observation(object:HANDLE,expected_name:&[u16],station:bool,token:Result<HANDLE,U>,
    runner:&PrivateTokenFacts)->StartupObjectObservation {
    finish_object(prepare_object(object,expected_name,station,runner),station,token)
}

// Pure bounded projection. Existing descriptor() still performs the original
// Win32 SD/ACL/SID validations. This narrower pass additionally rejects aliases,
// complex ACEs and unsupported substitutions rather than dropping their meaning.
fn projected_sid_end(bytes:&[u8],start:usize,limit:usize)->Result<usize,O> {
    if start%4!=0 {return Err(O::InvalidBuffer);}
    let head=bytes.get(start..start.checked_add(8).ok_or(O::InvalidBuffer)?).ok_or(O::InvalidBuffer)?;
    if head[0]!=1 || head[1]>15 {return Err(O::InvalidBuffer);}
    let end=start.checked_add(8+head[1] as usize*4).ok_or(O::InvalidBuffer)?;
    if end>limit || end>bytes.len() {return Err(O::InvalidBuffer);}
    Ok(end)
}
fn mapped_dacl(original:&Buffer,station:bool,visible:bool)->Result<Buffer,O> {
    let b=original.bytes();
    if b.len()>SECURITY_CAP {return Err(O::BoundExceeded);}
    if b.len()<20 || b[0]!=1 {return Err(O::InvalidBuffer);}
    let control=u16::from_le_bytes([b[2],b[3]]);
    if control&0x8000==0 || u32_at(b,12).map_err(|_|O::InvalidBuffer)?!=0 {return Err(O::InvalidBuffer);}
    let acl=u32_at(b,16).map_err(|_|O::InvalidBuffer)? as usize;
    if control&4==0 || acl==0 {return Err(O::NullOrAbsentDacl);}
    if acl<20 || acl%4!=0 || acl.checked_add(8).ok_or(O::InvalidBuffer)?>b.len() {
        return Err(O::InvalidBuffer);
    }
    if !matches!(b[acl],2|4) {return Err(O::InvalidBuffer);}
    let acl_end=acl.checked_add(u16::from_le_bytes([b[acl+2],b[acl+3]]) as usize).ok_or(O::InvalidBuffer)?;
    if acl_end<acl+8 || acl_end>b.len() {return Err(O::InvalidBuffer);}
    let owner=u32_at(b,4).map_err(|_|O::InvalidBuffer)? as usize;
    let group=u32_at(b,8).map_err(|_|O::InvalidBuffer)? as usize;
    if owner<20 || group<20 {return Err(O::InvalidBuffer);}
    let regions=[(0,20),(owner,projected_sid_end(b,owner,b.len())?),
        (group,projected_sid_end(b,group,b.len())?),(acl,acl_end)];
    for i in 0..regions.len() {for j in i+1..regions.len() {
        if regions[i].0<regions[j].1 && regions[j].0<regions[i].1 {return Err(O::OverlappingLayout);}
    }}
    let count=u16::from_le_bytes([b[acl+4],b[acl+5]]) as usize;
    if count>1024 {return Err(O::BoundExceeded);}
    let mut projected=Buffer::new(b.len(),SECURITY_CAP).map_err(|_|O::BoundExceeded)?;
    projected.bytes_mut().copy_from_slice(b);
    let mut cursor=acl+8;
    let map=mapping(station,visible);
    for _ in 0..count {
        if cursor.checked_add(8).ok_or(O::InvalidBuffer)?>acl_end {return Err(O::InvalidBuffer);}
        if !matches!(b[cursor],0|1) {return Err(O::UnsupportedAce);}
        let size=u16::from_le_bytes([b[cursor+2],b[cursor+3]]) as usize;
        let end=cursor.checked_add(size).ok_or(O::InvalidBuffer)?;
        if size<16 || size%4!=0 || end>acl_end {return Err(O::InvalidBuffer);}
        if projected_sid_end(b,cursor+8,end)?!=end {return Err(O::UnsupportedAce);}
        if b[cursor+8..end]==[1,1,0,0,0,0,0,5,10,0,0,0] {return Err(O::PrincipalSelf);}
        let mask=u32_at(b,cursor+4).map_err(|_|O::InvalidBuffer)?;
        let mapped=(mask&0x0fffffff)
            |if mask&0x80000000!=0 {map.GenericRead}else{0}
            |if mask&0x40000000!=0 {map.GenericWrite}else{0}
            |if mask&0x20000000!=0 {map.GenericExecute}else{0}
            |if mask&0x10000000!=0 {map.GenericAll}else{0};
        projected.bytes_mut()[cursor+4..cursor+8].copy_from_slice(&mapped.to_le_bytes());
        cursor=end;
    }
    Ok(projected)
}
fn authz_reply(out:&mut AuthzObjectObservation,ok:i32,error:u32,granted:u32,reply_error:u32) {
    // No SACL input: under NO_AUDIT it may remain completely unwritten.
    if ok==0 {out.state=O::ApiFailed;out.error=error;out.granted_mask=0;}
    else {out.state=O::ReplyReturned;out.error=reply_error;out.granted_mask=granted;}
}
fn authz_object_with(prepared:&PreparedObject,station:bool,out:&mut AuthzObjectObservation,
    call:impl FnOnce(&mut Buffer)->(i32,u32,u32,u32)) {
    if out.state!=O::NotAttempted {return;}
    if prepared.out.name_equals_creation_object!=F::Observed(true) {out.state=O::ObjectIdentityUnavailable;return;}
    let visible=if station {match prepared.visible {
        Ok(value)=>value,Err(_)=>{out.state=O::VisibilityUnavailable;return;},
    }}else{false};
    let original=match &prepared.descriptor {
        Ok(value)=>value,Err(error)=>{out.state=O::DescriptorUnavailable;
            out.error=match error {U::Win32{code}=>*code,_=>0};return;},
    };
    out.mapping=if station {if visible {M::VisibleStation}else{M::NoninteractiveStation}}else{M::Desktop};
    let mut projected=match mapped_dacl(original,station,visible) {
        Ok(value)=>value,Err(state)=>{out.state=state;return;},
    };
    out.state=O::InFlight;
    let (ok,error,granted,reply_error)=call(&mut projected);
    authz_reply(out,ok,error,granted,reply_error);
}
fn authz_object_once(context:isize,prepared:&PreparedObject,station:bool,out:&mut AuthzObjectObservation) {
    authz_object_with(prepared,station,out,|projected| {
        let request=AUTHZ_ACCESS_REQUEST {DesiredAccess:MAXIMUM_ALLOWED_MASK,
            PrincipalSelfSid:ptr::null_mut(),ObjectTypeList:ptr::null_mut(),
            ObjectTypeListLength:0,OptionalArguments:ptr::null_mut()};
        let mut granted=0xa5a5a5a5u32;let mut error=0xa5a5a5a5u32;
        let mut sacl=0xa5a5a5a5u32;
        let mut reply=AUTHZ_ACCESS_REPLY {ResultListLength:1,GrantedAccessMask:&mut granted,
            SaclEvaluationResults:&mut sacl,Error:&mut error};
        let ok=unsafe{AuthzAccessCheck(0,context,&request,0,projected.raw(),ptr::null(),0,&mut reply,ptr::null_mut())};
        let api_error=if ok==0 {unsafe{GetLastError()}}else{0};
        // No reply member is consumed on API failure. SACL is never read, even
        // on success: NO_AUDIT can leave its initialized sentinel untouched.
        if ok==0 {(ok,api_error,0,0)}else{(ok,0,granted,error)}
    });
}
#[cfg(test)]
#[path="startup_authz_tests.rs"]
mod authz_tests;
#[cfg(test)]
pub(crate) fn missing_token_object_fixture(object:HANDLE,name:&[u16])->StartupObjectObservation {
    // Inject only the labelled upstream API result. Descriptor/name reads are
    // real on the fixture's original object; no AccessCheck is manufactured.
    let error=upstream(Stage::QueryPrimaryOpen,U::Win32{code:5});
    object_observation(object,name,false,Err(error),&PrivateTokenFacts::unavailable(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    #[test]
    fn restrictions_one_byte_return_accepts_zero_and_noncanonical_nonzero_flags() {
        for (value,expected) in [(0,false),(1,true),(2,true),(0x80,true),(0xff,true)] {
            assert_eq!(parse_restrictions_flag(TokenHasRestrictions,&[value]),Ok(expected));
        }
    }

    #[test]
    fn restrictions_dword_return_uses_full_nonzero_semantics() {
        for (value,expected) in [(0u32,false),(1,true),(2,true),(0x100,true),
            (0x01000000,true),(0x80000000,true),(u32::MAX,true)] {
            assert_eq!(parse_restrictions_flag(TokenHasRestrictions,&value.to_le_bytes()),Ok(expected));
        }
    }

    #[test]
    fn restrictions_short_return_does_not_consume_a5_allocation_padding() {
        let mut buffer=Buffer::new(4,TOKEN_CAP).unwrap();
        buffer.words[0]=u64::from_ne_bytes([0,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5]);
        buffer.len=1; // Labelled actual-return-length model, not allocation size.
        assert_eq!(buffer.bytes(),&[0]);
        assert_eq!(parse_restrictions_flag(TokenHasRestrictions,buffer.bytes()),Ok(false));
        buffer.words[0]=u64::from_ne_bytes([2,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5]);
        assert_eq!(parse_restrictions_flag(TokenHasRestrictions,buffer.bytes()),Ok(true));
        buffer.words[0]=u64::from_ne_bytes([0,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5,0xa5]);
        buffer.len=4; // Only an actual four-byte return may include these bytes.
        assert_eq!(parse_restrictions_flag(TokenHasRestrictions,buffer.bytes()),Ok(true));
    }

    #[test]
    fn restrictions_parser_rejects_other_widths_and_information_classes() {
        for bytes in [&[][..],&[0,0][..],&[0,0,0][..],&[0;5][..],&[0;8][..]] {
            assert_eq!(parse_restrictions_flag(TokenHasRestrictions,bytes),Err(U::InvalidBuffer));
        }
        for class in [TokenSessionId,TokenIsAppContainer,TokenGroups] {
            assert_eq!(parse_restrictions_flag(class,&[0]),Err(U::InvalidBuffer));
            assert_eq!(parse_restrictions_flag(class,&[0;4]),Err(U::InvalidBuffer));
        }
    }

    #[test]
    fn current_query_token_reads_production_restrictions_flag_and_checked_closes_its_original() {
        // Ordinary current process only; no alternate logon or product/VM proof.
        let mut slot=QuerySlot::default();
        let (opening,result)=slot.open_for(unsafe{GetCurrentProcess()},TokenOpenPurpose::PrimaryQuery,TOKEN_QUERY);
        result.unwrap();
        assert_eq!(opening.requested_mask,8);
        let observed=token_restrictions_flag(slot.raw);
        // Close the actual fixture original even if the parser reports failure.
        slot.close_native().unwrap();
        assert_eq!(slot.state(),QueryHandleClose::ClosedVerified);
        let _classification=observed.expect("actual current-token Restrictions classification must be available");
        assert_eq!(slot.state(),QueryHandleClose::ClosedVerified);
    }

    fn labelled_facts()->PrivateTokenFacts {
        // Pure relationship model, not an observed runner or access grant.
        let sid=vec![1,1,0,0,0,0,0,1,0,0,0,0];
        PrivateTokenFacts {user:Ok(sid.clone()),logon:Ok((sid,LOGON_GROUP)),session:Ok(1),
            restricted:Ok(false),app_container:Ok(false)}
    }

    #[test]
    fn independent_query_and_dup_purposes_have_three_real_checked_close_tombstones() {
        // Ordinary current process only; not cross-account/native acceptance.
        let mut queries=StartupQueries::default();
        let process=unsafe{GetCurrentProcess()};
        let (query,result)=queries.primary.open_for(process,TokenOpenPurpose::PrimaryQuery,TOKEN_QUERY);
        result.unwrap();
        assert_eq!(query.requested_mask,8);
        assert_eq!(query.purpose,TokenOpenPurpose::PrimaryQuery);
        assert_eq!(query.result,F::Observed(TokenAllocationObservation {
            api_returned_success:true,allocation:TokenOutputAllocation::AllocatedOriginal,win32_error:None}));
        let facts=PrivateTokenFacts::read(queries.primary.raw);
        let closed=queries.primary.close_native();
        duplicate_prerequisite(Ok(()),&facts,&closed).unwrap();
        assert_eq!(queries.all_states(),[QueryHandleClose::ClosedVerified,
            QueryHandleClose::NotAllocated,QueryHandleClose::NotAllocated]);
        let (source,result)=queries.duplicate_source.open_for(process,TokenOpenPurpose::DuplicationSource,TOKEN_DUPLICATE);
        result.unwrap();
        assert_eq!(source.requested_mask,2);
        assert_eq!(source.purpose,TokenOpenPurpose::DuplicationSource);
        let ok=unsafe{DuplicateToken(queries.duplicate_source.raw,SecurityIdentification,&mut queries.impersonation.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        queries.impersonation.captured(ok,error).unwrap();
        assert_eq!(queries.all_states(),[QueryHandleClose::ClosedVerified,
            QueryHandleClose::Held,QueryHandleClose::Held]);
        queries.close_once().unwrap();
        assert_eq!(queries.all_states(),[QueryHandleClose::ClosedVerified;3]);
        assert!(!queries.unresolved());
        assert_eq!(facts.read_results(),PrimaryTokenReadResults {user:F::Observed(()),
            logon_groups:F::Observed(()),session:F::Observed(()),
            restrictions:F::Observed(()),app_container:F::Observed(())});
        queries.close_once().unwrap();
        assert_eq!(queries.all_states(),[QueryHandleClose::ClosedVerified;3]);
    }

    #[test]
    fn failed_query_gates_dup_and_retains_upstream_code_instead_of_a_downstream_api_result() {
        let error=U::Win32{code:5};
        let reason=upstream(Stage::QueryPrimaryOpen,error);
        let facts=PrivateTokenFacts::unavailable(reason);
        assert_eq!(duplicate_prerequisite(Err(error),&facts,&Ok(())),Err(reason));
        let reads=facts.read_results();
        assert_eq!(reads.user,F::NotObserved(reason));
        assert_eq!(reads.logon_groups,F::NotObserved(reason));
        let queries=StartupQueries::default();
        assert_eq!(queries.all_states(),[QueryHandleClose::NotAllocated;3]);
        assert_eq!(upstream(Stage::DuplicateToken,reason),reason);
    }

    #[test]
    fn query_success_dup_source_failure_keeps_primary_facts_and_marks_duplicate_not_called() {
        let facts=labelled_facts();
        duplicate_prerequisite(Ok(()),&facts,&Ok(())).unwrap();
        let mut out=WindowsStartupContext::default();
        out.query_stages.primary_reads=facts.read_results();
        out.token=token_relations(&facts,&facts,facts.user.as_ref().unwrap());
        let before=out.token;
        let reads=out.query_stages.primary_reads;
        let failed=TokenOpenObservation {purpose:TokenOpenPurpose::DuplicationSource,requested_mask:2,
            result:F::Observed(TokenAllocationObservation{api_returned_success:false,
                allocation:TokenOutputAllocation::NotAllocated,win32_error:Some(5)})};
        let reason=upstream(Stage::DuplicateSourceOpen,U::Win32{code:5});
        assert_eq!(record_duplicate_source(&mut out,failed,Err(U::Win32{code:5})),Err(reason));
        assert_eq!(out.query_stages.duplicate_source_open,failed);
        assert_eq!(out.query_stages.impersonation_duplicate,F::NotObserved(reason));
        assert_eq!(out.query_stages.primary_reads,reads);
        assert_eq!(out.token,before);
        assert_eq!(out.original_runner_identity,None);
    }

    #[test]
    fn unavailable_primary_field_or_its_close_debt_prevents_the_independent_dup_purpose() {
        let mut facts=labelled_facts();
        facts.session=Err(U::Win32{code:5});
        assert_eq!(facts.read_results().session,F::NotObserved(U::Win32{code:5}));
        assert_eq!(duplicate_prerequisite(Ok(()),&facts,&Ok(())),
            Err(upstream(Stage::QuerySession,U::Win32{code:5})));
        let facts=labelled_facts();
        assert_eq!(duplicate_prerequisite(Ok(()),&facts,&Err(io::Error::from_raw_os_error(6))),
            Err(upstream(Stage::QueryPrimaryClose,U::Win32{code:6})));
    }

    #[test]
    fn every_query_slot_retains_its_own_uncertain_original_without_os_retry() {
        for index in 0..3 {
            let mut queries=StartupQueries::retained_close_debt_for_slot_fixture(index);
            let before=queries.all_states();
            assert_eq!(before[index],QueryHandleClose::CloseFailed{code:5});
            for _ in 0..2 {
                assert_eq!(queries.close_once().unwrap_err().raw_os_error(),Some(5));
                assert_eq!(queries.all_states(),before);
                assert!(queries.unresolved());
            }
        }
    }

    #[test]
    fn upstream_stage_preserves_the_original_buffer_or_win32_failure() {
        let reason=upstream(Stage::QueryLogonGroups,U::BoundExceeded);
        assert_eq!(reason,U::Upstream{stage:Stage::QueryLogonGroups,reason:ContextFailure::BoundExceeded});
        assert_eq!(upstream(Stage::DuplicateSourceOpen,reason),reason);
        let code=upstream(Stage::QueryPrimaryOpen,U::Win32{code:5});
        assert_eq!(code,U::Upstream{stage:Stage::QueryPrimaryOpen,reason:ContextFailure::Win32{code:5}});
        assert_eq!(upstream(Stage::StationDescriptor,code),code);
    }

    #[test]
    fn malformed_buffers_reject_offsets_lengths_and_sid_pointers_before_os_parsing() {
        assert!(matches!(Buffer::new(TOKEN_CAP+1,TOKEN_CAP),Err(U::BoundExceeded)));
        let mut b=Buffer::new(32,SECURITY_CAP).unwrap();
        assert_eq!(validate_descriptor(&b),Err(U::InvalidBuffer));
        assert_eq!(pointed_sid(&b,ptr::null_mut()),Err(U::InvalidBuffer));
        assert_eq!(sid_at(&[1,255,0,0,0,0,0,0],0),Err(U::InvalidBuffer));
        let bytes=unsafe{std::slice::from_raw_parts_mut(b.raw().cast::<u8>(),32)};
        bytes[0]=1;bytes[3]=0x80;bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(validate_descriptor(&b),Err(U::InvalidBuffer));
    }

    #[test]
    fn group_and_privilege_counts_require_complete_aligned_arrays_with_fixed_caps() {
        assert_eq!(group_array_end(GROUP_CAP+1,TOKEN_CAP),Err(U::BoundExceeded));
        assert_eq!(group_array_end(1,mem::offset_of!(TOKEN_GROUPS,Groups)),Err(U::InvalidBuffer));
        let end=mem::offset_of!(TOKEN_GROUPS,Groups)+mem::size_of::<SID_AND_ATTRIBUTES>();
        assert_eq!(group_array_end(1,end),Ok(end));
        assert_eq!(validate_privileges(&[0;8],8),Ok(()));
        assert_eq!(validate_privileges(&[0;8],7),Err(U::InvalidBuffer));
        let mut short=[0u8;8];short[..4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(validate_privileges(&short,8),Err(U::InvalidBuffer));
        short[..4].copy_from_slice(&257u32.to_le_bytes());
        assert_eq!(validate_privileges(&short,8),Err(U::BoundExceeded));
    }

    #[test]
    fn object_mappings_keep_desktop_and_station_and_visible_masks_distinct() {
        let desktop=mapping(false,false);
        let visible=mapping(true,true);
        let noninteractive=mapping(true,false);
        assert_eq!(desktop.GenericRead,READ_CONTROL_MASK|0x41);
        assert_eq!(desktop.GenericWrite,READ_CONTROL_MASK|0xbe);
        assert_eq!(desktop.GenericExecute,READ_CONTROL_MASK|0x100);
        assert_eq!(desktop.GenericAll,STANDARD_REQUIRED|0x1ff);
        assert_eq!(visible.GenericAll,STANDARD_REQUIRED|0x37f);
        assert_eq!(noninteractive.GenericAll,STANDARD_REQUIRED|0x16f);
        assert_eq!(visible.GenericRead^noninteractive.GenericRead,0x200);
        assert_eq!(visible.GenericWrite^noninteractive.GenericWrite,0x10);
    }

    #[test]
    fn real_current_token_query_and_identification_duplicate_have_checked_normal_closes() {
        // Current test process only; not alternate-logon runner or business proof.
        let mut query=StartupQueries::default();
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_DUPLICATE,&mut query.primary.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        query.primary.captured(ok,error).unwrap();
        let facts=PrivateTokenFacts::read(query.primary.raw);
        assert!(facts.user.is_ok());assert!(facts.session.is_ok());
        let ok=unsafe{DuplicateToken(query.primary.raw,SecurityIdentification,&mut query.impersonation.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        query.impersonation.captured(ok,error).unwrap();
        assert!(query.unresolved());
        assert_eq!(query.states(),[QueryHandleClose::Held;2]);
        query.close_once().unwrap();
        assert!(!query.unresolved());
        assert_eq!(query.states(),[QueryHandleClose::ClosedVerified;2]);
        query.close_once().unwrap(); // Data-only idempotence; no second native close.
        assert!(facts.user.is_ok()); // Finite copied data survives all query handles.
    }

    #[test]
    fn injected_close_failure_keeps_debt_and_never_retries_the_original() {
        let mut slot=QuerySlot::default();
        let ok=unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut slot.raw)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        slot.captured(ok,error).unwrap();
        // Real fixture handle is actually closed once; inject only the returned
        // error to exercise uncertain-report retention without leaking a test token.
        assert_eq!(slot.close_with(|raw|{
            assert_ne!(unsafe{CloseHandle(raw)},0);Err(5)
        }).unwrap_err().raw_os_error(),Some(5));
        assert_eq!(slot.state(),QueryHandleClose::CloseFailed{code:5});
        assert_ne!(slot.raw,0);
        assert_eq!(slot.close_with(|_|panic!("must not reclose uncertain original"))
            .unwrap_err().raw_os_error(),Some(5));
        assert_eq!(slot.state(),QueryHandleClose::CloseFailed{code:5});
    }

    #[test]
    fn undefined_failed_output_is_debt_not_permission_to_close_a_numeric_handle() {
        // Scalar negative fixture: no Win32 call on this value.
        let mut slot=QuerySlot{raw:1234,..QuerySlot::default()};
        assert_eq!(slot.captured(0,5),Err(U::Win32{code:5}));
        assert!(slot.close_with(|_|panic!("undefined failed output must never be operated")).is_err());
        assert_eq!(slot.raw,1234);
    }
}
