//! Private, in-memory startup observations for the SAME admitted P/import owner.
//! No I/O, clock, native query, HANDLE/PID/path/nonce, callback, registry or permit.
//! This is NOT the A1/v6/v7 wire. Encoding alone grants no operation or acceptance.
use std::io;

pub(crate) const BYTES: usize = 256;
const MAGIC: &[u8; 8] = b"C28PSD01";
const HISTORY: usize = 16;

// Finite diagnostic data only: no handle, PID, callback or retirement permit.
// The value identifies a predicate that actually rejected an existing capture;
// it never substitutes for an exit observation or authorizes another operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ParentExitPredicate {
    RetainedIdentity = 1, RetainedJobMembership = 2, RetainedNoninheritance = 3,
    ProcessSignal = 4, PrimaryThreadSignal = 5, PostExitJobEmpty = 6,
    JobActiveMissing = 7, JobStillActive = 8, ProcessExitMissing = 9,
    BootstrapDelivered = 10, OriginalIdentity = 11, ChildIdentity = 12,
    ProcessExitNotZero = 13, PrimaryThreadExitNotZero = 14,
    JobAccountingLength = 15, JobLimitedTermination = 16, JobTotalMissingOrZero = 17,
}
impl TryFrom<u8> for ParentExitPredicate {
    type Error = ();
    fn try_from(value:u8)->Result<Self,Self::Error> {
        use ParentExitPredicate::*;
        Ok(match value {
            1=>RetainedIdentity,2=>RetainedJobMembership,3=>RetainedNoninheritance,
            4=>ProcessSignal,5=>PrimaryThreadSignal,6=>PostExitJobEmpty,
            7=>JobActiveMissing,8=>JobStillActive,9=>ProcessExitMissing,
            10=>BootstrapDelivered,11=>OriginalIdentity,12=>ChildIdentity,
            13=>ProcessExitNotZero,14=>PrimaryThreadExitNotZero,
            15=>JobAccountingLength,16=>JobLimitedTermination,17=>JobTotalMissingOrZero,
            _=>return Err(()),
        })
    }
}
#[derive(Debug)]
pub struct ParentExitFailure { predicate:ParentExitPredicate }
impl ParentExitFailure {
    pub fn error(predicate:ParentExitPredicate)->io::Error {
        io::Error::new(io::ErrorKind::InvalidData,Self{predicate})
    }
    pub fn predicate(&self)->ParentExitPredicate {self.predicate}
}
impl std::fmt::Display for ParentExitFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        write!(f,"A1 original P exit predicate rejected: {:?}",self.predicate)
    }
}
impl std::error::Error for ParentExitFailure {}


macro_rules! byte_enum {
    ($name:ident { $($variant:ident = $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u8)]
        pub(crate) enum $name { $($variant = $value),+ }
        impl TryFrom<u8> for $name {
            type Error = CodecError;
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value { $($value => Ok(Self::$variant),)+ _ => Err(CodecError::Enum) }
            }
        }
    }
}
byte_enum!(Scope { Parent = 1, Import = 2 });
byte_enum!(Status { Recording = 0, FunctionReturnedOk = 1, IoErrorLatched = 2, UnwindLatched = 3 });
byte_enum!(Stage {
    NotObserved = 0,
    ParentSubject = 1, ParentImage = 2, ParentPins = 3, ParentDigests = 4,
    BootstrapRandom = 5, BootstrapSecurity = 6, BootstrapEvent = 7,
    BootstrapServer = 8, BootstrapConnect = 9, BootstrapReader = 10,
    BootstrapConnected = 11, ParentAlias = 12, StdioAliases = 13,
    NamespaceInputs = 14, NamespaceSecurity = 15, NamespaceCreate = 16,
    NamespacePin = 17, PrecreateSubject = 18, ChildCreate = 19,
    ChildIdentity = 20, ChildSelfAlias = 21, RecordEncode = 22,
    RevalidatePins = 23, ChildResume = 24, BootstrapWrite = 25,
    BootstrapAliasClose = 26, InheritedAliasClose = 27, WaitChild = 28,
    ObserveChild = 29, ResourceClose = 30, RetireChild = 31,
    ObserveRetainedChild = 32, ObserveThreadExit = 33, ObserveJobAccounting = 34,
    ValidateChildExit = 35,
    ImportAliases = 64, ImportParentIdentity = 65, ImportServerIdentity = 66,
    ImportParentImage = 67, ImportAuthPins = 68, ImportParentDigest = 69,
    ImportToken = 70, ImportSubject = 71, ImportRead = 72, ImportDecode = 73,
    ImportTransferredAliases = 74, ImportRecordIdentity = 75,
    ImportFiles = 76, ImportRevalidate = 77, ImportAliasClose = 78
});
impl Stage {
    fn belongs(self, scope: Scope) -> bool {
        self == Self::NotObserved || match scope {
            Scope::Parent => (1..=35).contains(&(self as u8)),
            Scope::Import => (64..=78).contains(&(self as u8)),
        }
    }
}
byte_enum!(Api {
    // Handle-returning APIs record ONLY their existing success predicate, never a value.
    CreateEvent = 1, SecurityDescriptor = 2, CreateNamedPipe = 3,
    ConnectNamedPipe = 4, CreateFile = 5, DuplicateHandle = 6,
    CreateDirectory = 7, ResetEvent = 8, WriteFile = 9,
    ReadFile = 10, GetOverlappedResult = 11
});
byte_enum!(Kind {
    NotFound = 1, PermissionDenied = 2, AlreadyExists = 3, InvalidInput = 4,
    InvalidData = 5, TimedOut = 6, WouldBlock = 7, Interrupted = 8,
    UnexpectedEof = 9, BrokenPipe = 10, Other = 11, Unwind = 12
});
impl Kind {
    fn from_io(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            io::ErrorKind::InvalidInput => Self::InvalidInput,
            io::ErrorKind::InvalidData => Self::InvalidData,
            io::ErrorKind::TimedOut => Self::TimedOut,
            io::ErrorKind::WouldBlock => Self::WouldBlock,
            io::ErrorKind::Interrupted => Self::Interrupted,
            io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            _ => Self::Other,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Failure { stage: Stage, kind: Kind, raw: Option<i32> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Native { stage: Stage, api: Api, succeeded: bool, raw: Option<i32> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event { kind: u8, stage: Stage, api: u8, flags: u8, sequence: u32, raw: i32 }
impl Event {
    const EMPTY: Self = Self { kind: 0, stage: Stage::NotObserved, api: 0, flags: 0, sequence: 0, raw: 0 };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodecError { Length, Magic, Version, Reserved, Enum, Inconsistent }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    scope: Scope, status: Status, stage: Stage, completed: Stage, in_flight: bool,
    first: Option<Failure>, last_native: Option<Native>,
    exit_predicate: Option<ParentExitPredicate>,
    events: [Event; HISTORY], len: u8, serial: u32,
    history_truncated: bool, sequence_saturated: bool, inconsistent: bool,
}
/// Mutated only through the already-exclusive &mut of its owning Parent/ImportCore.
/// A snapshot is a bounded COPY; it cannot retain an owner, native object or lease.
pub(crate) struct Capture { value: Snapshot }
impl Capture {
    pub(crate) fn new(scope: Scope) -> Self {
        Self { value: Snapshot { scope, status: Status::Recording,
            stage: Stage::NotObserved, completed: Stage::NotObserved, in_flight: false,
            first: None, last_native: None, exit_predicate: None, events: [Event::EMPTY; HISTORY], len: 0, serial: 0,
            history_truncated: false, sequence_saturated: false, inconsistent: false } }
    }
    fn recording(&self) -> bool { self.value.status == Status::Recording && !self.value.inconsistent }
    fn push(&mut self, kind: u8, stage: Stage, api: u8, flags: u8, raw: i32) {
        let Some(next) = self.value.serial.checked_add(1) else {
            self.value.sequence_saturated = true; self.value.history_truncated = true; return;
        };
        self.value.serial = next;
        if self.value.len as usize == HISTORY {
            self.value.events.copy_within(1..HISTORY, 0);
            self.value.len -= 1; self.value.history_truncated = true;
        }
        self.value.events[self.value.len as usize] = Event { kind, stage, api, flags, sequence: next, raw };
        self.value.len += 1;
    }
    pub(crate) fn enter(&mut self, stage: Stage) {
        if !self.recording() { return; }
        if stage == Stage::NotObserved || !stage.belongs(self.value.scope) || self.value.in_flight {
            self.value.inconsistent = true; return;
        }
        self.value.stage = stage; self.value.in_flight = true;
        self.push(1, stage, 0, 0, 0);
    }
    pub(crate) fn completed(&mut self, stage: Stage) {
        if !self.recording() { return; }
        if stage != self.value.stage || !self.value.in_flight {
            self.value.inconsistent = true; return;
        }
        self.value.completed = stage; self.value.in_flight = false;
        self.push(2, stage, 0, 0, 0);
    }
    /// Native arguments/results are supplied only by an ALREADY returned original call.
    /// No GetLastError occurs here. Success has no fabricated last-error=0 observation.
    /// An ERROR_IO_PENDING native false is an observation, not a latched business failure.
    pub(crate) fn native(&mut self, api: Api, succeeded: bool, raw: Option<i32>) {
        if !self.recording() { return; }
        if !self.value.in_flight || (succeeded && raw.is_some()) {
            self.value.inconsistent = true; return;
        }
        self.value.last_native = Some(Native { stage: self.value.stage, api, succeeded, raw });
        self.push(3, self.value.stage, api as u8,
            (if succeeded { 1 } else { 2 }) | if raw.is_some() { 128 } else { 0 }, raw.unwrap_or(0));
    }
    pub(crate) fn latch_io(&mut self, error: &io::Error) {
        if self.value.status != Status::Recording { return; }
        self.value.first = Some(Failure { stage: self.value.stage,
            kind: Kind::from_io(error.kind()), raw: error.raw_os_error() });
        self.value.exit_predicate=error.get_ref().and_then(|e|e.downcast_ref::<ParentExitFailure>()).map(|e|e.predicate());
        self.value.status = Status::IoErrorLatched;
    }
    pub(crate) fn latch_unwind(&mut self) {
        if self.value.status != Status::Recording { return; }
        self.value.first = Some(Failure { stage: self.value.stage, kind: Kind::Unwind, raw: None });
        self.value.status = Status::UnwindLatched;
    }
    pub(crate) fn returned_ok(&mut self) {
        if !self.recording() { return; }
        if self.value.in_flight || self.value.stage == Stage::NotObserved {
            self.value.inconsistent = true; return;
        }
        self.value.status = Status::FunctionReturnedOk;
    }
    pub(crate) fn snapshot(&self) -> Snapshot { self.value }
}
fn put32(out: &mut [u8], value: u32) { out.copy_from_slice(&value.to_le_bytes()); }
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset+1], bytes[offset+2], bytes[offset+3]])
}
impl Snapshot {
    fn validate(&self) -> Result<(), CodecError> {
        let bad = || Err(CodecError::Inconsistent);
        if self.inconsistent || !self.stage.belongs(self.scope) || !self.completed.belongs(self.scope)
            || self.len as usize > HISTORY { return bad(); }
        if !self.in_flight && self.stage != self.completed { return bad(); }
        match (self.status, self.first) {
            (Status::Recording | Status::FunctionReturnedOk, None) => {},
            (Status::IoErrorLatched, Some(f)) if f.kind != Kind::Unwind && f.stage == self.stage => {},
            (Status::UnwindLatched, Some(f)) if f.kind == Kind::Unwind && f.raw.is_none() && f.stage == self.stage => {},
            _ => return bad(),
        }
        if self.exit_predicate.is_some() && (self.scope!=Scope::Parent
            || self.status!=Status::IoErrorLatched || !self.in_flight
            || !matches!(self.stage,Stage::ObserveChild|Stage::ObserveRetainedChild|Stage::ObserveThreadExit|Stage::ObserveJobAccounting|Stage::ValidateChildExit)
            || self.first.is_none_or(|f|f.kind!=Kind::InvalidData||f.raw.is_some())) {return bad();}
        if self.status == Status::FunctionReturnedOk && (self.in_flight || self.stage == Stage::NotObserved) { return bad(); }
        if self.sequence_saturated && (!self.history_truncated || self.serial != u32::MAX) { return bad(); }
        if !self.history_truncated && self.serial != u32::from(self.len) { return bad(); }
        if self.history_truncated && (self.len as usize != HISTORY || self.serial <= HISTORY as u32) { return bad(); }
        let mut prior = 0;
        for (i, e) in self.events.iter().enumerate() {
            if i >= self.len as usize { if *e != Event::EMPTY { return bad(); } continue; }
            if e.stage == Stage::NotObserved || !e.stage.belongs(self.scope)
                || e.sequence == 0 || e.sequence <= prior || e.sequence > self.serial { return bad(); }
            if i > 0 && e.sequence != prior + 1 { return bad(); }
            prior = e.sequence;
            match e.kind {
                1 | 2 if e.api == 0 && e.flags == 0 && e.raw == 0 => {},
                3 => {
                    Api::try_from(e.api)?;
                    let code=e.flags & 127;
                    if (code != 1 && code != 2) || (code == 1 && e.flags & 128 != 0)
                        || (e.flags & 128 == 0 && e.raw != 0) { return bad(); }
                },
                _ => return bad(),
            }
        }
        if prior != self.serial { return bad(); }
        // Only the bounded prefix may be missing. Within retained history, entered,
        // completed and native-returned records must describe one ordered operation.
        let mut active: Option<(Stage,bool)> = None;
        let mut last_completed = None;
        for e in &self.events[..self.len as usize] {
            match e.kind {
                1 => {
                    if active.is_some_and(|(_,busy)|busy) { return bad(); }
                    active=Some((e.stage,true));
                },
                2 => {
                    if active.is_some_and(|(stage,busy)|stage!=e.stage||!busy) { return bad(); }
                    active=Some((e.stage,false));last_completed=Some(e.stage);
                },
                3 => {
                    if active.is_some_and(|(stage,busy)|stage!=e.stage||!busy) { return bad(); }
                    active=Some((e.stage,true));
                },
                _ => return bad(),
            }
        }
        if !self.history_truncated && last_completed.is_none() && self.completed != Stage::NotObserved { return bad(); }
        if !self.sequence_saturated {
            if active.is_some_and(|(stage,busy)|stage!=self.stage||busy!=self.in_flight)
                || last_completed.is_some_and(|stage|stage!=self.completed) { return bad(); }
        }
        if self.len == 0 && (self.stage != Stage::NotObserved || self.completed != Stage::NotObserved
            || self.in_flight || self.last_native.is_some()) { return bad(); }
        if let Some(n) = self.last_native {
            if n.stage == Stage::NotObserved || !n.stage.belongs(self.scope)
                || (n.succeeded && n.raw.is_some()) { return bad(); }
            // Absence of a native event after truncation is NOT proof it never happened.
            if let Some(e) = self.events[..self.len as usize].iter().rev().find(|e|e.kind==3) {
                if !self.sequence_saturated && (e.stage != n.stage || e.api != n.api as u8 || (e.flags & 127 == 1) != n.succeeded
                    || (e.flags & 128 != 0) != n.raw.is_some() || e.raw != n.raw.unwrap_or(0)) { return bad(); }
            } else if !self.history_truncated { return bad(); }
        } else if self.events[..self.len as usize].iter().any(|e|e.kind==3) { return bad(); }
        Ok(())
    }
    pub(crate) fn encode(&self) -> Result<[u8; BYTES], CodecError> {
        self.validate()?;
        let mut out = [0u8; BYTES];out[..8].copy_from_slice(MAGIC);out[8]=2;
        out[9]=self.scope as u8;out[10]=self.status as u8;out[11]=self.stage as u8;
        out[12]=self.completed as u8;out[13]=self.len;
        out[14]=u8::from(self.history_truncated)|(u8::from(self.sequence_saturated)<<1)|(u8::from(self.in_flight)<<2);
        put32(&mut out[16..20],self.serial);
        if let Some(f)=self.first {out[20]=f.stage as u8;out[21]=f.kind as u8;out[22]=u8::from(f.raw.is_some());put32(&mut out[24..28],f.raw.unwrap_or(0) as u32);}
        if let Some(n)=self.last_native {out[28]=n.api as u8;out[29]=n.stage as u8;out[30]=if n.succeeded{1}else{2};out[31]=u8::from(n.raw.is_some());put32(&mut out[32..36],n.raw.unwrap_or(0) as u32);}
        out[36]=self.exit_predicate.map_or(0,|p|p as u8);
        for (i,e) in self.events[..self.len as usize].iter().enumerate() {
            let at=64+i*12;out[at]=e.kind;out[at+1]=e.stage as u8;out[at+2]=e.api;out[at+3]=e.flags;
            put32(&mut out[at+4..at+8],e.sequence);put32(&mut out[at+8..at+12],e.raw as u32);
        }
        Ok(out)
    }
    pub(crate) fn decode(bytes:&[u8]) -> Result<Self,CodecError> {
        if bytes.len()!=BYTES{return Err(CodecError::Length);}
        if &bytes[..8]!=MAGIC{return Err(CodecError::Magic);}
        if bytes[8]!=1&&bytes[8]!=2{return Err(CodecError::Version);}
        if bytes[14]&!7!=0||bytes[15]!=0||bytes[23]!=0||bytes[37..64].iter().any(|v|*v!=0)||(bytes[8]==1&&bytes[36]!=0)
            ||bytes[22]>1||bytes[31]>1{return Err(CodecError::Reserved);}
        let exit_predicate=if bytes[36]==0{None}else{Some(ParentExitPredicate::try_from(bytes[36]).map_err(|_|CodecError::Enum)?)};
        let status=Status::try_from(bytes[10])?;let stage=Stage::try_from(bytes[11])?;
        let completed=Stage::try_from(bytes[12])?;
        let first=if bytes[21]==0 {
            if bytes[20]!=0||bytes[22]!=0||u32_at(bytes,24)!=0{return Err(CodecError::Inconsistent);}None
        }else{
            if bytes[22]==0&&u32_at(bytes,24)!=0{return Err(CodecError::Inconsistent);}
            Some(Failure{stage:Stage::try_from(bytes[20])?,kind:Kind::try_from(bytes[21])?,
                raw:if bytes[22]==1{Some(u32_at(bytes,24) as i32)}else{None}})
        };
        let last_native=if bytes[28]==0{
            if bytes[29..36].iter().any(|v|*v!=0){return Err(CodecError::Inconsistent);}None
        }else{
            if bytes[30]!=1&&bytes[30]!=2{return Err(CodecError::Inconsistent);}
            if bytes[31]==0&&u32_at(bytes,32)!=0{return Err(CodecError::Inconsistent);}
            Some(Native{stage:Stage::try_from(bytes[29])?,api:Api::try_from(bytes[28])?,
                succeeded:bytes[30]==1,raw:if bytes[31]==1{Some(u32_at(bytes,32) as i32)}else{None}})
        };
        let mut events=[Event::EMPTY;HISTORY];
        for (i,e) in events.iter_mut().enumerate(){
            let at=64+i*12;
            *e=Event{kind:bytes[at],stage:Stage::try_from(bytes[at+1])?,api:bytes[at+2],flags:bytes[at+3],
                sequence:u32_at(bytes,at+4),raw:u32_at(bytes,at+8) as i32};
        }
        let value=Self{scope:Scope::try_from(bytes[9])?,status,stage,completed,first,last_native,exit_predicate,events,
            len:bytes[13],serial:u32_at(bytes,16),history_truncated:bytes[14]&1!=0,
            sequence_saturated:bytes[14]&2!=0,in_flight:bytes[14]&4!=0,inconsistent:false};
        value.validate()?;Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pending()->Capture {let mut c=Capture::new(Scope::Parent);c.enter(Stage::BootstrapReader);c}
    fn failed()->Snapshot {let mut c=pending();c.native(Api::CreateFile,false,Some(2));c.latch_io(&io::Error::from_raw_os_error(2));c.snapshot()}
    #[test] fn d2103_01_initial_is_not_observed_and_not_returned_ok(){
        for scope in [Scope::Parent,Scope::Import] {let s=Capture::new(scope).snapshot();assert_eq!(s.stage,Stage::NotObserved);assert_eq!(s.status,Status::Recording);assert_eq!(s.first,None);assert_eq!(s.last_native,None);assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);}
    }
    #[test] fn d2103_02_entered_is_not_completed_or_native_success(){
        let s=pending().snapshot();assert!(s.in_flight);assert_eq!(s.completed,Stage::NotObserved);assert_eq!(s.last_native,None);assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
    }
    #[test] fn d2103_03_returned_native_error_keeps_exact_bits_and_first_error(){
        let s=failed();assert_eq!(s.status,Status::IoErrorLatched);assert_eq!(s.first.unwrap().raw,Some(2));assert_eq!(s.last_native.unwrap().raw,Some(2));assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
        let mut c=pending();c.native(Api::CreateFile,false,Some(0xc0000001u32 as i32));c.latch_io(&io::Error::from_raw_os_error(0xc0000001u32 as i32));assert_eq!(Snapshot::decode(&c.snapshot().encode().unwrap()).unwrap(),c.snapshot());
    }
    #[test] fn d2103_04_success_does_not_invent_a_last_error_zero(){
        let mut c=pending();c.native(Api::CreateFile,true,None);assert_eq!(c.snapshot().last_native.unwrap().raw,None);
        let mut bad=pending();bad.native(Api::CreateFile,true,Some(0));assert!(bad.snapshot().encode().is_err());
    }
    #[test] fn d2103_05_pending_native_false_is_not_a_business_failure(){
        let mut c=pending();c.native(Api::ConnectNamedPipe,false,Some(997));let s=c.snapshot();assert_eq!(s.status,Status::Recording);assert_eq!(s.first,None);assert!(s.in_flight);assert_eq!(s.last_native.unwrap().raw,Some(997));
    }
    #[test] fn d2103_06_first_error_and_whole_snapshot_freeze_without_late_queries(){
        let mut c=pending();c.latch_io(&io::Error::new(io::ErrorKind::TimedOut,"not exported"));let first=c.snapshot();
        c.latch_io(&io::Error::from_raw_os_error(5));c.latch_unwind();c.enter(Stage::RetireChild);c.completed(Stage::RetireChild);c.native(Api::CreateFile,true,None);c.returned_ok();assert_eq!(c.snapshot(),first);
    }
    #[test] fn d2103_07_unwind_has_no_fabricated_io_code_or_payload(){
        let mut c=pending();c.latch_unwind();let s=c.snapshot();assert_eq!(s.first.unwrap().kind,Kind::Unwind);assert_eq!(s.first.unwrap().raw,None);assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
    }
    #[test] fn d2103_08_same_scope_required_and_wrong_completion_cannot_encode(){
        let mut c=Capture::new(Scope::Import);c.enter(Stage::ChildCreate);assert!(c.snapshot().encode().is_err());
        let mut c=pending();c.completed(Stage::ChildCreate);assert!(c.snapshot().encode().is_err());
    }
    #[test] fn d2103_09_bounded_history_reports_truncation_and_retains_first(){
        let mut c=Capture::new(Scope::Parent);
        for _ in 0..40 {c.enter(Stage::ParentPins);c.completed(Stage::ParentPins);}
        assert_eq!(c.snapshot().len,16);assert!(c.snapshot().history_truncated);assert_eq!(c.snapshot().serial,80);
        c.enter(Stage::ParentDigests);c.latch_io(&io::Error::from_raw_os_error(23));let s=c.snapshot();assert_eq!(s.first.unwrap().raw,Some(23));assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
    }
    #[test] fn d2103_10_counter_saturation_is_explicit_and_does_not_wrap(){
        let mut c=Capture::new(Scope::Parent);c.enter(Stage::ParentPins);c.native(Api::CreateFile,true,None);c.completed(Stage::ParentPins);
        for _ in 0..6{c.enter(Stage::ParentPins);c.completed(Stage::ParentPins);}c.enter(Stage::BootstrapReader);
        c.value.serial=u32::MAX-1;c.value.history_truncated=true;
        for (i,e) in c.value.events.iter_mut().enumerate(){e.sequence=u32::MAX-16+i as u32;}
        c.native(Api::CreateFile,false,Some(2));c.native(Api::ReadFile,true,None);c.completed(Stage::BootstrapReader);
        let s=c.snapshot();assert!(s.sequence_saturated);assert_eq!(s.serial,u32::MAX);assert_eq!(s.len,16);
        assert_eq!(s.last_native.unwrap().api,Api::ReadFile);assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
    }
    #[test] fn d2103_11_exact_length_magic_version_and_reserved_bits_required(){
        let b=failed().encode().unwrap();assert!(Snapshot::decode(&b[..255]).is_err());let mut long=b.to_vec();long.push(0);assert!(Snapshot::decode(&long).is_err());
        for at in [0,8,15,23,36,63] {let mut x=b;x[at]^=128;assert!(Snapshot::decode(&x).is_err(),"{at}");}
    }
    #[test] fn d2103_12_invalid_enum_bool_history_count_and_scope_are_rejected(){
        let b=failed().encode().unwrap();for (at,value) in [(9,3),(10,4),(11,63),(12,63),(13,17),(14,8),(21,255),(22,2),(28,255),(30,3),(31,2)] {let mut x=b;x[at]=value;assert!(Snapshot::decode(&x).is_err(),"{at}");}
        let mut x=b;x[9]=Scope::Import as u8;assert!(Snapshot::decode(&x).is_err());
    }
    #[test] fn d2103_13_absent_fields_and_unused_history_are_canonical_zero(){
        let b=Capture::new(Scope::Parent).snapshot().encode().unwrap();
        for at in [20,22,24,29,30,31,32,64,68,255] {let mut x=b;x[at]=1;assert!(Snapshot::decode(&x).is_err(),"{at}");}
    }
    #[test] fn d2103_14_event_sequences_must_be_consecutive_and_match_header(){
        let b=failed().encode().unwrap();let mut x=b;put32(&mut x[16..20],3);assert!(Snapshot::decode(&x).is_err());
        let mut x=b;put32(&mut x[80..84],1);assert!(Snapshot::decode(&x).is_err());
        let mut x=b;x[64]=3;assert!(Snapshot::decode(&x).is_err());
    }
    #[test] fn d2103_15_logical_success_cannot_erase_error_or_become_native_exit(){
        let mut s=failed();s.status=Status::FunctionReturnedOk;assert!(s.encode().is_err());
        let mut c=pending();c.returned_ok();assert!(c.snapshot().encode().is_err());
        let mut c=pending();c.completed(Stage::BootstrapReader);c.returned_ok();let s=c.snapshot();
        assert_eq!(s.status,Status::FunctionReturnedOk);assert_eq!(s.last_native,None);assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
        // The type has no native-exit DWORD, process identity or close/permission field.
    }
    #[test] fn d2103_16_prior_native_result_is_not_promoted_to_current_failure_cause(){
        let mut c=Capture::new(Scope::Parent);c.enter(Stage::BootstrapServer);c.native(Api::CreateNamedPipe,true,None);c.completed(Stage::BootstrapServer);
        c.enter(Stage::ParentPins);c.latch_io(&io::Error::new(io::ErrorKind::InvalidData,"secret-free-encoding"));
        let s=c.snapshot();assert_eq!(s.first.unwrap().stage,Stage::ParentPins);assert_eq!(s.first.unwrap().raw,None);assert_eq!(s.last_native.unwrap().stage,Stage::BootstrapServer);assert!(s.last_native.unwrap().succeeded);
        assert_eq!(Snapshot::decode(&s.encode().unwrap()).unwrap(),s);
    }
    #[test] fn d2103_17_native_capture_requires_an_entered_original_operation(){
        let mut c=Capture::new(Scope::Parent);c.native(Api::CreateFile,false,Some(2));assert!(c.snapshot().encode().is_err());
    }
    #[test] fn d2103_18_unwind_and_io_error_kinds_are_not_interchangeable(){
        let mut x=failed().encode().unwrap();x[10]=Status::UnwindLatched as u8;assert!(Snapshot::decode(&x).is_err());
        let mut c=pending();c.latch_unwind();let mut x=c.snapshot().encode().unwrap();x[10]=Status::IoErrorLatched as u8;assert!(Snapshot::decode(&x).is_err());
    }
    #[test] fn d2103_19_sealed_copy_does_not_borrow_or_follow_a_different_owner(){
        let mut a=pending();a.latch_io(&io::Error::from_raw_os_error(5));let saved=a.snapshot();
        let mut b=Capture::new(Scope::Import);b.enter(Stage::ImportRead);b.native(Api::ReadFile,true,None);b.completed(Stage::ImportRead);b.returned_ok();
        assert_eq!(a.snapshot(),saved);assert_ne!(saved,b.snapshot());
    }
    #[test] fn d2103_20_encoder_never_serializes_free_error_strings(){
        let mut c=pending();c.latch_io(&io::Error::new(io::ErrorKind::PermissionDenied,"NEVER_EXPORT_PRIVATE_DETAIL"));
        let b=c.snapshot().encode().unwrap();assert!(!b.windows(27).any(|v|v==b"NEVER_EXPORT_PRIVATE_DETAIL"));assert_eq!(b.len(),256);
    }
    #[test] fn d2103_21_cached_header_cannot_claim_unrecorded_completion(){
        let b=failed().encode().unwrap();let mut x=b;x[12]=Stage::ParentPins as u8;assert!(Snapshot::decode(&x).is_err());
        let mut x=b;x[11]=Stage::ParentPins as u8;x[20]=Stage::ParentPins as u8;assert!(Snapshot::decode(&x).is_err());
        let mut c=pending();c.completed(Stage::BootstrapReader);let mut x=c.snapshot().encode().unwrap();x[14]|=4;assert!(Snapshot::decode(&x).is_err());
    }
    #[test] fn exit_diagnostic_v2_roundtrips_each_actual_predicate_and_freezes_without_queries(){
        for code in 1..=17 {
            let predicate=ParentExitPredicate::try_from(code).unwrap();
            let mut c=Capture::new(Scope::Parent);c.enter(Stage::ObserveRetainedChild);
            c.latch_io(&ParentExitFailure::error(predicate));let s=c.snapshot();
            assert_eq!(s.exit_predicate,Some(predicate));assert_eq!(s.first.unwrap().raw,None);
            let encoded=s.encode().unwrap();assert_eq!(encoded[8],2);assert_eq!(encoded[36],code);
            assert_eq!(Snapshot::decode(&encoded).unwrap(),s);
            c.latch_io(&ParentExitFailure::error(ParentExitPredicate::ProcessSignal));
            c.completed(Stage::ObserveRetainedChild);assert_eq!(c.snapshot(),s);
        }
    }
    #[test] fn exit_diagnostic_keeps_v1_readable_and_rejects_unobserved_or_wrong_scope_predicates(){
        let original=failed();let mut legacy=original.encode().unwrap();legacy[8]=1;
        assert_eq!(Snapshot::decode(&legacy).unwrap(),original);
        for version in [1,2] {let mut malformed=legacy;malformed[8]=version;malformed[36]=255;assert!(Snapshot::decode(&malformed).is_err());}
        let mut c=Capture::new(Scope::Parent);c.enter(Stage::ValidateChildExit);
        c.latch_io(&ParentExitFailure::error(ParentExitPredicate::JobStillActive));
        let encoded=c.snapshot().encode().unwrap();let mut v1=encoded;v1[8]=1;assert!(Snapshot::decode(&v1).is_err());
        let mut fabricated=c.snapshot();fabricated.status=Status::Recording;fabricated.first=None;assert!(fabricated.encode().is_err());
        let mut wrong=Capture::new(Scope::Import);wrong.enter(Stage::ImportRead);
        wrong.latch_io(&ParentExitFailure::error(ParentExitPredicate::JobStillActive));assert!(wrong.snapshot().encode().is_err());
        let mut opaque=pending();opaque.latch_io(&io::Error::new(io::ErrorKind::InvalidData,"private free text"));
        assert_eq!(opaque.snapshot().exit_predicate,None);
    }
    #[test] fn d2103_22_retained_history_cannot_invent_overlapping_operations(){
        let mut c=pending();c.native(Api::CreateFile,false,Some(2));let b=c.snapshot().encode().unwrap();
        let mut x=b;x[76]=1;x[78]=0;x[79]=0;put32(&mut x[84..88],0);assert!(Snapshot::decode(&x).is_err());
    }

}
