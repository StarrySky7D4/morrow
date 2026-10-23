//! Existing-P opt-in producer and C original-capability inlet, Source047.
//! Private sibling of the separate A1 helper: primitives retain Apache-2.0
//! frozen046 station_broker_native provenance recorded in the parent module.
//! No Drop/TerminateProcess/job termination, retry, alternate identity or wire.
use super::*;
use crate::asuser_parent_record::{Record,FileIdentity,BYTES,MAX_FILES};
use crate::asuser_parent_diagnostic::{Capture,Scope,Stage,Api,ParentExitPredicate,ParentExitFailure};
use crate::asuser_startup_observation::{StartupCarrier,StartupObservation,SdkStage};
use crate::asuser_error_diagnostic::{ErrorDiagnosticRole,StartupFailureTrigger};
use crate::asuser_error_diagnostic_native::RetainedErrorDiagnostic;
use std::{path::{Path,PathBuf,Component,Prefix},sync::{Arc,atomic::{AtomicBool,Ordering}},
    fs::File,mem::ManuallyDrop,os::windows::{io::{FromRawHandle,AsRawHandle,BorrowedHandle},fs::FileExt}};
use windows_sys::Win32::Foundation::{CompareObjectHandles,GetHandleInformation};
use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandleEx,FileIdInfo,
    FILE_ID_INFO,GetFileInformationByHandle,BY_HANDLE_FILE_INFORMATION,GetFinalPathNameByHandleW};
use windows_sys::Win32::System::JobObjects::{QueryInformationJobObject,JobObjectBasicAccountingInformation,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION};
use windows_sys::Win32::System::Console::{GetStdHandle,STD_INPUT_HANDLE,STD_OUTPUT_HANDLE,STD_ERROR_HANDLE};
#[path="asuser_refusal_qualification.rs"]
mod refusal_qualification;
pub use refusal_qualification::CallerRefusalQualification;
#[path="asuser_parent_session_admission.rs"]
mod session_admission;

const PREFIX:&str="--c28-parent-launch-p1";
const SESSION_PREFIX:&str="--c28-parent-refusal-session-p1";
const IMPORT:&str="--c28-parent-origin-p1";
const C_NAME:&str="morrow-protected-checked-vm-harness.exe";
const P_NAME:&str="morrow-workbench-supervisor.exe";
const R_NAME:&str="codex-command-runner.exe";
const S_NAME:&str="codex-windows-sandbox-setup.exe";
const LOCAL_FILES:usize=24;
// Bootstrap direction contract: P writes, C inherits a read-only client.
// Preserve OVERLAPPED | FIRST_PIPE_INSTANCE; never widen this pipe to duplex.
const BOOTSTRAP_OUTBOUND_SERVER_OPEN_MODE:u32=0x40000000|0x00080000|0x00000002;
const BOOTSTRAP_READ_ONLY_CLIENT_ACCESS:u32=0x80000000;
/// The existing workbench P supplies ONE concrete retained native-pipe owner.
/// This dependency-inverted seam adds no Cargo edge from codex back to Morrow.
/// Cross-version HANDLE values never masquerade as a shared Rust HANDLE type:
/// only std BorrowedHandle lifetimes cross this private local call boundary.
pub struct ParentCreate<'a> {
    pub application:&'a Path,pub cwd:&'a Path,pub arguments:&'a [String],
    pub deadline:Instant,
    // These three system values come from the same opaque native system-root
    // observation below. The existing trusted-host builder may add only its
    // original token-derived LOCALAPPDATA; never import ambient environment.
    pub environment:&'a std::collections::BTreeMap<String,String>,
    pub extra_inherited:&'a [BorrowedHandle<'a>],pub stdio:[BorrowedHandle<'a>;3],
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ParentChildIdentity {pub pid:u32,pub thread_id:u32,pub creation_filetime:u64}
#[derive(Clone,Copy)]
pub struct ParentNativeExit {
    pub process_signaled:bool,pub thread_signaled:bool,pub process_dword:u32,
    // The retaining library does not export these three native fields. None
    // cannot become a clean zero: P fills them only from the same ORIGINAL
    // thread/Job read-only queries below, after actual signal observations.
    pub thread_dword:Option<u32>,pub job_terminated:Option<u32>,pub job_total:Option<u32>,
    pub identity:ParentChildIdentity,pub job_active:u32,
}
/// Not constructible from a caller boolean, record or public counter tuple.
/// It follows actual C0 on the fixed trusted C program's full A1-only exit path;
/// ROOT still separately observes its ORIGINAL outer readers' two EOF and joins.
pub struct ParentCClosed {identity:ParentChildIdentity,deadline:Instant}
impl ParentCClosed {
    pub fn identity(&self)->ParentChildIdentity{self.identity}
    pub fn deadline(&self)->Instant{self.deadline}
}
struct ParentC0Observed {identity:ParentChildIdentity}
impl ParentC0Observed {
    fn identity(&self)->ParentChildIdentity{self.identity}
    fn after_original_alias_closes(self,parent:&Parent)->io::Result<ParentCClosed> {
        remaining(parent.deadline)?;
        if parent.first.is_some()||!parent.created||!parent.resume_started||!parent.namespace_created
            ||!parent.namespace_creation_started||!parent.remote_transfer_started||parent.child_retired
            ||parent.count==0||parent.count>MAX_FILES
            ||parent.files[..parent.count].iter().any(|slot|slot.state!=HandleState::ClosedVerified)
            ||parent.files[parent.count..].iter().any(|slot|slot.state!=HandleState::NotAllocated)
            ||parent.stdio.iter().any(|slot|slot.state!=HandleState::ClosedVerified)
            ||[&parent.parent,&parent.reader,&parent.namespace,&parent.sd,&parent.namespace_sd]
                .iter().any(|slot|slot.state!=HandleState::ClosedVerified)
            ||parent.token.state!=HandleState::ClosedVerified||parent.precreate_token.state!=HandleState::ClosedVerified
            ||parent.thread_token.state!=HandleState::Rejected||parent.thread_token.raw!=0
            ||parent.bootstrap.pending||!parent.bootstrap.used||parent.bootstrap.pipe.state!=HandleState::ClosedVerified
            ||parent.bootstrap.event.state!=HandleState::ClosedVerified
            ||!parent.remote_self.is_some_and(|value|value>0){return Err(bad());}
        Ok(ParentCClosed{identity:self.identity,deadline:parent.deadline})
    }
}
pub trait ParentOriginalProcess {
    // The supplied owner constructor MUST be allocation-only and preinstall
    // all Job/PI/resume/close slots. Method implementation uses only the new
    // reviewed retaining native-pipe API, never legacy JobChild/spawn_encoded.
    fn create_suspended(&mut self,input:ParentCreate<'_>)->io::Result<ParentChildIdentity>;
    fn original_process(&mut self)->io::Result<BorrowedHandle<'_>>;
    fn original_primary_thread(&mut self)->io::Result<BorrowedHandle<'_>>;
    fn original_job(&mut self)->io::Result<BorrowedHandle<'_>>;
    fn begin_bootstrap_duplication(&mut self,source:BorrowedHandle<'_>)->io::Result<usize>;
    fn complete_bootstrap_duplication(&mut self,slot:usize,result:io::Result<()>)->io::Result<()>;
    fn resume_original_once(&mut self)->io::Result<()>;
    fn observe_original_exit(&mut self,deadline:Instant)->io::Result<ParentNativeExit>;
    fn close_after_natural_c(&mut self,proof:ParentCClosed)->io::Result<()>;
}
fn parent_c_closed(original:ParentChildIdentity,actual:ParentNativeExit,delivered:bool)->io::Result<ParentC0Observed> {
    use ParentExitPredicate::*;
    // Same acceptance predicates, now retaining the first actually rejected one.
    for (passed,predicate) in [
        (delivered,BootstrapDelivered),(actual.process_signaled,ProcessSignal),
        (actual.thread_signaled,PrimaryThreadSignal),(actual.process_dword==0,ProcessExitNotZero),
        (actual.thread_dword==Some(0),PrimaryThreadExitNotZero),
        (original.pid!=0&&original.thread_id!=0&&original.creation_filetime!=0,OriginalIdentity),
        (actual.identity==original,ChildIdentity),(actual.job_active==0,JobStillActive),
        (actual.job_terminated==Some(0),JobLimitedTermination),
        (actual.job_total.is_some_and(|value|value>0),JobTotalMissingOrZero),
    ] {if !passed{return Err(ParentExitFailure::error(predicate));}}
    // MAX32 limits the transferred ORIGINAL aliases, not the number of real
    // processes ever observed in this Job. Setup/inventory and the full fixed
    // business sequence may naturally create more than32 lifetime processes.
    // Positive total alone proves nothing: original C/thread0, same identity,
    // empty Job, no termination and genuine bootstrap discharge remain gates.
    Ok(ParentC0Observed{identity:original})
}

fn absolute(path:&Path)->io::Result<()> {
    let text=path.to_str().ok_or_else(bad)?;
    if text.len()>32700||text.contains(['\0','"'])||text.split(['\\','/']).any(|v|v=="."||v=="..") {return Err(bad());}
    let mut parts=path.components();
    if !matches!(parts.next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)))
        ||parts.next()!=Some(Component::RootDir)
        ||!parts.all(|p|matches!(p,Component::Normal(v) if !v.to_string_lossy().contains(':'))) {return Err(bad());}
    Ok(())
}
fn handle_text(value:&str)->io::Result<isize> {
    if value.is_empty()||value.len()>19||value.starts_with('0')||!value.bytes().all(|b|b.is_ascii_digit()) {return Err(bad());}
    value.parse::<isize>().ok().filter(|v|*v>0).ok_or_else(bad)
}
fn hex(value:&str)->io::Result<[u8;32]> {
    if value.len()!=64||!value.bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v)){return Err(bad());}
    let mut result=[0;32];for (i,p) in value.as_bytes().chunks_exact(2).enumerate() {
        let n=|v:u8|if v<=b'9'{v-b'0'}else{v-b'a'+10};result[i]=(n(p[0])<<4)|n(p[1]);
    }if result==[0;32]{return Err(bad());}Ok(result)
}
fn hex_text(value:[u8;32])->String {value.iter().map(|v|format!("{v:02x}")).collect()}
fn file_path(raw:isize)->io::Result<PathBuf> {
    let mut units=[0u16;32768];let length=unsafe{GetFinalPathNameByHandleW(raw,units.as_mut_ptr(),units.len() as u32,0)} as usize;
    if length==0{return Err(os_error());}if length>=units.len(){return Err(bad());}
    let text=String::from_utf16(&units[..length]).map_err(|_|bad())?;
    let text=text.strip_prefix(r"\\?\").ok_or_else(bad)?;
    let path=PathBuf::from(text);absolute(&path)?;Ok(path)
}
fn file_identity(raw:isize,kind:u8)->io::Result<FileIdentity> {
    let mut basic:BY_HANDLE_FILE_INFORMATION=unsafe{mem::zeroed()};
    check(unsafe{GetFileInformationByHandle(raw,&mut basic)})?;
    let mut id:FILE_ID_INFO=unsafe{mem::zeroed()};
    check(unsafe{GetFileInformationByHandleEx(raw,FileIdInfo,(&mut id as *mut FILE_ID_INFO).cast(),mem::size_of::<FILE_ID_INFO>() as u32)})?;
    let ft=|v:FILETIME|(u64::from(v.dwHighDateTime)<<32)|u64::from(v.dwLowDateTime);
    let length=(u64::from(basic.nFileSizeHigh)<<32)|u64::from(basic.nFileSizeLow);
    if basic.dwFileAttributes&0x400!=0||basic.nNumberOfLinks!=1||id.FileId.Identifier==[0;16]
        ||(basic.dwFileAttributes&0x10!=0)!=matches!(kind,2|4|5)
        ||matches!(kind,1|3)&&(length==0||length>512*1024*1024){return Err(bad());}
    Ok(FileIdentity{alias:raw as u64,volume:id.VolumeSerialNumber,id:id.FileId.Identifier,
        attributes:basic.dwFileAttributes,kind,length,creation:ft(basic.ftCreationTime),modified:ft(basic.ftLastWriteTime)})
}
fn file_digest(raw:isize,expected:[u8;32])->io::Result<()> {
    let file=ManuallyDrop::new(unsafe{File::from_raw_handle(raw as *mut c_void)});
    let mut header=[0u8;64];if file.seek_read(&mut header,0)?!=64||header[..2]!=*b"MZ"{return Err(bad());}
    let pe_offset=u32::from_le_bytes(header[60..64].try_into().map_err(|_|bad())?) as u64;
    let mut pe=[0u8;26];if pe_offset>512*1024*1024-26||file.seek_read(&mut pe,pe_offset)?!=26
        ||pe[..6]!=[b'P',b'E',0,0,0x64,0x86]||pe[24..26]!=[0x0b,2]{return Err(bad());}
    let mut digest=Sha256::new();let mut buffer=[0u8;32768];let mut offset=0u64;
    loop {let length=file.seek_read(&mut buffer,offset)?;if length==0{break;}
        offset=offset.checked_add(length as u64).filter(|v|*v<=512*1024*1024).ok_or_else(bad)?;
        digest.update(&buffer[..length]);}
    if <[u8;32]>::from(digest.finalize())!=expected{return Err(bad());}Ok(())
}
fn inherited(raw:isize)->io::Result<()> {
    let mut flags=0;check(unsafe{GetHandleInformation(raw,&mut flags)})?;
    if flags&1==0{return Err(bad());}Ok(())
}
// The 2048-byte transfer has its OWN stable allocation; a 512-byte A1 Channel
// is never repurposed, and pending I/O is never cancelled/reused/freed.
struct Bootstrap {pipe:Slot,event:Slot,operation:OVERLAPPED,bytes:[u8;BYTES],pending:bool,used:bool}
fn observed_check(diagnostic:&mut Capture,api:Api,returned:i32)->io::Result<()> {
    // check is the SAME existing GetLastError path, once. Never re-query for diagnostics.
    let result=check(returned);
    diagnostic.native(api,returned!=0,result.as_ref().err().and_then(io::Error::raw_os_error));
    result
}
impl Bootstrap {
    fn new()->Box<Self>{Box::new(Self{pipe:Slot::new(CloseKind::Kernel),event:Slot::new(CloseKind::Kernel),
        operation:unsafe{mem::zeroed()},bytes:[0;BYTES],pending:false,used:false})}
    fn event(&mut self,diagnostic:&mut Capture)->io::Result<()> {
        self.event.begin()?;let raw=unsafe{CreateEventW(ptr::null(),1,0,ptr::null())};let error=if raw==0{unsafe{GetLastError()}}else{0};
        diagnostic.native(Api::CreateEvent,raw!=0,if raw==0{Some(error as i32)}else{None});
        self.event.captured(raw!=0,raw,error)?;self.operation.hEvent=raw;Ok(())
    }
    fn completed(&mut self,deadline:Instant,diagnostic:&mut Capture)->io::Result<u32>{
        wait(self.event.raw,deadline)?;let mut length=0;
        observed_check(diagnostic,Api::GetOverlappedResult,unsafe{GetOverlappedResult(self.pipe.raw,&mut self.operation,&mut length,0)})?;
        self.pending=false;Ok(length)
    }
    fn transfer(&mut self,write:bool,deadline:Instant,diagnostic:&mut Capture)->io::Result<()> {
        remaining(deadline)?;if self.used||self.pending{return Err(bad());}self.used=true;
        observed_check(diagnostic,Api::ResetEvent,unsafe{ResetEvent(self.event.raw)})?;self.pending=true;
        self.operation=unsafe{mem::zeroed()};self.operation.hEvent=self.event.raw;let mut length=0;
        let ok=unsafe{if write {WriteFile(self.pipe.raw,self.bytes.as_ptr(),BYTES as u32,&mut length,&mut self.operation)}
            else {ReadFile(self.pipe.raw,self.bytes.as_mut_ptr(),BYTES as u32,&mut length,&mut self.operation)}};
        if ok==0 {let error=unsafe{GetLastError()};diagnostic.native(if write{Api::WriteFile}else{Api::ReadFile},false,Some(error as i32));if error!=997{return Err(io::Error::from_raw_os_error(error as i32));}
            length=self.completed(deadline,diagnostic)?;}else{diagnostic.native(if write{Api::WriteFile}else{Api::ReadFile},true,None);self.pending=false;}
        if length!=BYTES as u32{return Err(bad());}remaining(deadline)?;Ok(())
    }
    fn close(&mut self)->io::Result<()> {if self.pending{return Err(bad());}self.pipe.close()?;self.event.close()}
}

struct Parent {
    diagnostic:Capture,
    // Independent stable frame/out-DWORD; no native handle and no data-Arc lease.
    error_output:RetainedErrorDiagnostic,
    files:[Slot;LOCAL_FILES],identities:[FileIdentity;MAX_FILES],paths:Vec<PathBuf>,count:usize,
    original_child:Box<dyn ParentOriginalProcess>,parent:Slot,stdio:[Slot;3],reader:Slot,token:Slot,
    thread_token:Slot,sd:Slot,bootstrap:Box<Bootstrap>,
    // Remote value is NOT a local handle and is never CloseHandle/DUP_CLOSE_SOURCE'd.
    remote_self:Option<u64>,remote_transfer_started:bool,child_retired:bool,
    first:Option<io::Error>,deadline:Instant,created:bool,resume_started:bool,
    defaults:Option<TrustedDefaults>,
    namespace_sd:Slot,namespace:Slot,namespace_creation_started:bool,namespace_created:bool,precreate_token:Slot,
}
impl Parent {
    fn new(original_child:Box<dyn ParentOriginalProcess>)->Box<Self>{Box::new(Self{diagnostic:Capture::new(Scope::Parent),error_output:RetainedErrorDiagnostic::prepare(),files:std::array::from_fn(|_|Slot::new(CloseKind::Kernel)),
        identities:[FileIdentity::EMPTY;MAX_FILES],paths:Vec::with_capacity(MAX_FILES),count:0,
        original_child,
        parent:Slot::new(CloseKind::Kernel),stdio:std::array::from_fn(|_|Slot::new(CloseKind::Kernel)),
        reader:Slot::new(CloseKind::Kernel),token:Slot::new(CloseKind::Kernel),thread_token:Slot::new(CloseKind::Kernel),
        sd:Slot::new(CloseKind::Local),bootstrap:Bootstrap::new(),
        remote_self:None,remote_transfer_started:false,child_retired:false,first:None,
        deadline:Instant::now()+Duration::from_secs(30),created:false,resume_started:false,defaults:None,
        namespace_sd:Slot::new(CloseKind::Local),namespace:Slot::new(CloseKind::Kernel),
        namespace_creation_started:false,namespace_created:false,precreate_token:Slot::new(CloseKind::Kernel)})}
    fn pin(&mut self,path:&Path,kind:u8)->io::Result<()> {
        absolute(path)?;if self.paths.iter().any(|v|v==path){return Ok(());}
        if self.count>=MAX_FILES{return Err(bad());}let index=self.count;self.count+=1;self.paths.push(path.to_owned());
        let slot=&mut self.files[index];slot.begin()?;
        let raw=unsafe{CreateFileW(wide(path.to_str().ok_or_else(bad)?).as_ptr(),0x80000000,1,
            &attributes(0,true),3,0x00200000|if kind==5{0x02000000}else{0},0)};
        let error=if raw==0||raw == -1{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::CreateFile,raw!=0&&raw != -1,if raw==0||raw == -1{Some(error as i32)}else{None});slot.captured(raw!=0&&raw != -1,raw,error)?;
        let first=file_identity(raw,kind)?;if file_path(raw)?!=path{return Err(bad());}
        self.identities[index]=first;Ok(())
    }
    fn duplicate(slot:&mut Slot,source:isize,desired:u32,diagnostic:&mut Capture)->io::Result<()> {
        slot.begin()?;let mut raw=0;
        let ok=unsafe{DuplicateHandle(GetCurrentProcess(),source,GetCurrentProcess(),&mut raw,desired,1,0)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};diagnostic.native(Api::DuplicateHandle,ok!=0,if ok==0{Some(error as i32)}else{None});slot.captured(ok!=0,raw,error)
    }
    fn checkpoint(&self)->io::Result<()> {remaining(self.deadline)?;Ok(())}
    fn run(&mut self,selection:Selection)->io::Result<()> {
        // Actual P prebirth remains ROOT's independently pinned original launch
        // obligation. This admission is not a late JSON/hash substitute for it.
        self.diagnostic.enter(Stage::ParentSubject);
        no_impersonation(&mut self.thread_token)?;
        let before=identity(unsafe{GetCurrentProcess()})?;
        let (context,caller,integrity)=subject(unsafe{GetCurrentProcess()},&mut self.token,&mut self.defaults)?;
        if integrity!=0x3000{return Err(bad());}
        self.diagnostic.completed(Stage::ParentSubject);
        self.diagnostic.enter(Stage::ParentImage);
        let p=image_path(unsafe{GetCurrentProcess()})?;absolute(&p)?;
        if p.file_name().is_none_or(|name|name!=P_NAME){return Err(bad());}
        let directory=p.parent().ok_or_else(bad)?;let c=directory.join(C_NAME);
        let parents=directory.ancestors().collect::<Vec<_>>();if parents.len()+2>MAX_FILES{return Err(bad());}
        self.diagnostic.completed(Stage::ParentImage);
        self.diagnostic.enter(Stage::ParentPins);
        for ancestor in parents.iter().rev(){self.pin(ancestor,5)?;}
        self.pin(&p,3)?;self.pin(&c,1)?;
        self.diagnostic.completed(Stage::ParentPins);
        self.diagnostic.enter(Stage::ParentDigests);
        file_digest(self.files[self.count-2].raw,selection.p)?;
        file_digest(self.files[self.count-1].raw,selection.c)?;
        self.checkpoint()?;
        self.diagnostic.completed(Stage::ParentDigests);
        // Named bootstrap is one instance, local only, CUser only; no nonce or
        // credential in argv/record. Original inherited parent capability and
        // C-only compiled P table provide first authorization, not pipe EOF.
        self.diagnostic.enter(Stage::BootstrapRandom);
        let mut nonce=[0u8;32];{use rand::RngCore;rand::rngs::OsRng.try_fill_bytes(&mut nonce).map_err(|_|bad())?;}
        if nonce==[0;32]{return Err(bad());}let name=format!(r"\\.\pipe\C28ParentA1-{}",hex_text(nonce));
        self.diagnostic.completed(Stage::BootstrapRandom);
        self.diagnostic.enter(Stage::BootstrapSecurity);
        self.sd.begin()?;let mut raw=ptr::null_mut();let text=wide(&format!("D:P(A;;GRGWRC;;;{})(A;;RC;;;OW)",sid_text(&caller)?));
        let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut raw,ptr::null_mut())};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::SecurityDescriptor,ok!=0,if ok==0{Some(error as i32)}else{None});self.sd.captured(ok!=0,raw as isize,error)?;
        self.diagnostic.completed(Stage::BootstrapSecurity);
        self.diagnostic.enter(Stage::BootstrapEvent);
        self.bootstrap.event(&mut self.diagnostic)?;
        self.diagnostic.completed(Stage::BootstrapEvent);self.diagnostic.enter(Stage::BootstrapServer);
        self.bootstrap.pipe.begin()?;
        let raw=unsafe{CreateNamedPipeW(wide(&name).as_ptr(),BOOTSTRAP_OUTBOUND_SERVER_OPEN_MODE,14,1,BYTES as u32,BYTES as u32,0,&attributes(self.sd.raw,false))};
        let error=if raw == -1||raw==0{unsafe{GetLastError()}}else{0};
        self.diagnostic.native(Api::CreateNamedPipe,raw!=0&&raw != -1,if raw==0||raw == -1{Some(error as i32)}else{None});self.bootstrap.pipe.captured(raw!=0&&raw != -1,raw,error)?;
        self.diagnostic.completed(Stage::BootstrapServer);
        self.diagnostic.enter(Stage::BootstrapConnect);
        self.bootstrap.pending=true;
        let ok=unsafe{ConnectNamedPipe(self.bootstrap.pipe.raw,&mut self.bootstrap.operation)};
        if ok!=0{self.diagnostic.native(Api::ConnectNamedPipe,true,None);self.bootstrap.pending=false;}else{let error=unsafe{GetLastError()};self.diagnostic.native(Api::ConnectNamedPipe,false,Some(error as i32));match error {
            997=>{},535=>self.bootstrap.pending=false,v=>return Err(io::Error::from_raw_os_error(v as i32))}}
        self.diagnostic.completed(Stage::BootstrapConnect);
        self.diagnostic.enter(Stage::BootstrapReader);
        self.reader.begin()?;
        let raw=unsafe{CreateFileW(wide(&name).as_ptr(),BOOTSTRAP_READ_ONLY_CLIENT_ACCESS,0,&attributes(0,true),3,0x40100000,0)};
        let error=if raw == -1||raw==0{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::CreateFile,raw!=0&&raw != -1,if raw==0||raw == -1{Some(error as i32)}else{None});self.reader.captured(raw!=0&&raw != -1,raw,error)?;
        self.diagnostic.completed(Stage::BootstrapReader);
        self.diagnostic.enter(Stage::BootstrapConnected);
        if self.bootstrap.pending{self.bootstrap.completed(self.deadline,&mut self.diagnostic)?;}
        self.diagnostic.completed(Stage::BootstrapConnected);
        self.diagnostic.enter(Stage::ParentAlias);
        Parent::duplicate(&mut self.parent,unsafe{GetCurrentProcess()},crate::asuser_a1_rights::PROCESS_MANAGEMENT,&mut self.diagnostic)?;
        self.diagnostic.completed(Stage::ParentAlias);
        self.diagnostic.enter(Stage::StdioAliases);
        for (index,std) in [STD_INPUT_HANDLE,STD_OUTPUT_HANDLE,STD_ERROR_HANDLE].into_iter().enumerate(){
            let original=unsafe{GetStdHandle(std)};if original==0||original == -1{return Err(bad());}
            Parent::duplicate(&mut self.stdio[index],original,if index==0{0x80000000}else{0x40000000},&mut self.diagnostic)?;
        }
        self.diagnostic.completed(Stage::StdioAliases);
        self.diagnostic.enter(Stage::NamespaceInputs);
        let mut handles=self.files[..self.count].iter().map(|slot|slot.raw).collect::<Vec<_>>();
        handles.extend([self.parent.raw,self.reader.raw]);
        if handles.len()+4>32{return Err(bad());} // three stdio + one remote self capability
        let namespace=format!(r"C:\Morrow-C28-A1-047-{}",selection.tag);
        let root=format!(r"{namespace}\case");
        let system=crate::asuser_a1_native::NativeSystemDirectory::read()?.into_root();
        let shell=format!(r"{system}\System32\WindowsPowerShell\v1.0\powershell.exe");
        let environment=std::collections::BTreeMap::from([
            ("SystemRoot".to_owned(),system.clone()),("WINDIR".to_owned(),system.clone()),
            ("COMSPEC".to_owned(),format!(r"{system}\System32\cmd.exe"))]);
        let arguments=vec![IMPORT.into(),self.parent.raw.to_string(),self.reader.raw.to_string(),
            "--guest-root".into(),root,"--runner".into(),directory.join(R_NAME).to_str().ok_or_else(bad)?.into(),
            "--runner-sha256".into(),hex_text(selection.r),"--setup".into(),directory.join(S_NAME).to_str().ok_or_else(bad)?.into(),
            "--setup-sha256".into(),hex_text(selection.s),"--helper".into(),c.to_str().ok_or_else(bad)?.into(),
            "--helper-sha256".into(),hex_text(selection.c),"--phase".into(),
            if selection.refusal_session {"live-guest-refusal-session-interactive".into()}else{"live-guest-interactive".into()},
            "--inventory-shell".into(),shell,"--inventory-shell-sha256".into(),hex_text(selection.shell),
            "--expected-guest-uuid".into(),"b2dd7c5f-88ee-4d69-8aeb-d0290e60950e".into()];
        self.diagnostic.completed(Stage::NamespaceInputs);
        // C is the SAME trusted high caller role, not private T. Its NULL
        // creation SAs use this P's independently observed safe primary-token
        // default DACL/TokenOwner; do not demand T's three-RC-ACE template on P.
        // Initial R process/thread explicit protected SAs remain mandatory.
        self.diagnostic.enter(Stage::NamespaceSecurity);
        let c_sid=crate::asuser_a1_policy::Sid::from_bytes(&caller)?;
        self.namespace_sd.begin()?;let mut namespace_sd=ptr::null_mut();
        let text=wide(&format!("D:P(A;OICI;FA;;;{})(A;OICI;RC;;;OW)",c_sid.text()));
        let ok=unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut namespace_sd,ptr::null_mut())};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::SecurityDescriptor,ok!=0,if ok==0{Some(error as i32)}else{None});self.namespace_sd.captured(ok!=0,namespace_sd as isize,error)?;
        self.diagnostic.completed(Stage::NamespaceSecurity);
        self.diagnostic.enter(Stage::NamespaceCreate);
        self.namespace_creation_started=true;
        observed_check(&mut self.diagnostic,Api::CreateDirectory,unsafe{windows_sys::Win32::Storage::FileSystem::CreateDirectoryW(wide(&namespace).as_ptr(),&attributes(self.namespace_sd.raw,false))})?;
        self.namespace_created=true;
        self.diagnostic.completed(Stage::NamespaceCreate);self.diagnostic.enter(Stage::NamespacePin);
        self.namespace.begin()?;
        let raw=unsafe{CreateFileW(wide(&namespace).as_ptr(),0x80000000,1,ptr::null(),3,0x02200000,0)};
        let error=if raw==0||raw == -1{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::CreateFile,raw!=0&&raw != -1,if raw==0||raw == -1{Some(error as i32)}else{None});self.namespace.captured(raw!=0&&raw != -1,raw,error)?;
        if file_path(raw)?!=PathBuf::from(&namespace){return Err(bad());}file_identity(raw,5)?;
        self.diagnostic.completed(Stage::NamespacePin);
        self.diagnostic.enter(Stage::PrecreateSubject);
        let mut before_create_defaults=None;
        let (before_create,_,before_create_il)=subject(unsafe{GetCurrentProcess()},&mut self.precreate_token,&mut before_create_defaults)?;
        if before_create!=context||before_create_il!=0x3000||before_create_defaults!=self.defaults{return Err(bad());}
        self.checkpoint()?;
        self.diagnostic.completed(Stage::PrecreateSubject);self.diagnostic.enter(Stage::ChildCreate);
        self.created=true;
        let inherited=handles.iter().map(|raw|unsafe{BorrowedHandle::borrow_raw(*raw as *mut c_void)}).collect::<Vec<_>>();
        let stdio=std::array::from_fn(|index|unsafe{BorrowedHandle::borrow_raw(self.stdio[index].raw as *mut c_void)});
        // EXACTLY ONE child creator: existing native-pipe retained owner from
        // workbench's adapter. It retains the original Job/PI even on Err/unwind.
        let created=self.original_child.create_suspended(ParentCreate{application:&c,cwd:directory,arguments:&arguments,environment:&environment,deadline:self.deadline,
            extra_inherited:&inherited,stdio})?;
        self.diagnostic.completed(Stage::ChildCreate);
        self.diagnostic.enter(Stage::ChildIdentity);
        let process=self.original_child.original_process()?.as_raw_handle() as isize;
        let child=identity(process)?;
        if child.0!=created.pid||child.1!=created.creation_filetime||created.thread_id==0
            ||before!=identity(unsafe{GetCurrentProcess()})? {return Err(bad());}
        self.checkpoint()?;
        self.diagnostic.completed(Stage::ChildIdentity);self.diagnostic.enter(Stage::ChildSelfAlias);
        self.remote_transfer_started=true;
        let duplicate=self.original_child.begin_bootstrap_duplication(unsafe{BorrowedHandle::borrow_raw(process as *mut c_void)})?;
        let mut remote=0;let ok=unsafe{DuplicateHandle(GetCurrentProcess(),process,process,
            &mut remote,crate::asuser_a1_rights::PROCESS_MANAGEMENT,0,0)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};
        // Capture remote output before any fallible operation. It belongs to C,
        // not P; failure with unexpected output is retained as remote debt.
        if remote!=0{self.remote_self=Some(remote as u64);}
        self.diagnostic.native(Api::DuplicateHandle,ok!=0,if ok==0{Some(error as i32)}else{None});
        self.original_child.complete_bootstrap_duplication(duplicate,if ok==0{Err(io::Error::from_raw_os_error(error as i32))}
            else if remote<=0{Err(bad())}else{Ok(())})?;
        let remote=self.remote_self.ok_or_else(bad)?;
        self.diagnostic.completed(Stage::ChildSelfAlias);
        self.diagnostic.enter(Stage::RecordEncode);
        let (now,frequency)=counter()?;let deadline=now.checked_add(frequency.checked_mul(25).ok_or_else(bad)?).ok_or_else(bad)?;
        self.bootstrap.bytes=Record{generation:now,parent_pid:before.0,parent_time:before.1,
            child_pid:child.0,child_time:child.1,deadline,parent_alias:self.parent.raw as u64,self_alias:remote,
            reader_alias:self.reader.raw as u64,parent_image:selection.p,child_image:selection.c,
            files:self.identities,count:self.count as u8}.encode()?;
        self.diagnostic.completed(Stage::RecordEncode);
        self.diagnostic.enter(Stage::RevalidatePins);
        for (index,path) in self.paths.iter().enumerate(){
            if !self.identities[index].same_held_observation(file_identity(self.files[index].raw,self.identities[index].kind)?)
                ||file_path(self.files[index].raw)?!=*path{return Err(bad());}
        }
        self.checkpoint()?;
        self.diagnostic.completed(Stage::RevalidatePins);self.diagnostic.enter(Stage::ChildResume);
        self.resume_started=true;
        self.original_child.resume_original_once()?;
        self.diagnostic.completed(Stage::ChildResume);
        self.diagnostic.enter(Stage::BootstrapWrite);
        self.bootstrap.transfer(true,self.deadline,&mut self.diagnostic)?;
        self.diagnostic.completed(Stage::BootstrapWrite);self.diagnostic.enter(Stage::BootstrapAliasClose);
        self.bootstrap.close()?;
        self.diagnostic.completed(Stage::BootstrapAliasClose);
        // Close only P's extra inherited aliases after the fixed record is
        // delivered. Root's actual stdout/stderr original readers remain ROOT's.
        self.diagnostic.enter(Stage::InheritedAliasClose);
        self.reader.close()?;self.parent.close()?;for slot in &mut self.stdio{slot.close()?;}
        self.sd.close()?;self.namespace_sd.close()?;
        self.diagnostic.completed(Stage::InheritedAliasClose);
        self.diagnostic.enter(Stage::WaitChild);
        self.deadline=Instant::now()+Duration::from_secs(90*60);
        let process=self.original_child.original_process()?.as_raw_handle() as isize;
        let milliseconds=self.deadline.checked_duration_since(Instant::now()).ok_or_else(bad)?.as_millis()
            .min(u128::from(u32::MAX-1)) as u32;
        match unsafe{WaitForSingleObject(process,milliseconds)} {
            0 if Instant::now()<self.deadline=>{},258|0=>return Err(io::Error::new(io::ErrorKind::TimedOut,"A1 original C native lifetime budget")),
            _=>return Err(os_error())}
        self.diagnostic.completed(Stage::WaitChild);
        self.diagnostic.enter(Stage::ObserveChild);
        if identity(process)?!=(created.pid,created.creation_filetime){return Err(ParentExitFailure::error(ParentExitPredicate::ChildIdentity));}
        self.diagnostic.completed(Stage::ObserveChild);self.diagnostic.enter(Stage::ObserveRetainedChild);
        let mut observed=self.original_child.observe_original_exit(self.deadline)?;
        // The retained library supplies its actual existing signal/DWORD
        // captures. These two additional read-only queries use ORIGINAL borrows
        // for fields its API does not yet expose; no PID reopen/duplicate/wait.
        if !observed.thread_signaled{return Err(ParentExitFailure::error(ParentExitPredicate::PrimaryThreadSignal));}
        self.diagnostic.completed(Stage::ObserveRetainedChild);self.diagnostic.enter(Stage::ObserveThreadExit);
        let thread=self.original_child.original_primary_thread()?.as_raw_handle() as isize;
        let mut thread_dword=0;check(unsafe{GetExitCodeThread(thread,&mut thread_dword)})?;observed.thread_dword=Some(thread_dword);
        self.diagnostic.completed(Stage::ObserveThreadExit);self.diagnostic.enter(Stage::ObserveJobAccounting);
        let job=self.original_child.original_job()?.as_raw_handle() as isize;
        let mut accounting:JOBOBJECT_BASIC_ACCOUNTING_INFORMATION=unsafe{mem::zeroed()};let mut returned=0;
        check(unsafe{QueryInformationJobObject(job,JobObjectBasicAccountingInformation,
            (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),mem::size_of_val(&accounting) as u32,&mut returned)})?;
        if returned as usize!=mem::size_of_val(&accounting){return Err(ParentExitFailure::error(ParentExitPredicate::JobAccountingLength));}
        observed.job_active=accounting.ActiveProcesses;observed.job_terminated=Some(accounting.TotalTerminatedProcesses);
        observed.job_total=Some(accounting.TotalProcesses);
        self.diagnostic.completed(Stage::ObserveJobAccounting);self.diagnostic.enter(Stage::ValidateChildExit);
        let proof=parent_c_closed(created,observed,self.bootstrap.used&&!self.bootstrap.pending)?;
        self.diagnostic.completed(Stage::ValidateChildExit);
        // The original source locks remain HELD through the actual C signal.
        // Individually checked closes happen before the provider is told its
        // borrowed parent copies are discharged. Failure leaves its same Job,
        // process and primary thread retained; no boolean can skip a close.
        self.diagnostic.enter(Stage::ResourceClose);
        if self.files[..self.count].iter().any(|slot|slot.state!=HandleState::Held){return Err(bad());}
        self.checkpoint()?;self.namespace.close()?;self.checkpoint()?;
        for index in 0..self.files.len(){self.files[index].close()?;self.checkpoint()?;}
        self.token.close()?;self.checkpoint()?;self.precreate_token.close()?;self.checkpoint()?;
        self.thread_token.close()?;self.checkpoint()?;
        let proof=proof.after_original_alias_closes(self)?;
        self.diagnostic.completed(Stage::ResourceClose);
        self.diagnostic.enter(Stage::RetireChild);
        self.original_child.close_after_natural_c(proof)?;self.child_retired=true;
        self.diagnostic.completed(Stage::RetireChild);
        Ok(())
    }
}

struct Selection {p:[u8;32],c:[u8;32],r:[u8;32],s:[u8;32],shell:[u8;32],tag:String,refusal_session:bool}
impl Selection {
    fn parse(args:&[String])->io::Result<Self> {
        if args.len()!=7||!matches!(args[0].as_str(),PREFIX|SESSION_PREFIX)||args[6].len()!=32||!args[6].bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v)) {return Err(bad());}
        let result=Self{p:hex(&args[1])?,c:hex(&args[2])?,r:hex(&args[3])?,s:hex(&args[4])?,shell:hex(&args[5])?,tag:args[6].clone(),refusal_session:args[0]==SESSION_PREFIX};
        let roles=[result.p,result.c,result.r,result.s];
        for i in 0..roles.len(){if roles[..i].contains(&roles[i]){return Err(bad());}}Ok(result)
    }
}
fn park()->! {loop {std::thread::park_timeout(Duration::from_secs(60));}}
/// Early existing-P branch, before Tokio/ControllerLease/legacy JobChild.
/// Invalid selection returns78 before native effects. Once installed, every
/// failure/timeout/unwind retains this SAME P allocation and parks without kill.
pub fn parent_dispatch(args:&[String],make_original:impl FnOnce()->Box<dyn ParentOriginalProcess>)->Option<u32> {
    if !args.first().is_some_and(|v|v.starts_with("--c28-parent-")){return None;}
    let selection=match Selection::parse(args){Ok(value)=>value,Err(_)=>return Some(78)};
    static ATTEMPTED:AtomicBool=AtomicBool::new(false);
    if ATTEMPTED.swap(true,Ordering::AcqRel){return Some(78);}
    let core=Box::leak(Parent::new(make_original()));
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||core.run(selection)));
    match result {Ok(Ok(()))=>{core.diagnostic.returned_ok();Some(0)},Ok(Err(error))=>{core.diagnostic.latch_io(&error);core.first=Some(error);
            // SAFETY: only the reviewed Root anonymous-pipe stderr qualification
            // chain (Root ErrWrite -> P -> retained stdio -> C) may select this
            // fixed path. This is an external source/PE binding precondition,
            // not a mode inferred from a handle/bool; stable slot is preinstalled.
            unsafe{core.error_output.legacy(ErrorDiagnosticRole::Parent,core.diagnostic.snapshot().encode(),StartupFailureTrigger::ReturnedError);}park()},
        Err(_)=>{core.diagnostic.latch_unwind();if core.first.is_none(){core.first=Some(io::Error::other("A1 original P unwind"));}
            // SAFETY: only the reviewed Root anonymous-pipe stderr qualification
            // chain (Root ErrWrite -> P -> retained stdio -> C) may select this
            // fixed path. This is an external source/PE binding precondition,
            // not a mode inferred from a handle/bool; stable slot is preinstalled.
            unsafe{core.error_output.legacy(ErrorDiagnosticRole::Parent,core.diagnostic.snapshot().encode(),StartupFailureTrigger::CaughtUnwind);}park()}}
}

// Import core is retained by this invocation, never a global latest-PID table.
// A failed import parks the C process with the SAME allocation. No capabilities
// are reconstructed from public Record data or a late path lock.
pub(crate) struct ImportCore {
    diagnostic:Capture,
    // Kept separately from the data-only Startup Arc, including pending IO.
    error_output:RetainedErrorDiagnostic,
    // Separate data-only Arc, allocated before import; never owns an alias/lease.
    startup:StartupCarrier,
    files:[Slot;MAX_FILES],parent:Slot,child:Slot,bootstrap:Box<Bootstrap>,
    token:Slot,peer_token:Slot,thread_token:Slot,record:Option<Record>,first:Option<io::Error>,
    verified:bool,closed:bool,busy:AtomicBool,
    defaults:[Option<TrustedDefaults>;2],auth_files:[Slot;MAX_FILES],auth_count:usize,
    caller_identity:Option<(Vec<u8>,u64,u32,u64)>,
    early_caller:Option<Box<crate::CallerPrerequisiteAttempt>>,
    early_admitted:bool,
    refusal_fixture:refusal_qualification::Fixture,
}
unsafe impl Send for ImportCore {}
unsafe impl Sync for ImportCore {}
impl ImportCore {
    fn new()->Box<Self>{Box::new(Self{diagnostic:Capture::new(Scope::Import),error_output:RetainedErrorDiagnostic::prepare(),startup:StartupCarrier::new(),files:std::array::from_fn(|_|Slot::new(CloseKind::Kernel)),
        parent:Slot::new(CloseKind::Kernel),child:Slot::new(CloseKind::Kernel),bootstrap:Bootstrap::new(),
        token:Slot::new(CloseKind::Kernel),peer_token:Slot::new(CloseKind::Kernel),thread_token:Slot::new(CloseKind::Kernel),
        record:None,first:None,verified:false,closed:false,busy:AtomicBool::new(false),
        defaults:std::array::from_fn(|_|None),auth_files:std::array::from_fn(|_|Slot::new(CloseKind::Kernel)),
        auth_count:0,caller_identity:None,early_caller:None,early_admitted:false,refusal_fixture:refusal_qualification::Fixture::new()})}
    fn import(&mut self,parent:isize,reader:isize,anchor:[u8;32])->io::Result<(PathBuf,[u8;32])> {
        self.diagnostic.enter(Stage::ImportAliases);
        self.parent.begin()?;self.parent.captured(true,parent,0)?;
        self.bootstrap.pipe.begin()?;self.bootstrap.pipe.captured(true,reader,0)?;
        inherited(parent)?;inherited(reader)?;
        self.diagnostic.completed(Stage::ImportAliases);
        self.diagnostic.enter(Stage::ImportParentIdentity);
        no_impersonation(&mut self.thread_token)?;
        let parent_identity=identity(parent)?;let current=identity(unsafe{GetCurrentProcess()})?;
        if current_creator()?!=parent_identity.0||unsafe{WaitForSingleObject(parent,0)}!=258{return Err(bad());}
        self.diagnostic.completed(Stage::ImportParentIdentity);
        self.diagnostic.enter(Stage::ImportServerIdentity);
        let mut server=0;check(unsafe{GetNamedPipeServerProcessId(reader,&mut server)})?;
        if server!=parent_identity.0{return Err(bad());}
        self.diagnostic.completed(Stage::ImportServerIdentity);
        self.diagnostic.enter(Stage::ImportParentImage);
        let parent_path=image_path(parent)?;
        if parent_path.file_name().is_none_or(|v|v!=P_NAME){return Err(bad());}
        // Independent C-only compiled P digest is checked BEFORE record data.
        // This late auth pin is explicitly NOT C's precreation image lock and
        // does not prove P prebirth. ROOT must independently launch selected P
        // under its original reviewed file/ancestor guards.
        absolute(&parent_path)?;let auth_paths=parent_path.ancestors().collect::<Vec<_>>();
        if auth_paths.len()>MAX_FILES{return Err(bad());}
        self.diagnostic.completed(Stage::ImportParentImage);
        self.diagnostic.enter(Stage::ImportAuthPins);
        for path in auth_paths.iter().rev(){
            let index=self.auth_count;self.auth_count+=1;let slot=&mut self.auth_files[index];
            slot.begin()?;let directory=*path!=parent_path;
            let raw=unsafe{CreateFileW(wide(path.to_str().ok_or_else(bad)?).as_ptr(),0x80000000,1,ptr::null(),3,
                0x00200000|if directory{0x02000000}else{0},0)};
            let error=if raw==0||raw == -1{unsafe{GetLastError()}}else{0};self.diagnostic.native(Api::CreateFile,raw!=0&&raw != -1,if raw==0||raw == -1{Some(error as i32)}else{None});slot.captured(raw!=0&&raw != -1,raw,error)?;
            file_identity(raw,if directory{5}else{3})?;if file_path(raw)?.as_path()!=*path{return Err(bad());}
        }
        self.diagnostic.completed(Stage::ImportAuthPins);
        self.diagnostic.enter(Stage::ImportParentDigest);
        file_digest(self.auth_files[self.auth_count-1].raw,anchor)?;
        self.diagnostic.completed(Stage::ImportParentDigest);
        self.diagnostic.enter(Stage::ImportToken);
        self.token.begin()?;let ok=unsafe{OpenProcessToken(GetCurrentProcess(),8,&mut self.token.raw)};
        let error=if ok==0{unsafe{GetLastError()}}else{0};self.token.captured(ok!=0,self.token.raw,error)?;
        let stats=stable_statistics(self.token.raw)?;let current_subject=retained_subject(self.token.raw)?;
        self.defaults[0]=Some(trusted_defaults(self.token.raw,&current_subject.sid)?);
        let (session_bytes,session_length)=token_buffer(self.token.raw,TokenSessionId)?;
        if session_length!=4||stable_statistics(self.token.raw)?!=stats{return Err(bad());}
        let session=session_bytes[0] as u32;
        self.diagnostic.completed(Stage::ImportToken);
        self.diagnostic.enter(Stage::ImportSubject);
        let (own,sid,own_il)=(current_subject.hash,current_subject.sid,current_subject.integrity);
        self.caller_identity=Some((sid.clone(),stats[1],session,stats[0]));
        let (peer,_,peer_il)=subject(parent,&mut self.peer_token,&mut self.defaults[1])?;
        if own_il!=0x3000||peer_il!=0x3000||own!=peer{return Err(bad());}
        self.diagnostic.completed(Stage::ImportSubject);
        self.diagnostic.enter(Stage::ImportRead);
        self.bootstrap.event(&mut self.diagnostic)?;let deadline=Instant::now()+Duration::from_secs(25);
        self.bootstrap.transfer(false,deadline,&mut self.diagnostic)?;
        self.diagnostic.completed(Stage::ImportRead);
        self.diagnostic.enter(Stage::ImportDecode);
        let record=Record::decode(&self.bootstrap.bytes)?;
        self.diagnostic.completed(Stage::ImportDecode);
        // Capture ALL transferred original values before interpreting any of
        // their identities. A later failure never closes an unverified alias.
        self.diagnostic.enter(Stage::ImportTransferredAliases);
        self.child.begin()?;self.child.captured(true,record.self_alias as isize,0)?;
        for (index,file) in record.files[..usize::from(record.count)].iter().enumerate(){
            self.files[index].begin()?;self.files[index].captured(true,file.alias as isize,0)?;
        }
        self.record=Some(record);
        self.diagnostic.completed(Stage::ImportTransferredAliases);
        self.diagnostic.enter(Stage::ImportRecordIdentity);
        let (now,frequency)=counter()?;shared_remaining(record.deadline,now,frequency)?;
        if record.parent_alias!=parent as u64||record.reader_alias!=reader as u64
            ||(record.parent_pid,record.parent_time)!=parent_identity
            ||(record.child_pid,record.child_time)!=current||record.parent_image!=anchor
            ||unsafe{CompareObjectHandles(self.child.raw,GetCurrentProcess())}==0
            ||identity(self.child.raw)?!=current||unsafe{WaitForSingleObject(parent,0)}!=258 {return Err(bad());}
        let current_path=image_path(unsafe{GetCurrentProcess()})?;absolute(&current_path)?;
        if current_path.file_name().is_none_or(|v|v!=C_NAME)||parent_path.parent()!=current_path.parent(){return Err(bad());}
        self.diagnostic.completed(Stage::ImportRecordIdentity);
        self.diagnostic.enter(Stage::ImportFiles);
        let mut paths=Vec::with_capacity(MAX_FILES);let mut c_leaf=false;let mut p_leaf=false;
        for (index,expected) in record.files[..usize::from(record.count)].iter().enumerate(){
            let raw=self.files[index].raw;inherited(raw)?;
            if !expected.same_held_observation(file_identity(raw,expected.kind)?){return Err(bad());}
            let path=file_path(raw)?;
            match expected.kind {
                1=>{if c_leaf||path!=current_path{return Err(bad());}file_digest(raw,record.child_image)?;c_leaf=true;},
                3=>{if p_leaf||path!=parent_path{return Err(bad());}file_digest(raw,anchor)?;
                    let mut auth=file_identity(self.auth_files[self.auth_count-1].raw,3)?;
                    auth.alias=expected.alias;if auth!=*expected{return Err(bad());}p_leaf=true;},
                5=>{if !current_path.ancestors().skip(1).any(|v|v==path){return Err(bad());}},
                _=>return Err(bad()),
            }
            if paths.contains(&path){return Err(bad());}paths.push(path);
        }
        if !c_leaf||!p_leaf||!current_path.ancestors().skip(1).all(|p|paths.iter().any(|v|v==p))
            ||paths.len()!=current_path.ancestors().skip(1).count()+2{return Err(bad());}
        self.diagnostic.completed(Stage::ImportFiles);
        // Repeat actual same-original identities after all bounded reads.
        self.diagnostic.enter(Stage::ImportRevalidate);
        for (index,expected) in record.files[..usize::from(record.count)].iter().enumerate(){
            if !expected.same_held_observation(file_identity(self.files[index].raw,expected.kind)?){return Err(bad());}
        }
        if identity(parent)?!=parent_identity||identity(unsafe{GetCurrentProcess()})?!=current{return Err(bad());}
        self.diagnostic.completed(Stage::ImportRevalidate);
        self.diagnostic.enter(Stage::ImportAliasClose);
        let _=sid;self.bootstrap.close()?;self.token.close()?;self.peer_token.close()?;self.thread_token.close()?;
        self.verified=true;
        self.diagnostic.completed(Stage::ImportAliasClose);Ok((current_path,record.child_image))
    }
}

pub(crate) struct OriginGuard {core:Option<std::cell::UnsafeCell<&'static mut ImportCore>>,busy:AtomicBool,
    startup:StartupCarrier,
    native_generation:std::sync::atomic::AtomicU64,unknown:AtomicBool,
    // Only the consume-only parent-session inlet creates a derived attempt.
    // P/C handles and prebirth pins stay charged to the common original parent
    // until the session's final close. Every attempt has a fresh one-shot guard.
    shared_parent:Option<Arc<OriginGuard>>,
    session_reservation:Option<session_admission::Reservation>,
    // The boxed address/thread identity belongs to this attempt. Access is
    // serialized by this guard's original atomic borrow, never a copied flag.
    session_caller:Option<std::cell::UnsafeCell<Box<crate::CallerPrerequisiteAttempt>>>,
    session_preflight:AtomicBool,session_closed:AtomicBool}
struct OriginalAttemptRetired {original:Arc<OriginGuard>}
unsafe impl Send for OriginGuard {}
unsafe impl Sync for OriginGuard {}
impl OriginGuard {
    fn retain_unknown(&self) {
        self.unknown.store(true,Ordering::Release);
        if let Some(reservation)=&self.session_reservation {
            let _=reservation.ledger.retain_unknown(reservation);
        }
    }
    pub(crate) fn claim_native_attempt(&self,generation:u64)->io::Result<()> {
        if self.unknown.load(Ordering::Acquire)||self.session_closed.load(Ordering::Acquire)||generation==0{return Err(bad());}
        if self.shared_parent.is_some()&&!self.session_preflight.load(Ordering::Acquire){return Err(bad());}
        if self.native_generation.compare_exchange(0,generation,Ordering::AcqRel,Ordering::Acquire).is_err(){
            self.retain_unknown();return Err(bad());}
        Ok(())
    }
    fn with<T>(&self,f:impl FnOnce(&mut ImportCore)->io::Result<T>)->io::Result<T> {
        if self.unknown.load(Ordering::Acquire)||self.session_closed.load(Ordering::Acquire){self.startup.unavailable();return Err(bad());}
        if self.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){
            self.retain_unknown();
            let error=io::Error::new(io::ErrorKind::WouldBlock,"A1 original origin borrowed");
            self.startup.unavailable();return Err(error);}
        struct Borrow<'a>(&'a AtomicBool);impl Drop for Borrow<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
        let _borrow=Borrow(&self.busy);
        if let Some(parent)=&self.shared_parent {
            // Parent's original atomic borrow still serializes access to the
            // same ImportCore. There is no copied mutable core or second lock.
            return match parent.with(f) {
                Ok(value) if !self.unknown.load(Ordering::Acquire)=>Ok(value),
                _=>{self.retain_unknown();self.startup.unavailable();Err(bad())},
            };
        }
        let original=self.core.as_ref().ok_or_else(bad)?;
        let core=unsafe{&mut **original.get()};
        if core.first.is_some(){self.startup.unavailable();return Err(io::Error::other("A1 original origin first failure retained"));}
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(core))) {
            Ok(Ok(value))=>if self.unknown.load(Ordering::Acquire){self.startup.unavailable();Err(bad())}else{Ok(value)},
            Ok(Err(error))=>{self.startup.sdk_io_current(&error);
                if core.first.is_none(){core.first=Some(error);}self.retain_unknown();
                Err(io::Error::other("A1 original origin failed; retained"))},
            Err(_)=>{self.startup.sdk_unwind_current();
                if core.first.is_none(){core.first=Some(io::Error::other("A1 original origin unwind"));}
                self.retain_unknown();Err(io::Error::other("A1 original origin unwind; retained"))}}
    }
    pub(crate) fn require_live(&self,pid:u32,creation:u64)->io::Result<()> {
        self.with(|core|{
            let record=core.record.ok_or_else(bad)?;
            if !core.verified||core.closed||(pid,creation)!=(record.child_pid,record.child_time)
                ||identity(core.child.raw)?!=(pid,creation)||unsafe{CompareObjectHandles(core.child.raw,GetCurrentProcess())}==0
                ||identity(core.parent.raw)?!=(record.parent_pid,record.parent_time)
                ||unsafe{WaitForSingleObject(core.parent.raw,0)}!=258{return Err(bad());}Ok(())})
    }
    pub(crate) fn require_admitted(&self,pid:u32,creation:u64)->io::Result<()> {
        // A common parent's earlier admission does not admit a fresh attempt.
        // Reject before touching that parent's original native resources.
        if self.shared_parent.is_some()&&!self.session_preflight.load(Ordering::Acquire){return Err(bad());}
        self.require_live(pid,creation)?;
        self.with(|core|if core.early_admitted&&core.early_caller.as_ref().is_some_and(|v|v.known_closed()) {Ok(())}else{Err(bad())})
    }
    // Called only after the original consumed SDK/owner/factory/setup normal
    // path, never from Drop/EOF/timers/public scalar predicates.
    pub(crate) fn close_normal(self:&Arc<Self>)->io::Result<()> {
        if self.shared_parent.is_some() {
            // The caller already consumed the exact native/SDK retirement and
            // verified its original factory/setup association. This releases
            // only this attempt's admission, never common parent native pins.
            if self.busy.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){
                self.retain_unknown();return Err(bad());
            }
            struct Borrow<'a>(&'a AtomicBool);impl Drop for Borrow<'_>{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
            let _borrow=Borrow(&self.busy);
            if self.unknown.load(Ordering::Acquire)
                || !self.session_preflight.load(Ordering::Acquire)
                || self.native_generation.load(Ordering::Acquire)==0 {return Err(bad());}
            let caller=self.session_caller.as_ref().ok_or_else(bad)?;
            if !unsafe{&**caller.get()}.known_closed(){self.retain_unknown();return Err(bad());}
            if self.session_closed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return Err(bad());}
            let reservation=self.session_reservation.as_ref().ok_or_else(bad)?;
            if reservation.ledger.accept_original_close(reservation,OriginalAttemptRetired{original:self.clone()}).is_err() {
                self.retain_unknown();return Err(bad());
            }
            return Ok(());
        }
        self.with(|core|{
            if !core.verified||core.closed||!core.early_admitted
                ||!core.early_caller.as_ref().is_some_and(|v|v.known_closed())
                ||!core.refusal_fixture.ready_for_origin_close(){return Err(bad());}
            core.bootstrap.close()?;core.child.close()?;
            for slot in &mut core.files{slot.close()?;}
            for slot in &mut core.auth_files{slot.close()?;}
            core.parent.close()?;core.closed=true;Ok(())})
    }
}
/// One genuine imported P/C lifetime, consumed before any native attempt.
/// Common original native handles remain charged through every derived attempt.
/// No public constructor accepts handles, hashes, booleans or serialized data.
pub struct AsUserParentSession {
    original:AsUserParentOrigin,
    ledger:Arc<session_admission::Ledger>,
    final_close_attempted:bool,
}
impl AsUserParentSession {
    pub fn begin_attempt(&mut self)->io::Result<Arc<AsUserParentAttempt>> {
        if self.final_close_attempted {return Err(bad());}
        self.original.guard.require_admitted(self.original.pid,self.original.creation)?;
        let reservation=self.ledger.reserve().map_err(|_|bad())?;
        let startup=StartupCarrier::new();startup.bind(self.original.generation);
        let guard=Arc::new(OriginGuard{core:None,busy:AtomicBool::new(false),startup,
            native_generation:std::sync::atomic::AtomicU64::new(0),unknown:AtomicBool::new(false),
            shared_parent:Some(self.original.guard.clone()),session_reservation:Some(reservation),
            session_caller:Some(std::cell::UnsafeCell::new(crate::CallerPrerequisiteAttempt::new())),
            session_preflight:AtomicBool::new(false),session_closed:AtomicBool::new(false)});
        let reservation=guard.session_reservation.as_ref().ok_or_else(bad)?;
        self.ledger.install(reservation,guard.clone()).map_err(|_|bad())?;
        let c=Arc::new(crate::MatchedRunnerArtifact::from_parent_original(
            self.original.c.path().to_path_buf(),self.original.c.sha256(),guard.clone()));
        let origin=Arc::new(AsUserParentOrigin{guard,c,pid:self.original.pid,
            creation:self.original.creation,generation:self.original.generation});
        Ok(Arc::new(AsUserParentAttempt{origin}))
    }
    /// Downward-only failure retention for this live original attempt. It can
    /// never release quota, reconstruct a completion or touch an OS resource.
    pub fn retain_unknown(&self,attempt:&Arc<AsUserParentAttempt>)->io::Result<()> {
        let original=&attempt.origin;
        let reservation=original.guard.session_reservation.as_ref().ok_or_else(bad)?;
        if !Arc::ptr_eq(&self.ledger,&reservation.ledger){return Err(bad());}
        self.ledger.retain_unknown(reservation).map_err(|_|bad())?;
        original.guard.unknown.store(true,Ordering::Release);Ok(())
    }
    /// Every issued attempt must have consumed its actual original retirement.
    /// Reserve once before any OS close. Failure retains the same parent and
    /// prevents another close attempt; no missing Arc or empty public table is
    /// accepted as a substitute for the ledger's sealed original capabilities.
    pub fn finish(&mut self)->io::Result<()> {
        if self.final_close_attempted {return Err(bad());}
        self.ledger.reserve_final_close().map_err(|_|bad())?;
        self.final_close_attempted=true;
        self.original.guard.close_normal()
    }
}
/// A fresh bounded attempt borrowing the original parent session's native
/// handles. It deliberately cannot be passed to the legacy imported-origin
/// constructor or reported as closing those common native resources.
pub struct AsUserParentAttempt {pub(crate) origin:Arc<AsUserParentOrigin>}
impl AsUserParentAttempt {
    pub fn shares_original_parent(&self,other:&Self)->bool {
        match (&self.origin.guard.shared_parent,&other.origin.guard.shared_parent) {
            (Some(original),Some(candidate))=>Arc::ptr_eq(original,candidate),
            _=>false,
        }
    }
    pub fn validate_before_setup(&self)->io::Result<()> {self.origin.validate_before_setup()}
    pub fn validate_independent_role_pins(&self,s:&crate::MatchedRunnerArtifact,r:&crate::MatchedRunnerArtifact)->io::Result<()> {
        self.origin.validate_independent_role_pins(s,r)
    }
    pub fn caller_artifact(&self)->io::Result<Arc<crate::MatchedRunnerArtifact>> {self.origin.caller_artifact()}
    pub fn startup_observation(&self)->StartupObservation {self.origin.startup_observation()}
    pub fn retain_after_failure(&self)->! {park()}
    pub fn retire_after_finished_lifecycle(&self,completion:crate::AsUserOriginRetirement)->io::Result<()> {
        self.origin.retire_exact_lifecycle(completion)
    }
    pub fn retire_after_finished_refusal(&self,completion:crate::AsUserOriginRefusalRetirement)->io::Result<()> {
        self.origin.retire_exact_refusal(completion)
    }
}
/// Consume-only original same-attempt inlet. There is no constructor from a
/// Record/hash/pid/path or public observations, and no Clone/Deserialize.
pub struct AsUserParentOrigin {
    pub(crate) guard:Arc<OriginGuard>,pub(crate) c:Arc<crate::MatchedRunnerArtifact>,
    pub(crate) pid:u32,pub(crate) creation:u64,pub(crate) generation:u64,
}
impl AsUserParentOrigin {
    pub(crate) fn is_session_attempt(&self)->bool {self.guard.shared_parent.is_some()}
    /// Consume the genuine inlet, keeping it returnable on admission failure.
    /// Existing one-attempt callers retain their original behavior unchanged.
    pub fn into_parent_session(self)->std::result::Result<AsUserParentSession,(Self,io::Error)> {
        if self.guard.shared_parent.is_some() || self.guard.native_generation.load(Ordering::Acquire)!=0 {
            return Err((self,bad()));
        }
        if let Err(error)=self.guard.require_admitted(self.pid,self.creation){return Err((self,error));}
        Ok(AsUserParentSession{original:self,ledger:session_admission::Ledger::new(),final_close_attempted:false})
    }
    /// Explicit qualification only, after this genuine P->C inlet was admitted.
    /// It reduces only C's own original token and never edits account rights.
    #[doc(hidden)]
    pub fn qualify_missing_assign_once(&self)->io::Result<CallerRefusalQualification> {
        self.guard.require_admitted(self.pid,self.creation)?;
        if self.guard.native_generation.load(Ordering::Acquire)!=0 {return Err(bad());}
        self.guard.with(|core|{
            let (_,auth,_,token)=core.caller_identity.as_ref().ok_or_else(bad)?;
            core.refusal_fixture.remove_assign_once(*token,*auth)
        })
    }
    /// Sealed same-generation DATA only. No origin/core borrow, native query,
    /// control callback or ownership extension occurs when reading history.
    pub fn startup_observation(&self)->StartupObservation {
        self.guard.startup.observation(self.generation)
    }
    /// Original preflight checks S's compiled C/R table before any owned setup
    /// effect. This is compatibility/role admission, never a setup proof.
    pub fn validate_independent_role_pins(&self,s:&crate::MatchedRunnerArtifact,r:&crate::MatchedRunnerArtifact)->io::Result<()> {
        let step=self.startup_observation().sdk_enter(SdkStage::RolePins);
        let admitted=self.guard.require_admitted(self.pid,self.creation);
        if let Err(error)=&admitted{step.io_error(error);return admitted;}
        let table=match s.asuser_s_table(){Ok(table)=>table,Err(error)=>{step.io_error(&error);return Err(error);}};
        if table.c!=self.c.sha256()||table.r!=r.sha256()||s.sha256()==r.sha256()
            ||self.c.sha256()==s.sha256()||self.c.sha256()==r.sha256()
            ||self.c.shares_original_pin(s)||self.c.shares_original_pin(r)||s.shares_original_pin(r){
                let error=bad();step.io_error(&error);return Err(error);}
        step.returned();Ok(())
    }
    pub fn validate_before_setup(&self)->io::Result<()> {
        if let Some(parent)=&self.guard.shared_parent {
            // The common inlet is necessary but cannot replace this attempt's
            // fresh original caller observation before any setup side effects.
            if self.guard.unknown.load(Ordering::Acquire)||self.guard.session_closed.load(Ordering::Acquire){return Err(bad());}
            if self.guard.session_preflight.load(Ordering::Acquire){self.guard.retain_unknown();return Err(bad());}
            parent.require_admitted(self.pid,self.creation)?;
            let observation=self.startup_observation();
            return self.guard.with(|core|{
                let install=observation.sdk_enter(SdkStage::EarlyInstall);
                let cell=self.guard.session_caller.as_ref().ok_or_else(bad)?;
                let original=unsafe{&mut **cell.get()};install.returned();
                let reserve=observation.sdk_enter(SdkStage::EarlyReserve);
                if !original.reserve(){return Err(bad());}reserve.returned();
                let observe=observation.sdk_enter(SdkStage::EarlyObserve);
                original.observe_once();observe.returned_void();
                let require=observation.sdk_enter(SdkStage::EarlyRequire);
                let (sid,auth,session,token)=core.caller_identity.as_ref().ok_or_else(bad)?;
                original.require_asuser_preflight(self.pid,self.creation,sid,*auth,*session,*token)?;require.returned();
                let admitted=observation.sdk_enter(SdkStage::EarlyAdmitted);
                self.guard.session_preflight.store(true,Ordering::Release);admitted.returned();Ok(())
            });
        }
        let observation=self.startup_observation();
        let live=observation.sdk_enter(SdkStage::OriginLive);
        self.guard.require_live(self.pid,self.creation)?;live.returned();
        self.guard.with(|core|{
            let install=observation.sdk_enter(SdkStage::EarlyInstall);
            if core.early_caller.is_some(){return Err(bad());}
            core.early_caller=Some(crate::CallerPrerequisiteAttempt::new());
            let original=core.early_caller.as_mut().ok_or_else(bad)?;install.returned();
            let reserve=observation.sdk_enter(SdkStage::EarlyReserve);
            if !original.reserve(){return Err(bad());}reserve.returned();
            let observe=observation.sdk_enter(SdkStage::EarlyObserve);
            original.observe_once();observe.returned_void();
            let require=observation.sdk_enter(SdkStage::EarlyRequire);
            let (sid,auth,session,token)=core.caller_identity.as_ref().ok_or_else(bad)?;
            original.require_asuser_preflight(self.pid,self.creation,sid,*auth,*session,*token)?;require.returned();
            let admitted=observation.sdk_enter(SdkStage::EarlyAdmitted);
            core.early_admitted=true;admitted.returned();Ok(())
        })
    }
    pub fn caller_artifact(&self)->io::Result<Arc<crate::MatchedRunnerArtifact>> {
        let step=self.startup_observation().sdk_enter(SdkStage::CallerArtifact);
        let admitted=self.guard.require_admitted(self.pid,self.creation);
        if let Err(error)=admitted{step.io_error(&error);return Err(error);}
        let artifact=self.c.clone();step.returned();Ok(artifact)
    }
    /// Original H keeps this object reachable on EVERY post-import failure.
    /// No error text, token, handle, key or claim is exported by this path.
    pub fn retain_after_failure(&self)->! {park()}
    // This crate's Q consumer calls only after genuine normal lifecycle
    // completion; no Drop or arbitrary public boolean can close the origin.
    pub fn close_after_finished_lifecycle(&self,completion:crate::AsUserOriginRetirement)->io::Result<()> {
        if self.is_session_attempt(){return Err(bad());}
        self.retire_exact_lifecycle(completion)
    }
    fn retire_exact_lifecycle(&self,completion:crate::AsUserOriginRetirement)->io::Result<()> {
        if !Arc::ptr_eq(&completion.origin,&self.guard)||completion.factory.upgrade().is_some()
            ||completion.startup==0||self.guard.native_generation.load(Ordering::Acquire)!=completion.startup{return Err(bad());}
        self.guard.close_normal()
    }
    pub fn close_after_finished_refusal(&self,completion:crate::AsUserOriginRefusalRetirement)->io::Result<()> {
        if self.is_session_attempt(){return Err(bad());}
        self.retire_exact_refusal(completion)
    }
    fn retire_exact_refusal(&self,completion:crate::AsUserOriginRefusalRetirement)->io::Result<()> {
        if !Arc::ptr_eq(&completion.origin,&self.guard) || completion.factory.upgrade().is_some()
            || completion.setup.upgrade().is_some() || completion.startup==0
            || self.guard.native_generation.load(Ordering::Acquire)!=completion.startup {return Err(bad());}
        self.guard.close_normal()
    }
    pub(crate) fn caller_identity(&self)->io::Result<(Vec<u8>,u64,u32,u64,crate::asuser_a1_policy::Sid)> {
        self.guard.with(|core|{
            let (user,auth,session,token)=core.caller_identity.clone().ok_or_else(bad)?;
            Ok((user,auth,session,token,core.defaults[0].as_ref().ok_or_else(bad)?.logon))})
    }
    /// C binary passes its OWN compiled-only P anchor. A zero table rejects
    /// before any native call. Imported selection data cannot replace it.
    pub fn import_arguments(args:&[String],compiled_parent:[u8;32])->io::Result<Option<(Self,Vec<String>)>> {
        if !args.first().is_some_and(|v|v.starts_with("--c28-parent-")){return Ok(None);}
        if compiled_parent==[0;32]||args.len()<4||args[0]!=IMPORT{return Err(bad());}
        let parent=handle_text(&args[1])?;let reader=handle_text(&args[2])?;if parent==reader{return Err(bad());}
        static IMPORTED:AtomicBool=AtomicBool::new(false);
        if IMPORTED.swap(true,Ordering::AcqRel){return Err(bad());}
        let core=Box::leak(ImportCore::new());
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||core.import(parent,reader,compiled_parent)));
        let (path,sha)=match result {Ok(Ok(value))=>{core.diagnostic.returned_ok();value},Ok(Err(error))=>{core.diagnostic.latch_io(&error);core.first=Some(error);
                // SAFETY: only the reviewed Root anonymous-pipe stderr qualification
                // chain (Root ErrWrite -> P -> retained stdio -> C) may select this
                // fixed path. This is an external source/PE binding precondition,
                // not a mode inferred from a handle/bool; stable slot is preinstalled.
                unsafe{core.error_output.legacy(ErrorDiagnosticRole::Import,core.diagnostic.snapshot().encode(),StartupFailureTrigger::ReturnedError);}park()},
            Err(_)=>{core.diagnostic.latch_unwind();core.first=Some(io::Error::other("A1 original import unwind"));
                // SAFETY: only the reviewed Root anonymous-pipe stderr qualification
                // chain (Root ErrWrite -> P -> retained stdio -> C) may select this
                // fixed path. This is an external source/PE binding precondition,
                // not a mode inferred from a handle/bool; stable slot is preinstalled.
                unsafe{core.error_output.legacy(ErrorDiagnosticRole::Import,core.diagnostic.snapshot().encode(),StartupFailureTrigger::CaughtUnwind);}park()}};
        let record=match core.record{Some(value)=>value,None=>park()};
        core.startup.bind(record.generation);
        let startup=core.startup.clone();
        let suffix=startup.observation(record.generation).sdk_enter(SdkStage::ImportSuffix);
        let guard=Arc::new(OriginGuard{core:Some(std::cell::UnsafeCell::new(core)),busy:AtomicBool::new(false),startup,
            native_generation:std::sync::atomic::AtomicU64::new(0),unknown:AtomicBool::new(false),
            shared_parent:None,session_reservation:None,session_caller:None,session_preflight:AtomicBool::new(false),session_closed:AtomicBool::new(false)});
        let c=Arc::new(crate::MatchedRunnerArtifact::from_parent_original(path,sha,guard.clone()));
        let arguments=args[3..].to_vec();suffix.returned();
        Ok(Some((Self{guard,c,pid:record.child_pid,creation:record.child_time,generation:record.generation},arguments)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Scripted state-only fixture: none of these methods is called. These
    // tests cannot create, query, resume, close or fabricate a native owner.
    struct NoNativeCalls;
    impl ParentOriginalProcess for NoNativeCalls {
        fn create_suspended(&mut self,_:ParentCreate<'_>)->io::Result<ParentChildIdentity>{unreachable!()}
        fn original_process(&mut self)->io::Result<BorrowedHandle<'_>>{unreachable!()}
        fn original_primary_thread(&mut self)->io::Result<BorrowedHandle<'_>>{unreachable!()}
        fn original_job(&mut self)->io::Result<BorrowedHandle<'_>>{unreachable!()}
        fn begin_bootstrap_duplication(&mut self,_:BorrowedHandle<'_>)->io::Result<usize>{unreachable!()}
        fn complete_bootstrap_duplication(&mut self,_:usize,_:io::Result<()>)->io::Result<()>{unreachable!()}
        fn resume_original_once(&mut self)->io::Result<()>{unreachable!()}
        fn observe_original_exit(&mut self,_:Instant)->io::Result<ParentNativeExit>{unreachable!()}
        fn close_after_natural_c(&mut self,_:ParentCClosed)->io::Result<()>{unreachable!()}
    }
    fn scripted_closed_parent()->Box<Parent> {
        let mut parent=Parent::new(Box::new(NoNativeCalls));
        parent.created=true;parent.resume_started=true;parent.namespace_created=true;
        parent.namespace_creation_started=true;parent.remote_transfer_started=true;parent.count=1;
        parent.remote_self=Some(4);parent.bootstrap.used=true;
        parent.files[0].state=HandleState::ClosedVerified;
        for slot in &mut parent.stdio{slot.state=HandleState::ClosedVerified;}
        for slot in [&mut parent.parent,&mut parent.reader,&mut parent.namespace,&mut parent.sd,
            &mut parent.namespace_sd,&mut parent.token,&mut parent.precreate_token,
            &mut parent.bootstrap.pipe,&mut parent.bootstrap.event]{slot.state=HandleState::ClosedVerified;}
        parent.thread_token.state=HandleState::Rejected;parent
    }
    #[test] fn native_c_zero_cannot_discharge_open_failed_uncertain_or_unobserved_parent_aliases() {
        let identity=ParentChildIdentity{pid:11,thread_id:13,creation_filetime:17};
        assert_eq!(ParentC0Observed{identity}.after_original_alias_closes(&scripted_closed_parent()).unwrap().identity(),identity);
        for state in [HandleState::NotAllocated,HandleState::InFlight,HandleState::Held,HandleState::Rejected,
            HandleState::Uncertain,HandleState::CloseFailed{error:5}] {
            for field in 0..14 {
                let mut parent=scripted_closed_parent();
                match field {
                    0=>parent.files[0].state=state,1..=3=>parent.stdio[field-1].state=state,
                    4=>parent.parent.state=state,5=>parent.reader.state=state,6=>parent.namespace.state=state,
                    7=>parent.sd.state=state,8=>parent.namespace_sd.state=state,9=>parent.token.state=state,
                    10=>parent.precreate_token.state=state,11=>parent.bootstrap.pipe.state=state,
                    12=>parent.bootstrap.event.state=state,_=>{parent.thread_token.state=state;parent.thread_token.raw=4;}
                }
                assert!(ParentC0Observed{identity}.after_original_alias_closes(&parent).is_err(),"field {field}, {state:?}");
            }
        }
        for field in 0..11 {
            let mut parent=scripted_closed_parent();match field {
                0=>parent.bootstrap.pending=true,1=>parent.bootstrap.used=false,2=>parent.remote_self=None,
                3=>parent.remote_transfer_started=false,4=>parent.namespace_creation_started=false,
                5=>parent.child_retired=true,6=>parent.files[1].state=HandleState::Held,
                7=>parent.first=Some(io::Error::from_raw_os_error(123)),8=>parent.count=0,
                9=>parent.remote_self=Some(0),_=>parent.count=MAX_FILES+1
            }
            assert!(ParentC0Observed{identity}.after_original_alias_closes(&parent).is_err(),"field {field}");
            if field==7{assert_eq!(parent.first.as_ref().unwrap().raw_os_error(),Some(123));}
        }
    }
    pub(super) fn guard()->Arc<OriginGuard>{
        let core=Box::leak(ImportCore::new());let startup=core.startup.clone();
        Arc::new(OriginGuard{core:Some(std::cell::UnsafeCell::new(core)),startup,
        busy:AtomicBool::new(false),native_generation:std::sync::atomic::AtomicU64::new(0),unknown:AtomicBool::new(false),
        shared_parent:None,session_reservation:None,session_caller:None,session_preflight:AtomicBool::new(false),session_closed:AtomicBool::new(false)})}
    #[test] fn fresh_session_attempt_cannot_borrow_or_claim_before_its_own_preflight() {
        let parent=guard();
        let attempt=Arc::new(OriginGuard{core:None,startup:StartupCarrier::new(),
            busy:AtomicBool::new(false),native_generation:std::sync::atomic::AtomicU64::new(0),
            unknown:AtomicBool::new(false),shared_parent:Some(parent.clone()),session_reservation:None,
            session_caller:None,
            session_preflight:AtomicBool::new(false),session_closed:AtomicBool::new(false)});
        assert!(attempt.require_admitted(0,0).is_err());
        assert!(attempt.claim_native_attempt(1).is_err());
        assert_eq!(attempt.native_generation.load(Ordering::Acquire),0);
        assert!(!parent.unknown.load(Ordering::Acquire));
        assert!(unsafe{&**parent.core.as_ref().unwrap().get()}.first.is_none());
    }
    #[test] fn one_original_parent_origin_cannot_issue_two_native_generations_or_hide_busy_first_error() {
        let original=guard();assert!(original.claim_native_attempt(0).is_err());
        original.claim_native_attempt(7).unwrap();assert!(original.claim_native_attempt(7).is_err());
        assert!(original.claim_native_attempt(8).is_err());assert_eq!(original.native_generation.load(Ordering::Acquire),7);
        assert!(original.with(|_|Ok(())).is_err());
        let busy=guard();busy.busy.store(true,Ordering::Release);
        assert_eq!(busy.with(|_|Ok(())).unwrap_err().kind(),io::ErrorKind::WouldBlock);
        busy.busy.store(false,Ordering::Release);assert!(busy.with(|_|Ok(())).is_err());
        let original=guard();original.claim_native_attempt(7).unwrap();
        assert!(original.with::<()>(|_|Err(io::Error::from_raw_os_error(123))).is_err());
        assert!(original.with(|_|Ok(())).is_err());
        assert_eq!(unsafe{&**original.core.as_ref().unwrap().get()}.first.as_ref().unwrap().raw_os_error(),Some(123));
        assert_eq!(original.native_generation.load(Ordering::Acquire),7);
    }
    #[test] fn parent_normal_proof_requires_original_both_signals_dwords_job_and_completed_delivery() {
        let original=ParentChildIdentity{pid:11,thread_id:13,creation_filetime:17};
        let good=ParentNativeExit{process_signaled:true,thread_signaled:true,process_dword:0,thread_dword:Some(0),
            identity:original,job_active:0,job_terminated:Some(0),job_total:Some(1)};
        assert_eq!(parent_c_closed(original,good,true).unwrap().identity(),original);
        assert!(parent_c_closed(original,good,false).is_err());
        let mut larger_history=good;larger_history.job_total=Some(33);
        assert!(parent_c_closed(original,larger_history,true).is_ok());
        for field in 0..10 {let mut bad=good;match field{
            0=>bad.process_signaled=false,1=>bad.thread_signaled=false,2=>bad.process_dword=0xc0000142,
            3=>bad.thread_dword=Some(259),4=>bad.identity.pid+=1,5=>bad.identity.thread_id+=1,
            6=>bad.identity.creation_filetime+=1,7=>bad.job_active=1,8=>bad.job_terminated=Some(1),
            _=>bad.job_total=Some(0)}
            let error=parent_c_closed(original,bad,true).err().unwrap();
            let predicate=error.get_ref().unwrap().downcast_ref::<ParentExitFailure>().unwrap().predicate();
            use ParentExitPredicate::*;
            assert_eq!(predicate,match field {0=>ProcessSignal,1=>PrimaryThreadSignal,2=>ProcessExitNotZero,
                3=>PrimaryThreadExitNotZero,4|5|6=>ChildIdentity,7=>JobStillActive,
                8=>JobLimitedTermination,_=>JobTotalMissingOrZero});
        }
        for field in 0..3 {let mut absent=good;match field {
            0=>absent.thread_dword=None,1=>absent.job_terminated=None,_=>absent.job_total=None}
            assert!(parent_c_closed(original,absent,true).is_err());}
    }
    #[test] fn independent_compiled_parent_anchor_is_required_before_any_native_import() {
        let args=vec![IMPORT.to_owned(),"4".into(),"8".into(),"--guest-root".into()];
        assert!(AsUserParentOrigin::import_arguments(&args,[0;32]).is_err());
        for text in ["0","-1","04"," 4","0x4","9223372036854775808"] {assert!(handle_text(text).is_err());}
        assert_eq!(handle_text("4").unwrap(),4);
        let mut foreign=args.clone();foreign[0]="--c28-parent-origin-p0".into();
        assert!(AsUserParentOrigin::import_arguments(&foreign,[1;32]).is_err());
        assert!(AsUserParentOrigin::import_arguments(&["--preflight".into()],[0;32]).unwrap().is_none());
    }
    #[test] fn public_selection_cannot_swap_roles_expand_paths_commands_or_reuse_zero_binding() {
        let args=vec![PREFIX.into(),"01".repeat(32),"02".repeat(32),"03".repeat(32),"04".repeat(32),"05".repeat(32),"ab".repeat(16)];
        assert!(!Selection::parse(&args).unwrap().refusal_session);
        let mut session=args.clone();session[0]=SESSION_PREFIX.into();
        assert!(Selection::parse(&session).unwrap().refusal_session);
        session.push("18".into());assert!(Selection::parse(&session).is_err());
        let mut bad=args.clone();bad[0]="--c28-parent-refusal-session-p2".into();
        assert!(Selection::parse(&bad).is_err());let mut bad=args.clone();bad[2]=bad[1].clone();
        assert!(Selection::parse(&bad).is_err());let mut bad=args.clone();bad.push("--command".into());
        assert!(Selection::parse(&bad).is_err());let mut bad=args;bad[6]="..\\old".into();
        assert!(Selection::parse(&bad).is_err());
    }
    #[test] fn equal_public_metadata_cannot_substitute_a_different_original_parent() {
        fn attempt(parent:Arc<OriginGuard>)->AsUserParentAttempt {
            let guard=Arc::new(OriginGuard{core:None,startup:StartupCarrier::new(),
                busy:AtomicBool::new(false),native_generation:std::sync::atomic::AtomicU64::new(0),
                unknown:AtomicBool::new(false),shared_parent:Some(parent),session_reservation:None,
                session_caller:None,session_preflight:AtomicBool::new(false),session_closed:AtomicBool::new(false)});
            let c=Arc::new(crate::MatchedRunnerArtifact::from_parent_original(
                PathBuf::from(r"C:\synthetic-metadata-only.exe"),[7;32],guard.clone()));
            AsUserParentAttempt{origin:Arc::new(AsUserParentOrigin{guard,c,pid:11,creation:13,generation:17})}
        }
        let original=guard();let first=attempt(original.clone());let next=attempt(original);
        let foreign=attempt(guard());
        assert!(first.shares_original_parent(&next));
        assert!(!first.shares_original_parent(&foreign));
        assert!(!foreign.shares_original_parent(&next));
        assert_eq!(first.origin.pid,foreign.origin.pid);
        assert_eq!(first.origin.creation,foreign.origin.creation);
        assert_eq!(first.origin.generation,foreign.origin.generation);
        assert_eq!(first.origin.c.sha256(),foreign.origin.c.sha256());
        assert!(first.origin.guard.claim_native_attempt(1).is_err());
    }
}
