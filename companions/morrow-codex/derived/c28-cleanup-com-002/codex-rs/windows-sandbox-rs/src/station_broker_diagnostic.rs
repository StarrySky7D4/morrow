//! Opt-in044 data ONLY. No create/adopt/Drop cleanup, transport or authority.
//! ROOT initializes a fresh 4096B pagefile section, holds a read-only view BEFORE
//! launch, and never mutates it thereafter. This is Windows AMD64 little-endian
//! only: aligned AtomicU64 loads/stores, not a portable Rust IPC ABI guarantee.
use super::*;
use std::{ffi::c_void,ptr,sync::atomic::{AtomicU64,Ordering}};
use windows_sys::Win32::Foundation::{CloseHandle,GetLastError};

pub const STATION_BROKER_DIAGNOSTIC_BYTES:usize=4096;
pub const STATION_BROKER_DIAGNOSTIC_FIXTURE_SWITCH:&str="--c28-station-broker-a0-diagnostic-fixture-v1";
pub(super) const HELPER_SWITCH:&str="--c28-station-broker-a0-diagnostic-helper-v1";
const MAGIC:u64=u64::from_le_bytes(*b"C28D044\0");
const WORDS:usize=128;
const MAX_COMMITS:u64=4096;

// Raw ABI declarations isolate view addresses from the public data model and
// avoid treating a mapped view as a CloseHandle object. Existing kernel32 only.
#[link(name="kernel32")]
unsafe extern "system" {
    fn OpenFileMappingW(access:u32,inherit:i32,name:*const u16)->isize;
    fn MapViewOfFile(mapping:isize,access:u32,high:u32,low:u32,bytes:usize)->*mut c_void;
    fn UnmapViewOfFile(view:*const c_void)->i32;
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StationBrokerDiagnosticRead {Disabled,NotAcquired,NotStarted,InFlight,Committed,
    Unavailable,Torn,WrongCase,Unsupported,Retired}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StationBrokerHelperDiagnosticClose {NotRequired,NotObserved,AuthenticatedReceipt,
    AuthenticatedReceiptAndOriginalNativeJoins}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationBrokerDiagnosticSnapshot {
    pub enabled:bool,pub mapping:StationBrokerHandle,pub view:StationBrokerHandle,
    pub record:StationBrokerDiagnosticRead,pub helper_close:StationBrokerHelperDiagnosticClose,
    pub last_stage:u64,
}
impl Default for StationBrokerDiagnosticSnapshot {
    fn default()->Self {Self {enabled:false,mapping:StationBrokerHandle::NotAllocated,
        view:StationBrokerHandle::NotAllocated,record:StationBrokerDiagnosticRead::Disabled,
            helper_close:StationBrokerHelperDiagnosticClose::NotRequired,last_stage:0}}
}
impl StationBrokerDiagnosticSnapshot {
    pub(super) fn closed(self)->bool {
        !self.enabled || (self.mapping==StationBrokerHandle::ClosedVerified
            && self.view==StationBrokerHandle::ClosedVerified)
    }
    pub(super) fn helper_normal(self)->bool {
        !self.enabled || self.helper_close==StationBrokerHelperDiagnosticClose::AuthenticatedReceiptAndOriginalNativeJoins
    }
}

#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) struct Case([u8;16]);
impl Case {
    pub(super) fn parse(text:&str)->Option<Self> {
        if text.len()!=32 || !text.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return None;}
        let mut bytes=[0;16];
        for (i,pair) in text.as_bytes().chunks_exact(2).enumerate() {
            let digit=|b:u8|if b.is_ascii_digit(){b-b'0'}else{b-b'a'+10};
            bytes[i]=(digit(pair[0])<<4)|digit(pair[1]);
        }
        (bytes!=[0;16]).then_some(Self(bytes))
    }
    pub(super) fn text(self)->String {self.0.iter().map(|v|format!("{v:02x}")).collect()}
    fn name(self)->Vec<u16> {format!("Local\\C28A0Diagnostic044-{}\0",self.text()).encode_utf16().collect()}
}
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) struct Scope {pub generation:u64,pub parent_pid:u32,pub parent_time:u64}
impl Scope {fn valid(self)->bool {self.generation!=0 && self.parent_pid!=0 && self.parent_time!=0}}
#[derive(Clone,Copy)]
pub(super) enum Writer {Parent,Helper}
#[derive(Clone,Copy)]
#[repr(u64)]
pub(super) enum Stage {
    ParentIdentity=1,MappingOpen=2,MappingView=3,MappingBound=4,
    HostAssociation=10,CallerToken=11,OwnToken=12,Random=13,SecurityDescriptor=14,
    PipeCreate=15,PipeConnect=16,Spawn=17,ProcessIdentity=18,ImageVerify=19,
    PeerIdentity=20,PeerToken=21,HelloSend=22,OfferReceive=23,DuplicateStation=24,
    DuplicateDesktop=25,ObjectVerify=26,AcceptSend=27,ClosedReceive=28,
    ProcessWait=30,ProcessDword=31,MainThreadWait=32,ReaderJoin=33,OriginalClose=34,
    HelperRetired=35,AliasClose=36,Admission=37,ViewUnmap=38,MappingClose=39,
    NormalAliasesOutput=40,NormalClosedOutput=41,
    HelperAuthenticated=50,AdoptStdio=51,StationCreate=52,StationSelect=53,
    DesktopCreate=54,OfferSend=55,AcceptReceive=56,AssociationRestore=57,ClosedSend=58,
    StdinOpen=59,ParentHandleDuplicate=60,AttributeList=61,ReaderSpawn=62,
}
pub(super) fn stage_for(target:StationBrokerDebt)->Stage {match target {
    StationBrokerDebt::CallerThreadToken=>Stage::CallerToken,StationBrokerDebt::OwnToken=>Stage::OwnToken,
    StationBrokerDebt::PeerToken=>Stage::PeerToken,StationBrokerDebt::SecurityDescriptor=>Stage::SecurityDescriptor,
    StationBrokerDebt::WindowStation|StationBrokerDebt::Desktop=>Stage::AliasClose,
    StationBrokerDebt::Authentication=>Stage::ObjectVerify,StationBrokerDebt::Association=>Stage::HostAssociation,
    StationBrokerDebt::Protocol=>Stage::PeerIdentity,StationBrokerDebt::Control=>Stage::PipeConnect,
    StationBrokerDebt::Process=>Stage::Spawn,StationBrokerDebt::MainThread=>Stage::MainThreadWait,
    StationBrokerDebt::StdoutReader|StationBrokerDebt::StderrReader=>Stage::ReaderJoin,
    StationBrokerDebt::Admission=>Stage::Admission,StationBrokerDebt::DiagnosticMapping=>Stage::MappingOpen,
    StationBrokerDebt::DiagnosticView=>Stage::MappingView,_=>Stage::OriginalClose,
}}
fn io_code(kind:io::ErrorKind)->u64 {match kind {
    io::ErrorKind::NotFound=>1,io::ErrorKind::PermissionDenied=>2,io::ErrorKind::ConnectionRefused=>3,
    io::ErrorKind::ConnectionReset=>4,io::ErrorKind::ConnectionAborted=>5,io::ErrorKind::NotConnected=>6,
    io::ErrorKind::AddrInUse=>7,io::ErrorKind::AddrNotAvailable=>8,io::ErrorKind::BrokenPipe=>9,
    io::ErrorKind::AlreadyExists=>10,io::ErrorKind::WouldBlock=>11,io::ErrorKind::InvalidInput=>12,
    io::ErrorKind::InvalidData=>13,io::ErrorKind::TimedOut=>14,io::ErrorKind::WriteZero=>15,
    io::ErrorKind::Interrupted=>16,io::ErrorKind::Unsupported=>17,io::ErrorKind::UnexpectedEof=>18,
    io::ErrorKind::OutOfMemory=>19,io::ErrorKind::Other=>20,_=>255,
}}
#[repr(C)]
struct Record {words:[AtomicU64;WORDS]}
#[repr(C,align(64))]
struct Region {header:[AtomicU64;32],parent:Record,helper:Record,reserved:[AtomicU64;224]}
const _: [();STATION_BROKER_DIAGNOSTIC_BYTES]=[();std::mem::size_of::<Region>()];
fn invalid()->io::Error {io::Error::from(io::ErrorKind::InvalidData)}
fn budget(deadline:Instant)->io::Result<()> {
    if Instant::now()>=deadline {Err(io::Error::from(io::ErrorKind::TimedOut))}else{Ok(())}
}
fn header(case:Case,sha:[u8;32])->[u64;32] {
    let mut w=[0;32];w[0]=MAGIC;w[1]=1;w[2]=4096;w[3]=8;w[4]=256;w[5]=1280;w[6]=1024;w[7]=0x8664;
    for (i,b) in case.0.chunks_exact(8).enumerate(){w[8+i]=u64::from_le_bytes(b.try_into().unwrap());}
    for (i,b) in sha.chunks_exact(8).enumerate(){w[10+i]=u64::from_le_bytes(b.try_into().unwrap());}
    w[14]=1;w
}
fn read_once(record:&Record)->Result<[u64;WORDS],StationBrokerDiagnosticRead> {
    let first=record.words[0].load(Ordering::SeqCst);
    if first==0 {return Err(StationBrokerDiagnosticRead::NotStarted);}
    if first&1!=0 {return Err(StationBrokerDiagnosticRead::InFlight);}
    let mut words=[0;WORDS];words[0]=first;
    for (slot,word) in words[1..].iter_mut().zip(&record.words[1..]) {*slot=word.load(Ordering::SeqCst);}
    validate_copy(first,words,record.words[0].load(Ordering::SeqCst))
}
fn validate_copy(first:u64,words:[u64;WORDS],last:u64)->Result<[u64;WORDS],StationBrokerDiagnosticRead> {
    if first==0 {return Err(StationBrokerDiagnosticRead::NotStarted);}
    if first&1!=0 {return Err(StationBrokerDiagnosticRead::InFlight);}
    if first>=MAX_COMMITS*2 {return Err(StationBrokerDiagnosticRead::Unavailable);}
    if first!=last || words[0]!=first {return Err(StationBrokerDiagnosticRead::Torn);}Ok(words)
}
fn matches_scope(words:&[u64;WORDS],scope:Scope)->bool {
    scope.valid() && words[3]==scope.generation && words[4]==u64::from(scope.parent_pid) && words[5]==scope.parent_time
}
fn parent_bound(words:&[u64;WORDS],scope:Scope,case:Case,sha:[u8;32],helper:(u64,u64))->bool {
    matches_scope(words,scope) && words[50..56]==header(case,sha)[8..14] && words[10]&1==0
        && words[6..8]==[helper.0,helper.1] && words[8]&3==3
}
fn retirement_ready(state:StationBrokerDiagnosticSnapshot,claimed:bool,h:&StationBrokerSnapshot)->bool {
    !h.no_replay && h.first_failure.is_none() && state.view==StationBrokerHandle::Held
        && state.mapping==StationBrokerHandle::Held && claimed && state.record==StationBrokerDiagnosticRead::Committed
}
fn handle_code(h:StationBrokerHandle)->u64 {match h {
    StationBrokerHandle::NotAllocated=>0,StationBrokerHandle::InFlight=>1,StationBrokerHandle::Held=>2,
    StationBrokerHandle::ClosedVerified=>3,StationBrokerHandle::Rejected=>4,StationBrokerHandle::Uncertain=>5,
    StationBrokerHandle::CloseFailed{error}=>6|(u64::from(error)<<32),
}}
fn debt_code(d:StationBrokerDebt)->u64 {match d {
    StationBrokerDebt::Process=>1,StationBrokerDebt::MainThread=>2,StationBrokerDebt::Stdout=>3,
    StationBrokerDebt::Stderr=>4,StationBrokerDebt::StdoutReader=>5,StationBrokerDebt::StderrReader=>6,
    StationBrokerDebt::Control=>7,StationBrokerDebt::ReadEvent=>8,StationBrokerDebt::WriteEvent=>9,
    StationBrokerDebt::InheritedParent=>10,StationBrokerDebt::StdinNull=>11,StationBrokerDebt::ChildStdout=>12,
    StationBrokerDebt::ChildStderr=>13,StationBrokerDebt::SecurityDescriptor=>14,StationBrokerDebt::WindowStation=>15,
    StationBrokerDebt::Desktop=>16,StationBrokerDebt::OwnToken=>17,StationBrokerDebt::PeerToken=>18,
    StationBrokerDebt::CallerThreadToken=>19,StationBrokerDebt::Association=>20,StationBrokerDebt::Authentication=>21,
    StationBrokerDebt::Protocol=>22,StationBrokerDebt::Deadline=>23,StationBrokerDebt::Unwind=>24,
    StationBrokerDebt::Admission=>25,StationBrokerDebt::FixtureDelivery=>26,StationBrokerDebt::DiagnosticMapping=>27,
    StationBrokerDebt::DiagnosticView=>28,
}}
fn kind_code(k:StationBrokerErrorKind)->u64 {match k {StationBrokerErrorKind::Win32=>1,
    StationBrokerErrorKind::NtStatus=>2,StationBrokerErrorKind::TimedOut=>3,StationBrokerErrorKind::InvalidData=>4,
    StationBrokerErrorKind::Unsupported=>5,StationBrokerErrorKind::Unwind=>6,StationBrokerErrorKind::Other=>7}}
fn encode(scope:Scope,case:Case,sha:[u8;32],stage:Stage,in_flight:bool,h:&StationBrokerSnapshot)->[u64;WORDS] {
    let mut w=[0;WORDS];w[1]=if h.no_replay {3}else if in_flight {1}else{2};w[2]=stage as u64;
    w[3]=scope.generation;w[4]=u64::from(scope.parent_pid);w[5]=scope.parent_time;
    w[6]=h.helper_pid.map(u64::from).unwrap_or(0);w[7]=h.helper_creation_filetime_100ns.unwrap_or(0);
    w[8]=u64::from(h.helper_pid.is_some())|u64::from(h.helper_creation_filetime_100ns.is_some())<<1
        |u64::from(h.helper_raw_dword.is_some())<<2|u64::from(h.reported_creation.is_some())<<3;
    w[9]=h.helper_raw_dword.map(u64::from).unwrap_or(0);
    w[10]=u64::from(h.no_replay)|u64::from(h.deadline_expired_observed)<<1|u64::from(h.delivery.outer_unknown)<<2;
    if let Some(f)=h.first_failure.or(h.delivery.first_failure) {w[11]=1;w[12]=debt_code(f.target);w[13]=kind_code(f.kind);
        w[14]=u64::from(f.win32_error.is_some())|u64::from(f.native_ntstatus.is_some())<<1;
        w[15]=f.win32_error.map(u64::from).unwrap_or(0);w[16]=f.native_ntstatus.map(|v|u64::from(v as u32)).unwrap_or(0);}
    if let Some(pi)=h.reported_creation {w[17]=u64::from(pi.api_returned_success);w[18]=u64::from(pi.process_id);w[19]=u64::from(pi.thread_id);}
    w[20]=u64::from(h.original_eof[0])|u64::from(h.original_eof[1])<<1
        |u64::from(h.original_reader_joined[0])<<2|u64::from(h.original_reader_joined[1])<<3
        |u64::from(h.helper_objects_closed_receipt)<<4|u64::from(h.host_association_unchanged)<<5;
    w[21]=u64::from(h.slots.read_pending)|u64::from(h.slots.write_pending)<<1;
    let s=h.slots;let states=[s.process,s.main_thread,s.stdout,s.stderr,s.child_stdout,s.child_stderr,
        s.stdin_null,s.inherited_parent,s.own_token,s.peer_token,s.caller_thread_token,s.station_sd,s.desktop_sd,
        s.readers[0],s.readers[1],s.control,s.read_event,s.write_event,h.station_alias,h.desktop_alias];
    for (out,state) in w[24..44].iter_mut().zip(states){*out=handle_code(state);}
    w[44]=handle_code(h.diagnostic.mapping);w[45]=handle_code(h.diagnostic.view);
    w[46]=match h.phase {StationBrokerPhase::Reserved=>0,StationBrokerPhase::Launching=>1,
        StationBrokerPhase::Authenticating=>2,StationBrokerPhase::Offered=>3,StationBrokerPhase::AliasesHeld=>4,
        StationBrokerPhase::HelperRetired=>5,StationBrokerPhase::ResourceOnlyClosed=>6,StationBrokerPhase::Unknown=>7};
    w[47]=match h.connect_release {StationBrokerConnectRelease::NotAttempted=>0,StationBrokerConnectRelease::InFlight=>1,
        StationBrokerConnectRelease::ReleasedSameStartupId=>2,StationBrokerConnectRelease::Missing=>3,
        StationBrokerConnectRelease::Mismatch=>4,StationBrokerConnectRelease::Busy=>5,StationBrokerConnectRelease::Poisoned=>6,
        StationBrokerConnectRelease::Active=>7,StationBrokerConnectRelease::LateRetained=>8};
    w[48]=match h.registry_release {StationBrokerRegistryRelease::Retained=>0,StationBrokerRegistryRelease::InFlight=>1,
        StationBrokerRegistryRelease::RetainedLate=>2,StationBrokerRegistryRelease::RetainedBusy=>3,
        StationBrokerRegistryRelease::RetainedPoisoned=>4,StationBrokerRegistryRelease::RetainedMismatch=>5,
        StationBrokerRegistryRelease::RetainedUnwind=>6,StationBrokerRegistryRelease::TimelyCommitted=>7};
    w[50..56].copy_from_slice(&header(case,sha)[8..14]);w
}

/// Private passive same-thread writer alias, not another lease/owner/HANDLE.
/// Channel drops this alias BEFORE the owning lease can attempt unmap. It only
/// copies pending/slot metadata and never invokes a callback, OS API or wait.
#[derive(Clone,Copy)]
pub(super) struct ChannelWriter {record:*const Record}
impl ChannelWriter {
    pub(super) fn pending(self,read:bool,write:bool,pipe:StationBrokerHandle,
        read_event:StationBrokerHandle,write_event:StationBrokerHandle) {
        let record=unsafe{&*self.record};let previous=record.words[0].load(Ordering::SeqCst);
        if previous==0 || previous&1!=0 || previous>=MAX_COMMITS*2-2 {return;}
        let next=previous+2;record.words[0].store(next-1,Ordering::SeqCst);
        record.words[21].store(u64::from(read)|u64::from(write)<<1,Ordering::SeqCst);
        for (index,state) in [(39,pipe),(40,read_event),(41,write_event)] {
            record.words[index].store(handle_code(state),Ordering::SeqCst);
        }
        record.words[0].store(next,Ordering::SeqCst);
    }
}

pub(super) struct Lease {
    case:Case,sha:[u8;32],scope:Scope,writer:Writer,raw:isize,view:*mut Region,
    state:StationBrokerDiagnosticSnapshot,claimed:bool,sequence:u64,
    helper_identity:(u64,u64),
    first_io_kind:Option<u64>,
    unmap_in_flight:bool,
}
impl Lease {
    pub(super) fn new(case:Case,sha:[u8;32],scope:Scope,writer:Writer)->Self {
        let mut state=StationBrokerDiagnosticSnapshot::default();state.enabled=true;
        state.record=StationBrokerDiagnosticRead::NotAcquired;state.helper_close=StationBrokerHelperDiagnosticClose::NotObserved;
        Self {case,sha,scope,writer,raw:0,view:ptr::null_mut(),state,claimed:false,sequence:0,
            helper_identity:(0,0),first_io_kind:None,unmap_in_flight:false}
    }
    pub(super) fn case(&self)->Case {self.case}
    pub(super) fn snapshot(&self)->StationBrokerDiagnosticSnapshot {self.state}
    pub(super) fn set_helper_close(&mut self,value:StationBrokerHelperDiagnosticClose){self.state.helper_close=value;}
    pub(super) fn set_verified_helper(&mut self,pid:u32,time:u64)->io::Result<()> {
        if pid==0 || time==0 || self.helper_identity!=(0,0) || self.state.mapping!=StationBrokerHandle::NotAllocated {return Err(invalid());}
        self.helper_identity=(u64::from(pid),time);Ok(())
    }
    pub(super) fn set_scope(&mut self,scope:Scope)->io::Result<()> {
        if self.scope.valid() || !scope.valid() || self.scope.generation!=scope.generation
            || self.state.mapping!=StationBrokerHandle::NotAllocated {return Err(invalid());}
        self.scope=scope;Ok(())
    }
    pub(super) fn first_io(&mut self,kind:io::ErrorKind){if self.first_io_kind.is_none(){self.first_io_kind=Some(io_code(kind));}}
    pub(super) fn channel_writer(&self)->Option<ChannelWriter> {
        if !self.claimed || self.unmap_in_flight{return None;}self.record().map(|record|ChannelWriter {record})
    }
    fn record(&self)->Option<&Record> {
        if self.view.is_null(){return None;}
        // Acquired fixed, aligned 4096B view. Only this facade writes its record;
        // no Rust non-atomic references to concurrently written record payload.
        Some(unsafe {match self.writer {Writer::Parent=>&(*self.view).parent,Writer::Helper=>&(*self.view).helper}})
    }
    pub(super) fn acquire(&mut self,deadline:Instant)->io::Result<()> {
        if !cfg!(all(target_arch="x86_64",target_endian="little")) {
            self.state.record=StationBrokerDiagnosticRead::Unsupported;return Err(io::Error::from(io::ErrorKind::Unsupported));
        }
        if !self.scope.valid() || self.sha==[0;32] || self.state.mapping!=StationBrokerHandle::NotAllocated {return Err(invalid());}
        let name=self.case.name();budget(deadline)?;
        self.state.last_stage=Stage::MappingOpen as u64;self.state.mapping=StationBrokerHandle::InFlight;
        let raw=unsafe{OpenFileMappingW(6,0,name.as_ptr())};let error=if raw==0 {unsafe{GetLastError()}}else{0};
        self.raw=raw;self.state.mapping=if raw==0 {StationBrokerHandle::Rejected}
            else if raw == -1 {StationBrokerHandle::Uncertain}else{StationBrokerHandle::Held};
        if raw==0 {return Err(io::Error::from_raw_os_error(error as i32));}
        if raw == -1 {return Err(invalid());}budget(deadline)?;
        self.state.last_stage=Stage::MappingView as u64;self.state.view=StationBrokerHandle::InFlight;
        let view=unsafe{MapViewOfFile(self.raw,6,0,0,STATION_BROKER_DIAGNOSTIC_BYTES)};
        let error=if view.is_null(){unsafe{GetLastError()}}else{0};
        self.view=view.cast();self.state.view=if view.is_null(){StationBrokerHandle::Rejected}else{StationBrokerHandle::Held};
        if view.is_null(){return Err(io::Error::from_raw_os_error(error as i32));}budget(deadline)?;
        if (view as usize)%64!=0 {return Err(invalid());}
        let region=unsafe{&*self.view};let expected=header(self.case,self.sha);
        for (actual,expected) in region.header.iter().zip(expected) {
            if actual.load(Ordering::SeqCst)!=expected {self.state.record=StationBrokerDiagnosticRead::WrongCase;return Err(invalid());}
        }
        match self.writer {
            Writer::Parent=>{
                if region.parent.words.iter().chain(region.helper.words.iter()).chain(region.reserved.iter())
                    .any(|v|v.load(Ordering::SeqCst)!=0) {self.state.record=StationBrokerDiagnosticRead::WrongCase;return Err(invalid());}
            },
            Writer::Helper=>{
                let parent=read_once(&region.parent).map_err(|reason|{self.state.record=reason;invalid()})?;
                if !parent_bound(&parent,self.scope,self.case,self.sha,
                    (self.verified_helper_pid(),self.verified_helper_time())) {
                    self.state.record=StationBrokerDiagnosticRead::WrongCase;return Err(invalid());
                }
                if region.helper.words.iter().any(|v|v.load(Ordering::SeqCst)!=0){return Err(invalid());}
            },
        }
        let record=self.record().ok_or_else(invalid)?;
        if record.words[0].compare_exchange(0,1,Ordering::SeqCst,Ordering::SeqCst).is_err(){return Err(invalid());}
        self.claimed=true;self.state.record=StationBrokerDiagnosticRead::InFlight;budget(deadline)
    }
    // Helper identity is installed from actual verified HELLO/own identity BEFORE
    // acquire. Stored outside Scope because parent binding never changes.
    fn verified_helper_pid(&self)->u64 {self.helper_identity.0}
    fn verified_helper_time(&self)->u64 {self.helper_identity.1}
    pub(super) fn publish(&mut self,stage:Stage,in_flight:bool,h:&StationBrokerSnapshot) {
        self.state.last_stage=stage as u64;
        if !self.claimed || self.view.is_null(){return;}
        if self.unmap_in_flight {self.state.record=StationBrokerDiagnosticRead::Unavailable;return;}
        let previous=match self.record(){Some(record)=>record.words[0].load(Ordering::SeqCst),None=>return};
        if previous>=MAX_COMMITS*2-2 || (previous&1!=0 && !(previous==1 && self.sequence==0)) {
            self.state.record=StationBrokerDiagnosticRead::Unavailable;return;
        }
        let next=if previous==1 {2}else{previous+2};
        let mut payload=encode(self.scope,self.case,self.sha,stage,in_flight,h);
        payload[22]=self.first_io_kind.unwrap_or(0);payload[23]=u64::from(self.first_io_kind.is_some());
        if let Some(record)=self.record() {
            record.words[0].store(next-1,Ordering::SeqCst);
            for (word,value) in record.words[1..].iter().zip(payload[1..].iter()){word.store(*value,Ordering::SeqCst);}
            record.words[0].store(next,Ordering::SeqCst);
        }
        self.sequence=next;self.state.record=StationBrokerDiagnosticRead::Committed;
    }
    pub(super) fn retire(&mut self,deadline:Instant,h:&StationBrokerSnapshot)->io::Result<()> {
        if !retirement_ready(self.state,self.claimed,h) {return Err(invalid());}
        budget(deadline)?;self.state.view=StationBrokerHandle::InFlight;
        let mut closing=*h;closing.diagnostic=self.state;self.publish(Stage::ViewUnmap,true,&closing);
        if self.state.record!=StationBrokerDiagnosticRead::Committed {return Err(invalid());}
        self.unmap_in_flight=true; // An uncertain/unwound return must never dereference the view.
        let ok=unsafe{UnmapViewOfFile(self.view.cast())};let error=if ok==0 {unsafe{GetLastError()}}else{0};
        if ok==0 {self.unmap_in_flight=false;self.state.view=StationBrokerHandle::CloseFailed{error};return Err(io::Error::from_raw_os_error(error as i32));}
        self.view=ptr::null_mut();self.claimed=false;self.unmap_in_flight=false;
        self.state.view=StationBrokerHandle::ClosedVerified;
        self.state.record=StationBrokerDiagnosticRead::Retired; // NEVER publish through retired view.
        budget(deadline)?;self.state.last_stage=Stage::MappingClose as u64;self.state.mapping=StationBrokerHandle::InFlight;
        let ok=unsafe{CloseHandle(self.raw)};let error=if ok==0 {unsafe{GetLastError()}}else{0};
        if ok==0 {self.state.mapping=StationBrokerHandle::CloseFailed{error};return Err(io::Error::from_raw_os_error(error as i32));}
        self.raw=0;self.state.mapping=StationBrokerHandle::ClosedVerified;budget(deadline)
    }
}
// No Drop implementation: neither Unknown nor stack unwind can unmap/close.

#[cfg(test)]
#[path="station_broker_diagnostic_tests.rs"]
mod tests;
