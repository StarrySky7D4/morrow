//! Separate Source047 A1 S helper, under the same crate/workspace license.
//! Native fixed-slot/channel/subject code derived from frozen046
//! station_broker_native.rs SHA256 671dc679f8a82708222eb2e3d77703ba724b4a49cea8426498a1b74cb9fc09ff.
//! A0 original source/layout/same-image gate are unchanged. No diagnostic mapping,
//! caller token HANDLE transport, business witness or resource-only admission.
use crate::asuser_a1_protocol::*;
#[path="asuser_parent_native.rs"]
pub(crate) mod parent_native;
#[cfg(test)]
mod producer_default_tests {
    use super::*;use crate::asuser_a1_policy::Sid;
    #[test] fn helper_partial_pi_and_unwind_keep_both_outputs_in_the_preinstalled_originals() {
        for (process,thread) in [(44,0),(0,48),(44,48)] {
            let mut original=Originals::new();original.process.begin().unwrap();original.thread.begin().unwrap();
            original.pi.hProcess=process;original.pi.hThread=thread;original.pi.dwProcessId=11;original.pi.dwThreadId=13;
            assert_eq!(original.capture_child(false,123).unwrap_err().raw_os_error(),Some(123));
            assert_eq!((original.process.raw,original.thread.raw),(process,thread));
            assert_eq!((original.pi.hProcess,original.pi.hThread,original.pi.dwProcessId,original.pi.dwThreadId),(process,thread,11,13));
            assert_eq!(original.process.state,if process==0{HandleState::Rejected}else{HandleState::Uncertain});
            assert_eq!(original.thread.state,if thread==0{HandleState::Rejected}else{HandleState::Uncertain});
            assert!(!original.all_retired());assert!(original.process.begin().is_err());assert!(original.thread.begin().is_err());
        }
        let mut original=Originals::new();original.process.begin().unwrap();original.thread.begin().unwrap();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            original.pi.hProcess=44;original.pi.hThread=48;panic!("scripted after native output before capture");
        })).is_err());
        assert_eq!((original.pi.hProcess,original.pi.hThread),(44,48));
        assert_eq!((original.process.state,original.thread.state),(HandleState::InFlight,HandleState::InFlight));
        assert!(!original.all_retired());
        // State-only models: no native creator, alias, close or fallback ran.
    }
    fn acl(entries:&[(Sid,u32)])->Vec<u8>{
        let mut bytes=vec![2,0,0,0,entries.len() as u8,0,0,0];
        for (sid,mask) in entries{bytes.extend([0,0]);bytes.extend(((8+sid.bytes().len()) as u16).to_le_bytes());
            bytes.extend(mask.to_le_bytes());bytes.extend(sid.bytes());}
        let length=bytes.len() as u16;bytes[2..4].copy_from_slice(&length.to_le_bytes());bytes
    }
    #[test] fn trusted_producer_defaults_remain_distinct_from_private_t_and_deny_managed_nonadmins() {
        let caller=Sid::from_text("S-1-5-21-1-2-3-1001").unwrap();
        let managed=Sid::from_text("S-1-5-21-1-2-3-1002").unwrap();
        let logon=Sid::from_text("S-1-5-5-7-11").unwrap();let fresh=Sid::from_text("S-1-5-5-13-17").unwrap();
        let system=Sid::from_text("S-1-5-18").unwrap();let admin=Sid::from_text("S-1-5-32-544").unwrap();
        let everyone=Sid::from_text("S-1-1-0").unwrap();let ow=Sid::from_text("S-1-3-4").unwrap();
        let good=acl(&[(caller,0x10000000),(system,0x10000000),(admin,0x10000000),(logon,0x10000000)]);
        assert!(validate_trusted_default_acl(&good,caller,admin,logon).is_ok());
        assert!(crate::asuser_a1_security::default_objects(&good,caller,fresh).is_err());
        for trustee in [managed,fresh,everyone,ow] {
            let dangerous=acl(&[(caller,0x10000000),(trustee,0x10000000)]);
            assert!(validate_trusted_default_acl(&dangerous,caller,caller,logon).is_err());
        }
        assert!(validate_trusted_default_acl(&good,caller,managed,logon).is_err());
        assert!(validate_trusted_default_acl(&[],caller,caller,logon).is_err());
        assert!(validate_trusted_default_acl(&acl(&[]),caller,caller,logon).is_err());
        let mut flagged=good;flagged[9]=0x10;assert!(validate_trusted_default_acl(&flagged,caller,caller,logon).is_err());
    }
}
use std::io;
use std::{ffi::c_void,mem,ptr,time::{Duration,Instant}};
use windows_sys::Win32::Foundation::{CloseHandle,DuplicateHandle,GetLastError,HANDLE,FILETIME,LocalFree};
use windows_sys::Win32::Security::{GetTokenInformation,IsValidSecurityDescriptor,
    TokenUser,TokenStatistics,TokenIntegrityLevel,TokenElevation,TokenElevationType,TokenSessionId,
    TokenHasRestrictions,TokenIsAppContainer,TokenGroups,TokenRestrictedSids,TokenCapabilities,TokenPrivileges,
    TokenUIAccess,TokenMandatoryPolicy,TokenVirtualizationAllowed,TokenVirtualizationEnabled,TOKEN_GROUPS,TOKEN_PRIVILEGES,
    SID_AND_ATTRIBUTES,LUID_AND_ATTRIBUTES,TOKEN_USER,TOKEN_MANDATORY_LABEL,TOKEN_STATISTICS,TOKEN_ELEVATION,SECURITY_ATTRIBUTES};
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Storage::FileSystem::{CreateFileW,ReadFile,WriteFile};
use windows_sys::Win32::System::IO::{OVERLAPPED,GetOverlappedResult};
use windows_sys::Win32::System::Pipes::{CreateNamedPipeW,ConnectNamedPipe,CreatePipe,
    GetNamedPipeClientProcessId,GetNamedPipeServerProcessId,SetNamedPipeHandleState};
use windows_sys::Win32::System::Threading::{CreateEventW,ResetEvent,WaitForSingleObject,
    GetCurrentProcess,GetCurrentProcessId,GetCurrentThreadId,GetProcessId,GetProcessTimes,
    CreateProcessW,CreateThread,GetExitCodeProcess,GetExitCodeThread,QueryFullProcessImageNameW,
    OpenProcessToken,OpenThreadToken,GetCurrentThread,PROCESS_INFORMATION,STARTUPINFOEXW};
use windows_sys::Win32::System::StationsAndDesktops::{GetProcessWindowStation,GetThreadDesktop,
    SetProcessWindowStation,SetThreadDesktop,CreateWindowStationW,CreateDesktopW,
    CloseDesktop,CloseWindowStation,GetUserObjectInformationW};
use sha2::{Digest,Sha256};

use crate::asuser_a1_rights::{STATION_MANAGEMENT as STATION_ACCESS,DESKTOP_MANAGEMENT as DESKTOP_ACCESS,
    STATION_TARGET as TARGET_STATION,DESKTOP_TARGET as TARGET_DESKTOP};
const HELPER_SECONDS:u64=30;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum HandleState {NotAllocated,InFlight,Held,Rejected,Uncertain,ClosedVerified,CloseFailed{error:u32}}
#[derive(Debug)] struct NativeStatus(i32);
impl std::fmt::Display for NativeStatus {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"A1 native status {}",self.0)}}
impl std::error::Error for NativeStatus {}
// QPC is machine-wide and boot-stable; unlike an Instant's representation it
// can bind the SAME original parent budget across the private process boundary.
#[link(name="kernel32")]
unsafe extern "system" {
    fn QueryPerformanceCounter(value:*mut i64)->i32;
    fn QueryPerformanceFrequency(value:*mut i64)->i32;
}
fn counter()->io::Result<(u64,u64)> {
    let mut now=0;let mut frequency=0;
    check(unsafe{QueryPerformanceCounter(&mut now)})?;
    check(unsafe{QueryPerformanceFrequency(&mut frequency)})?;
    if now<=0 || frequency<=0 {return Err(bad());}Ok((now as u64,frequency as u64))
}
fn shared_remaining(end:u64,now:u64,frequency:u64)->io::Result<Duration> {
    if now==0 || frequency==0 || end.saturating_sub(now)<=1 {return Err(io::Error::new(io::ErrorKind::TimedOut,"A1 original shared budget expired"));}
    // Microsoft's cross-thread QPC ordering uncertainty is +/- one tick.
    let nanos=u128::from(end-now-1)*1_000_000_000/u128::from(frequency);
    if nanos==0 || nanos>u128::from(HELPER_SECONDS)*1_000_000_000 {return Err(bad());}
    Ok(Duration::from_nanos(nanos as u64))
}
fn shared_deadline(deadline:Instant)->io::Result<u64> {
    // Counter precedes the remaining-Instant sample, making this conservative.
    let (now,frequency)=counter()?;let remaining=deadline.checked_duration_since(Instant::now()).ok_or_else(bad)?;
    let ticks=remaining.as_nanos()*u128::from(frequency)/1_000_000_000;
    let ticks=u64::try_from(ticks).map_err(|_|bad())?.checked_sub(2).ok_or_else(bad)?;
    now.checked_add(ticks).filter(|end|*end>now).ok_or_else(bad)
}
fn bad()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"A1 original binding/operation rejected")}
fn os_error()->io::Error {io::Error::last_os_error()}
fn check(ok:i32)->io::Result<()> {if ok!=0 {Ok(())}else{Err(os_error())}}
fn wide(value:&str)->Vec<u16> {value.encode_utf16().chain(Some(0)).collect()}

#[derive(Clone,Copy)]
enum CloseKind {Kernel,Station,Desktop,Local}
struct Slot {raw:isize,state:HandleState,kind:CloseKind}
impl Slot {
    fn new(kind:CloseKind)->Self {Self {raw:0,state:HandleState::NotAllocated,kind}}
    fn begin(&mut self)->io::Result<()> {
        if self.state!=HandleState::NotAllocated {return Err(bad());}
        self.state=HandleState::InFlight;Ok(())
    }
    fn captured(&mut self,ok:bool,raw:isize,error:u32)->io::Result<()> {
        if self.state!=HandleState::InFlight {return Err(bad());}
        self.raw=raw;
        self.state=match (ok,raw!=0 && raw != -1) {
            (true,true)=>HandleState::Held,
            (false,false)=>HandleState::Rejected,
            _=>HandleState::Uncertain,
        };
        if self.state==HandleState::Held {Ok(())}
        else if !ok {Err(io::Error::from_raw_os_error(error as i32))}else{Err(bad())}
    }
    fn close_with(&mut self,close:impl FnOnce(isize)->io::Result<()>)->io::Result<()> {
        match self.state {
            HandleState::NotAllocated|HandleState::Rejected|HandleState::ClosedVerified=>return Ok(()),
            HandleState::Held=>{},_=>return Err(bad()),
        }
        self.state=HandleState::InFlight;
        match close(self.raw) {
            Ok(())=>{self.raw=0;self.state=HandleState::ClosedVerified;Ok(())},
            Err(e)=>{self.state=match e.raw_os_error(){Some(v)=>HandleState::CloseFailed{error:v as u32},
                None=>HandleState::Uncertain};Err(e)},
        }
    }
    fn close(&mut self)->io::Result<()> {
        let kind=self.kind;
        self.close_with(|raw|unsafe{match kind {
            CloseKind::Kernel=>check(CloseHandle(raw)),
            CloseKind::Station=>check(CloseWindowStation(raw)),
            CloseKind::Desktop=>check(CloseDesktop(raw)),
            CloseKind::Local=>if LocalFree(raw as *mut c_void).is_null(){Ok(())}else{Err(os_error())},
        }})
    }
    fn retired(&self)->bool {matches!(self.state,HandleState::NotAllocated|HandleState::Rejected|HandleState::ClosedVerified)}
}
// Both parent and helper create this entire finite set before the first OS
// allocation. Borrowed original associations are deliberately NOT in this set.
struct Originals {
    // Native writes directly into this preinstalled original allocation. A
    // false/partial return or unwind cannot strand PI outputs on a local stack.
    pi:PROCESS_INFORMATION,
    process:Slot,thread:Slot,stdout:Slot,stderr:Slot,child_out:Slot,child_err:Slot,
    nul:Slot,parent:Slot,own_token:Slot,peer_token:Slot,caller_thread_token:Slot,sd:Slot,desktop_sd:Slot,station:Slot,desktop:Slot,
    readers:[Slot;2],helper_process_sd:Slot,helper_thread_sd:Slot,
    gui_reads:[Slot;4],label_sd:Slot,
    primary_defaults:[Option<TrustedDefaults>;2],
}
impl Originals {
    fn new()->Self {Self {pi:unsafe{mem::zeroed()},process:Slot::new(CloseKind::Kernel),thread:Slot::new(CloseKind::Kernel),
        stdout:Slot::new(CloseKind::Kernel),stderr:Slot::new(CloseKind::Kernel),
        child_out:Slot::new(CloseKind::Kernel),child_err:Slot::new(CloseKind::Kernel),
        nul:Slot::new(CloseKind::Kernel),parent:Slot::new(CloseKind::Kernel),
        own_token:Slot::new(CloseKind::Kernel),peer_token:Slot::new(CloseKind::Kernel),caller_thread_token:Slot::new(CloseKind::Kernel),sd:Slot::new(CloseKind::Local),desktop_sd:Slot::new(CloseKind::Local),
        station:Slot::new(CloseKind::Station),desktop:Slot::new(CloseKind::Desktop),
        readers:[Slot::new(CloseKind::Kernel),Slot::new(CloseKind::Kernel)],
        helper_process_sd:Slot::new(CloseKind::Local),helper_thread_sd:Slot::new(CloseKind::Local),
        gui_reads:std::array::from_fn(|_|Slot::new(CloseKind::Local)),label_sd:Slot::new(CloseKind::Local),
        primary_defaults:std::array::from_fn(|_|None)}}
    fn capture_child(&mut self,ok:bool,error:u32)->io::Result<()> {
        // Always account for BOTH original outputs before propagating either
        // error. PI, including raw IDs/partial values, stays in the same owner.
        let process=self.process.captured(ok,self.pi.hProcess,error);
        let thread=self.thread.captured(ok,self.pi.hThread,error);process.and(thread)
    }
    fn all_retired(&self)->bool {
        [&self.process,&self.thread,&self.stdout,&self.stderr,&self.child_out,&self.child_err,
            &self.nul,&self.parent,&self.own_token,&self.peer_token,&self.caller_thread_token,&self.sd,&self.desktop_sd,&self.station,&self.desktop,
            &self.readers[0],&self.readers[1],&self.helper_process_sd,&self.helper_thread_sd,&self.label_sd,
            &self.gui_reads[0],&self.gui_reads[1],&self.gui_reads[2],&self.gui_reads[3]].iter().all(|v|v.retired())
    }
}

// Stable heap addresses survive pending I/O and unwind. A timeout never frees,
// cancels or reuses OVERLAPPED/event/buffer/pipe storage. Exactly two events.
struct Channel {
    pipe:Slot,read_event:Slot,write_event:Slot,read:OVERLAPPED,write:OVERLAPPED,
    input:[u8;FRAME_BYTES],output:[u8;FRAME_BYTES],read_pending:bool,write_pending:bool,
}
impl Channel {
    fn new()->Box<Self> {Box::new(Self {pipe:Slot::new(CloseKind::Kernel),
        read_event:Slot::new(CloseKind::Kernel),write_event:Slot::new(CloseKind::Kernel),
        read:unsafe{mem::zeroed()},write:unsafe{mem::zeroed()},input:[0;FRAME_BYTES],output:[0;FRAME_BYTES],
        read_pending:false,write_pending:false})}
    fn events(&mut self)->io::Result<()> {
        for slot in [&mut self.read_event,&mut self.write_event] {
            slot.begin()?;let raw=unsafe{CreateEventW(ptr::null(),1,0,ptr::null())};let error=if raw==0 {unsafe{GetLastError()}}else{0};
            slot.captured(raw!=0,raw,error)?;
        }
        self.read.hEvent=self.read_event.raw;self.write.hEvent=self.write_event.raw;Ok(())
    }
    fn connect_server(&mut self)->io::Result<()> {
        self.read_pending=true;
        let ok=unsafe{ConnectNamedPipe(self.pipe.raw,&mut self.read)};
        if ok!=0 {self.read_pending=false;return Ok(());}
        match unsafe{GetLastError()} {997=>Ok(()),535=>{self.read_pending=false;Ok(())},v=>Err(io::Error::from_raw_os_error(v as i32))}
    }
    fn completed(&mut self,read:bool,deadline:Instant)->io::Result<u32> {
        let (event,overlapped,pending)=if read {(&self.read_event,&mut self.read,&mut self.read_pending)}
            else{(&self.write_event,&mut self.write,&mut self.write_pending)};
        wait(event.raw,deadline)?;let mut bytes=0;
        check(unsafe{GetOverlappedResult(self.pipe.raw,overlapped,&mut bytes,0)})?;
        *pending=false;Ok(bytes)
    }
    fn connected(&mut self,deadline:Instant)->io::Result<()> {
        if self.read_pending {self.completed(true,deadline)?;}Ok(())
    }
    fn send(&mut self,frame:Frame,deadline:Instant)->io::Result<()> {
        remaining(deadline)?;
        if self.write_pending {return Err(bad());}check(unsafe{ResetEvent(self.write_event.raw)})?;
        self.write=unsafe{mem::zeroed()};self.write.hEvent=self.write_event.raw;
        self.output=encode(frame)?;self.write_pending=true;let mut bytes=0;
        let ok=unsafe{WriteFile(self.pipe.raw,self.output.as_ptr().cast(),FRAME_BYTES as u32,&mut bytes,&mut self.write)};
        if ok==0 {let error=unsafe{GetLastError()};if error!=997 {return Err(io::Error::from_raw_os_error(error as i32));}
            bytes=self.completed(false,deadline)?;}else{self.write_pending=false;}
        if bytes!=FRAME_BYTES as u32 {return Err(bad());}remaining(deadline)?;Ok(())
    }
    fn receive(&mut self,kind:u8,binding:Option<Binding>,deadline:Instant)->io::Result<Frame> {
        remaining(deadline)?;
        if self.read_pending {return Err(bad());}check(unsafe{ResetEvent(self.read_event.raw)})?;
        self.read=unsafe{mem::zeroed()};self.read.hEvent=self.read_event.raw;
        self.input.fill(0);self.read_pending=true;let mut bytes=0;
        let ok=unsafe{ReadFile(self.pipe.raw,self.input.as_mut_ptr().cast(),FRAME_BYTES as u32,&mut bytes,&mut self.read)};
        if ok==0 {let error=unsafe{GetLastError()};if error!=997 {return Err(io::Error::from_raw_os_error(error as i32));}
            bytes=self.completed(true,deadline)?;}else{self.read_pending=false;}
        if bytes!=FRAME_BYTES as u32 {return Err(bad());}remaining(deadline)?;decode(&self.input,kind,binding)
    }
    fn close(&mut self)->io::Result<()> {
        if !self.can_close() {return Err(bad());}
        self.pipe.close()?;self.read_event.close()?;self.write_event.close()
    }
    fn can_close(&self)->bool {!self.read_pending && !self.write_pending}
    fn retired(&self)->bool {!self.read_pending && !self.write_pending && self.pipe.retired() && self.read_event.retired() && self.write_event.retired()}
}
fn remaining(deadline:Instant)->io::Result<u32> {
    let now=Instant::now();if now>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"A1 original deadline"));}
    Ok(deadline.saturating_duration_since(now).as_millis().min(30_000) as u32)
}
fn wait(raw:isize,deadline:Instant)->io::Result<()> {
    let ms=remaining(deadline)?;
    match unsafe{WaitForSingleObject(raw,ms)} {0=>{remaining(deadline)?;Ok(())},258=>Err(io::Error::new(io::ErrorKind::TimedOut,"A1 original native wait expired")),_=>Err(os_error())}
}
fn identity(raw:isize)->io::Result<(u32,u64)> {
    let pid=unsafe{GetProcessId(raw)};if pid==0 {return Err(os_error());}
    let mut created:FILETIME=unsafe{mem::zeroed()};let mut ended=created;let mut kernel=created;let mut user=created;
    check(unsafe{GetProcessTimes(raw,&mut created,&mut ended,&mut kernel,&mut user)})?;
    let time=(u64::from(created.dwHighDateTime)<<32)|u64::from(created.dwLowDateTime);
    if time==0 {return Err(bad());}Ok((pid,time))
}
fn image_path(raw:isize)->io::Result<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut b=[0u16;32768];let mut len=b.len() as u32;
    check(unsafe{QueryFullProcessImageNameW(raw,0,b.as_mut_ptr(),&mut len)})?;
    if len==0 || len as usize>=b.len() {return Err(bad());}
    Ok(std::ffi::OsString::from_wide(&b[..len as usize]).into())
}
// Documented native ProcessBasicInformation is read on the CURRENT helper only,
// never OpenProcess by a protocol PID. It corroborates the inherited original.
#[repr(C)]
struct BasicProcess {exit:i32,peb:*mut c_void,affinity:usize,priority:i32,pid:usize,parent:usize}
fn current_creator()->io::Result<u32> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW,GetProcAddress};
    // Borrow the already-loaded system module; do not acquire a new loader
    // reference or guess an alternate implementation when the export is absent.
    let module=unsafe{GetModuleHandleW(wide("ntdll.dll").as_ptr())};if module==0 {return Err(os_error());}
    let address=unsafe{GetProcAddress(module,b"NtQueryInformationProcess\0".as_ptr())}.ok_or_else(bad)?;
    type Query=unsafe extern "system" fn(HANDLE,u32,*mut c_void,u32,*mut u32)->i32;
    let query:Query=unsafe{mem::transmute(address)};
    let mut b:BasicProcess=unsafe{mem::zeroed()};let mut returned=0;
    let status=unsafe{query(GetCurrentProcess(),0,(&mut b as *mut BasicProcess).cast(),mem::size_of::<BasicProcess>() as u32,&mut returned)};
    if status<0 {return Err(io::Error::other(NativeStatus(status)));}
    if returned as usize!=mem::size_of::<BasicProcess>() || b.pid!=unsafe{GetCurrentProcessId()} as usize {return Err(bad());}
    u32::try_from(b.parent).map_err(|_|bad())
}
const TOKEN_ELEVATION_BYTES:u32=4;
const _: [();TOKEN_ELEVATION_BYTES as usize]=[();mem::size_of::<TOKEN_ELEVATION>()];
struct TokenQueryReply {ok:bool,returned:u32,error:u32}
fn token_buffer(token:isize,class:i32)->io::Result<(Vec<u64>,usize)> {
    token_buffer_with(class,|buffer,capacity| {
        let mut returned=0;
        let ok=unsafe{GetTokenInformation(token,class,buffer,capacity,&mut returned)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0}; // Capture before other work.
        TokenQueryReply {ok:ok!=0,returned,error}
    })
}
/// Private synchronous query seam: production captures this SAME original token;
/// pure tests supply replies without a HANDLE/WinAPI. Never stored or retried.
fn token_buffer_with(class:i32,mut query:impl FnMut(*mut c_void,u32)->TokenQueryReply)
    ->io::Result<(Vec<u64>,usize)> {
    // TOKEN_ELEVATION is one DWORD. Select this contract UP FRONT, never probe
    // NULL and then fall back on ERROR_BAD_LENGTH/another native failure.
    if class==TokenElevation {
        let mut b=vec![0u64;1]; // Preserve existing aligned words + actual len4.
        let reply=query(b.as_mut_ptr().cast(),TOKEN_ELEVATION_BYTES);
        if !reply.ok {return Err(io::Error::from_raw_os_error(reply.error as i32));}
        if reply.returned!=TOKEN_ELEVATION_BYTES {return Err(bad());}
        return Ok((b,TOKEN_ELEVATION_BYTES as usize));
    }
    let probe=query(ptr::null_mut(),0);
    if probe.ok {return Err(bad());}
    if probe.error!=122 {return Err(io::Error::from_raw_os_error(probe.error as i32));}
    let needed=probe.returned;
    if needed==0 || needed>65536 {return Err(bad());}
    let mut b=vec![0u64;(needed as usize).div_ceil(8)];
    let fill=query(b.as_mut_ptr().cast(),needed);
    if !fill.ok {return Err(io::Error::from_raw_os_error(fill.error as i32));}
    if fill.returned==0 || fill.returned>needed {return Err(bad());}
    Ok((b,fill.returned as usize))
}
fn no_impersonation(slot:&mut Slot)->io::Result<()> {
    slot.begin()?;
    let ok=unsafe{OpenThreadToken(GetCurrentThread(),8,1,&mut slot.raw)};
    let error=if ok==0 {unsafe{GetLastError()}}else{0};
    let captured=slot.captured(ok!=0,slot.raw,error);
    if ok==0 && error==1008 && slot.state==HandleState::Rejected && slot.raw==0 {
        return Ok(()); // Actual ERROR_NO_TOKEN, never an assumed empty context.
    }
    captured?;
    // Do not launch with the stronger primary token of an impersonating caller.
    // Preserve the actual opened thread-token alias on this Unknown attempt.
    Err(io::Error::new(io::ErrorKind::Unsupported,"A1 impersonating caller unsupported"))
}
fn sid_from_token_buffer(words:&[u64],len:usize,sid:*mut c_void,minimum:usize)->io::Result<Vec<u8>> {
    let base=words.as_ptr() as usize;let start=(sid as usize).checked_sub(base).ok_or_else(bad)?;
    if len>mem::size_of_val(words) || start<minimum {return Err(bad());}
    let bytes=unsafe{std::slice::from_raw_parts(words.as_ptr().cast::<u8>(),len)};
    let head=bytes.get(start..start.checked_add(8).ok_or_else(bad)?).ok_or_else(bad)?;
    if start%4!=0 || head[0]!=1 || head[1]>15 {return Err(bad());}
    let end=start.checked_add(8+head[1] as usize*4).ok_or_else(bad)?;
    Ok(bytes.get(start..end).ok_or_else(bad)?.to_vec())
}
pub(super) struct Subject {
    pub(super) hash:[u8;32],pub(super) security_context:[u8;32],pub(super) sid:Vec<u8>,pub(super) integrity:u32,
    pub(super) privileges:Vec<(u32,i32,u32)>,
}
fn subject(process:isize,slot:&mut Slot,defaults:&mut Option<TrustedDefaults>)->io::Result<([u8;32],Vec<u8>,u32)> {
    slot.begin()?;let ok=unsafe{OpenProcessToken(process,8,&mut slot.raw)};let error=if ok==0 {unsafe{GetLastError()}}else{0};
    slot.captured(ok!=0,slot.raw,error)?;
    let before=stable_statistics(slot.raw)?;let result=retained_subject(slot.raw)?;
    *defaults=Some(trusted_defaults(slot.raw,&result.sid)?);
    if stable_statistics(slot.raw)?!=before{return Err(bad());}slot.close()?;
    Ok((result.hash,result.sid,result.integrity))
}
pub(super) fn stable_statistics(token:isize)->io::Result<[u64;4]> {
    let (bytes,length)=token_buffer(token,TokenStatistics)?;
    if length!=mem::size_of::<TOKEN_STATISTICS>(){return Err(bad());}
    let stats=unsafe{ptr::read(bytes.as_ptr().cast::<TOKEN_STATISTICS>())};
    let luid=|v:windows_sys::Win32::Foundation::LUID|(u64::from(v.HighPart as u32)<<32)|u64::from(v.LowPart);
    Ok([luid(stats.TokenId),luid(stats.AuthenticationId),luid(stats.ModifiedId),stats.TokenType as u64])
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct TrustedDefaults {pub(super) owner:crate::asuser_a1_policy::Sid,
    pub(super) logon:crate::asuser_a1_policy::Sid,pub(super) acl:Vec<u8>}
/// P/C/S only: read actual primary TokenOwner/default ACL. Trusted current
/// high user, SYSTEM and built-in Administrators may manage their own objects;
/// an arbitrary managed nonadmin/fresh session/Everyone may not. This is NOT
/// the private-T three-RC-ACE policy and never mutates the producer's token.
pub(super) fn trusted_defaults(token:isize,user:&[u8])->io::Result<TrustedDefaults> {
    use windows_sys::Win32::Security::{TokenOwner,TokenDefaultDacl,TOKEN_OWNER,TOKEN_DEFAULT_DACL};
    let sid=crate::asuser_a1_policy::Sid::from_bytes(user)?;
    let system=crate::asuser_a1_policy::Sid::from_bytes(&[1,1,0,0,0,0,0,5,18,0,0,0])?;
    let admin=crate::asuser_a1_policy::Sid::from_bytes(&[1,2,0,0,0,0,0,5,32,0,0,0,32,2,0,0])?;
    if sid.is_logon()||sid==system||sid==admin{return Err(bad());}
    let (groups,length)=token_buffer(token,TokenGroups)?;
    if length<4{return Err(bad());}let count=groups[0] as u32 as usize;
    let start=mem::offset_of!(TOKEN_GROUPS,Groups);let stride=mem::size_of::<SID_AND_ATTRIBUTES>();
    let end=start.checked_add(count.checked_mul(stride).ok_or_else(bad)?).ok_or_else(bad)?;
    if count>256||end>length{return Err(bad());}let mut logon=None;
    for index in 0..count {
        let group=unsafe{ptr::read((groups.as_ptr() as *const u8).add(start+index*stride).cast::<SID_AND_ATTRIBUTES>())};
        let group_sid=crate::asuser_a1_policy::Sid::from_bytes(&sid_from_token_buffer(&groups,length,group.Sid,end)?)?;
        if group_sid.is_logon()||group.Attributes&0xc0000000!=0 {
            if logon.is_some()||!group_sid.is_logon()||group.Attributes&0xc0000004!=0xc0000004
                ||group.Attributes&0x10!=0{return Err(bad());}logon=Some(group_sid);
        }
    }
    let logon=logon.ok_or_else(bad)?;
    let (words,length)=token_buffer(token,TokenOwner)?;
    if length<mem::size_of::<TOKEN_OWNER>(){return Err(bad());}
    let raw=unsafe{ptr::read(words.as_ptr().cast::<TOKEN_OWNER>())};
    let owner=crate::asuser_a1_policy::Sid::from_bytes(&sid_from_token_buffer(&words,length,raw.Owner,mem::size_of::<TOKEN_OWNER>())?)?;
    if ![sid,system,admin].contains(&owner){return Err(bad());}
    let (words,length)=token_buffer(token,TokenDefaultDacl)?;
    if length<mem::size_of::<TOKEN_DEFAULT_DACL>(){return Err(bad());}
    let raw=unsafe{ptr::read(words.as_ptr().cast::<TOKEN_DEFAULT_DACL>())};
    let offset=(raw.DefaultDacl as usize).checked_sub(words.as_ptr() as usize).ok_or_else(bad)?;
    if offset<mem::size_of::<TOKEN_DEFAULT_DACL>()||offset%4!=0{return Err(bad());}
    let bytes=unsafe{std::slice::from_raw_parts(words.as_ptr().cast::<u8>(),length)};
    let head=bytes.get(offset..offset.checked_add(8).ok_or_else(bad)?).ok_or_else(bad)?;
    let size=u16::from_le_bytes(head[2..4].try_into().map_err(|_|bad())?) as usize;
    let acl=bytes.get(offset..offset.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
    validate_trusted_default_acl(acl,sid,owner,logon)?;
    Ok(TrustedDefaults{owner,logon,acl:acl.to_vec()})
}
fn validate_trusted_default_acl(acl:&[u8],sid:crate::asuser_a1_policy::Sid,
    owner:crate::asuser_a1_policy::Sid,logon:crate::asuser_a1_policy::Sid)->io::Result<()> {
    use crate::asuser_a1_policy::Sid;
    let system=Sid::from_bytes(&[1,1,0,0,0,0,0,5,18,0,0,0])?;
    let admin=Sid::from_bytes(&[1,2,0,0,0,0,0,5,32,0,0,0,32,2,0,0])?;
    let owner_rights=Sid::from_bytes(&[1,1,0,0,0,0,0,3,4,0,0,0])?;
    if sid.is_logon()||!logon.is_logon()||![sid,system,admin].contains(&owner){return Err(bad());}
    crate::asuser_a1_security::default_layout(acl)?;
    let count=u16::from_le_bytes(acl[4..6].try_into().map_err(|_|bad())?) as usize;
    if count==0{return Err(bad());}let mut at=8;
    for _ in 0..count {
        let size=u16::from_le_bytes(acl[at+2..at+4].try_into().map_err(|_|bad())?) as usize;
        let trustee=crate::asuser_a1_policy::Sid::from_bytes(&acl[at+8..at+size])?;
        let mask=u32::from_le_bytes(acl[at+4..at+8].try_into().map_err(|_|bad())?);
        if acl[at+1]!=0||acl[at]==0&&!([sid,system,admin,logon].contains(&trustee)
            ||trustee==owner_rights&&mask==crate::asuser_a1_rights::OWNER_READ_CONTROL){return Err(bad());}
        at+=size;
    }
    Ok(())
}
pub(super) fn retained_subject(token:isize)->io::Result<Subject> {
    let (b,len)=token_buffer(token,TokenUser)?;if len<mem::size_of::<TOKEN_USER>() {return Err(bad());}
    let user=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_USER>())};let sid=sid_from_token_buffer(&b,len,user.User.Sid,mem::size_of::<TOKEN_USER>())?;
    let mut hash=Sha256::new();hash.update(&sid);
    let (b,len)=token_buffer(token,TokenStatistics)?;if len<mem::size_of::<TOKEN_STATISTICS>() {return Err(bad());}
    let stats=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_STATISTICS>())};
    hash.update(stats.AuthenticationId.LowPart.to_le_bytes());hash.update(stats.AuthenticationId.HighPart.to_le_bytes());
    let (b,len)=token_buffer(token,TokenIntegrityLevel)?;if len<mem::size_of::<TOKEN_MANDATORY_LABEL>() {return Err(bad());}
    let label=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_MANDATORY_LABEL>())};
    let integrity_sid=sid_from_token_buffer(&b,len,label.Label.Sid,mem::size_of::<TOKEN_MANDATORY_LABEL>())?;
    if integrity_sid.len()!=12||integrity_sid[..8]!=[1,1,0,0,0,0,0,16]{return Err(bad());}
    let integrity=u32::from_le_bytes(integrity_sid[8..12].try_into().map_err(|_|bad())?);hash.update(integrity_sid);
    for class in [TokenElevation,TokenElevationType,TokenSessionId,TokenIsAppContainer,
        TokenUIAccess,TokenMandatoryPolicy,TokenVirtualizationAllowed,TokenVirtualizationEnabled] {
        let (b,len)=token_buffer(token,class)?;if len!=4 {return Err(bad());}
        // This ordinary-caller A1 cannot equate distinct app-container SIDs.
        // Reject rather than omit that identity dimension or gift a capability.
        if class==TokenIsAppContainer && b[0] as u32!=0 {
            return Err(io::Error::new(io::ErrorKind::Unsupported,"A1 app-container caller unsupported"));
        }
        hash.update((b[0] as u32).to_le_bytes());
    }
    let (b,len)=token_buffer(token,TokenHasRestrictions)?;
    let restricted=match len {1=>b[0]&255!=0,4=>b[0] as u32!=0,_=>return Err(bad())};
    let mut security_context=hash.clone();
    hash.update([u8::from(restricted)]);
    // Same SID/LUID/elevation alone is insufficient when groups or privileges
    // were removed without a different integrity level. Compare finite actual
    // normalized sets, not an administrator-name or permission boolean.
    for class in [TokenGroups,TokenRestrictedSids,TokenCapabilities] {
    let (b,len)=token_buffer(token,class)?;if len<4 {return Err(bad());}let count=b[0] as u32 as usize;
    let offset=mem::offset_of!(TOKEN_GROUPS,Groups);
    if count>256 {return Err(bad());}
    let array_end=offset.checked_add(count*mem::size_of::<SID_AND_ATTRIBUTES>()).ok_or_else(bad)?;
    if count!=0 && array_end>len {return Err(bad());}
    let mut groups=Vec::with_capacity(count);
    for i in 0..count {
        let entry=unsafe{ptr::read((b.as_ptr() as *const u8).add(offset+i*mem::size_of::<SID_AND_ATTRIBUTES>()).cast::<SID_AND_ATTRIBUTES>())};
        groups.push((sid_from_token_buffer(&b,len,entry.Sid,array_end)?,entry.Attributes));
    }
    groups.sort();hash.update((groups.len() as u32).to_le_bytes());
    security_context.update((groups.len() as u32).to_le_bytes());
    for (sid,attributes) in groups {
        for digest in [&mut hash,&mut security_context] {
            digest.update((sid.len() as u32).to_le_bytes());digest.update(&sid);digest.update(attributes.to_le_bytes());
        }
    }
    }
    let (b,len)=token_buffer(token,TokenPrivileges)?;if len<4 {return Err(bad());}let count=b[0] as u32 as usize;
    let offset=mem::offset_of!(TOKEN_PRIVILEGES,Privileges);
    if count>256 || offset.checked_add(count*mem::size_of::<LUID_AND_ATTRIBUTES>()).is_none_or(|v|v>len) {return Err(bad());}
    let mut privileges=Vec::with_capacity(count);
    for i in 0..count {
        let p=unsafe{ptr::read((b.as_ptr() as *const u8).add(offset+i*mem::size_of::<LUID_AND_ATTRIBUTES>()).cast::<LUID_AND_ATTRIBUTES>())};
        privileges.push((p.Luid.LowPart,p.Luid.HighPart,p.Attributes));
    }
    privileges.sort();hash.update((privileges.len() as u32).to_le_bytes());
    for (low,high,attributes) in &privileges {hash.update(low.to_le_bytes());hash.update(high.to_le_bytes());hash.update(attributes.to_le_bytes());}
    Ok(Subject {hash:hash.finalize().into(),security_context:security_context.finalize().into(),sid,integrity,privileges})
}
fn sid_text(sid:&[u8])->io::Result<String> {
    if sid.len()<8 || sid[0]!=1 || sid.len()!=8+sid[1] as usize*4 {return Err(bad());}
    let authority=sid[2..8].iter().fold(0u64,|a,b|(a<<8)|u64::from(*b));
    let mut text=format!("S-1-{authority}");
    for sub in sid[8..].chunks_exact(4) {text.push_str(&format!("-{}",u32::from_le_bytes(sub.try_into().unwrap())));}
    Ok(text)
}
fn attributes(sd:isize,inherit:bool)->SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {nLength:mem::size_of::<SECURITY_ATTRIBUTES>() as u32,lpSecurityDescriptor:sd as *mut c_void,bInheritHandle:i32::from(inherit)}
}
fn object_text(raw:isize,class:i32)->io::Result<String> {
    let mut text=[0u16;512];let mut bytes=0;
    check(unsafe{GetUserObjectInformationW(raw,class,text.as_mut_ptr().cast(),1024,&mut bytes)})?;
    let n=bytes as usize/2;if bytes%2!=0 || n==0 || n>512 || text[n-1]!=0 || text[..n-1].contains(&0) {return Err(bad());}
    String::from_utf16(&text[..n-1]).map_err(|_|bad())
}
fn gui_sd(slot:&mut Slot,caller:&[u8],logon:&[u8],desktop:bool)->io::Result<()> {
    let caller=crate::asuser_a1_policy::Sid::from_bytes(caller)?;
    let logon=crate::asuser_a1_policy::Sid::from_bytes(logon)?;
    if caller.is_logon()||!logon.is_logon()||caller==logon{return Err(bad());}
    let c=if desktop{DESKTOP_ACCESS}else{STATION_ACCESS};
    let t=if desktop{TARGET_DESKTOP}else{TARGET_STATION};
    // Legal owner is actual S TokenUser, independent of TokenOwner group.
    // DACL is protected from birth. Label is observed, then only the selected
    // one-shot High-to-Medium initialization is allowed before OFFER/R.
    let text=wide(&format!("O:{}D:P(A;;0x{c:x};;;{})(A;;0x{t:x};;;{})(A;;RC;;;OW)",caller.text(),caller.text(),logon.text()));
    slot.begin()?;let mut raw=ptr::null_mut();
    let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut raw,ptr::null_mut())};
    let error=if ok==0{unsafe{GetLastError()}}else{0};slot.captured(ok!=0,raw as isize,error)
}
fn allocation_bytes(slot:&Slot)->io::Result<&[u8]> {
    if slot.state!=HandleState::Held{return Err(bad());}
    let base=slot.raw as *mut c_void;
    let size=unsafe{windows_sys::Win32::System::Memory::LocalSize(base)};
    if size<20||size>65536{return Err(bad());}
    if unsafe{windows_sys::Win32::Security::IsValidSecurityDescriptor(base)}==0{return Err(bad());}
    let length=unsafe{windows_sys::Win32::Security::GetSecurityDescriptorLength(base)} as usize;
    if length<20||length>size{return Err(bad());}
    Ok(unsafe{std::slice::from_raw_parts(base.cast::<u8>(),length)})
}
fn gui_observation(slot:&mut Slot,raw:isize,caller:&[u8],logon:&[u8],desktop:bool)
    ->io::Result<crate::asuser_a1_security::GuiReadback> {
    use windows_sys::Win32::Security::{OWNER_SECURITY_INFORMATION,GROUP_SECURITY_INFORMATION,
        DACL_SECURITY_INFORMATION,LABEL_SECURITY_INFORMATION};
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo,SE_WINDOW_OBJECT};
    slot.begin()?;
    let mut base=ptr::null_mut();
    let status=unsafe{GetSecurityInfo(raw,SE_WINDOW_OBJECT,OWNER_SECURITY_INFORMATION|GROUP_SECURITY_INFORMATION
        |DACL_SECURITY_INFORMATION|LABEL_SECURITY_INFORMATION,ptr::null_mut(),ptr::null_mut(),
        ptr::null_mut(),ptr::null_mut(),&mut base)};
    // Status is the returned DWORD. Never GetLastError, full-SACL fallback or an
    // interior pointer as an allocation owner. Unexpected output is retained.
    slot.captured(status==0,base as isize,status)?;
    let observed=crate::asuser_a1_security::gui_observed(allocation_bytes(slot)?,
        crate::asuser_a1_policy::Sid::from_bytes(caller)?,crate::asuser_a1_policy::Sid::from_bytes(logon)?,desktop)?;
    slot.close()?;Ok(observed)
}
fn gui_descriptor(slot:&mut Slot,raw:isize,caller:&[u8],logon:&[u8],desktop:bool)->io::Result<[u8;32]> {
    let observed=gui_observation(slot,raw,caller,logon,desktop)?;
    if !observed.label.medium(){return Err(bad());}Ok(observed.digest())
}

struct Helper {
    originals:Originals,channel:Box<Channel>,deadline:Instant,
    old_station:isize,old_desktop:isize,thread:u32,shared:Option<u64>,
    allocation_attempted:bool,first_error:Option<io::Error>,
    parent_pin:Option<crate::MatchedRunnerArtifact>,own_pin:Option<crate::MatchedRunnerArtifact>,
    labels:[crate::asuser_a1_security::LabelAttempt;2],
}
impl Helper {
    fn new()->Self {Self{originals:Originals::new(),channel:Channel::new(),
        deadline:Instant::now()+Duration::from_secs(HELPER_SECONDS),old_station:0,old_desktop:0,
        thread:0,shared:None,allocation_attempted:false,first_error:None,parent_pin:None,own_pin:None,
        labels:std::array::from_fn(|_|crate::asuser_a1_security::LabelAttempt::new())}}
    fn checkpoint(&self)->io::Result<()> {
        remaining(self.deadline)?;
        if let Some(end)=self.shared {let(now,freq)=counter()?;shared_remaining(end,now,freq)?;}Ok(())
    }

    fn initialize_gui(&mut self,desktop:bool,caller:&[u8],logon:&[u8])->io::Result<[u8;32]> {
        use windows_sys::Win32::Security::Authorization::{SetSecurityInfo,SE_WINDOW_OBJECT};
        use windows_sys::Win32::Security::{ACL,LABEL_SECURITY_INFORMATION};
        let index=usize::from(desktop);let read=index*2;
        let raw=if desktop{self.originals.desktop.raw}else{self.originals.station.raw};
        let before=gui_observation(&mut self.originals.gui_reads[read],raw,caller,logon,desktop)?;
        self.checkpoint()?;
        if !self.labels[index].observe(before)? {return Ok(before.digest());}
        // Only this S's one newly-created object in the create-only station is
        // eligible. No R exists before this helper's verified CLOSED/native exit.
        if self.originals.station.state!=HandleState::Held
            ||desktop&&self.originals.desktop.state!=HandleState::Held {return Err(bad());}
        if self.originals.label_sd.state==HandleState::NotAllocated {
            self.originals.label_sd.begin()?;
            let text=wide("S:(ML;;NW;;;ME)");let mut base=ptr::null_mut();
            let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut base,ptr::null_mut())};
            let error=if ok==0{unsafe{GetLastError()}}else{0};
            self.originals.label_sd.captured(ok!=0,base as isize,error)?;
        }
        let bytes=allocation_bytes(&self.originals.label_sd)?;
        let offset=crate::asuser_a1_security::medium_sacl_offset(bytes)?;
        let sacl=unsafe{bytes.as_ptr().add(offset).cast::<ACL>()};
        self.checkpoint()?;self.labels[index].begin()?;
        let status=unsafe{SetSecurityInfo(raw,SE_WINDOW_OBJECT,LABEL_SECURITY_INFORMATION,
            ptr::null_mut(),ptr::null_mut(),ptr::null(),sacl)};
        self.labels[index].returned(status)?;self.checkpoint()?;
        let after=gui_observation(&mut self.originals.gui_reads[read+1],raw,caller,logon,desktop)?;
        self.labels[index].verify(after)?;self.checkpoint()?;Ok(after.digest())
    }

    fn run(&mut self,endpoint:&str,parent_inherited:isize,table:RoleTable)->io::Result<()> {
        let mut flags=0;
        check(unsafe{windows_sys::Win32::Foundation::GetHandleInformation(parent_inherited,&mut flags)})?;
        if flags&1==0{return Err(bad());}
        let creator=current_creator()?;let parent_id=identity(parent_inherited)?;
        if creator==0||creator!=parent_id.0||unsafe{WaitForSingleObject(parent_inherited,0)}!=258{return Err(bad());}
        // Compiled S-only C digest plus original kernel creating-parent handle.
        // This is BEFORE reading/authorizing HELLO; HELLO's own MAC is not trust.
        self.parent_pin=Some(crate::MatchedRunnerArtifact::acquire(&image_path(parent_inherited)?,table.c)
            .map_err(|_|bad())?);
        self.checkpoint()?;
        self.allocation_attempted=true;
        no_impersonation(&mut self.originals.caller_thread_token)?;
        self.originals.parent.begin()?;self.originals.parent.captured(true,parent_inherited,0)?;
        check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(parent_inherited,1,0)})?;
        self.channel.pipe.begin()?;
        let raw=unsafe{CreateFileW(wide(endpoint).as_ptr(),0xc0000000,0,ptr::null(),3,0x40100000,0)};
        let error=if raw == -1{unsafe{GetLastError()}}else{0};
        self.channel.pipe.captured(raw != -1,raw,error)?;
        let mut server=0;check(unsafe{GetNamedPipeServerProcessId(raw,&mut server)})?;
        if server!=creator{return Err(bad());}
        let (own,caller_sid,own_il)=subject(unsafe{GetCurrentProcess()},&mut self.originals.own_token,&mut self.originals.primary_defaults[0])?;
        let (peer,_,peer_il)=subject(parent_inherited,&mut self.originals.peer_token,&mut self.originals.primary_defaults[1])?;
        if peer_il!=0x3000{return Err(bad());}
        if own!=peer{return Err(bad());}self.checkpoint()?;
        let mode=2u32;check(unsafe{SetNamedPipeHandleState(raw,&mode,ptr::null(),ptr::null())})?;
        self.channel.events()?;
        let hello=self.channel.receive(HELLO,None,self.deadline)?;
        let binding=hello.binding;
        if endpoint!=crate::asuser_a1_protocol::endpoint(&binding.nonce)
            ||(binding.c_pid,binding.c_time)!=parent_id
            ||(binding.s_pid,binding.s_time)!=identity(unsafe{GetCurrentProcess()})?
            ||binding.c_image!=table.c||binding.r_image!=table.r{return Err(bad());}
        self.own_pin=Some(crate::MatchedRunnerArtifact::acquire(&image_path(unsafe{GetCurrentProcess()})?,binding.s_image)
            .map_err(|_|bad())?);
        let(now,freq)=counter()?;
        self.deadline=self.deadline.min(Instant::now()+shared_remaining(binding.deadline_ticks,now,freq)?);
        self.shared=Some(binding.deadline_ticks);self.checkpoint()?;
        self.adopt_stdio()?;
        self.thread=unsafe{GetCurrentThreadId()};self.old_station=unsafe{GetProcessWindowStation()};
        self.old_desktop=unsafe{GetThreadDesktop(self.thread)};
        if self.old_station==0||self.old_desktop==0{return Err(os_error());}
        let digest=Sha256::digest(binding.nonce);
        let suffix=digest[..16].iter().map(|b|format!("{b:02x}")).collect::<String>();
        let station=format!("C28StationA1-{}-{suffix}",binding.generation);
        let desktop=format!("C28DesktopA1-{suffix}");
        let logon=&binding.logon_sid[..usize::from(binding.sid_len)];
        gui_sd(&mut self.originals.sd,&caller_sid,logon,false)?;self.checkpoint()?;
        let sa=attributes(self.originals.sd.raw,false);self.originals.station.begin()?;
        let raw=unsafe{CreateWindowStationW(wide(&station).as_ptr(),windows_sys::Win32::UI::WindowsAndMessaging::CWF_CREATE_ONLY,STATION_ACCESS,&sa)};
        let error=if raw==0{unsafe{GetLastError()}}else{0};self.originals.station.captured(raw!=0,raw,error)?;
        check(unsafe{SetProcessWindowStation(raw)})?;self.checkpoint()?;
        gui_sd(&mut self.originals.desktop_sd,&caller_sid,logon,true)?;self.checkpoint()?;
        let sa=attributes(self.originals.desktop_sd.raw,false);self.originals.desktop.begin()?;
        let raw=unsafe{CreateDesktopW(wide(&desktop).as_ptr(),ptr::null(),ptr::null_mut(),0,DESKTOP_ACCESS,&sa)};
        let error=if raw==0{unsafe{GetLastError()}}else{0};self.originals.desktop.captured(raw!=0,raw,error)?;
        self.checkpoint()?;
        if object_text(self.originals.station.raw,2)?!=station||object_text(self.originals.desktop.raw,2)?!=desktop{return Err(bad());}
        let station_sd=self.initialize_gui(false,&caller_sid,logon)?;
        let desktop_sd=self.initialize_gui(true,&caller_sid,logon)?;
        self.originals.label_sd.close()?;self.checkpoint()?;
        let offer=Frame{kind:OFFER,binding,station:self.originals.station.raw as u64,desktop:self.originals.desktop.raw as u64,
            station_sd,desktop_sd};
        self.originals.desktop_sd.close()?;self.originals.sd.close()?;self.checkpoint()?;
        self.channel.send(offer,self.deadline)?;
        // Parent owns original S hProcess; numeric offer fields alone confer no
        // DuplicateHandle right. It must verify its pulled aliases before ACCEPT.
        self.channel.receive(ACCEPT,Some(binding),self.deadline)?;self.checkpoint()?;
        check(unsafe{SetProcessWindowStation(self.old_station)})?;
        check(unsafe{SetThreadDesktop(self.old_desktop)})?;
        if unsafe{GetProcessWindowStation()}!=self.old_station
            ||unsafe{GetThreadDesktop(self.thread)}!=self.old_desktop{return Err(bad());}
        self.originals.desktop.close()?;self.checkpoint()?;
        self.originals.station.close()?;self.checkpoint()?;
        self.originals.parent.close()?;self.checkpoint()?;
        self.channel.send(message(CLOSED,binding),self.deadline)?;self.checkpoint()?;
        self.channel.close()?;self.checkpoint()?;
        self.originals.stdout.close()?;self.originals.stderr.close()?;self.originals.nul.close()?;
        if !self.originals.all_retired()||!self.channel.retired(){return Err(bad());}
        self.checkpoint()
    }
    fn adopt_stdio(&mut self)->io::Result<()> {
        use windows_sys::Win32::System::Console::GetStdHandle;
        let mut first=None;
        for (slot,which) in [(&mut self.originals.nul,-10i32),(&mut self.originals.stdout,-11),(&mut self.originals.stderr,-12)] {
            slot.begin()?;let raw=unsafe{GetStdHandle(which as u32)};
            let error=if raw == -1{unsafe{GetLastError()}}else{0};
            if let Err(error)=slot.captured(raw!=0&&raw != -1,raw,error) {if first.is_none(){first=Some(error);}}
        }
        if let Some(error)=first{return Err(error);}
        let [i,o,e]=[self.originals.nul.raw,self.originals.stdout.raw,self.originals.stderr.raw];
        if i==o||i==e||o==e{return Err(bad());}
        for raw in [i,o,e] {check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(raw,1,0)})?;}Ok(())
    }
}
/// S-binary-only early dispatch. A malformed A1 prefix never falls through into
/// ordinary setup or A0. The table is that binary's compiled .c28a1 section.
pub fn dispatch(compiled_table:&[u8;80])->Option<i32> {
    let args=std::env::args_os().collect::<Vec<_>>();let mode=args.get(1)?;
    if !mode.to_string_lossy().starts_with("--c28-asuser-a1-"){return None;}
    let parsed=(||->io::Result<(&str,isize,RoleTable)>{
        if mode.to_str()!=Some(A1_HELPER_SWITCH)||args.len()!=4{return Err(bad());}
        let endpoint=args[2].to_str().filter(|v|valid_endpoint(v)).ok_or_else(bad)?;
        let text=args[3].to_str().filter(|v|!v.is_empty()&&v.len()<=20&&!v.starts_with('0')
            &&v.bytes().all(|c|c.is_ascii_digit())).ok_or_else(bad)?;
        let raw=text.parse::<isize>().ok().filter(|v|*v>0).ok_or_else(bad)?;
        Ok((endpoint,raw,RoleTable::decode(compiled_table)?))
    })();
    let (endpoint,parent,table)=match parsed{Ok(v)=>v,Err(_)=>return Some(78)};
    let mut original=Box::new(Helper::new());
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||original.run(endpoint,parent,table)));
    match result {
        Ok(Ok(()))=>Some(0),
        error=>{
            original.first_error=Some(match error{Ok(Err(e))=>e,_=>io::Error::other("A1 original S unwind")});
            if !original.allocation_attempted{return Some(78);}
            // Same original S and all fixed slots/buffers stay alive. No close,
            // kill, replay, replacement owner or fake CLOSED receipt on Unknown.
            loop {std::hint::black_box(&original);std::thread::park();}
        }
    }
}

#[derive(Clone,Copy)]
struct Capture {len:usize,eof:bool,error:Option<u32>}
fn reader_completed(code:u32,capture:Capture)->bool {
    code==0 && capture.eof && capture.error.is_none() && capture.len==0
}
struct Reader {pipe:isize,result:std::sync::Mutex<Option<Capture>>,buffer:std::cell::UnsafeCell<[u8;4096]>}
impl Reader {fn new()->Box<Self> {Box::new(Self {pipe:0,result:std::sync::Mutex::new(None),
    buffer:std::cell::UnsafeCell::new([0;4096])})}}
unsafe extern "system" fn read_original(context:*mut c_void)->u32 {
    let reader=unsafe{&*(context as *const Reader)};
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Only this ONE original native reader writes its preinstalled buffer.
        // Owner history never reads its bytes, and its Box survives through the
        // original thread's native/TLS join or indefinitely on Unknown.
        let bytes=unsafe{&mut *reader.buffer.get()};
        let mut capture=Capture {len:0,eof:false,error:None};
        loop {
            if capture.len==4096 {capture.error=Some(122);break;}
            let mut n=0;let ok=unsafe{ReadFile(reader.pipe,bytes[capture.len..].as_mut_ptr().cast(),
                (4096-capture.len) as u32,&mut n,ptr::null_mut())};
            if ok==0 {let error=unsafe{GetLastError()};if error==109 {capture.eof=true;}else{capture.error=Some(error);}break;}
            if n==0 {capture.eof=true;break;}
            if n as usize>4096-capture.len {capture.error=Some(87);break;}capture.len+=n as usize;
        }
        capture
    }));
    match (result,reader.result.lock()) {
        (Ok(capture),Ok(mut slot))=>{let code=u32::from(capture.error.is_some());*slot=Some(capture);code},_=>1,
    }
}


/// C-side original S process, endpoints, native reader threads and GUI aliases.
/// Installed inside the existing CheckedRunnerOwner's A1 cell before login.
pub(crate) struct Broker {
    originals:Originals,channel:Box<Channel>,readers:[Box<Reader>;2],
    bundle:std::sync::Arc<crate::asuser_a1_policy::Bundle>,
    generation:u64,deadline:Instant,started:bool,helper_closed:bool,
    aliases_closed:bool,binding:Option<Binding>,qualified:Option<String>,
    attributes:Option<crate::proc_thread_attr::ProcThreadAttributeList>,
}
impl Broker {
    pub(crate) fn new(bundle:std::sync::Arc<crate::asuser_a1_policy::Bundle>,generation:u64,deadline:Instant)->Box<Self> {
        Box::new(Self{originals:Originals::new(),channel:Channel::new(),readers:[Reader::new(),Reader::new()],
            bundle,generation,deadline,started:false,helper_closed:false,aliases_closed:false,
            binding:None,qualified:None,attributes:None})
    }
    fn pipe_pair(read:&mut Slot,write:&mut Slot)->io::Result<()> {
        read.begin()?;write.begin()?;let sa=attributes(0,true);
        let ok=unsafe{CreatePipe(&mut read.raw,&mut write.raw,&sa,4096)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        let a=read.captured(ok!=0,read.raw,error);let b=write.captured(ok!=0,write.raw,error);a.and(b)?;
        check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(read.raw,1,0)})
    }
    fn caller_sd(slot:&mut Slot,sid:&[u8],mask:u32)->io::Result<()> {
        let text=wide(&format!("D:P(A;;0x{mask:x};;;{})(A;;RC;;;OW)",sid_text(sid)?));
        slot.begin()?;let mut raw=ptr::null_mut();
        let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut raw,ptr::null_mut())};
        let error=if ok==0{unsafe{GetLastError()}}else{0};slot.captured(ok!=0,raw as isize,error)
    }
    fn spawn(&mut self,name:&str,sid:&[u8],environment:&mut crate::asuser_a1_policy::Environment,site:&mut crate::WindowsAsUserNativeSite)->io::Result<(u32,u64)> {
        use crate::WindowsAsUserNativeSite as Site;
        *site=Site::HelperSpawnDeadline;
        remaining(self.deadline)?;
        *site=Site::HelperStdoutPipePair;
        Self::pipe_pair(&mut self.originals.stdout,&mut self.originals.child_out)?;
        *site=Site::HelperStderrPipePair;
        Self::pipe_pair(&mut self.originals.stderr,&mut self.originals.child_err)?;
        *site=Site::HelperNulReservation;
        self.originals.nul.begin()?;let sa=attributes(0,true);
        *site=Site::HelperCreateNulResult;
        let raw=unsafe{CreateFileW(wide("NUL").as_ptr(),0x80000000,3,&sa,3,0,0)};
        let error=if raw == -1{unsafe{GetLastError()}}else{0};self.originals.nul.captured(raw != -1,raw,error)?;
        *site=Site::HelperParentReservation;
        self.originals.parent.begin()?;
        *site=Site::HelperDuplicateParentResult;
        let ok=unsafe{DuplicateHandle(GetCurrentProcess(),GetCurrentProcess(),GetCurrentProcess(),
            &mut self.originals.parent.raw,0x00101000,1,0)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.originals.parent.captured(ok!=0,self.originals.parent.raw,error)?;
        *site=Site::HelperAttributeList;
        self.attributes=Some(crate::proc_thread_attr::ProcThreadAttributeList::new(1)?);
        let attrs=self.attributes.as_mut().ok_or_else(bad)?;
        attrs.set_handle_list(vec![self.originals.child_out.raw,self.originals.child_err.raw,self.originals.nul.raw,self.originals.parent.raw])?;
        let mut si:STARTUPINFOEXW=unsafe{mem::zeroed()};si.StartupInfo.cb=mem::size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags=0x100;si.StartupInfo.hStdInput=self.originals.nul.raw;
        si.StartupInfo.hStdOutput=self.originals.child_out.raw;si.StartupInfo.hStdError=self.originals.child_err.raw;
        si.lpAttributeList=attrs.as_mut_ptr();
        *site=Site::HelperCommandGuard;
        let app=self.bundle.s.path().to_str().ok_or_else(bad)?.to_owned();
        if app.contains('"')||app.contains('\0'){return Err(bad());}
        let cwd=self.bundle.s.path().parent().and_then(|p|p.to_str()).ok_or_else(bad)?.to_owned();
        let mut command=wide(&format!("\"{app}\" {A1_HELPER_SWITCH} {name} {}",self.originals.parent.raw));
        // Both S objects have protected creation-time DACLs, C-only management.
        *site=Site::HelperProcessDescriptor;
        Self::caller_sd(&mut self.originals.helper_process_sd,sid,crate::asuser_a1_policy::PROCESS_MANAGEMENT)?;
        *site=Site::HelperThreadDescriptor;
        Self::caller_sd(&mut self.originals.helper_thread_sd,sid,crate::asuser_a1_policy::THREAD_MANAGEMENT)?;
        let process_sa=attributes(self.originals.helper_process_sd.raw,false);let thread_sa=attributes(self.originals.helper_thread_sd.raw,false);
        *site=Site::HelperCreateReservation;
        self.originals.process.begin()?;self.originals.thread.begin()?;remaining(self.deadline)?;
        *site=Site::HelperCreateProcessResult;
        let ok=unsafe{CreateProcessW(wide(&app).as_ptr(),command.as_mut_ptr(),&process_sa,&thread_sa,1,
            0x08080000|0x00000400,environment.as_mut_ptr(),wide(&cwd).as_ptr(),&si.StartupInfo,&mut self.originals.pi)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        self.originals.capture_child(ok!=0,error)?;
        *site=Site::HelperIdentity;
        let original=identity(self.originals.process.raw)?;
        let pi=&self.originals.pi;
        *site=Site::HelperIdentityGuard;
        if original.0!=pi.dwProcessId||pi.dwThreadId==0||pi.hProcess==pi.hThread{return Err(bad());}
        *site=Site::HelperImageReadback;
        self.bundle.s.verify_resolved(&image_path(self.originals.process.raw)?).map_err(|_|bad())?;
        *site=Site::HelperPrivateClose;
        self.attributes.take(); // initialized list has no native pending IO
        self.originals.helper_thread_sd.close()?;self.originals.helper_process_sd.close()?;
        self.originals.child_out.close()?;self.originals.child_err.close()?;
        self.originals.nul.close()?;self.originals.parent.close()?;
        for i in 0..2 {
            remaining(self.deadline)?;
            self.readers[i].pipe=if i==0{self.originals.stdout.raw}else{self.originals.stderr.raw};
            *site=Site::HelperReaderReservation;
            self.originals.readers[i].begin()?;let mut tid=0;
            *site=Site::HelperCreateReaderResult;
            let raw=unsafe{CreateThread(ptr::null(),0,Some(read_original),(&mut *self.readers[i] as *mut Reader).cast(),0,&mut tid)};
            let error=if raw==0{unsafe{GetLastError()}}else{0};self.originals.readers[i].captured(raw!=0,raw,error)?;
            *site=Site::HelperReaderIdGuard;
            if tid==0{return Err(bad());}
        }
        Ok(original)
    }
    pub(crate) fn start(&mut self,logon:crate::asuser_a1_policy::Sid,
        environment:&mut crate::asuser_a1_policy::Environment,site:&mut crate::WindowsAsUserNativeSite)->io::Result<&str> {
        use crate::WindowsAsUserNativeSite as Site;
        *site=Site::BrokerInputGuard;
        if self.started||!logon.is_logon(){return Err(bad());}
        *site=Site::BrokerDeadline;
        self.started=true;remaining(self.deadline)?;
        *site=Site::BrokerCallerIdentity;
        let c=identity(unsafe{GetCurrentProcess()})?;*site=Site::BrokerOriginalBinding;
        self.bundle.verify_originals(c.0,c.1)?;
        *site=Site::BrokerTableGuard;
        let table=self.bundle.s.asuser_s_table()?;
        if table.c!=self.bundle.c.sha256()||table.r!=self.bundle.r.sha256(){return Err(bad());}
        *site=Site::BrokerNoImpersonation;
        no_impersonation(&mut self.originals.caller_thread_token)?;
        *site=Site::BrokerCallerSubject;
        let (own,sid,own_il)=subject(unsafe{GetCurrentProcess()},&mut self.originals.own_token,&mut self.originals.primary_defaults[0])?;
        let mut nonce=[0u8;32];
        *site=Site::BrokerEntropy;
        {use rand::RngCore;rand::rngs::OsRng.try_fill_bytes(&mut nonce).map_err(|_|bad())?;}
        *site=Site::BrokerNonceGuard;
        if nonce==[0;32]{return Err(bad());}let name=endpoint(&nonce);
        *site=Site::BrokerDescriptor;
        Self::caller_sd(&mut self.originals.sd,&sid,0x0012019f)?;
        *site=Site::BrokerPipeReservation;
        let sa=attributes(self.originals.sd.raw,false);self.channel.pipe.begin()?;
        *site=Site::BrokerCreateNamedPipeResult;
        let raw=unsafe{CreateNamedPipeW(wide(&name).as_ptr(),0x40080003,0x0000000e,1,512,512,0,&sa)};
        let error=if raw == -1{unsafe{GetLastError()}}else{0};self.channel.pipe.captured(raw != -1,raw,error)?;
        *site=Site::BrokerChannelPrepare;
        self.originals.sd.close()?;self.channel.events()?;self.channel.connect_server()?;
        *site=Site::BrokerSpawn;
        let helper=self.spawn(&name,&sid,environment,site)?;
        *site=Site::BrokerConnect;
        self.channel.connected(self.deadline)?;
        *site=Site::BrokerGetClientPidResult;
        let mut peer=0;check(unsafe{GetNamedPipeClientProcessId(self.channel.pipe.raw,&mut peer)})?;
        *site=Site::BrokerPeerPidGuard;
        if peer!=helper.0{return Err(bad());}
        *site=Site::BrokerPeerSubject;
        let (peer_context,_,peer_il)=subject(self.originals.process.raw,&mut self.originals.peer_token,&mut self.originals.primary_defaults[1])?;
        *site=Site::BrokerPeerContextGuard;
        if own!=peer_context||own_il!=0x3000||peer_il!=0x3000{return Err(bad());}
        *site=Site::BrokerBinding;
        let mut logon_bytes=[0u8;68];logon_bytes[..logon.bytes().len()].copy_from_slice(logon.bytes());
        let binding=Binding{generation:self.generation,c_pid:c.0,c_time:c.1,s_pid:helper.0,s_time:helper.1,
            deadline_ticks:shared_deadline(self.deadline)?,c_image:self.bundle.c.sha256(),
            s_image:self.bundle.s.sha256(),r_image:self.bundle.r.sha256(),nonce,logon_sid:logon_bytes,sid_len:logon.bytes().len() as u8};
        self.binding=Some(binding);
        *site=Site::BrokerHelloSend;
        self.channel.send(message(HELLO,binding),self.deadline)?;
        *site=Site::BrokerOfferReceive;
        let offer=self.channel.receive(OFFER,Some(binding),self.deadline)?;
        *site=Site::BrokerPullStation;
        self.pull(offer.station,false)?;
        *site=Site::BrokerPullDesktop;
        self.pull(offer.desktop,true)?;
        let digest=Sha256::digest(nonce);
        let suffix=digest[..16].iter().map(|v|format!("{v:02x}")).collect::<String>();
        let station=format!("C28StationA1-{}-{suffix}",self.generation);let desktop=format!("C28DesktopA1-{suffix}");
        *site=Site::BrokerGuiIdentity;
        if object_text(self.originals.station.raw,2)?!=station||object_text(self.originals.desktop.raw,2)?!=desktop
            ||gui_descriptor(&mut self.originals.gui_reads[0],self.originals.station.raw,&sid,logon.bytes(),false)?!=offer.station_sd
            ||gui_descriptor(&mut self.originals.gui_reads[1],self.originals.desktop.raw,&sid,logon.bytes(),true)?!=offer.desktop_sd{return Err(bad());}
        *site=Site::BrokerAcceptSend;
        self.channel.send(message(ACCEPT,binding),self.deadline)?;
        *site=Site::BrokerClosedReceive;
        self.channel.receive(CLOSED,Some(binding),self.deadline)?;
        *site=Site::BrokerProcessWait;
        wait(self.originals.process.raw,self.deadline)?;let mut code=0;
        *site=Site::BrokerProcessExitCodeResult;
        check(unsafe{GetExitCodeProcess(self.originals.process.raw,&mut code)})?;*site=Site::BrokerProcessExitGuard;
        if code!=0{return Err(bad());}
        *site=Site::BrokerThreadWait;
        wait(self.originals.thread.raw,self.deadline)?;
        *site=Site::BrokerThreadExitCodeResult;
        check(unsafe{GetExitCodeThread(self.originals.thread.raw,&mut code)})?;*site=Site::BrokerThreadExitGuard;
        if code!=0{return Err(bad());}
        for i in 0..2 {
            *site=Site::BrokerReaderWait;
            wait(self.originals.readers[i].raw,self.deadline)?;
            *site=Site::BrokerReaderExitCodeResult;
            check(unsafe{GetExitCodeThread(self.originals.readers[i].raw,&mut code)})?;
            *site=Site::BrokerReaderCapture;
            let capture=self.readers[i].result.lock().map_err(|_|bad())?.ok_or_else(bad)?;
            *site=Site::BrokerReaderCompletionGuard;
            if !reader_completed(code,capture){return Err(bad());}
            *site=Site::BrokerReaderClose;
            self.originals.readers[i].close()?;
        }
        *site=Site::BrokerOriginalClose;
        self.originals.stdout.close()?;self.originals.stderr.close()?;
        self.originals.thread.close()?;self.originals.process.close()?;self.channel.close()?;
        *site=Site::BrokerCompletion;
        remaining(self.deadline)?;self.helper_closed=true;
        self.qualified=Some(format!("{station}\\{desktop}"));self.qualified.as_deref().ok_or_else(bad)
    }
    fn pull(&mut self,value:u64,desktop:bool)->io::Result<()> {
        let source=isize::try_from(value).ok().filter(|v|*v>0).ok_or_else(bad)?;
        let process=self.originals.process.raw;
        let slot=if desktop{&mut self.originals.desktop}else{&mut self.originals.station};slot.begin()?;
        let desired=if desktop{crate::asuser_a1_rights::DESKTOP_OBSERVE}else{crate::asuser_a1_rights::STATION_OBSERVE};
        let ok=unsafe{DuplicateHandle(process,source,GetCurrentProcess(),&mut slot.raw,desired,0,0)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};slot.captured(ok!=0,slot.raw,error)
    }
    pub(crate) fn retire_aliases(&mut self)->io::Result<()> {
        if !self.helper_closed||self.aliases_closed{return Err(bad());}
        self.originals.desktop.close()?;self.originals.station.close()?;
        if !self.originals.all_retired()||!self.channel.retired(){return Err(bad());}
        self.aliases_closed=true;Ok(())
    }
}
