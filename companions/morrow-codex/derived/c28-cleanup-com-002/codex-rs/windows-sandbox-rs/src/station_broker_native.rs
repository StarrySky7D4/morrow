//! Native originals for A0 only. No Drop closes, PID reopen, impersonation,
//! suspended launch, resume, kill, provisioning, target ACE or global retention.
use super::*;
use super::diagnostic::{self,Lease as DiagnosticLease,Stage as DiagnosticStage};
use std::{ffi::c_void,mem,ptr,sync::Mutex,time::{Duration,Instant}};
use windows_sys::Win32::Foundation::{CloseHandle,DuplicateHandle,GetLastError,HANDLE,FILETIME,LocalFree};
use windows_sys::Win32::Security::{GetTokenInformation,GetUserObjectSecurity,IsValidSecurityDescriptor,
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
const STATION_ACCESS:u32=0x0006012b; // Caller management, no Everyone/target/system ACE.
const DESKTOP_ACCESS:u32=0x000600c3;
const HELPER_SECONDS:u64=30;
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
    if now==0 || frequency==0 || end.saturating_sub(now)<=1 {return Err(io::Error::new(io::ErrorKind::TimedOut,"A0 original shared budget expired"));}
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
fn bad()->io::Error {io::Error::new(io::ErrorKind::InvalidData,"A0 original binding/operation rejected")}
fn os_error()->io::Error {io::Error::last_os_error()}
fn check(ok:i32)->io::Result<()> {if ok!=0 {Ok(())}else{Err(os_error())}}
fn wide(value:&str)->Vec<u16> {value.encode_utf16().chain(Some(0)).collect()}

#[derive(Clone,Copy)]
enum CloseKind {Kernel,Station,Desktop,Local}
struct Slot {raw:isize,state:StationBrokerHandle,kind:CloseKind}
impl Slot {
    fn new(kind:CloseKind)->Self {Self {raw:0,state:StationBrokerHandle::NotAllocated,kind}}
    fn begin(&mut self)->io::Result<()> {
        if self.state!=StationBrokerHandle::NotAllocated {return Err(bad());}
        self.state=StationBrokerHandle::InFlight;Ok(())
    }
    fn captured(&mut self,ok:bool,raw:isize,error:u32)->io::Result<()> {
        if self.state!=StationBrokerHandle::InFlight {return Err(bad());}
        self.raw=raw;
        self.state=match (ok,raw!=0 && raw != -1) {
            (true,true)=>StationBrokerHandle::Held,
            (false,false)=>StationBrokerHandle::Rejected,
            _=>StationBrokerHandle::Uncertain,
        };
        if self.state==StationBrokerHandle::Held {Ok(())}
        else if !ok {Err(io::Error::from_raw_os_error(error as i32))}else{Err(bad())}
    }
    fn close_with(&mut self,close:impl FnOnce(isize)->io::Result<()>)->io::Result<()> {
        match self.state {
            StationBrokerHandle::NotAllocated|StationBrokerHandle::Rejected|StationBrokerHandle::ClosedVerified=>return Ok(()),
            StationBrokerHandle::Held=>{},_=>return Err(bad()),
        }
        self.state=StationBrokerHandle::InFlight;
        match close(self.raw) {
            Ok(())=>{self.raw=0;self.state=StationBrokerHandle::ClosedVerified;Ok(())},
            Err(e)=>{self.state=match e.raw_os_error(){Some(v)=>StationBrokerHandle::CloseFailed{error:v as u32},
                None=>StationBrokerHandle::Uncertain};Err(e)},
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
    fn retired(&self)->bool {matches!(self.state,StationBrokerHandle::NotAllocated|StationBrokerHandle::Rejected|StationBrokerHandle::ClosedVerified)}
}
// Both parent and helper create this entire finite set before the first OS
// allocation. Borrowed original associations are deliberately NOT in this set.
struct Originals {
    process:Slot,thread:Slot,stdout:Slot,stderr:Slot,child_out:Slot,child_err:Slot,
    nul:Slot,parent:Slot,own_token:Slot,peer_token:Slot,caller_thread_token:Slot,sd:Slot,desktop_sd:Slot,station:Slot,desktop:Slot,
    readers:[Slot;2],
}
impl Originals {
    fn new()->Self {Self {process:Slot::new(CloseKind::Kernel),thread:Slot::new(CloseKind::Kernel),
        stdout:Slot::new(CloseKind::Kernel),stderr:Slot::new(CloseKind::Kernel),
        child_out:Slot::new(CloseKind::Kernel),child_err:Slot::new(CloseKind::Kernel),
        nul:Slot::new(CloseKind::Kernel),parent:Slot::new(CloseKind::Kernel),
        own_token:Slot::new(CloseKind::Kernel),peer_token:Slot::new(CloseKind::Kernel),caller_thread_token:Slot::new(CloseKind::Kernel),sd:Slot::new(CloseKind::Local),desktop_sd:Slot::new(CloseKind::Local),
        station:Slot::new(CloseKind::Station),desktop:Slot::new(CloseKind::Desktop),
        readers:[Slot::new(CloseKind::Kernel),Slot::new(CloseKind::Kernel)]}}
    fn all_retired(&self)->bool {
        [&self.process,&self.thread,&self.stdout,&self.stderr,&self.child_out,&self.child_err,
            &self.nul,&self.parent,&self.own_token,&self.peer_token,&self.caller_thread_token,&self.sd,&self.desktop_sd,&self.station,&self.desktop,
            &self.readers[0],&self.readers[1]].iter().all(|v|v.retired())
    }
}

// Stable heap addresses survive pending I/O and unwind. A timeout never frees,
// cancels or reuses OVERLAPPED/event/buffer/pipe storage. Exactly two events.
struct Channel {
    pipe:Slot,read_event:Slot,write_event:Slot,read:OVERLAPPED,write:OVERLAPPED,
    input:[u8;FRAME_BYTES],output:[u8;FRAME_BYTES],read_pending:bool,write_pending:bool,
    diagnostic:Option<diagnostic::ChannelWriter>,
}
impl Channel {
    fn new()->Box<Self> {Box::new(Self {pipe:Slot::new(CloseKind::Kernel),
        read_event:Slot::new(CloseKind::Kernel),write_event:Slot::new(CloseKind::Kernel),
        read:unsafe{mem::zeroed()},write:unsafe{mem::zeroed()},input:[0;FRAME_BYTES],output:[0;FRAME_BYTES],
        read_pending:false,write_pending:false,diagnostic:None})}
    fn publish_pending(&self) {
        if let Some(writer)=self.diagnostic {
            writer.pending(self.read_pending,self.write_pending,self.pipe.state,self.read_event.state,self.write_event.state);
        }
    }
    fn events(&mut self)->io::Result<()> {
        for slot in [&mut self.read_event,&mut self.write_event] {
            slot.begin()?;let raw=unsafe{CreateEventW(ptr::null(),1,0,ptr::null())};let error=if raw==0 {unsafe{GetLastError()}}else{0};
            slot.captured(raw!=0,raw,error)?;
        }
        self.read.hEvent=self.read_event.raw;self.write.hEvent=self.write_event.raw;self.publish_pending();Ok(())
    }
    fn connect_server(&mut self)->io::Result<()> {
        self.read_pending=true;self.publish_pending();
        let ok=unsafe{ConnectNamedPipe(self.pipe.raw,&mut self.read)};
        if ok!=0 {self.read_pending=false;self.publish_pending();return Ok(());}
        match unsafe{GetLastError()} {997=>Ok(()),535=>{self.read_pending=false;self.publish_pending();Ok(())},v=>Err(io::Error::from_raw_os_error(v as i32))}
    }
    fn completed(&mut self,read:bool,deadline:Instant)->io::Result<u32> {
        let (event,overlapped,pending)=if read {(&self.read_event,&mut self.read,&mut self.read_pending)}
            else{(&self.write_event,&mut self.write,&mut self.write_pending)};
        wait(event.raw,deadline)?;let mut bytes=0;
        check(unsafe{GetOverlappedResult(self.pipe.raw,overlapped,&mut bytes,0)})?;
        *pending=false;self.publish_pending();Ok(bytes)
    }
    fn connected(&mut self,deadline:Instant)->io::Result<()> {
        if self.read_pending {self.completed(true,deadline)?;}Ok(())
    }
    fn send(&mut self,frame:Frame,deadline:Instant)->io::Result<()> {
        remaining(deadline)?;
        if self.write_pending {return Err(bad());}check(unsafe{ResetEvent(self.write_event.raw)})?;
        self.write=unsafe{mem::zeroed()};self.write.hEvent=self.write_event.raw;
        self.output=encode(frame);self.write_pending=true;self.publish_pending();let mut bytes=0;
        let ok=unsafe{WriteFile(self.pipe.raw,self.output.as_ptr().cast(),FRAME_BYTES as u32,&mut bytes,&mut self.write)};
        if ok==0 {let error=unsafe{GetLastError()};if error!=997 {return Err(io::Error::from_raw_os_error(error as i32));}
            bytes=self.completed(false,deadline)?;}else{self.write_pending=false;self.publish_pending();}
        if bytes!=FRAME_BYTES as u32 {return Err(bad());}remaining(deadline)?;Ok(())
    }
    fn receive(&mut self,kind:u8,binding:Option<Binding>,deadline:Instant)->io::Result<Frame> {
        remaining(deadline)?;
        if self.read_pending {return Err(bad());}check(unsafe{ResetEvent(self.read_event.raw)})?;
        self.read=unsafe{mem::zeroed()};self.read.hEvent=self.read_event.raw;
        self.input.fill(0);self.read_pending=true;self.publish_pending();let mut bytes=0;
        let ok=unsafe{ReadFile(self.pipe.raw,self.input.as_mut_ptr().cast(),FRAME_BYTES as u32,&mut bytes,&mut self.read)};
        if ok==0 {let error=unsafe{GetLastError()};if error!=997 {return Err(io::Error::from_raw_os_error(error as i32));}
            bytes=self.completed(true,deadline)?;}else{self.read_pending=false;self.publish_pending();}
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
    let now=Instant::now();if now>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"A0 original deadline"));}
    Ok(deadline.saturating_duration_since(now).as_millis().min(30_000) as u32)
}
fn wait(raw:isize,deadline:Instant)->io::Result<()> {
    let ms=remaining(deadline)?;
    match unsafe{WaitForSingleObject(raw,ms)} {0=>{remaining(deadline)?;Ok(())},258=>Err(io::Error::new(io::ErrorKind::TimedOut,"A0 original native wait expired")),_=>Err(os_error())}
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
    if status<0 {return Err(io::Error::other(BrokerNtStatus(status)));}
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
    if ok==0 && error==1008 && slot.state==StationBrokerHandle::Rejected && slot.raw==0 {
        return Ok(()); // Actual ERROR_NO_TOKEN, never an assumed empty context.
    }
    captured?;
    // Do not launch with the stronger primary token of an impersonating caller.
    // Preserve the actual opened thread-token alias on this Unknown attempt.
    Err(io::Error::new(io::ErrorKind::Unsupported,"A0 impersonating caller unsupported"))
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
fn subject(process:isize,slot:&mut Slot)->io::Result<([u8;32],Vec<u8>)> {
    slot.begin()?;let ok=unsafe{OpenProcessToken(process,8,&mut slot.raw)};let error=if ok==0 {unsafe{GetLastError()}}else{0};
    slot.captured(ok!=0,slot.raw,error)?;
    let (b,len)=token_buffer(slot.raw,TokenUser)?;if len<mem::size_of::<TOKEN_USER>() {return Err(bad());}
    let user=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_USER>())};let sid=sid_from_token_buffer(&b,len,user.User.Sid,mem::size_of::<TOKEN_USER>())?;
    let mut hash=Sha256::new();hash.update(&sid);
    let (b,len)=token_buffer(slot.raw,TokenStatistics)?;if len<mem::size_of::<TOKEN_STATISTICS>() {return Err(bad());}
    let stats=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_STATISTICS>())};
    hash.update(stats.AuthenticationId.LowPart.to_le_bytes());hash.update(stats.AuthenticationId.HighPart.to_le_bytes());
    let (b,len)=token_buffer(slot.raw,TokenIntegrityLevel)?;if len<mem::size_of::<TOKEN_MANDATORY_LABEL>() {return Err(bad());}
    let label=unsafe{ptr::read(b.as_ptr().cast::<TOKEN_MANDATORY_LABEL>())};hash.update(sid_from_token_buffer(&b,len,label.Label.Sid,mem::size_of::<TOKEN_MANDATORY_LABEL>())?);
    for class in [TokenElevation,TokenElevationType,TokenSessionId,TokenIsAppContainer,
        TokenUIAccess,TokenMandatoryPolicy,TokenVirtualizationAllowed,TokenVirtualizationEnabled] {
        let (b,len)=token_buffer(slot.raw,class)?;if len!=4 {return Err(bad());}
        // This ordinary-caller A0 cannot equate distinct app-container SIDs.
        // Reject rather than omit that identity dimension or gift a capability.
        if class==TokenIsAppContainer && b[0] as u32!=0 {
            return Err(io::Error::new(io::ErrorKind::Unsupported,"A0 app-container caller unsupported"));
        }
        hash.update((b[0] as u32).to_le_bytes());
    }
    let (b,len)=token_buffer(slot.raw,TokenHasRestrictions)?;
    let restricted=match len {1=>b[0]&255!=0,4=>b[0] as u32!=0,_=>return Err(bad())};
    hash.update([u8::from(restricted)]);
    // Same SID/LUID/elevation alone is insufficient when groups or privileges
    // were removed without a different integrity level. Compare finite actual
    // normalized sets, not an administrator-name or permission boolean.
    for class in [TokenGroups,TokenRestrictedSids,TokenCapabilities] {
    let (b,len)=token_buffer(slot.raw,class)?;if len<4 {return Err(bad());}let count=b[0] as u32 as usize;
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
    for (sid,attributes) in groups {hash.update((sid.len() as u32).to_le_bytes());hash.update(sid);hash.update(attributes.to_le_bytes());}
    }
    let (b,len)=token_buffer(slot.raw,TokenPrivileges)?;if len<4 {return Err(bad());}let count=b[0] as u32 as usize;
    let offset=mem::offset_of!(TOKEN_PRIVILEGES,Privileges);
    if count>256 || offset.checked_add(count*mem::size_of::<LUID_AND_ATTRIBUTES>()).is_none_or(|v|v>len) {return Err(bad());}
    let mut privileges=Vec::with_capacity(count);
    for i in 0..count {
        let p=unsafe{ptr::read((b.as_ptr() as *const u8).add(offset+i*mem::size_of::<LUID_AND_ATTRIBUTES>()).cast::<LUID_AND_ATTRIBUTES>())};
        privileges.push((p.Luid.LowPart,p.Luid.HighPart,p.Attributes));
    }
    privileges.sort();hash.update((privileges.len() as u32).to_le_bytes());
    for (low,high,attributes) in privileges {hash.update(low.to_le_bytes());hash.update(high.to_le_bytes());hash.update(attributes.to_le_bytes());}
    slot.close()?;Ok((hash.finalize().into(),sid))
}
fn sid_text(sid:&[u8])->io::Result<String> {
    if sid.len()<8 || sid[0]!=1 || sid.len()!=8+sid[1] as usize*4 {return Err(bad());}
    let authority=sid[2..8].iter().fold(0u64,|a,b|(a<<8)|u64::from(*b));
    let mut text=format!("S-1-{authority}");
    for sub in sid[8..].chunks_exact(4) {text.push_str(&format!("-{}",u32::from_le_bytes(sub.try_into().unwrap())));}
    Ok(text)
}
fn sd(slot:&mut Slot,sid:&[u8],mask:u32)->io::Result<()> {
    let sddl=wide(&format!("D:P(A;;0x{mask:x};;;{})",sid_text(sid)?));slot.begin()?;
    let mut output=ptr::null_mut();let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(),1,&mut output,ptr::null_mut())};
    let error=if ok==0 {unsafe{GetLastError()}}else{0};slot.captured(ok!=0,output as isize,error)
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
fn descriptor_hash(raw:isize)->io::Result<[u8;32]> {
    let mut flags=7u32;let mut needed=0;
    let first=unsafe{GetUserObjectSecurity(raw,&mut flags,ptr::null_mut(),0,&mut needed)};
    if first!=0 || unsafe{GetLastError()}!=122 || needed<20 || needed>65536 {return Err(bad());}
    let mut b=vec![0u64;(needed as usize).div_ceil(8)];let mut returned=0;
    check(unsafe{GetUserObjectSecurity(raw,&mut flags,b.as_mut_ptr().cast(),needed,&mut returned)})?;
    if returned<20 || returned>needed || unsafe{IsValidSecurityDescriptor(b.as_ptr().cast_mut().cast())}==0 {return Err(bad());}
    let bytes=unsafe{std::slice::from_raw_parts(b.as_ptr().cast::<u8>(),returned as usize)};
    Ok(Sha256::digest(bytes).into())
}

#[derive(Clone,Copy)]
struct Capture {len:usize,eof:bool,error:Option<u32>}
fn reader_completed(code:u32,capture:Capture)->bool {
    code==0 && capture.eof && capture.error.is_none() && capture.len==0
}
pub(super) fn helper_closed_facts(h:&StationBrokerSnapshot)->bool {
    !h.no_replay
        && h.diagnostic.helper_normal()
        && h.helper_pid.is_some_and(|v|v!=0) && h.helper_creation_filetime_100ns.is_some_and(|v|v!=0)
        && h.helper_raw_dword==Some(0) && h.helper_objects_closed_receipt
        && h.original_eof==[true;2] && h.original_reader_joined==[true;2]
        && h.host_association_unchanged && h.first_failure.is_none()
}
pub(super) fn helper_retired_gate(h:&StationBrokerSnapshot)->bool {
    h.phase==StationBrokerPhase::HelperRetired && helper_closed_facts(h)
}
struct Reader {pipe:isize,result:Mutex<Option<Capture>>,buffer:std::cell::UnsafeCell<[u8;STDIO_CAP]>}
impl Reader {fn new()->Box<Self> {Box::new(Self {pipe:0,result:Mutex::new(None),
    buffer:std::cell::UnsafeCell::new([0;STDIO_CAP])})}}
unsafe extern "system" fn read_original(context:*mut c_void)->u32 {
    let reader=unsafe{&*(context as *const Reader)};
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Only this ONE original native reader writes its preinstalled buffer.
        // Owner history never reads its bytes, and its Box survives through the
        // original thread's native/TLS join or indefinitely on Unknown.
        let bytes=unsafe{&mut *reader.buffer.get()};
        let mut capture=Capture {len:0,eof:false,error:None};
        loop {
            if capture.len==STDIO_CAP {capture.error=Some(122);break;}
            let mut n=0;let ok=unsafe{ReadFile(reader.pipe,bytes[capture.len..].as_mut_ptr().cast(),
                (STDIO_CAP-capture.len) as u32,&mut n,ptr::null_mut())};
            if ok==0 {let error=unsafe{GetLastError()};if error==109 {capture.eof=true;}else{capture.error=Some(error);}break;}
            if n==0 {capture.eof=true;break;}
            if n as usize>STDIO_CAP-capture.len {capture.error=Some(87);break;}capture.len+=n as usize;
        }
        capture
    }));
    match (result,reader.result.lock()) {
        (Ok(capture),Ok(mut slot))=>{let code=u32::from(capture.error.is_some());*slot=Some(capture);code},_=>1,
    }
}

pub(crate) struct BrokerCore {
    history:StationBrokerSnapshot,target:StationBrokerDebt,deadline:Instant,pin:Arc<MatchedRunnerArtifact>,
    originals:Originals,channel:Box<Channel>,readers:[Box<Reader>;2],
    binding:Option<Binding>,host_station:isize,host_desktop:isize,host_thread:u32,
    attributes:Option<crate::proc_thread_attr::ProcThreadAttributeList>,
    diagnostic:Option<DiagnosticLease>,diagnostic_stage:DiagnosticStage,
}
impl BrokerCore {
    pub(crate) fn new(generation:u64,deadline:Instant,pin:Arc<MatchedRunnerArtifact>)->Self {
        Self {history:StationBrokerSnapshot::new(generation),target:StationBrokerDebt::Protocol,deadline,pin,
            originals:Originals::new(),channel:Channel::new(),readers:[Reader::new(),Reader::new()],
            binding:None,host_station:0,host_desktop:0,host_thread:0,attributes:None,
            diagnostic:None,diagnostic_stage:DiagnosticStage::ParentIdentity}
    }
    pub(crate) fn snapshot(&self)->StationBrokerSnapshot {
        let mut h=self.history;let o=&self.originals;
        h.station_alias=o.station.state;h.desktop_alias=o.desktop.state;
        h.slots=StationBrokerSlots {process:o.process.state,main_thread:o.thread.state,
            stdout:o.stdout.state,stderr:o.stderr.state,child_stdout:o.child_out.state,child_stderr:o.child_err.state,
            stdin_null:o.nul.state,inherited_parent:o.parent.state,own_token:o.own_token.state,peer_token:o.peer_token.state,
            caller_thread_token:o.caller_thread_token.state,
            station_sd:o.sd.state,desktop_sd:o.desktop_sd.state,readers:[o.readers[0].state,o.readers[1].state],
            control:self.channel.pipe.state,read_event:self.channel.read_event.state,write_event:self.channel.write_event.state,
            read_pending:self.channel.read_pending,write_pending:self.channel.write_pending};
        if let Some(lease)=self.diagnostic.as_ref(){h.diagnostic=lease.snapshot();}h
    }
    pub(super) fn enable_diagnostic(&mut self,case:diagnostic::Case)->io::Result<()> {
        if self.history.phase!=StationBrokerPhase::Reserved || self.history.no_replay || self.diagnostic.is_some(){return Err(bad());}
        self.diagnostic=Some(DiagnosticLease::new(case,self.pin.sha256(),
            diagnostic::Scope {generation:self.history.generation,parent_pid:0,parent_time:0},diagnostic::Writer::Parent));
        self.note(DiagnosticStage::ParentIdentity);
        remaining(self.deadline)?;
        let (pid,time)=identity(unsafe{GetCurrentProcess()})?;
        let lease=self.diagnostic.as_mut().ok_or_else(bad)?;
        lease.set_scope(diagnostic::Scope {generation:self.history.generation,parent_pid:pid,parent_time:time})?;
        self.set_target(StationBrokerDebt::DiagnosticMapping);self.note(DiagnosticStage::MappingOpen);
        let result=self.diagnostic.as_mut().ok_or_else(bad)?.acquire(self.deadline);
        if self.diagnostic.as_ref().is_some_and(|d|d.snapshot().view!=StationBrokerHandle::NotAllocated) {
            self.set_target(StationBrokerDebt::DiagnosticView);
        }
        result?;self.note(DiagnosticStage::MappingBound);
        self.channel.diagnostic=self.diagnostic.as_ref().and_then(|lease|lease.channel_writer());Ok(())
    }
    fn note(&mut self,stage:DiagnosticStage){self.diagnostic_stage=stage;self.publish(true);}
    fn set_target(&mut self,target:StationBrokerDebt){self.target=target;self.note(diagnostic::stage_for(target));}
    fn publish(&mut self,in_flight:bool) {
        let history=self.snapshot();
        if let Some(lease)=self.diagnostic.as_mut(){lease.publish(self.diagnostic_stage,in_flight,&history);}
    }
    pub(crate) fn fail(&mut self,target:StationBrokerDebt,error:Option<u32>) {
        if self.history.first_failure.is_none() {self.history.first_failure=Some(StationBrokerFailure {
            target,win32_error:error,native_ntstatus:None,kind:if target==StationBrokerDebt::Unwind {
                StationBrokerErrorKind::Unwind
            } else if error.is_some() {StationBrokerErrorKind::Win32} else {StationBrokerErrorKind::Other}});}
        self.history.phase=StationBrokerPhase::Unknown;self.history.no_replay=true;
        self.publish(false);
    }
    pub(crate) fn fail_io(&mut self,target:StationBrokerDebt,error:&io::Error) {
        if self.history.first_failure.is_none() {
            if let Some(lease)=self.diagnostic.as_mut(){lease.first_io(error.kind());}
        }
        if error.kind()==io::ErrorKind::TimedOut {self.history.deadline_expired_observed=true;}
        if self.history.first_failure.is_none() {self.history.first_failure=Some(StationBrokerFailure::from_io(target,error));}
        self.history.phase=StationBrokerPhase::Unknown;self.history.no_replay=true;
        self.publish(false);
    }
    pub(crate) fn fail_unclassified(&mut self,e:&io::Error) {
        self.fail_io(if e.kind()==io::ErrorKind::TimedOut {StationBrokerDebt::Deadline}else{self.target},e);
    }
    pub(crate) fn record_connect_release(&mut self,value:StationBrokerConnectRelease) {
        self.history.connect_release=value;self.note(DiagnosticStage::Admission);
    }
    pub(crate) fn record_registry_release(&mut self,value:StationBrokerRegistryRelease) {
        self.history.registry_release=value;
        if value==StationBrokerRegistryRelease::RetainedLate {self.history.deadline_expired_observed=true;}
        self.note(DiagnosticStage::Admission);
    }
    pub(crate) fn admission_failed(&mut self,error:&io::Error) {
        if self.history.registry_release==StationBrokerRegistryRelease::TimelyCommitted {
            let failure=StationBrokerFailure::from_io(StationBrokerDebt::Admission,error);
            latch_delivery(&mut self.history,ReportStage::Closed,
                StationBrokerOutput::Unavailable(StationBrokerUnavailable::TransportUnqualified),failure);
        } else {self.fail_io(StationBrokerDebt::Admission,error);}
    }
    pub(crate) fn admission_unwound(&mut self) {
        if self.history.registry_release==StationBrokerRegistryRelease::InFlight {
            self.history.registry_release=StationBrokerRegistryRelease::RetainedUnwind;
        }
        if self.history.registry_release==StationBrokerRegistryRelease::TimelyCommitted {
            latch_delivery(&mut self.history,ReportStage::Closed,StationBrokerOutput::Unwound,
                StationBrokerFailure {target:StationBrokerDebt::Admission,kind:StationBrokerErrorKind::Unwind,
                    win32_error:None,native_ntstatus:None});
        } else {self.fail(StationBrokerDebt::Unwind,None);}
    }
    pub(super) fn delivery_record(&mut self,stage:ReportStage,state:StationBrokerOutput,
        failure:Option<StationBrokerFailure>)->StationBrokerSnapshot {
        if let Some(failure)=failure {
            if failure.kind==StationBrokerErrorKind::TimedOut {self.history.deadline_expired_observed=true;}
            latch_delivery(&mut self.history,stage,state,failure);
        } else {*delivery_slot(&mut self.history,stage)=state;}
        self.diagnostic_stage=match stage {ReportStage::Aliases=>DiagnosticStage::NormalAliasesOutput,
            ReportStage::Closed=>DiagnosticStage::NormalClosedOutput};
        self.publish(state==StationBrokerOutput::InFlight);
        self.snapshot()
    }
    fn host_unchanged(&mut self)->io::Result<()> {
        if self.host_thread!=unsafe{GetCurrentThreadId()} || self.host_station!=unsafe{GetProcessWindowStation()}
            || self.host_desktop!=unsafe{GetThreadDesktop(self.host_thread)} {return Err(bad());}
        self.history.host_association_unchanged=true;Ok(())
    }
    pub(crate) fn resource_only_closed(&self)->bool {
        self.history.phase==StationBrokerPhase::ResourceOnlyClosed && !self.history.no_replay
            && helper_closed_facts(&self.snapshot())
            && self.originals.station.state==StationBrokerHandle::ClosedVerified
            && self.originals.desktop.state==StationBrokerHandle::ClosedVerified
            && self.originals.all_retired() && self.channel.retired() && self.attributes.is_none()
            && self.snapshot().diagnostic.closed()
    }
    fn pipe_pair(read:&mut Slot,write:&mut Slot)->io::Result<()> {
        read.begin()?;write.begin()?;let sa=attributes(0,true);
        let ok=unsafe{CreatePipe(&mut read.raw,&mut write.raw,&sa,STDIO_CAP as u32)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        // Preserve BOTH outputs even when the first classification returns Err.
        let a=read.captured(ok!=0,read.raw,error);let b=write.captured(ok!=0,write.raw,error);
        a.and(b)?;
        check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(read.raw,1,0)})
    }
    fn spawn(&mut self,name:&str)->io::Result<()> {
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Stdout);self.note(DiagnosticStage::PipeCreate);
        Self::pipe_pair(&mut self.originals.stdout,&mut self.originals.child_out)?;
        self.set_target(StationBrokerDebt::Stderr);self.note(DiagnosticStage::PipeCreate);
        Self::pipe_pair(&mut self.originals.stderr,&mut self.originals.child_err)?;
        self.set_target(StationBrokerDebt::StdinNull);self.note(DiagnosticStage::StdinOpen);
        self.originals.nul.begin()?;let sa=attributes(0,true);
        let raw=unsafe{CreateFileW(wide("NUL").as_ptr(),0x80000000,3,&sa,3,0,0)};
        let error=if raw == -1 {unsafe{GetLastError()}}else{0};
        self.originals.nul.captured(raw != -1,raw,error)?;
        self.set_target(StationBrokerDebt::InheritedParent);self.note(DiagnosticStage::ParentHandleDuplicate);
        self.originals.parent.begin()?;
        let ok=unsafe{DuplicateHandle(GetCurrentProcess(),GetCurrentProcess(),GetCurrentProcess(),
            &mut self.originals.parent.raw,0x00101000,1,0)}; // QUERY_LIMITED + SYNCHRONIZE, no DUP right in helper.
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        self.originals.parent.captured(ok!=0,self.originals.parent.raw,error)?;
        self.note(DiagnosticStage::AttributeList);self.attributes=Some(crate::proc_thread_attr::ProcThreadAttributeList::new(1)?);
        let attrs=self.attributes.as_mut().ok_or_else(bad)?;
        attrs.set_handle_list(vec![self.originals.child_out.raw,self.originals.child_err.raw,
            self.originals.nul.raw,self.originals.parent.raw])?;
        let mut si:STARTUPINFOEXW=unsafe{mem::zeroed()};si.StartupInfo.cb=mem::size_of::<STARTUPINFOEXW>() as u32;
        si.StartupInfo.dwFlags=0x100;si.StartupInfo.hStdInput=self.originals.nul.raw;
        si.StartupInfo.hStdOutput=self.originals.child_out.raw;si.StartupInfo.hStdError=self.originals.child_err.raw;
        si.lpAttributeList=attrs.as_mut_ptr();
        let exe=self.pin.path().to_str().ok_or_else(bad)?;
        if exe.contains('"') {return Err(bad());}
        let helper_args=match self.diagnostic.as_ref() {
            Some(lease)=>format!("{} {name} {} {}",diagnostic::HELPER_SWITCH,self.originals.parent.raw,lease.case().text()),
            None=>format!("{} {name} {}",STATION_BROKER_A0_SWITCH,self.originals.parent.raw),
        };
        let mut command=wide(&format!("\"{exe}\" {helper_args}"));
        let application=wide(exe); // End the pin-path borrow before mutable diagnostic checkpoints.
        let mut pi:PROCESS_INFORMATION=unsafe{mem::zeroed()};
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Process);self.originals.process.begin()?;self.originals.thread.begin()?;
        let ok=unsafe{CreateProcessW(application.as_ptr(),command.as_mut_ptr(),ptr::null(),ptr::null(),1,
            0x08080000,ptr::null(),ptr::null(),&si.StartupInfo,&mut pi)};
        let error=if ok==0 {unsafe{GetLastError()}}else{0};
        // No allocation/formatting/check between native return and BOTH original slots.
        let process=self.originals.process.captured(ok!=0,pi.hProcess,error);
        let thread=self.originals.thread.captured(ok!=0,pi.hThread,error);
        self.history.reported_creation=Some(StationBrokerReportedCreation {api_returned_success:ok!=0,
            process_id:pi.dwProcessId,thread_id:pi.dwThreadId});
        process.and(thread)?;self.attributes.take();
        self.note(DiagnosticStage::ProcessIdentity);let (pid,time)=identity(self.originals.process.raw)?;
        if pid!=pi.dwProcessId || pi.dwThreadId==0 {return Err(bad());}
        self.history.helper_pid=Some(pid);self.history.helper_creation_filetime_100ns=Some(time);
        self.note(DiagnosticStage::ImageVerify);self.pin.verify_resolved(&image_path(self.originals.process.raw)?).map_err(|_|bad())?;
        self.set_target(StationBrokerDebt::ChildStdout);self.originals.child_out.close()?;
        self.set_target(StationBrokerDebt::ChildStderr);self.originals.child_err.close()?;
        self.set_target(StationBrokerDebt::StdinNull);self.originals.nul.close()?;
        self.set_target(StationBrokerDebt::InheritedParent);self.originals.parent.close()?;
        for i in 0..2 {
            remaining(self.deadline)?;
            self.set_target(if i==0 {StationBrokerDebt::StdoutReader}else{StationBrokerDebt::StderrReader});
            self.note(DiagnosticStage::ReaderSpawn);
            self.readers[i].pipe=if i==0 {self.originals.stdout.raw}else{self.originals.stderr.raw};
            self.originals.readers[i].begin()?;let mut tid=0;
            let raw=unsafe{CreateThread(ptr::null(),0,Some(read_original),
                (&mut *self.readers[i] as *mut Reader).cast(),0,&mut tid)};
            let error=if raw==0 {unsafe{GetLastError()}}else{0};self.originals.readers[i].captured(raw!=0,raw,error)?;
            if tid==0 {return Err(bad());}
        }
        Ok(())
    }
    pub(crate) fn construct_and_transfer(&mut self)->io::Result<()> {
        if self.history.phase!=StationBrokerPhase::Reserved || self.history.no_replay {return Err(bad());}
        remaining(self.deadline)?;
        self.history.phase=StationBrokerPhase::Launching;
        self.note(DiagnosticStage::HostAssociation);
        self.host_thread=unsafe{GetCurrentThreadId()};self.host_station=unsafe{GetProcessWindowStation()};
        self.host_desktop=unsafe{GetThreadDesktop(self.host_thread)};
        if self.host_station==0 || self.host_desktop==0 {return Err(os_error());}
        self.set_target(StationBrokerDebt::CallerThreadToken);
        no_impersonation(&mut self.originals.caller_thread_token)?;
        self.set_target(StationBrokerDebt::OwnToken);
        let (own_subject,sid)=subject(unsafe{GetCurrentProcess()},&mut self.originals.own_token)?;
        self.note(DiagnosticStage::Random);
        let mut nonce=[0u8;32];{use rand::RngCore;rand::rngs::OsRng.try_fill_bytes(&mut nonce).map_err(|_|bad())?;}
        if nonce==[0;32] {return Err(bad());}
        let name=endpoint(&nonce);
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::SecurityDescriptor);sd(&mut self.originals.sd,&sid,0x0012019f)?;
        self.set_target(StationBrokerDebt::Control);self.note(DiagnosticStage::PipeCreate);self.channel.pipe.begin()?;
        let sa=attributes(self.originals.sd.raw,false);
        let raw=unsafe{CreateNamedPipeW(wide(&name).as_ptr(),0x40080003,0x0000000e,1,
            FRAME_BYTES as u32,FRAME_BYTES as u32,0,&sa)}; // OVERLAPPED,FIRST_INSTANCE,DUPLEX; message/readmode/reject-remote.
        let error=if raw == -1 {unsafe{GetLastError()}}else{0};self.channel.pipe.captured(raw != -1,raw,error)?;
        self.set_target(StationBrokerDebt::SecurityDescriptor);self.originals.sd.close()?;
        self.note(DiagnosticStage::PipeConnect);self.channel.events()?;self.channel.connect_server()?;self.spawn(&name)?;
        self.history.phase=StationBrokerPhase::Authenticating;
        self.set_target(StationBrokerDebt::Control);self.channel.connected(self.deadline)?;
        self.note(DiagnosticStage::PeerIdentity);let mut peer=0;check(unsafe{GetNamedPipeClientProcessId(self.channel.pipe.raw,&mut peer)})?;
        if Some(peer)!=self.history.helper_pid {return Err(bad());}
        self.set_target(StationBrokerDebt::PeerToken);
        let (peer_subject,_)=subject(self.originals.process.raw,&mut self.originals.peer_token)?;
        if peer_subject!=own_subject {return Err(bad());}
        let (parent_pid,parent_time)=identity(unsafe{GetCurrentProcess()})?;
        let binding=Binding {generation:self.history.generation,parent_pid,parent_time,
            helper_pid:self.history.helper_pid.ok_or_else(bad)?,helper_time:self.history.helper_creation_filetime_100ns.ok_or_else(bad)?,
            deadline_ticks:shared_deadline(self.deadline)?,nonce};
        self.binding=Some(binding);self.set_target(StationBrokerDebt::Protocol);
        self.note(DiagnosticStage::HelloSend);self.channel.send(message(HELLO,binding,self.pin.sha256()),self.deadline)?;
        self.note(DiagnosticStage::OfferReceive);let offered=self.channel.receive(OFFER,Some(binding),self.deadline)?;
        if offered.image!=self.pin.sha256() {return Err(bad());}
        self.history.phase=StationBrokerPhase::Offered;
        self.set_target(StationBrokerDebt::WindowStation);
        remaining(self.deadline)?;
        self.note(DiagnosticStage::DuplicateStation);pull(&mut self.originals.station,self.originals.process.raw,offered.station)?;
        self.set_target(StationBrokerDebt::Desktop);
        remaining(self.deadline)?;
        self.note(DiagnosticStage::DuplicateDesktop);pull(&mut self.originals.desktop,self.originals.process.raw,offered.desktop)?;
        let (station,desktop)=names(binding);
        self.set_target(StationBrokerDebt::Authentication);
        if object_text(self.originals.station.raw,2)?!=station || object_text(self.originals.desktop.raw,2)?!=desktop
            || !object_text(self.originals.station.raw,3)?.eq_ignore_ascii_case("WindowStation")
            || !object_text(self.originals.desktop.raw,3)?.eq_ignore_ascii_case("Desktop")
            || descriptor_hash(self.originals.station.raw)?!=offered.station_sd
            || descriptor_hash(self.originals.desktop.raw)?!=offered.desktop_sd {return Err(bad());}
        self.history.phase=StationBrokerPhase::AliasesHeld;self.note(DiagnosticStage::HostAssociation);self.host_unchanged()?;
        self.note(DiagnosticStage::AcceptSend);self.channel.send(message(ACCEPT,binding,self.pin.sha256()),self.deadline)?;
        self.note(DiagnosticStage::ClosedReceive);let closed=self.channel.receive(CLOSED,Some(binding),self.deadline)?;
        if closed.image!=self.pin.sha256() {return Err(bad());}
        self.history.helper_objects_closed_receipt=true;
        if let Some(lease)=self.diagnostic.as_mut(){lease.set_helper_close(StationBrokerHelperDiagnosticClose::AuthenticatedReceipt);}
        self.reap_helper()?;
        self.note(DiagnosticStage::HostAssociation);self.host_unchanged()?;remaining(self.deadline)?;
        self.history.phase=StationBrokerPhase::HelperRetired;self.note(DiagnosticStage::HelperRetired);self.publish(false);Ok(())
    }
    fn reap_helper(&mut self)->io::Result<()> {
        self.set_target(StationBrokerDebt::Process);self.note(DiagnosticStage::ProcessWait);wait(self.originals.process.raw,self.deadline)?;
        self.note(DiagnosticStage::ProcessDword);let mut code=0;check(unsafe{GetExitCodeProcess(self.originals.process.raw,&mut code)})?;
        self.history.helper_raw_dword=Some(code);if code!=0 {return Err(bad());}remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::MainThread);wait(self.originals.thread.raw,self.deadline)?;
        check(unsafe{GetExitCodeThread(self.originals.thread.raw,&mut code)})?;if code!=0 {return Err(bad());}remaining(self.deadline)?;
        for i in 0..2 {
            self.set_target(if i==0 {StationBrokerDebt::StdoutReader}else{StationBrokerDebt::StderrReader});
            // A genuine Win32 join of the ORIGINAL CreateThread HANDLE: signal
            // includes native/TLS termination; same-handle DWORD then checkedClose.
            // No detached Rust JoinHandle or substitute anchor is used here.
            wait(self.originals.readers[i].raw,self.deadline)?;
            check(unsafe{GetExitCodeThread(self.originals.readers[i].raw,&mut code)})?;
            if code!=0 {return Err(bad());}
            let capture=self.readers[i].result.try_lock().map_err(|_|bad())?.ok_or_else(bad)?;
            if !reader_completed(code,capture) {return Err(bad());}
            self.history.original_eof[i]=true;self.history.original_reader_joined[i]=true;
            remaining(self.deadline)?;
            self.note(DiagnosticStage::OriginalClose);
            self.originals.readers[i].close()?;
        }
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Stdout);self.originals.stdout.close()?;
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Stderr);self.originals.stderr.close()?;
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::MainThread);self.note(DiagnosticStage::OriginalClose);self.originals.thread.close()?;
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Process);self.note(DiagnosticStage::OriginalClose);self.originals.process.close()?;
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Control);self.channel.close()?;remaining(self.deadline)?;
        if let Some(lease)=self.diagnostic.as_mut() {
            if lease.snapshot().helper_close!=StationBrokerHelperDiagnosticClose::AuthenticatedReceipt {return Err(bad());}
            lease.set_helper_close(StationBrokerHelperDiagnosticClose::AuthenticatedReceiptAndOriginalNativeJoins);
        }
        Ok(())
    }
    pub(crate) fn retire_aliases(&mut self)->io::Result<()> {
        if !helper_retired_gate(&self.snapshot()) {return Err(bad());}
        self.note(DiagnosticStage::HostAssociation);remaining(self.deadline)?;self.host_unchanged()?;remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::Desktop);self.originals.desktop.close()?;
        remaining(self.deadline)?;
        self.set_target(StationBrokerDebt::WindowStation);self.originals.station.close()?;
        remaining(self.deadline)?;
        if !self.originals.all_retired() || !self.channel.retired() {return Err(bad());}
        self.note(DiagnosticStage::Admission);
        if self.diagnostic.is_some() {
            self.channel.diagnostic=None; // Retire nonowning alias BEFORE unmap.
            self.set_target(StationBrokerDebt::DiagnosticView);
            let history=self.snapshot();let result=self.diagnostic.as_mut().ok_or_else(bad)?.retire(self.deadline,&history);
            if self.diagnostic.as_ref().is_some_and(|d|d.snapshot().view==StationBrokerHandle::ClosedVerified) {
                self.set_target(StationBrokerDebt::DiagnosticMapping);
            }
            result?;
        }
        remaining(self.deadline)?;
        self.history.phase=StationBrokerPhase::ResourceOnlyClosed;Ok(())
    }
}
fn pull(slot:&mut Slot,source_process:isize,source_value:u64)->io::Result<()> {
    let source=isize::try_from(source_value).map_err(|_|bad())?;if source<=0 {return Err(bad());}
    slot.begin()?;let ok=unsafe{DuplicateHandle(source_process,source,GetCurrentProcess(),&mut slot.raw,0,0,2)};
    let error=if ok==0 {unsafe{GetLastError()}}else{0};slot.captured(ok!=0,slot.raw,error)
}

struct HelperCore {
    originals:Originals,channel:Box<Channel>,deadline:Instant,
    old_station:isize,old_desktop:isize,thread:u32,
    failure:Option<StationBrokerFailure>,target:StationBrokerDebt,
    shared_deadline:Option<u64>,
    allocation_attempted:bool,
    diagnostic_case:Option<diagnostic::Case>,diagnostic:Option<DiagnosticLease>,
    diagnostic_binding:Option<Binding>,diagnostic_stage:DiagnosticStage,association_restored:bool,
}
impl HelperCore {
    fn new()->Self {Self {originals:Originals::new(),channel:Channel::new(),
        deadline:Instant::now()+Duration::from_secs(HELPER_SECONDS),old_station:0,old_desktop:0,
        thread:0,failure:None,target:StationBrokerDebt::Authentication,shared_deadline:None,allocation_attempted:false,
        diagnostic_case:None,diagnostic:None,diagnostic_binding:None,
        diagnostic_stage:DiagnosticStage::HelperAuthenticated,association_restored:false}}
    fn snapshot(&self)->StationBrokerSnapshot {
        let mut h=StationBrokerSnapshot::new(self.diagnostic_binding.map(|b|b.generation).unwrap_or(0));
        if let Some(b)=self.diagnostic_binding {h.helper_pid=Some(b.helper_pid);h.helper_creation_filetime_100ns=Some(b.helper_time);}
        h.first_failure=self.failure;h.no_replay=self.failure.is_some();h.host_association_unchanged=self.association_restored;
        if h.no_replay {h.phase=StationBrokerPhase::Unknown;}
        h.deadline_expired_observed=self.failure.is_some_and(|f|f.kind==StationBrokerErrorKind::TimedOut);
        let o=&self.originals;h.station_alias=o.station.state;h.desktop_alias=o.desktop.state;
        h.slots=StationBrokerSlots {process:o.process.state,main_thread:o.thread.state,
            stdout:o.stdout.state,stderr:o.stderr.state,child_stdout:o.child_out.state,child_stderr:o.child_err.state,
            stdin_null:o.nul.state,inherited_parent:o.parent.state,own_token:o.own_token.state,peer_token:o.peer_token.state,
            caller_thread_token:o.caller_thread_token.state,station_sd:o.sd.state,desktop_sd:o.desktop_sd.state,
            readers:[o.readers[0].state,o.readers[1].state],control:self.channel.pipe.state,
            read_event:self.channel.read_event.state,write_event:self.channel.write_event.state,
            read_pending:self.channel.read_pending,write_pending:self.channel.write_pending};
        if let Some(lease)=self.diagnostic.as_ref(){h.diagnostic=lease.snapshot();}h
    }
    fn note(&mut self,stage:DiagnosticStage){self.diagnostic_stage=stage;self.publish(true);}
    fn set_target(&mut self,target:StationBrokerDebt){self.target=target;self.note(diagnostic::stage_for(target));}
    fn publish(&mut self,in_flight:bool) {
        let history=self.snapshot();
        if let Some(lease)=self.diagnostic.as_mut(){lease.publish(self.diagnostic_stage,in_flight,&history);}
    }
    fn checkpoint(&self)->io::Result<()> {
        remaining(self.deadline)?;
        if let Some(end)=self.shared_deadline {let (now,freq)=counter()?;shared_remaining(end,now,freq)?;}
        Ok(())
    }
    fn run(&mut self,endpoint:&str,parent_inherited:isize)->io::Result<()> {
        // Validate provenance WITHOUT adopting/closing an arbitrary numeric input.
        // The only accepted selector is an inherited original creating parent.
        let mut flags=0;check(unsafe{windows_sys::Win32::Foundation::GetHandleInformation(parent_inherited,&mut flags)})?;
        if flags&1==0 {return Err(bad());}
        let creator=current_creator()?;
        let parent_identity=identity(parent_inherited)?;
        if creator==0 || creator!=parent_identity.0 {return Err(bad());}
        // A0's only public launch fixture is the reviewed setup PE launching
        // itself. A1's different production caller is NOT silently authorized.
        if image_path(parent_inherited)?!=image_path(unsafe{GetCurrentProcess()})?
            || unsafe{WaitForSingleObject(parent_inherited,0)}!=258 {return Err(bad());}
        self.allocation_attempted=true;
        self.set_target(StationBrokerDebt::CallerThreadToken);
        no_impersonation(&mut self.originals.caller_thread_token)?;
        self.set_target(StationBrokerDebt::Control);self.channel.pipe.begin()?;
        let pipe=unsafe{CreateFileW(wide(endpoint).as_ptr(),0xc0000000,0,ptr::null(),3,0x40100000,0)};
        // SQOS_PRESENT|ANONYMOUS: a forged endpoint never gets an impersonation
        // token from this higher-privilege helper before peer checks finish.
        let error=if pipe == -1 {unsafe{GetLastError()}}else{0};self.channel.pipe.captured(pipe != -1,pipe,error)?;
        let mut server=0;check(unsafe{GetNamedPipeServerProcessId(self.channel.pipe.raw,&mut server)})?;
        if server!=creator {return Err(bad());}
        self.originals.parent.begin()?;self.originals.parent.captured(true,parent_inherited,0)?;
        check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(parent_inherited,1,0)})?;
        self.set_target(StationBrokerDebt::OwnToken);
        let (own,sid)=subject(unsafe{GetCurrentProcess()},&mut self.originals.own_token)?;
        self.set_target(StationBrokerDebt::PeerToken);
        let (parent,_)=subject(parent_inherited,&mut self.originals.peer_token)?;
        if own!=parent {return Err(bad());}
        // Both ends use the same fixed message boundary, including the client.
        let mode=2u32;check(unsafe{SetNamedPipeHandleState(self.channel.pipe.raw,&mode,ptr::null(),ptr::null())})?;
        self.set_target(StationBrokerDebt::Control);
        self.channel.events()?;
        self.set_target(StationBrokerDebt::Protocol);
        let hello=self.channel.receive(HELLO,None,self.deadline)?;
        let own_identity=identity(unsafe{GetCurrentProcess()})?;
        if endpoint!=super::endpoint(&hello.binding.nonce)
            || (hello.binding.parent_pid,hello.binding.parent_time)!=parent_identity
            || (hello.binding.helper_pid,hello.binding.helper_time)!=own_identity || hello.image==[0;32] {return Err(bad());}
        let (now,freq)=counter()?;
        self.deadline=self.deadline.min(Instant::now()+shared_remaining(hello.binding.deadline_ticks,now,freq)?);
        self.shared_deadline=Some(hello.binding.deadline_ticks);self.checkpoint()?;
        self.diagnostic_binding=Some(hello.binding);
        if let Some(case)=self.diagnostic_case {
            // Only AFTER original creator/peer/security/HELLO validation. No
            // prebinding helper record is trusted or synthesized on failure.
            let scope=diagnostic::Scope {generation:hello.binding.generation,
                parent_pid:parent_identity.0,parent_time:parent_identity.1};
            self.diagnostic=Some(DiagnosticLease::new(case,hello.image,scope,diagnostic::Writer::Helper));
            self.set_target(StationBrokerDebt::DiagnosticMapping);
            let lease=self.diagnostic.as_mut().ok_or_else(bad)?;
            lease.set_verified_helper(own_identity.0,own_identity.1)?;
            let result=lease.acquire(self.deadline);
            if self.diagnostic.as_ref().is_some_and(|d|d.snapshot().view!=StationBrokerHandle::NotAllocated) {
                self.set_target(StationBrokerDebt::DiagnosticView);
            }
            result?;
            self.note(DiagnosticStage::HelperAuthenticated);
            self.channel.diagnostic=self.diagnostic.as_ref().and_then(|lease|lease.channel_writer());
        }
        // Parent sent HELLO only after verifying THIS process against its held
        // exact setup pin. The kernel peer + original creating-parent handle +
        // full security-context comparison authenticate the initial channel.
        self.note(DiagnosticStage::AdoptStdio);self.adopt_stdio()?;
        self.note(DiagnosticStage::HostAssociation);self.thread=unsafe{GetCurrentThreadId()};self.old_station=unsafe{GetProcessWindowStation()};
        self.old_desktop=unsafe{GetThreadDesktop(self.thread)};
        if self.old_station==0 || self.old_desktop==0 {return Err(os_error());}
        let (station,desktop)=names(hello.binding);
        self.set_target(StationBrokerDebt::SecurityDescriptor);sd(&mut self.originals.sd,&sid,STATION_ACCESS)?;
        self.checkpoint()?;
        self.originals.station.begin()?;self.set_target(StationBrokerDebt::WindowStation);self.note(DiagnosticStage::StationCreate);
        let sa=attributes(self.originals.sd.raw,false);
        let raw=unsafe{CreateWindowStationW(wide(&station).as_ptr(),1,STATION_ACCESS,&sa)};
        let error=if raw==0 {unsafe{GetLastError()}}else{0};self.originals.station.captured(raw!=0,raw,error)?;
        self.set_target(StationBrokerDebt::Association);self.note(DiagnosticStage::StationSelect);check(unsafe{SetProcessWindowStation(self.originals.station.raw)})?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::SecurityDescriptor);sd(&mut self.originals.desktop_sd,&sid,DESKTOP_ACCESS)?;
        self.set_target(StationBrokerDebt::Desktop);self.note(DiagnosticStage::DesktopCreate);self.originals.desktop.begin()?;
        let sa=attributes(self.originals.desktop_sd.raw,false);
        let raw=unsafe{CreateDesktopW(wide(&desktop).as_ptr(),ptr::null(),ptr::null_mut(),0,DESKTOP_ACCESS,&sa)};
        let error=if raw==0 {unsafe{GetLastError()}}else{0};self.originals.desktop.captured(raw!=0,raw,error)?;
        self.set_target(StationBrokerDebt::SecurityDescriptor);
        self.originals.desktop_sd.close()?;self.originals.sd.close()?;
        self.note(DiagnosticStage::ObjectVerify);if object_text(self.originals.station.raw,2)?!=station || object_text(self.originals.desktop.raw,2)?!=desktop {return Err(bad());}
        let offer=Frame {kind:OFFER,binding:hello.binding,station:self.originals.station.raw as u64,
            desktop:self.originals.desktop.raw as u64,station_sd:descriptor_hash(self.originals.station.raw)?,
            desktop_sd:descriptor_hash(self.originals.desktop.raw)?,image:hello.image};
        self.set_target(StationBrokerDebt::Protocol);
        self.note(DiagnosticStage::OfferSend);self.channel.send(offer,self.deadline)?;
        // Originals remain held and cannot be replaced/reused while parent pulls.
        self.note(DiagnosticStage::AcceptReceive);let accepted=self.channel.receive(ACCEPT,Some(hello.binding),self.deadline)?;
        if accepted.image!=hello.image {return Err(bad());}
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::Association);
        self.note(DiagnosticStage::AssociationRestore);
        check(unsafe{SetProcessWindowStation(self.old_station)})?;
        check(unsafe{SetThreadDesktop(self.old_desktop)})?;
        if unsafe{GetProcessWindowStation()}!=self.old_station || unsafe{GetThreadDesktop(self.thread)}!=self.old_desktop {return Err(bad());}
        self.association_restored=true;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::Desktop);self.originals.desktop.close()?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::WindowStation);self.originals.station.close()?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::InheritedParent);self.originals.parent.close()?;
        if self.diagnostic.is_some() {
            self.channel.diagnostic=None; // No dangling publisher after actual unmap.
            self.set_target(StationBrokerDebt::DiagnosticView);let history=self.snapshot();
            let result=self.diagnostic.as_mut().ok_or_else(bad)?.retire(self.deadline,&history);
            if self.diagnostic.as_ref().is_some_and(|d|d.snapshot().view==StationBrokerHandle::ClosedVerified) {
                self.set_target(StationBrokerDebt::DiagnosticMapping);
            }
            result?;
            if !self.diagnostic.as_ref().is_some_and(|d|d.snapshot().closed()){return Err(bad());}
        }
        // CLOSED covers created objects/associations/SD. Endpoint/stdio closure
        // is later proven by helper native exit0 and the parent's two EOF/joins.
        self.note(DiagnosticStage::ClosedSend);self.channel.send(message(CLOSED,hello.binding,hello.image),self.deadline)?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::Control);self.channel.close()?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::Stdout);self.originals.stdout.close()?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::Stderr);self.originals.stderr.close()?;
        self.checkpoint()?;
        self.set_target(StationBrokerDebt::StdinNull);self.originals.nul.close()?;
        if !self.originals.all_retired() || !self.channel.retired()
            || self.diagnostic.as_ref().is_some_and(|lease|!lease.snapshot().closed()) {return Err(bad());}
        self.checkpoint()?;Ok(())
    }
    fn adopt_stdio(&mut self)->io::Result<()> {
        use windows_sys::Win32::System::Console::GetStdHandle;
        // Only after authenticating the original creator and its fixed handle-list
        // launch do standard handles become the helper's own inherited aliases.
        // Capture ALL three before a fallible flag change, so a partial failure
        // never leaves a later inherited original represented as NotAllocated.
        let mut first=None;
        for (slot,which) in [(&mut self.originals.nul,-10i32),(&mut self.originals.stdout,-11),(&mut self.originals.stderr,-12)] {
            slot.begin()?;let raw=unsafe{GetStdHandle(which as u32)};
            let error=if raw == -1 {unsafe{GetLastError()}}else{0};
            if let Err(e)=slot.captured(raw!=0 && raw != -1,raw,error) {
                if first.is_none() {first=Some(if raw==0 {bad()}else{e});}
            }
        }
        if let Some(error)=first {return Err(error);}
        let (input,output,error)=(self.originals.nul.raw,self.originals.stdout.raw,self.originals.stderr.raw);
        if input==output || output==error || error==input {return Err(bad());}
        for raw in [input,output,error] {
            check(unsafe{windows_sys::Win32::Foundation::SetHandleInformation(raw,1,0)})?;
        }
        Ok(())
    }
}
pub(super) fn helper_entry(endpoint:&str,parent:isize)->i32 {
    helper_entry_diagnostic(endpoint,parent,None)
}
pub(super) fn helper_entry_diagnostic(endpoint:&str,parent:isize,case:Option<diagnostic::Case>)->i32 {
    let mut original=Box::new(HelperCore::new());
    original.diagnostic_case=case;
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||original.run(endpoint,parent)));
    match result {
        Ok(Ok(()))=>0,
        other=>{
            original.failure=Some(match other {
                Ok(Err(e))=>{
                    if let Some(lease)=original.diagnostic.as_mut(){lease.first_io(e.kind());}
                    StationBrokerFailure::from_io(original.target,&e)
                },
                _=>StationBrokerFailure {target:StationBrokerDebt::Unwind,kind:StationBrokerErrorKind::Unwind,
                    win32_error:None,native_ntstatus:None},
            });
            original.publish(false); // Existing held view only; no new API/IO on Unknown.
            if !original.allocation_attempted {
                // Known unbound rejection before any owned broker allocation.
                return 78;
            }
            // Retain the SAME finite core and existing main process on failure.
            // No stack-drop cleanup, kill, replay or replacement owner. Spurious
            // park wakeups neither retry native operations nor allocate workers.
            loop {std::hint::black_box(&original);std::thread::park();}
        },
    }
}

#[cfg(test)]
#[path="station_broker_tests.rs"]
mod tests;
