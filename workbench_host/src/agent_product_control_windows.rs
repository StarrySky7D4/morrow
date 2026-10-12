//! Windows retained original-worker adapter. All identifiers below are local
//! correlation slots. Original authority and pending handles remain host-only.
use super::{Request,Reply,Error,MAX_IDS,wire};
use crate::{Workbench,io_tasks::TaskKey};
use crate::agent_tasks::{AgentError,agent_product::{ProductSessionController,Submission,SessionPoll,SessionCompletion,Writer}};
use morrow_agent_session_exec_v1_r2 as r2;
use morrow_core::dispatch::HostBinding;
use zeroize::Zeroize;

struct Pending { slot:u64, original:Submission, request:Option<r2::Request> }
struct OwnedWriter { slot:u64, original:Writer }
struct Attached {
    namespace:u64, task:TaskKey, owner:HostBinding,
    controller:ProductSessionController, pending:Option<Pending>, writers:Vec<OwnedWriter>,
}
#[derive(Default)]
pub(crate) struct NativeSlot {
    attached:Option<Attached>, next:u64, seen:Vec<[u8;32]>, sealed:bool,
}
impl NativeSlot {
    fn sequence(&mut self)->std::result::Result<u64,wire::Status> {
        self.next=self.next.checked_add(1).ok_or(wire::Status::Limit)?;Ok(self.next)
    }
    pub(super) fn seal(&mut self) {
        self.sealed=true;
        if let Some(a)=&mut self.attached {a.controller.mark_delivery_unknown();}
    }
    fn execute<F>(&mut self,q:&Request,owner:HostBinding,current_task:bool,make:F)->Reply
    where F:FnOnce(TaskKey)->crate::Result<ProductSessionController> {
        use wire::{Operation as Op,Status as S};
        if self.seen.contains(&q.id) {return self.observation(S::Invalid)}
        // Only effect-producing requests consume persistent duplicate history.
        // Status, original Read and rights reduction must remain possible when
        // effect history is full; they cannot create or replay an R2 mutation.
        if matches!(q.operation,Op::Attach|Op::Open|Op::Submit|Op::AdmitWriter) {
            if self.seen.len()>=MAX_IDS {return self.observation(S::Limit)}
            self.seen.push(q.id);
        }
        if self.sealed && matches!(q.operation,Op::Attach|Op::Open|Op::Submit|Op::AdmitWriter) {
            return self.observation(S::Unknown)
        }
        if q.operation==Op::Attach {
            if self.attached.is_some() {return self.observation(S::Busy)}
            if !current_task {return self.observation(S::Unavailable)}
            let task=match TaskKey::from_bytes(&q.task) {Ok(t)=>t,Err(_)=>return self.observation(S::Invalid)};
            let controller=match make(task) {Ok(c)=>c,Err(_)=>return self.observation(S::Unavailable)};
            if controller.owner_binding()!=owner || controller.status().task!=task {
                return self.observation(S::Unavailable)
            }
            let namespace=match self.sequence(){Ok(n)=>n,Err(e)=>return self.observation(e)};
            self.attached=Some(Attached {namespace,task,owner,controller,pending:None,writers:Vec::new()});
            return self.observation(S::Attached)
        }
        let Some(a)=self.attached.as_ref() else {return self.observation(S::Unavailable)};
        if q.attachment!=a.namespace || &q.task!=a.task.as_bytes() {return self.observation(S::Invalid)}
        if owner!=a.owner || !current_task {self.seal();return self.observation(S::Unknown)}
        if a.controller.status().delivery_unknown {self.sealed=true;}
        if self.sealed && matches!(q.operation,Op::Open|Op::Submit|Op::AdmitWriter) {return self.observation(S::Unknown)}
        match q.operation {
            Op::Status=>self.observation(S::Observed),
            Op::MarkLost=>{self.seal();self.observation(S::Unknown)},
            Op::Detach=>{
                let mut r=self.observation(S::Detached);
                if self.attached.as_ref().is_some_and(|a|a.pending.is_some()) {self.seal();}
                r.delivery_unknown=self.sealed;r.lease_live=false;r.submission=0;
                // Drop closes original authorities/cancels original pending handle.
                // This is not an acknowledgment, owner join, recovery or task stop.
                self.attached.take();r
            },
            Op::CloseWriter=>{
                let a=self.attached.as_mut().unwrap();
                let Some(i)=a.writers.iter().position(|w|w.slot==q.writer_slot) else {return self.observation(S::Invalid)};
                let result=a.controller.close_writer(a.writers[i].original);
                if result.is_ok(){a.writers.remove(i);}
                if a.controller.status().delivery_unknown {self.sealed=true;}
                self.observation(result.map(|_|S::Observed).unwrap_or_else(status))
            },
            Op::Open|Op::Submit|Op::AdmitWriter=>{
                if self.attached.as_ref().unwrap().pending.is_some(){return self.observation(S::Busy)}
                // Reserve correlation before dispatch; overflow cannot lose a submitted command.
                let local=match self.sequence(){Ok(n)=>n,Err(e)=>return self.observation(e)};
                let a=self.attached.as_mut().unwrap();
                let request=if q.operation==Op::Submit {
                    if a.controller.status().endpoint_generation!=Some(q.generation){return self.observation(S::Invalid)}
                    match q.canonical_request(){Ok(r)=>Some(r),Err(_)=>return self.observation(S::Invalid)}
                }else{None};
                let result=match q.operation {
                    Op::Open=>a.controller.open(),
                    Op::AdmitWriter=>a.controller.admit_writer(&q.session_id),
                    Op::Submit=>{
                        let raw=request.as_ref().unwrap().raw();
                        if q.writer_slot==0 {a.controller.exchange(raw)} else {
                            let Some(w)=a.writers.iter().find(|w|w.slot==q.writer_slot) else {return self.observation(S::Invalid)};
                            a.controller.writer_exchange(w.original,raw)
                        }
                    },
                    _=>unreachable!(),
                };
                match result {
                    Ok(original)=>{a.pending=Some(Pending {slot:local,original,request});self.observation(S::Submitted)},
                    Err(e)=>{if uncertain(e){self.seal();}self.observation(status(e))},
                }
            },
            Op::Read=>{
                let a=self.attached.as_mut().unwrap();
                let Some(p)=a.pending.as_ref() else{return self.observation(S::Invalid)};
                if q.submission!=p.slot {return self.observation(S::Invalid)}
                // No reconstruction from untrusted integers: exact original handle token only.
                match a.controller.poll(p.original) {
                    Ok(SessionPoll::Pending)=>self.observation(S::Pending),
                    Err(e)=>{a.pending.take();if uncertain(e){self.seal();}self.observation(status(e))},
                    Ok(SessionPoll::Complete(completed))=>{
                        let p=a.pending.take().unwrap();
                        match completed {
                            SessionCompletion::Opened{generation}=>{let mut r=self.observation(S::Opened);r.generation=generation;r.submission=p.slot;r},
                            SessionCompletion::WriterAdmitted{writer,generation}=>{
                                // The original Writer object is opaque and remains local.
                                a.writers.push(OwnedWriter{slot:p.slot,original:writer});
                                let mut r=self.observation(S::WriterAdmitted);r.writer_slot=p.slot;r.generation=generation;r.submission=p.slot;r
                            },
                            SessionCompletion::Reply(raw)=>{
                                let Some(original)=p.request else {self.seal();return self.observation(S::Unknown)};
                                let decoded=match r2::Reply::decode_for(&original,&raw){Ok(r)=>r,Err(_)=>{self.seal();return self.observation(S::Unknown)}};
                                if matches!(&decoded.outcome,r2::Outcome::Tool(_)|r2::Outcome::Claimed{..}) {self.seal();return self.observation(S::Unknown)}
                                if a.controller.status().delivery_unknown {self.sealed=true;}
                                let mut r=self.observation(S::Reply);r.submission=p.slot;
                                r.canonical_request_sha256=decoded.request_sha256;r.canonical_reply=raw.to_vec();r.outcome=Some(decoded.outcome);r
                            },
                        }
                    },
                }
            },
            _=>self.observation(S::Unsupported),
        }
    }
    fn observation(&self,status:wire::Status)->Reply {
        let mut r=Reply::new(status);r.delivery_unknown=self.sealed;
        if let Some(a)=&self.attached {
            let s=a.controller.status();r.attachment=a.namespace;r.generation=s.endpoint_generation.unwrap_or(0);
            r.submission=a.pending.as_ref().map(|p|p.slot).unwrap_or(0);
            r.lease_live=s.original_lease_live;r.delivery_unknown|=s.delivery_unknown;
        }
        r
    }
}
fn uncertain(e:AgentError)->bool {
    !matches!(e,AgentError::Busy|AgentError::Limit|AgentError::Invalid|AgentError::Unsupported
        |AgentError::Session(r2::Error::Denied|r2::Error::Invalid|r2::Error::Contract|r2::Error::Correlation|r2::Error::Conflict|r2::Error::NotFound|r2::Error::Limit))
}
fn status(e:AgentError)->wire::Status {use AgentError::*;match e {
    Busy=>wire::Status::Busy,Limit=>wire::Status::Limit,Invalid=>wire::Status::Invalid,Unsupported=>wire::Status::Unsupported,
    Session(r2::Error::Invalid|r2::Error::Contract|r2::Error::Correlation)=>wire::Status::Invalid,
    Session(r2::Error::Limit)=>wire::Status::Limit,
    Unknown|Session(r2::Error::CommitUnknown|r2::Error::Storage)=>wire::Status::Unknown,
    _=>wire::Status::Unavailable,
}}

pub fn respond(host:&mut Workbench,input:&[u8])->crate::Result<Vec<u8>> {
    let q=Request::decode(input)?;
    let gate=host.state.product_gate.clone();let owner=host.agent_owner_binding();
    let current_task=TaskKey::from_bytes(&q.task).is_ok_and(|k|host.state.agent_worker(k).is_ok());
    let mut slot=std::mem::take(&mut host.product_session_wire);
    let reply=if gate.closed(){slot.seal();slot.observation(wire::Status::Unknown)} else {
        slot.execute(&q,owner,current_task,|task|host.product_session_controller(task))
    };
    let result=deliver(&mut slot,&gate,&q,input,reply);
    // Restore even on encoding failure; Unknown is never erased by a new request ID.
    host.product_session_wire=slot;
    Ok(result?)
}
fn deliver(slot:&mut NativeSlot,gate:&crate::product_gate::ProductGate,q:&Request,input:&[u8],reply:Reply)
    ->std::result::Result<Vec<u8>,Error> {
    deliver_with(slot,gate,q,input,reply,||{})
}
fn deliver_with(slot:&mut NativeSlot,gate:&crate::product_gate::ProductGate,q:&Request,input:&[u8],reply:Reply,before_final:impl FnOnce())
    ->std::result::Result<Vec<u8>,Error> {
    let mut bytes=match reply.encode_for(q,input){Ok(b)=>b,Err(e)=>{slot.seal();return Err(e)}};
    before_final();
    // Completion can already have consumed a producer reply. Gate failure after
    // encoding must revoke original delivery and discard those encoded bytes.
    if gate.closed(){bytes.zeroize();slot.seal();return slot.observation(wire::Status::Unknown).encode_for(q,input)}
    Ok(bytes)
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    pub(crate) fn attached(controller:ProductSessionController)->NativeSlot {
        let owner=controller.owner_binding();let task=controller.status().task;
        NativeSlot{attached:Some(Attached{namespace:1,task,owner,controller,pending:None,writers:Vec::new()}),next:1,seen:Vec::new(),sealed:false}
    }
    pub(crate) fn execute(slot:&mut NativeSlot,q:&Request,owner:HostBinding)->Reply {
        q.validate().unwrap();slot.execute(q,owner,true,|_|panic!("test must not reconstruct a controller"))
    }
    pub(crate) fn delivery_after_revoke(slot:&mut NativeSlot,q:&Request,reply:Reply)->Vec<u8> {
        let gate=crate::product_gate::ProductGate::default();let input=q.encode().unwrap();
        // Exercise the production encoding/final delivery branch itself.
        deliver_with(slot,&gate,q,&input,reply,||gate.revoke()).unwrap()
    }
}
