//! Host-issued nomination names and bounded directory calls, never grants.
//! Unknown outcomes and invalid correlation do not authorize automatic replay.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{message::{Builder, ReaderOptions}, serialize, traits::{HasStructSize, IntoInternalStructReader}};
use sha2::{Digest, Sha256};
use std::fmt;
#[allow(unsafe_op_in_unsafe_fn)]
mod fs_directory_request_capnp {
    include!(concat!(env!("OUT_DIR"), "/fs_directory_request_capnp.rs"));
}
use fs_directory_request_capnp as wire;
pub use morrow_fs_directory_v1::FsDirectoryPage;
pub mod state;
pub mod transport;
pub mod ffi;
#[cfg(test)]
mod tests;
pub use state::{ClientState, Phase, Snapshot};
pub const FEATURE: &str = "fs-directory-request-v1";
pub const PROFILE: &str = FEATURE;
pub const IMPORT_MODULE: &str = "morrow_fs_directory_v1";
pub const IMPORT_NAME: &str = "call";
pub const VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 512;
pub const MAX_RESPONSE_BYTES: usize = 65536;
pub const MAX_PAGE_BYTES: usize = morrow_fs_directory_v1::MAX_ENVELOPE_BYTES;
pub const MAX_REQUEST: usize = MAX_REQUEST_BYTES;
pub const MAX_RESPONSE: usize = MAX_RESPONSE_BYTES;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/fs_directory_request.capnp");
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
pub const fn schema_digest() -> [u8; 32] { SCHEMA_DIGEST }
pub const fn page_schema_digest() -> [u8; 32] { morrow_fs_directory_v1::SCHEMA_DIGEST }
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Invalid=1, Contract=2, Limit=3, Buffer=4, Correlation=5, State=6, Budget=7, Unsupported=8, OutcomeUnknown=9 }
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f,"directory request {self:?}") } }
impl std::error::Error for Error {}
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind { Open=0, Next=1, Finish=2, Cancel=3 }
impl TryFrom<u16> for ActionKind {
    type Error=Error;
    fn try_from(v:u16)->Result<Self> { match v { 0=>Ok(Self::Open),1=>Ok(Self::Next),2=>Ok(Self::Finish),3=>Ok(Self::Cancel),_=>Err(Error::Invalid) } }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Next { selection_epoch:[u8;32], page_sequence:u64, after_entry_id:Option<[u8;32]> },
    Finish { selection_epoch:[u8;32] },
    Cancel { selection_epoch:[u8;32] },
}
impl Action {
    pub const fn kind(self)->ActionKind { match self {Self::Open=>ActionKind::Open,Self::Next{..}=>ActionKind::Next,Self::Finish{..}=>ActionKind::Finish,Self::Cancel{..}=>ActionKind::Cancel} }
    pub fn validate(self)->Result<()> {
        match self {
            Self::Open=>Ok(()),
            Self::Next{selection_epoch,page_sequence,after_entry_id}=>{
                if !nonzero(&selection_epoch) || page_sequence==0 || (page_sequence==1&&after_entry_id.is_some()) || after_entry_id.is_some_and(|id|!nonzero(&id)) {Err(Error::Invalid)}else{Ok(())}
            },
            Self::Finish{selection_epoch}|Self::Cancel{selection_epoch}=>if nonzero(&selection_epoch){Ok(())}else{Err(Error::Invalid)},
        }
    }
    fn epoch(self)->Option<[u8;32]> {match self{Self::Open=>None,Self::Next{selection_epoch,..}|Self::Finish{selection_epoch}|Self::Cancel{selection_epoch}=>Some(selection_epoch)}}
}
impl fmt::Debug for Action {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {
        let mut d=f.debug_struct("Action");d.field("kind",&self.kind());
        if let Self::Next{page_sequence,after_entry_id,..}=self {d.field("page_sequence",page_sequence).field("has_after_entry_id",&after_entry_id.is_some());}
        d.finish_non_exhaustive()
    }
}
pub(crate) fn nonzero(v:&[u8;32])->bool { v.iter().any(|b|*b!=0) }
fn id(v:&[u8])->Result<[u8;32]> { let v:[u8;32]=v.try_into().map_err(|_|Error::Invalid)?;if nonzero(&v){Ok(v)}else{Err(Error::Invalid)} }
fn invalid<T>(_:T)->Error { Error::Invalid }
pub(crate) fn page_error(error:morrow_fs_directory_v1::Error)->Error {match error {morrow_fs_directory_v1::Error::Limit=>Error::Limit,morrow_fs_directory_v1::Error::Contract=>Error::Contract,morrow_fs_directory_v1::Error::Budget=>Error::Budget,_=>Error::Invalid}}
fn read(bytes:&[u8],max:usize,words:usize)->Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.is_empty()||bytes.len()>max {return Err(Error::Limit);}
    let mut remaining=bytes;
    let message=serialize::read_message(&mut remaining,ReaderOptions{traversal_limit_in_words:Some(words),nesting_limit:8}).map_err(invalid)?;
    if !remaining.is_empty(){return Err(Error::Invalid);}
    Ok(message)
}
fn finish(builder:&Builder<capnp::message::HeapAllocator>,max:usize)->Result<Vec<u8>> {
    let bytes=serialize::write_message_to_words(builder);if bytes.len()>max{Err(Error::Limit)}else{Ok(bytes)}
}
fn exact_layout<'a>(root:impl IntoInternalStructReader<'a>,data:u16,pointers:u16)->Result<()> {
    let reader=root.into_internal_struct_reader();
    if reader.get_data_section_size()!=u32::from(data)*64 || reader.get_pointer_section_size()!=pointers {return Err(Error::Contract);}
    Ok(())
}
#[derive(Clone, PartialEq, Eq)]
pub struct Request { bytes:Vec<u8>, nomination_ref:[u8;32], nonce:[u8;32], action:Action, digest:[u8;32] }
impl fmt::Debug for Request {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("Request").field("action",&self.action).field("wire_bytes",&self.bytes.len()).finish_non_exhaustive()}}
impl Request {
    pub fn new(nomination_ref:[u8;32],nonce:[u8;32],action:Action)->Result<Self> {
        if !nonzero(&nomination_ref)||!nonzero(&nonce){return Err(Error::Invalid);}action.validate()?;
        let mut message=Builder::new_default();
        {let mut root=message.init_root::<wire::request::Builder<'_>>();root.set_version(VERSION);root.set_action(action.kind() as u16);root.set_reserved(0);root.set_schema_sha256(&SCHEMA_DIGEST);root.set_nomination_ref(&nomination_ref);root.set_nonce(&nonce);
        if let Some(epoch)=action.epoch(){root.set_selection_epoch(&epoch);}
        if let Action::Next{page_sequence,after_entry_id,..}=action{root.set_page_sequence(page_sequence);if let Some(id)=after_entry_id{root.set_after_entry_id(&id);}}}
        let bytes=finish(&message,MAX_REQUEST_BYTES)?;let digest=Sha256::digest(&bytes).into();Ok(Self{bytes,nomination_ref,nonce,action,digest})
    }
    pub fn decode(bytes:&[u8])->Result<Self> {
        let message=read(bytes,MAX_REQUEST_BYTES,128)?;let root=message.get_root::<wire::request::Reader<'_>>().map_err(invalid)?;
        let size=<wire::request::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root,size.data,size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count!=0{return Err(Error::Invalid);}
        if root.get_version()!=VERSION||root.get_schema_sha256().map_err(invalid)?!=SCHEMA_DIGEST{return Err(Error::Contract);}
        if root.get_reserved()!=0{return Err(Error::Invalid);}
        let kind=ActionKind::try_from(root.get_action())?;
        let nomination_ref=id(root.get_nomination_ref().map_err(invalid)?)?;let nonce=id(root.get_nonce().map_err(invalid)?)?;
        let epoch=root.get_selection_epoch().map_err(invalid)?;let sequence=root.get_page_sequence();let after=root.get_after_entry_id().map_err(invalid)?;
        let action=match kind {
            ActionKind::Open=>{if !epoch.is_empty()||sequence!=0||!after.is_empty(){return Err(Error::Invalid);}Action::Open},
            ActionKind::Next=>Action::Next{selection_epoch:id(epoch)?,page_sequence:sequence,after_entry_id:if after.is_empty(){None}else{Some(id(after)?)}},
            ActionKind::Finish|ActionKind::Cancel=>{if sequence!=0||!after.is_empty(){return Err(Error::Invalid);}let selection_epoch=id(epoch)?;if kind==ActionKind::Finish{Action::Finish{selection_epoch}}else{Action::Cancel{selection_epoch}}},
        };action.validate()?;let digest=Sha256::digest(bytes).into();Ok(Self{bytes:bytes.to_vec(),nomination_ref,nonce,action,digest})
    }
    pub fn encode(&self)->Result<Vec<u8>> {Ok(self.bytes.clone())}
    pub fn wire(&self)->&[u8] {&self.bytes}
    pub const fn nomination_ref(&self)->[u8;32] {self.nomination_ref}
    pub const fn nonce(&self)->[u8;32] {self.nonce}
    pub const fn action(&self)->Action {self.action}
    pub const fn digest(&self)->[u8;32] {self.digest}
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Opened { pub selection_epoch:[u8;32], pub page_sequence:u64, pub after_entry_id:Option<[u8;32]>, pub entries:u32, pub metadata_bytes:u64 }
impl Opened {
    pub fn validate(self)->Result<()> {if !nonzero(&self.selection_epoch)||self.page_sequence!=1||self.after_entry_id.is_some()||self.entries>1024||self.metadata_bytes>1024*1024{Err(Error::Invalid)}else{Ok(())}}
}
impl fmt::Debug for Opened {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("Opened").field("page_sequence",&self.page_sequence).field("entries",&self.entries).field("metadata_bytes",&self.metadata_bytes).finish_non_exhaustive()}}
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Denied=1,Closed=2,Busy=3,Invalid=4,Limit=5,Unsupported=6,OutcomeUnknown=7 }
impl TryFrom<u16> for Status {type Error=Error;fn try_from(v:u16)->Result<Self>{match v{1=>Ok(Self::Denied),2=>Ok(Self::Closed),3=>Ok(Self::Busy),4=>Ok(Self::Invalid),5=>Ok(Self::Limit),6=>Ok(Self::Unsupported),7=>Ok(Self::OutcomeUnknown),_=>Err(Error::Invalid)}}}
#[derive(Clone, PartialEq, Eq)]
pub enum Reply { Opened(Opened),Page(FsDirectoryPage),Finished,Cancelled,Error(Status) }
impl fmt::Debug for Reply {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self {Self::Opened(v)=>v.fmt(f),Self::Page(p)=>f.debug_struct("PageReply").field("page_sequence",&p.page_sequence).field("entries",&p.entries.len()).field("terminal",&p.terminal).finish_non_exhaustive(),Self::Finished=>f.write_str("Finished"),Self::Cancelled=>f.write_str("Cancelled"),Self::Error(s)=>s.fmt(f)}}}
#[derive(Clone, PartialEq, Eq)]
pub struct Response { bytes:Vec<u8>, reply:Reply, page_wire:Option<Vec<u8>>, request:Request }
impl fmt::Debug for Response {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.debug_struct("Response").field("reply",&self.reply).field("wire_bytes",&self.bytes.len()).finish_non_exhaustive()}}
impl Response {
    pub fn new(request:&Request,reply:Reply)->Result<Self> {
        let mut message=Builder::new_default();let mut page_wire=None;
        {let mut root=message.init_root::<wire::response::Builder<'_>>();root.set_version(VERSION);root.set_action(request.action.kind() as u16);root.set_page_version(morrow_fs_directory_v1::VERSION);root.set_schema_sha256(&SCHEMA_DIGEST);root.set_page_schema_sha256(&page_schema_digest());root.set_nomination_ref(&request.nomination_ref);root.set_nonce(&request.nonce);root.set_request_digest(&request.digest);root.set_reserved(0);
        match &reply {
            Reply::Opened(opened)=>{if request.action!=Action::Open{return Err(Error::Correlation);}opened.validate()?;root.set_selection_epoch(&opened.selection_epoch);root.set_page_sequence(1);root.set_entries(opened.entries);root.set_metadata_bytes(opened.metadata_bytes);},
            Reply::Page(page)=>{let Action::Next{selection_epoch,page_sequence,..}=request.action else{return Err(Error::Correlation);};if page.selection_epoch!=selection_epoch||page.page_sequence!=page_sequence{return Err(Error::Correlation);}page.validate().map_err(page_error)?;let bytes=page.encode().map_err(page_error)?;if bytes.len()>MAX_PAGE_BYTES{return Err(Error::Limit);}root.set_selection_epoch(&selection_epoch);root.set_page_sequence(page_sequence);root.set_page(&bytes);page_wire=Some(bytes);},
            Reply::Finished=>{let Action::Finish{selection_epoch}=request.action else{return Err(Error::Correlation);};root.set_selection_epoch(&selection_epoch);},
            Reply::Cancelled=>{let Action::Cancel{selection_epoch}=request.action else{return Err(Error::Correlation);};root.set_selection_epoch(&selection_epoch);},
            Reply::Error(status)=>root.set_status(*status as u16),
        }}
        let bytes=finish(&message,MAX_RESPONSE_BYTES)?;Ok(Self{bytes,reply,page_wire,request:request.clone()})
    }
    pub fn decode_for(bytes:&[u8],request:&Request)->Result<Self> {
        let message=read(bytes,MAX_RESPONSE_BYTES,16384)?;let root=message.get_root::<wire::response::Reader<'_>>().map_err(invalid)?;
        let size=<wire::response::Builder<'_> as HasStructSize>::STRUCT_SIZE;exact_layout(root,size.data,size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count!=0{return Err(Error::Invalid);}
        if root.get_version()!=VERSION||root.get_schema_sha256().map_err(invalid)?!=SCHEMA_DIGEST||root.get_page_version()!=morrow_fs_directory_v1::VERSION||root.get_page_schema_sha256().map_err(invalid)?!=page_schema_digest(){return Err(Error::Contract);}
        if root.get_reserved()!=0{return Err(Error::Invalid);}
        if root.get_action()!=request.action.kind() as u16||root.get_nomination_ref().map_err(invalid)?!=request.nomination_ref||root.get_nonce().map_err(invalid)?!=request.nonce||root.get_request_digest().map_err(invalid)?!=request.digest{return Err(Error::Correlation);}
        let epoch=root.get_selection_epoch().map_err(invalid)?;let sequence=root.get_page_sequence();let entries=root.get_entries();let metadata_bytes=root.get_metadata_bytes();let page=root.get_page().map_err(invalid)?;
        let reply=if root.get_status()!=0 {
            if !epoch.is_empty()||sequence!=0||entries!=0||metadata_bytes!=0||!page.is_empty(){return Err(Error::Invalid);}Reply::Error(Status::try_from(root.get_status())?)
        } else { match request.action {
            Action::Open=>{if !page.is_empty(){return Err(Error::Invalid);}let opened=Opened{selection_epoch:id(epoch)?,page_sequence:sequence,after_entry_id:None,entries,metadata_bytes};opened.validate()?;Reply::Opened(opened)},
            Action::Next{selection_epoch,page_sequence,..}=>{if epoch!=selection_epoch||sequence!=page_sequence||entries!=0||metadata_bytes!=0||page.is_empty()||page.len()>MAX_PAGE_BYTES{return Err(Error::Invalid);}let page=FsDirectoryPage::decode(page).map_err(page_error)?;if page.selection_epoch!=selection_epoch||page.page_sequence!=page_sequence{return Err(Error::Correlation);}Reply::Page(page)},
            Action::Finish{selection_epoch}|Action::Cancel{selection_epoch}=>{if epoch!=selection_epoch||sequence!=0||entries!=0||metadata_bytes!=0||!page.is_empty(){return Err(Error::Invalid);}if matches!(request.action,Action::Finish{..}){Reply::Finished}else{Reply::Cancelled}},
        }};
        let page_wire=matches!(reply,Reply::Page(_)).then(||page.to_vec());
        Ok(Self{bytes:bytes.to_vec(),reply,page_wire,request:request.clone()})
    }
    pub fn encode(&self)->Result<Vec<u8>> {Ok(self.bytes.clone())}
    pub fn wire(&self)->&[u8] {&self.bytes}
    pub fn reply(&self)->&Reply {&self.reply}
    /// Borrowed original embedded page bytes; valid until this Response drops.
    pub fn page_wire(&self)->Option<&[u8]> {self.page_wire.as_deref()}
    pub fn request(&self)->&Request {&self.request}
}
