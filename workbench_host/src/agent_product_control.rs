//! Bounded private direct-UI profile, independent of the frozen host/catalog/R2 schemas.
//! Correlation slots are observations, never authority. No task preparation or Start.
use crate::agent_product_control_capnp as wire;
use capnp::{message::{Builder, ReaderOptions}, serialize};
use morrow_agent_session_exec_v1_r2::{self as r2, Action, Event, Outcome};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"MROWPS30";
pub const VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 128 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 384 * 1024;
pub const MAX_IDS: usize = 64;
pub fn digest() -> [u8; 32] {
    Sha256::digest(include_bytes!("../schemas/agent_product_control.capnp")).into()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Invalid, Contract, Limit }
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "private session {self:?}") }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn parsed<T>(v: capnp::Result<T>) -> Result<T> { v.map_err(|_| Error::Contract) }
fn text(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let v = parsed(v)?.to_str().map_err(|_| Error::Invalid)?;
    if v.len() > 256 { return Err(Error::Limit); }
    Ok(v.to_owned())
}
fn fixed(v: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    parsed(v)?.try_into().map_err(|_| Error::Invalid)
}
fn frame(b: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let mut out = MAGIC.to_vec();
    out.extend(serialize::write_message_to_words(b));
    if out.len() > MAX_RESPONSE_BYTES { return Err(Error::Limit); }
    Ok(out)
}
fn reader(input: &[u8]) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if input.len() > MAX_RESPONSE_BYTES { return Err(Error::Limit); }
    if input.len() < 16 || !input.starts_with(MAGIC) { return Err(Error::Contract); }
    let mut rest = &input[8..];
    let r = parsed(serialize::read_message_from_flat_slice(&mut rest, ReaderOptions {
        traversal_limit_in_words: Some(MAX_RESPONSE_BYTES / 8 * 4), nesting_limit: 12,
    }))?;
    if !rest.is_empty() { return Err(Error::Contract); }
    Ok(r)
}

#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    pub id: [u8; 32], pub task: [u8; 32], pub attachment: u64,
    pub operation: wire::Operation, pub submission: u64, pub writer_slot: u64,
    pub generation: u64, pub action: wire::Action, pub session_id: String,
    pub parent: String, pub parent_tail: u64, pub after: u64, pub limit: u32,
    pub epoch: u64, pub expected_tail: u64, pub events: Vec<Event>,
}
impl Request {
    pub fn empty(id: [u8;32], task: [u8;32], attachment: u64, operation: wire::Operation) -> Self {
        Self { id, task, attachment, operation, submission: 0, writer_slot: 0,
            generation: 0, action: wire::Action::Unsupported, session_id: String::new(),
            parent: String::new(), parent_tail: 0, after: 0, limit: 0, epoch: 0,
            expected_tail: 0, events: Vec::new() }
    }
    pub fn canonical_request(&self) -> Result<r2::Request> {
        let action = match self.action {
            wire::Action::List if self.writer_slot == 0 => Action::List,
            wire::Action::Create if self.writer_slot == 0 => Action::Create {
                session_id: self.session_id.clone(), parent: (!self.parent.is_empty()).then(||self.parent.clone()),
                parent_tail: self.parent_tail,
            },
            wire::Action::Snapshot => Action::Snapshot { session_id: self.session_id.clone(), after: self.after, limit: self.limit },
            wire::Action::OpenWriter if self.writer_slot != 0 => Action::OpenWriter { session_id: self.session_id.clone(), expected_epoch: self.epoch },
            wire::Action::Append if self.writer_slot != 0 => Action::Append { session_id: self.session_id.clone(), epoch: self.epoch,
                expected_tail: self.expected_tail, events: self.events.clone() },
            _ => return Err(Error::Invalid),
        };
        // Original R2 validates SID, parent, CAS, event IDs, bodies and event count.
        let id: String = self.id.iter().map(|b| format!("{b:02x}")).collect();
        r2::Request::new_for_generation(id, self.generation, action).map_err(|_| Error::Invalid)
    }
    pub fn validate(&self) -> Result<()> {
        if self.id == [0;32] || self.task == [0;32] { return Err(Error::Invalid); }
        if self.session_id.len() > 256 || self.parent.len() > 256 || self.events.len() > r2::MAX_EVENTS {
            return Err(Error::Limit);
        }
        for e in &self.events { e.validate().map_err(|_| Error::Invalid)?; }
        use wire::Operation::*;
        if self.operation == Unsupported || (self.operation == Attach) != (self.attachment == 0) {
            return Err(Error::Invalid);
        }
        if (self.operation == Read) != (self.submission != 0) { return Err(Error::Invalid); }
        // Irrelevant fields must be zero/empty, not hidden alternate commands.
        let mut canonical = Self::empty(self.id, self.task, self.attachment, self.operation);
        canonical.submission = self.submission;
        match self.operation {
            Submit => {
                canonical.generation = self.generation; canonical.action = self.action;
                canonical.writer_slot = self.writer_slot;
                match self.action {
                    wire::Action::List => {},
                    wire::Action::Create => { canonical.session_id = self.session_id.clone(); canonical.parent = self.parent.clone(); canonical.parent_tail = self.parent_tail; },
                    wire::Action::Snapshot => { canonical.session_id=self.session_id.clone(); canonical.after=self.after; canonical.limit=self.limit; },
                    wire::Action::OpenWriter => { canonical.session_id=self.session_id.clone(); canonical.epoch=self.epoch; },
                    wire::Action::Append => { canonical.session_id=self.session_id.clone(); canonical.epoch=self.epoch; canonical.expected_tail=self.expected_tail; canonical.events=self.events.clone(); },
                    _ => return Err(Error::Invalid),
                }
                self.canonical_request()?;
            },
            AdmitWriter => {
                canonical.session_id=self.session_id.clone();
                // Reuse original identity validation without inventing an R2 admission frame.
                Action::Snapshot {session_id:self.session_id.clone(),after:0,limit:1}.validate().map_err(|_|Error::Invalid)?;
            },
            CloseWriter => { if self.writer_slot==0 {return Err(Error::Invalid)} canonical.writer_slot=self.writer_slot; },
            _ => {},
        }
        if &canonical != self { return Err(Error::Invalid); }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut b=Builder::new(capnp::message::HeapAllocator::new()
            .first_segment_words((MAX_REQUEST_BYTES / 8) as u32));
        {
            let mut w=b.init_root::<wire::request::Builder>();
            w.set_version(VERSION);w.set_digest(&digest());w.set_id(&self.id);w.set_task(&self.task);
            w.set_attachment(self.attachment);w.set_operation(self.operation);w.set_submission(self.submission);
            w.set_writer_slot(self.writer_slot);w.set_generation(self.generation);w.set_action(self.action);
            w.set_session_id(self.session_id.as_str());w.set_parent(self.parent.as_str());w.set_parent_tail(self.parent_tail);
            w.set_after(self.after);w.set_limit(self.limit);w.set_epoch(self.epoch);w.set_expected_tail(self.expected_tail);
            let mut list=w.init_events(self.events.len() as u32);
            for (i,e) in self.events.iter().enumerate() {let mut x=list.reborrow().get(i as u32);x.set_event_id(e.event_id.as_str());x.set_body(&e.body);}
        }
        let bytes=frame(&b)?;
        if bytes.len()>MAX_REQUEST_BYTES {return Err(Error::Limit)}
        Ok(bytes)
    }
    pub fn decode(input:&[u8])->Result<Self> {
        if input.len()>MAX_REQUEST_BYTES {return Err(Error::Limit)}
        let b=reader(input)?;let w=parsed(b.get_root::<wire::request::Reader>())?;
        if w.get_version()!=VERSION || parsed(w.get_digest())?!=digest() {return Err(Error::Contract)}
        let list=parsed(w.get_events())?;if list.len() as usize>r2::MAX_EVENTS {return Err(Error::Limit)}
        let mut events=Vec::new();
        for e in list.iter() {let body=parsed(e.get_body())?;if body.len()>r2::MAX_BODY_BYTES {return Err(Error::Limit)}
            events.push(Event{event_id:text(e.get_event_id())?,body:body.to_vec()});}
        let out=Self {id:fixed(w.get_id())?,task:fixed(w.get_task())?,attachment:w.get_attachment(),
            operation:w.get_operation().map_err(|_|Error::Contract)?,submission:w.get_submission(),writer_slot:w.get_writer_slot(),
            generation:w.get_generation(),action:w.get_action().map_err(|_|Error::Contract)?,session_id:text(w.get_session_id())?,
            parent:text(w.get_parent())?,parent_tail:w.get_parent_tail(),after:w.get_after(),limit:w.get_limit(),epoch:w.get_epoch(),
            expected_tail:w.get_expected_tail(),events};
        if out.encode()?.as_slice()!=input {return Err(Error::Contract)}
        Ok(out)
    }
}

pub struct Reply {
    pub status: wire::Status, pub attachment:u64, pub submission:u64, pub writer_slot:u64,
    pub generation:u64, pub lease_live:bool, pub delivery_unknown:bool,
    pub canonical_reply:Vec<u8>, pub canonical_request_sha256:[u8;32], pub outcome:Option<Outcome>,
}
impl Reply {
    pub fn new(status:wire::Status)->Self { Self {status,attachment:0,submission:0,writer_slot:0,generation:0,
        lease_live:false,delivery_unknown:false,canonical_reply:Vec::new(),canonical_request_sha256:[0;32],outcome:None} }
    pub fn encode_for(&self,req:&Request,input:&[u8])->Result<Vec<u8>> {
        if self.canonical_reply.len()>r2::MAX_FRAME_BYTES {return Err(Error::Limit)}
        match &self.outcome {
            Some(Outcome::Session(v))=>v.validate().map_err(|_|Error::Contract)?,
            Some(Outcome::Sessions(v))=>{
                if v.len()>r2::MAX_SESSIONS {return Err(Error::Limit)}
                for info in v {info.validate().map_err(|_|Error::Contract)?;}
            },
            Some(Outcome::Snapshot(v))=>v.validate().map_err(|_|Error::Contract)?,
            Some(Outcome::Tool(_)|Outcome::Claimed{..})=>return Err(Error::Contract),
            _=>{},
        }
        let mut b=Builder::new_default();
        {
            let mut w=b.init_root::<wire::reply::Builder>();
            w.set_version(VERSION);w.set_digest(&digest());w.set_id(&req.id);w.set_task(&req.task);
            w.set_operation(req.operation);w.set_request_sha256(&Sha256::digest(input));w.set_attachment(self.attachment);
            w.set_status(self.status);w.set_submission(self.submission);w.set_writer_slot(self.writer_slot);
            w.set_generation(self.generation);w.set_lease_live(self.lease_live);w.set_delivery_unknown(self.delivery_unknown);
            w.set_canonical_reply(&self.canonical_reply);w.set_canonical_request_sha256(&self.canonical_request_sha256);
            match &self.outcome {
                None=>{},
                Some(Outcome::Rejected(e))=>{w.set_result_kind(wire::ResultKind::Rejected);w.set_rejection_code(error_code(*e));},
                Some(Outcome::Session(info))=>{w.set_result_kind(wire::ResultKind::Session);set_info(w.reborrow().init_sessions(1).get(0),info);},
                Some(Outcome::Sessions(infos))=>{w.set_result_kind(wire::ResultKind::Sessions);let mut l=w.reborrow().init_sessions(infos.len() as u32);
                    for(i,info)in infos.iter().enumerate(){set_info(l.reborrow().get(i as u32),info);}},
                Some(Outcome::Snapshot(s))=>{w.set_result_kind(wire::ResultKind::Snapshot);set_info(w.reborrow().init_sessions(1).get(0),&s.info);
                    w.set_gap(s.gap);w.set_checkpoint(&s.checkpoint);let mut l=w.reborrow().init_events(s.events.len() as u32);
                    for(i,e)in s.events.iter().enumerate(){let mut x=l.reborrow().get(i as u32);x.set_sequence(e.sequence);x.set_event_id(e.event.event_id.as_str());x.set_body(&e.event.body);}},
                // This private profile never exposes Tool/Claimed or claim secrets.
                _=>return Err(Error::Contract),
            }
        }
        frame(&b)
    }
}
fn set_info(mut w:wire::session_info::Builder<'_>,v:&r2::SessionInfo){
    w.set_session_id(v.session_id.as_str());w.set_parent(v.parent.as_deref().unwrap_or(""));w.set_parent_tail(v.parent_tail);
    w.set_epoch(v.epoch);w.set_tail(v.tail);w.set_floor(v.floor);w.set_archived(v.archived);w.set_revision(v.revision);
    w.set_checkpoint_tail(v.checkpoint_tail);w.set_checkpoint_sha256(&v.checkpoint_sha256);w.set_checkpoint_sealed(v.checkpoint_sealed);
    w.set_parent_checkpoint_sha256(v.parent_checkpoint_sha256.as_ref().map(|x|x.as_slice()).unwrap_or(&[]));
}
fn error_code(e:r2::Error)->u16 { use r2::Error::*;match e {Invalid=>1,Contract=>2,Limit=>3,Correlation=>4,Denied=>5,Conflict=>6,NotFound=>7,CommitUnknown=>8,Storage=>9} }

impl crate::Workbench {
    /// Trusted outer pipe reports missing delivery; never an acknowledgment or
    /// recovery. Non-Windows has no retained native controller to mark.
    pub fn mark_product_session_delivery_unknown(&mut self) {
        #[cfg(windows)] self.product_session_wire.seal();
    }
}
/// Used only for this private profile by the native CLI. The old input cap and
/// other profiles remain unchanged. Any gate/write/flush failure seals the
/// retained original controller and preserves the original error.
#[cfg(not(target_arch="wasm32"))]
pub fn deliver_native_reply(output:&mut impl std::io::Write,response:&[u8],gate:&crate::product_gate::ProductGate,mut lost:impl FnMut())
    ->crate::Result<()> {
    let result=(|| ->crate::Result<()> {
        if response.len()>MAX_RESPONSE_BYTES {return Err(Error::Limit.into())}
        gate.check()?;
        output.write_all(&(response.len() as u32).to_le_bytes())?;
        output.write_all(response)?;output.flush()?;Ok(())
    })();
    if result.is_err(){lost();}result
}

#[cfg(windows)]
#[path="agent_product_control_windows.rs"]
mod native;
#[cfg(windows)] pub(crate) use native::NativeSlot;
#[cfg(windows)] pub use native::respond;
#[cfg(all(test,windows))] pub(crate) use native::testing;
#[cfg(all(not(windows),not(target_arch="wasm32")))]
pub fn respond(_host:&mut crate::Workbench,input:&[u8])->crate::Result<Vec<u8>> {
    let req=Request::decode(input)?;Ok(Reply::new(wire::Status::Unsupported).encode_for(&req,input)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attach()->Request {Request::empty([1;32],[2;32],0,wire::Operation::Attach)}
    #[test] fn canonical_attach_and_trailing_bytes() {let q=attach();let mut b=q.encode().unwrap();assert!(Request::decode(&b).unwrap()==q);b.push(0);assert!(Request::decode(&b).is_err());}
    #[test] fn default_fields_never_authorize() {let mut b=Builder::new_default();b.init_root::<wire::request::Builder>();assert!(Request::decode(&frame(&b).unwrap()).is_err());}
    #[test] fn stale_read_requires_attachment_and_exact_token_shape() {let mut q=Request::empty([1;32],[2;32],3,wire::Operation::Read);assert!(q.encode().is_err());q.submission=9;assert!(q.encode().is_ok());q.attachment=0;assert!(q.encode().is_err());}
    #[test] fn typed_builder_reuses_r2_and_rejects_hidden_fields() {let mut q=Request::empty([1;32],[2;32],3,wire::Operation::Submit);q.generation=4;q.action=wire::Action::Create;q.session_id="s".into();let original=q.canonical_request().unwrap();assert!(r2::Request::decode(original.raw()).is_ok());q.expected_tail=7;assert!(q.encode().is_err());}
    #[test] fn writer_actions_need_slot_and_bounded_events() {let mut q=Request::empty([1;32],[2;32],3,wire::Operation::Submit);q.generation=4;q.action=wire::Action::Append;q.session_id="s".into();q.epoch=1;q.events=vec![Event{event_id:"e".into(),body:vec![0]}];assert!(q.encode().is_err());q.writer_slot=8;assert!(q.encode().is_ok());q.events[0].body=vec![0;r2::MAX_BODY_BYTES+1];assert!(q.encode().is_err());}
    #[test] fn reply_binds_request_bytes_and_never_defaults_to_grant() {let q=attach();let input=q.encode().unwrap();let output=Reply::new(wire::Status::Unavailable).encode_for(&q,&input).unwrap();let b=reader(&output).unwrap();let w=b.get_root::<wire::reply::Reader>().unwrap();assert_eq!(w.get_status().unwrap(),wire::Status::Unavailable);assert_eq!(w.get_request_sha256().unwrap(),Sha256::digest(&input).as_slice());assert!(!w.get_lease_live());assert_eq!(w.get_submission(),0);}
    #[cfg(not(target_arch="wasm32"))]
    #[test] fn output_header_body_and_flush_errors_report_lost_and_preserve_error() {
        struct Broken {fail_at:u8,calls:u8}
        impl std::io::Write for Broken {
            fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.calls+=1;if self.calls==self.fail_at {Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe,"fixed write failure"))}else{Ok(b.len())}}
            fn flush(&mut self)->std::io::Result<()>{if self.fail_at==3 {Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe,"fixed flush failure"))}else{Ok(())}}
        }
        for fail_at in 1..=3 {let mut lost=0;let e=deliver_native_reply(&mut Broken{fail_at,calls:0},b"reply",&crate::product_gate::ProductGate::default(),||lost+=1).unwrap_err();
            assert_eq!(lost,1);assert_eq!(e.downcast_ref::<std::io::Error>().unwrap().kind(),std::io::ErrorKind::BrokenPipe);}
    }
    #[cfg(not(target_arch="wasm32"))]
    #[test] fn final_output_gate_rejects_before_writing_and_success_is_exact_frame() {
        let gate=crate::product_gate::ProductGate::default();let mut output=Vec::new();let mut lost=0;
        deliver_native_reply(&mut output,b"reply",&gate,||lost+=1).unwrap();assert_eq!(output,b"\x05\0\0\0reply");assert_eq!(lost,0);
        gate.revoke();output.clear();assert!(deliver_native_reply(&mut output,b"reply",&gate,||lost+=1).is_err());assert!(output.is_empty());assert_eq!(lost,1);
    }
}
