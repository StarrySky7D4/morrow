//! Metadata notification consumption only. No content read or business effect.
use morrow_plugin_sdk::{channel::{self,Action,Directory,Kind,Request,Status},wasm,Error};
fn run() -> Result<(), Error> {
    let task=wasm::read_task()?;
    let input=task.transform().ok_or(Error::InvalidArgument)?;
    if input.handler!="changes.metadata" || input.input_type!="morrow.channel.directory.v1"
        || input.output_type!="morrow.changes.metadata.count.v1" {return Err(Error::InvalidArgument);}
    let directory=Directory::decode(&input.input).map_err(|_| Error::InvalidArgument)?;
    if directory.channels.len()!=1 || directory.channels[0].kind!=Kind::Events {return Err(Error::InvalidArgument);}
    let endpoint=&directory.channels[0];
    let mut request=Request{call_id:[1;32],reference:endpoint.reference,source_epoch:endpoint.source_epoch,action:Action::Query};
    let mut calls=0u64;
    let mut submit=|action| {
        calls=calls.checked_add(1).ok_or(Error::BadReply)?;
        if calls>endpoint.budget.max_requests {return Err(Error::Limit);}
        request.call_id=[0x47;32];request.call_id[..8].copy_from_slice(&calls.to_le_bytes());request.action=action;
        channel::transport::call_wasm(&request)
    };
    let mut count=0u64;let mut total=0u64;let mut scope=None;
    loop {
        let reply=submit(Action::Receive{last_acked:count,credit_bytes:endpoint.budget.max_frame_bytes})?;
        if reply.status==Status::Closed {break;}
        if matches!(reply.status,Status::Idle|Status::ClosingUnconfirmed) {continue;}
        if reply.status!=Status::Frame {return Err(Error::BadReply);}
        let frame=reply.frame.ok_or(Error::BadReply)?;
        if frame.sequence!=count+1 || frame.sequence>endpoint.budget.max_messages {return Err(Error::BadReply);}
        total=total.checked_add(frame.bytes.len() as u64).ok_or(Error::Limit)?;
        if total>endpoint.budget.max_bytes {return Err(Error::Limit);}
        let metadata=morrow_changes_metadata_v1::decode(&frame.bytes,&endpoint.source_epoch,&frame.cursor).map_err(|_|Error::BadReply)?;
        if scope.is_some_and(|s|s!=metadata.scope_digest) {return Err(Error::BadReply);}
        scope=Some(metadata.scope_digest);
        let ack=submit(Action::Ack{sequence:frame.sequence,frame_sha256:frame.digest().map_err(|_|Error::BadReply)?,cursor:frame.cursor})?;
        if ack.status!=Status::Acked || ack.last_acked!=frame.sequence {return Err(Error::BadReply);}
        count=frame.sequence;
    }
    wasm::complete_output(&task,&count.to_le_bytes())
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run()->i32 {if run().is_ok(){0}else{-1}}
