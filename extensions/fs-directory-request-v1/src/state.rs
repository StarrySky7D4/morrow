//! Bounded local client state. No authority, host clock, native open or retry.
use crate::{Action,Error,Opened,Reply,Request,Response,Result,Status,nonzero};
use morrow_fs_directory_v1::{DirectoryState,StateLimits};
use std::collections::BTreeSet;
pub const MAX_CALLS:u64=1024;
pub const MAX_REQUEST_WIRE:u64=512*1024;
pub const MAX_RESPONSE_WIRE:u64=1024*1024;
#[repr(u32)]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Phase {Unopened=0,Ready=1,Pending=2,Terminal=3,Unknown=4}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Snapshot {pub phase:Phase,pub calls:u64,pub request_wire_bytes:u64,pub response_wire_bytes:u64,pub next_sequence:u64,pub resident_nonces:usize}
pub struct ClientState { nomination_ref:[u8;32],epoch:Option<[u8;32]>,after:Option<[u8;32]>,pending:Option<Request>,seen:BTreeSet<[u8;32]>,page_state:Option<DirectoryState>,snapshot:Snapshot }
impl std::fmt::Debug for ClientState {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.debug_struct("ClientState").field("snapshot",&self.snapshot).finish_non_exhaustive()}}
impl ClientState {
    pub fn new(nomination_ref:[u8;32])->Result<Self> {
        if !nonzero(&nomination_ref){return Err(Error::Invalid);}
        Ok(Self{nomination_ref,epoch:None,after:None,pending:None,seen:BTreeSet::new(),page_state:None,snapshot:Snapshot{phase:Phase::Unopened,calls:0,request_wire_bytes:0,response_wire_bytes:0,next_sequence:1,resident_nonces:0}})
    }
    pub fn snapshot(&self)->Snapshot {self.snapshot}
    pub const fn nomination_ref(&self)->[u8;32] {self.nomination_ref}
    fn close(&mut self,phase:Phase) {self.pending=None;self.seen.clear();self.snapshot.resident_nonces=0;self.snapshot.phase=phase;if let Some(s)=&mut self.page_state{s.release();}}
    pub fn begin(&mut self,nonce:[u8;32],action:Action)->Result<Request> {
        self.begin_request(Request::new(self.nomination_ref,nonce,action)?)
    }
    /// Preserve the exact host invocation Open bytes, including valid alternate encoding.
    pub fn begin_request(&mut self,request:Request)->Result<Request> {
        if self.snapshot.phase==Phase::Pending{return Err(Error::State);}
        if request.nomination_ref()!=self.nomination_ref{return Err(Error::Correlation);}
        let nonce=request.nonce();let action=request.action();
        let valid=match (self.snapshot.phase,action) {
            (Phase::Unopened,Action::Open)=>true,
            (Phase::Ready,Action::Next{selection_epoch,page_sequence,after_entry_id})=>Some(selection_epoch)==self.epoch&&page_sequence==self.snapshot.next_sequence&&after_entry_id==self.after,
            (Phase::Ready,Action::Finish{selection_epoch}|Action::Cancel{selection_epoch})=>Some(selection_epoch)==self.epoch,
            _=>false,
        };if !valid||self.seen.contains(&nonce){return Err(Error::State);}
        let calls=self.snapshot.calls.checked_add(1).ok_or(Error::Budget)?;
        let bytes=self.snapshot.request_wire_bytes.checked_add(request.wire().len() as u64).ok_or(Error::Budget)?;
        if calls>MAX_CALLS||bytes>MAX_REQUEST_WIRE{return Err(Error::Budget);}
        self.snapshot.calls=calls;self.snapshot.request_wire_bytes=bytes;self.seen.insert(nonce);self.snapshot.resident_nonces=self.seen.len();self.pending=Some(request.clone());self.snapshot.phase=Phase::Pending;Ok(request)
    }
    /// No second import occurs. Transport uncertainty closes the client forever.
    pub fn transport_unknown(&mut self)->Result<()> {if self.pending.is_none(){return Err(Error::State);}self.close(Phase::Unknown);Ok(())}
    pub fn accept(&mut self,wire:&[u8])->Result<Response> {
        let Some(request)=self.pending.as_ref() else{return Err(Error::State);};
        if wire.is_empty()||wire.len()>crate::MAX_RESPONSE_BYTES{self.close(Phase::Unknown);return Err(Error::Limit);}
        let Some(bytes)=self.snapshot.response_wire_bytes.checked_add(wire.len() as u64) else{self.close(Phase::Unknown);return Err(Error::Budget);};
        if bytes>MAX_RESPONSE_WIRE{self.close(Phase::Unknown);return Err(Error::Budget);}
        self.snapshot.response_wire_bytes=bytes;
        let response=match Response::decode_for(wire,request){Ok(v)=>v,Err(e)=>{self.close(Phase::Unknown);return Err(e);}};
        let result:Result<()>=(|| {match response.reply() {
            Reply::Opened(Opened{selection_epoch,..})=>{
                self.epoch=Some(*selection_epoch);self.page_state=Some(DirectoryState::new(*selection_epoch,StateLimits::default()).map_err(|_|Error::Invalid)?);self.snapshot.phase=Phase::Ready;Ok(())
            },
            Reply::Page(page)=>{
                let raw=response.page_wire().ok_or(Error::Invalid)?;
                let state=self.page_state.as_mut().ok_or(Error::State)?;
                state.admit(raw).map_err(crate::page_error)?;
                if page.terminal{self.close(Phase::Terminal);}else{
                    self.snapshot.next_sequence=page.page_sequence.checked_add(1).ok_or(Error::Limit)?;
                    self.after=page.entries.last().map(|e|e.entry_id);self.snapshot.phase=Phase::Ready;
                }Ok(())
            },
            Reply::Finished|Reply::Cancelled=>{self.close(Phase::Terminal);Ok(())},
            Reply::Error(status)=>{self.close(if *status==Status::OutcomeUnknown{Phase::Unknown}else{Phase::Terminal});Ok(())},
        }})();
        if let Err(e)=result{self.close(Phase::Unknown);return Err(e);}
        self.pending=None;Ok(response)
    }
    pub fn next_action(&self)->Result<Action> {if self.snapshot.phase!=Phase::Ready{return Err(Error::State);}Ok(Action::Next{selection_epoch:self.epoch.ok_or(Error::State)?,page_sequence:self.snapshot.next_sequence,after_entry_id:self.after})}
    pub fn finish_action(&self)->Result<Action> {if self.snapshot.phase!=Phase::Ready{return Err(Error::State);}Ok(Action::Finish{selection_epoch:self.epoch.ok_or(Error::State)?})}
    pub fn cancel_action(&self)->Result<Action> {if self.snapshot.phase!=Phase::Ready{return Err(Error::State);}Ok(Action::Cancel{selection_epoch:self.epoch.ok_or(Error::State)?})}
    pub fn call_once_with(&mut self,nonce:[u8;32],action:Action,call:impl FnOnce(&[u8],&mut[u8])->i32)->Result<Response> {
        self.call_request_once_with(Request::new(self.nomination_ref,nonce,action)?,call)
    }
    pub fn call_request_once_with(&mut self,request:Request,call:impl FnOnce(&[u8],&mut[u8])->i32)->Result<Response> {
        let request=self.begin_request(request)?;
        match crate::transport::exchange_once_with(&request,call) {
            Ok(wire)=>self.accept(&wire),
            Err(error)=>{self.close(if error==Error::Unsupported{Phase::Terminal}else{Phase::Unknown});Err(error)}
        }
    }
    pub fn call_once(&mut self,nonce:[u8;32],action:Action)->Result<Response> {
        self.call_request_once(Request::new(self.nomination_ref,nonce,action)?)
    }
    pub fn call_request_once(&mut self,request:Request)->Result<Response> {
        let request=self.begin_request(request)?;
        match crate::transport::exchange_once(&request) {
            Ok(wire)=>self.accept(&wire),
            Err(error)=>{self.close(if error==Error::Unsupported{Phase::Terminal}else{Phase::Unknown});Err(error)}
        }
    }
}
