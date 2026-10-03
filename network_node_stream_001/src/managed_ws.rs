//! One explicitly host-approved duplex WebSocket source, using the existing
//! channel Receive/ACK/Send contract. Public IO WebSocketConnect stays unsupported.
//! Only the original HostRuntime Store prepares/reserves/claims one attempt;
//! no observed business outcome, replay, reconnect or second Store is created.
mod envelope;
pub use crate::websocket::{Message as MessageEnvelope, MessageKind, Quotas};
pub use envelope::{VERSION, MAX_ENVELOPE_BYTES, schema_digest};
use crate::{client::{Client,EndpointPolicy},stream,websocket,Limits,RawHttpRequest};
use morrow_core::{dispatch::HostRuntime,io,io_evidence::{Kind as MaterialKind,Material},
    io_intent::{Command,Phase,Record},plugin_package::io::IoCapability};
use morrow_plugin_runtime::{channel::{ChannelBroker,Producer,SourceGrant},manager::{Manager,ManagedInstance}};
use sha2::{Digest,Sha256};
use std::{collections::VecDeque,sync::{Arc,Mutex,Condvar,OnceLock,atomic::{AtomicBool,Ordering}},time::{Instant,Duration}};
use tokio_util::sync::CancellationToken;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum NetworkProfile { PublicWss, LoopbackWs, LoopbackWss }
// Deliberately without Debug: approval URLs/headers contain private content.
#[derive(Clone)]
pub struct Approval {
    /// Exact ws/wss URL, GET, no body. No credential/header escape hatch.
    pub request:RawHttpRequest, pub operation_id:String, pub approval_epoch:[u8;32],
    pub deadline:Instant, pub profile:NetworkProfile, pub limits:Limits,
    pub subprotocols:Vec<String>, pub quotas:Quotas,
    /// Cumulative inbound/outbound envelope bytes, no ACK refunds.
    pub max_encoded_bytes:u64,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error { Invalid,UnsupportedCredentials,Limit,AlreadyStarted,Conflict,OutcomeUnknown,CommitUnknown,
    Storage,Thread,Truncated,Source(morrow_plugin_runtime::channel::Error),Transport(websocket::Error) }
impl std::fmt::Display for Error {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"managed WebSocket: {self:?}")}}
impl std::error::Error for Error {}
pub type Result<T>=std::result::Result<T,Error>;
/// Socket worker task, native producer callback and broker OS-thread join are
/// three different facts. Only the broker's CleanupProof::Joined proves the latter.
pub struct Completion {
    pub outcome:Result<()>,pub websocket:Option<websocket::Completion>,
    pub opening_cleanup:Option<websocket::OpeningCleanup>,
    pub messages_queued:u64,pub messages_acked:u64,
    /// Mature sink actually flushed these guest-selected messages.
    pub outgoing_written:u64,pub encoded_bytes:u64,
}
impl std::fmt::Debug for Completion {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
    f.debug_struct("Completion").field("outcome",&self.outcome).field("websocket",&self.websocket)
        .field("opening_cleanup",&self.opening_cleanup).field("messages_queued",&self.messages_queued)
        .field("messages_acked",&self.messages_acked).field("outgoing_written",&self.outgoing_written)
        .field("encoded_bytes",&self.encoded_bytes).finish()}}
impl Completion {fn empty(outcome:Result<()>)->Self {Self{outcome,websocket:None,opening_cleanup:None,
    messages_queued:0,messages_acked:0,outgoing_written:0,encoded_bytes:0}}}
struct State {grant:SourceGrant,approval:Approval,client:Client,request:Vec<u8>,http_request:RawHttpRequest,
    frame_limit:usize,cursor_epoch:[u8;32],started:AtomicBool,cancel:CancellationToken,
    completion:Mutex<Option<Arc<Completion>>>,changed:Condvar}
#[derive(Clone)]
pub struct WsSource {state:Arc<State>}
impl std::fmt::Debug for WsSource {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
    f.debug_struct("WsSource").field("started",&self.state.started.load(Ordering::Acquire)).finish_non_exhaustive()}}
impl WsSource {
    #[allow(clippy::too_many_arguments)]
    pub fn approve(manager:&Manager,host:&HostRuntime,instance:&ManagedInstance,broker:&ChannelBroker,
        expected_revision:u64,approval:Approval,now:u64)->Result<Self> {
        Self::approve_with_live_guard(manager,host,instance,broker,expected_revision,approval,now,||true)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn approve_with_live_guard(manager:&Manager,host:&HostRuntime,instance:&ManagedInstance,broker:&ChannelBroker,
        expected_revision:u64,approval:Approval,now:u64,live:impl Fn()->bool+Send+Sync+'static)->Result<Self> {
        let (client,request,http_request,target_digest,approval_digest)=prepare(&approval,broker)?;
        let package=instance.package().package();
        let expected=Command{operation_id:approval.operation_id.clone(),subject:package.manifest().package_id.clone(),
            package_sha256:package.digest(),capability:IoCapability::WebSocketConnect,protocol_sha256:io::schema_digest(),
            request_sha256:Sha256::digest(&request).into(),approval_sha256:approval_digest,target_sha256:target_digest,
            request_bytes:request.len() as u64,response_limit:approval.quotas.max_incoming_wire_bytes};
        Record::prepared(expected.clone()).map_err(storage)?;
        let grant=SourceGrant::issue_websocket_with_deadline_and_live_guard(manager,host,instance,broker,
            expected_revision,expected,now,approval.deadline,live).map_err(Error::Source)?;
        let endpoint=broker.endpoint();
        Ok(Self{state:Arc::new(State{grant,approval,client,request,http_request,
            frame_limit:(endpoint.budget.max_frame_bytes as usize).min(MAX_ENVELOPE_BYTES),cursor_epoch:endpoint.source_epoch,
            started:AtomicBool::new(false),cancel:CancellationToken::new(),completion:Mutex::new(None),changed:Condvar::new()})})
    }
    pub fn command(&self)->&Command {self.state.grant.expected_command()}
    pub fn grant(&self)->&SourceGrant {&self.state.grant}
    pub fn revoke(&self) {self.state.grant.revoke();self.state.cancel.cancel();}
    pub fn completion(&self)->Option<Arc<Completion>> {self.state.completion.lock().unwrap_or_else(|e|e.into_inner()).clone()}
    pub fn wait_completion(&self,timeout:Duration)->Option<Arc<Completion>> {
        let end=Instant::now().checked_add(timeout)?;
        let mut slot=self.state.completion.lock().unwrap_or_else(|e|e.into_inner());
        loop {if slot.is_some(){return slot.clone();}
            let remaining=end.checked_duration_since(Instant::now())?;
            let (next,wait)=self.state.changed.wait_timeout(slot,remaining).unwrap_or_else(|e|e.into_inner());slot=next;
            if wait.timed_out(){return slot.clone();}}
    }
    pub fn start(&self,manager:&Manager,host:&mut HostRuntime,instance:&ManagedInstance,broker:&ChannelBroker)->Result<()> {
        if self.state.started.swap(true,Ordering::AcqRel){return Err(Error::AlreadyStarted);}
        let result=self.start_inner(manager,host,instance,broker);
        if let Err(e)=result{publish(&self.state,Completion::empty(Err(e)));}result
    }
    fn start_inner(&self,manager:&Manager,host:&mut HostRuntime,instance:&ManagedInstance,broker:&ChannelBroker)->Result<()> {
        self.state.grant.validate_owner(manager,host,instance,broker).map_err(Error::Source)?;
        self.state.grant.check().map_err(Error::Source)?;
        let expected=self.command();let store=host.store_local_mut();
        let prepared=match store.lookup_io_intent(&expected.subject,&expected.operation_id).map_err(storage)? {
            Some(record)=>{record.matches_command(expected).map_err(storage)?;
                if record.phase()==Phase::OutcomeUnknown{return Err(Error::OutcomeUnknown);}
                if record.phase()!=Phase::Prepared{return Err(Error::Conflict);}record}
            None=>{let record=Record::prepared(expected.clone()).map_err(storage)?;
                store.append_io_intent_local_authorized(&record,||authorize(&self.state.grant)).map_err(storage)?}
        };
        let original=Material::encode(MaterialKind::Request,&expected.operation_id,&expected.subject,
            expected.request_sha256,&self.state.request).map_err(storage)?;
        store.reserve_io_materials(expected,||authorize(&self.state.grant)).map_err(storage)?;
        store.store_io_material(&expected.subject,MaterialKind::Request,&original,||authorize(&self.state.grant)).map_err(storage)?;
        store.reserve_io_intent_followup(expected,||authorize(&self.state.grant)).map_err(storage)?;
        let boundary=prepared.propose_dispatch_boundary().map_err(storage)?;
        store.claim_io_dispatch_local_authorized(&boundary,||authorize(&self.state.grant)).map_err(storage)?;
        // The strict claim precedes the sole HTTP opening, even on spawn failure.
        // Everything after it remains historically Unknown, never a fake IO Reply.
        self.state.grant.check().map_err(Error::Source)?;
        let state=Arc::clone(&self.state);broker.spawn(move|producer|run_source(state,producer)).map_err(Error::Source)
    }
}
fn authorize(grant:&SourceGrant)->morrow_core::Result<()> {grant.check().map_err(|_|morrow_core::Error::Invalid("WebSocket source no longer approved"))}
fn storage(e:morrow_core::Error)->Error {match e {morrow_core::Error::Limit=>Error::Limit,
    morrow_core::Error::OperationConflict|morrow_core::Error::RevisionConflict=>Error::Conflict,
    morrow_core::Error::CommitUnknown=>Error::CommitUnknown,morrow_core::Error::Invalid(_)|morrow_core::Error::UnsupportedVersion=>Error::Invalid,_=>Error::Storage}}
fn publish(state:&State,done:Completion){let mut slot=state.completion.lock().unwrap_or_else(|e|e.into_inner());if slot.is_none(){*slot=Some(Arc::new(done));}state.changed.notify_all();}
struct Guard{grant:SourceGrant,cancel:CancellationToken}
impl stream::StreamGuard for Guard {fn check(&self)->crate::Result<()> {match self.grant.check(){Ok(())=>Ok(()),Err(e)=>{
    self.cancel.cancel();Err(match e{morrow_plugin_runtime::channel::Error::Expired=>crate::Error::Timeout,
        morrow_plugin_runtime::channel::Error::Closed=>crate::Error::Closed,_=>crate::Error::Denied})}}}}
fn run_source(state:Arc<State>,producer:Producer){
    // A dedicated runtime on the broker-owned native thread, never a shared
    // executor/Store lock. Socket task join does not claim native OS-thread join.
    let runtime=match tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build(){
        Ok(v)=>v,Err(_)=>{drop(producer);publish(&state,Completion::empty(Err(Error::Thread)));return;}};
    let guard=Arc::new(Guard{grant:state.grant.clone(),cancel:state.cancel.clone()});
    let context=stream::SendContext::guarded(tokio::time::Instant::from_std(state.approval.deadline),state.cancel.clone(),guard);
    let opened=runtime.block_on(websocket::open(&state.client,state.http_request.clone(),state.approval.subprotocols.clone(),state.approval.quotas,context));
    let mut lease=match opened{Ok(v)=>v,Err(e)=>{
        let mut done=Completion::empty(Err(Error::Transport(e.cause)));
        if let websocket::OpeningCleanup::Worker(worker)=&e.cleanup{done.websocket=Some(worker.clone());}
        done.opening_cleanup=Some(e.cleanup);drop(producer);publish(&state,done);return;}};
    let mut done=Completion::empty(Ok(()));
    let result=runtime.block_on(pump(&state,&producer,&mut lease,&mut done));
    let transport=if result.is_err(){runtime.block_on(lease.cancel_and_wait())}else{runtime.block_on(lease.finish())};
    let result=result.and_then(|()|transport.outcome.map_err(Error::Transport)).and_then(|()|{
        if transport.peer_close.is_some()&&transport.transport_eof&&transport.worker_joined{Ok(())}else{Err(Error::Truncated)}});
    done.websocket=Some(transport);
    if let Err(e)=result{done.outcome=Err(e);drop(producer);}else{done.outcome=producer.finish().map_err(Error::Source);}
    publish(&state,done);
}
async fn pump(state:&State,producer:&Producer,lease:&mut websocket::WebSocketLease,done:&mut Completion)->Result<()> {
    let mut pending:VecDeque<Vec<u8>>=VecDeque::new();let mut pending_bytes=0usize;
    let mut inflight:Option<u64>=None;let mut eof=false;
    let mut tick=tokio::time::interval(Duration::from_millis(5));tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Reserve capacity for the full next possible encoded message BEFORE poll.
    // One separate socket-worker result slot and library read/reassembly buffers
    // are independently bounded; no provider event is silently discarded.
    let reserve=state.approval.quotas.max_message_bytes.checked_add(128).ok_or(Error::Limit)?;
    loop {
        state.grant.check().map_err(Error::Source)?;
        if let Some(terminal)=lease.terminal(){if let Err(error)=terminal.outcome{return Err(Error::Transport(error));}}
        if let Some(sequence)=inflight {
            if producer.try_acked(sequence).map_err(Error::Source)?{done.messages_acked+=1;inflight=None;}
        }
        if inflight.is_none() {
            if let Some(bytes)=pending.pop_front(){pending_bytes-=bytes.len();
                let sequence=done.messages_queued.checked_add(1).ok_or(Error::Limit)?;
                let cursor=cursor(state,sequence,&bytes);
                state.grant.check().map_err(Error::Source)?;
                producer.push(bytes,cursor.to_vec()).map_err(Error::Source)?;
                done.messages_queued=sequence;inflight=Some(sequence);
            }
        }
        // Local admission/observation is not socket-written. Only mature flush
        // success below increments the actual outgoing receipt counter.
        if let Some((_sequence,bytes))=producer.receive_sent().map_err(Error::Source)? {
            reserve_encoded(state,done,bytes.len())?;
            let message=MessageEnvelope::decode(&bytes)?;
            message.validate(state.approval.quotas.max_message_bytes).map_err(Error::Transport)?;
            state.grant.check().map_err(Error::Source)?;
            lease.send_message(message).await.map_err(Error::Transport)?;
            done.outgoing_written+=1;
        }
        if eof&&inflight.is_none()&&pending.is_empty(){return Ok(());}
        let space=pending.len()<state.approval.quotas.max_pending_messages
            && pending_bytes.checked_add(reserve).is_some_and(|n|n<=state.approval.quotas.max_pending_bytes);
        // A full ordinary queue pauses read demand. It is not an error or a
        // reason to drop a healthy connection. TCP-order means this cannot promise
        // to reach unseen controls behind unread data. Already queued control
        // writes, guest sends, cancellation and absolute expiry still progress.
        tokio::select!{biased;
            _=tick.tick()=>{},
            message=lease.next_message(),if !eof&&space=>{
                match message.map_err(Error::Transport)? {
                    Some(message)=>{
                        let bytes=message.encode()?;
                        reserve_encoded(state,done,bytes.len())?;
                        if bytes.len()>state.frame_limit{return Err(Error::Limit);}
                        pending_bytes=pending_bytes.checked_add(bytes.len()).ok_or(Error::Limit)?;
                        if pending_bytes>state.approval.quotas.max_pending_bytes{return Err(Error::Limit);}
                        pending.push_back(bytes);
                    }
                    None=>eof=true,
                }
            }
        }
    }
}
fn reserve_encoded(state:&State,done:&mut Completion,bytes:usize)->Result<()> {
    let total=done.encoded_bytes.checked_add(bytes as u64).ok_or(Error::Limit)?;
    if total>state.approval.max_encoded_bytes{return Err(Error::Limit);}done.encoded_bytes=total;Ok(())
}
fn cursor(state:&State,ordinal:u64,bytes:&[u8])->[u8;32]{let mut hash=Sha256::new();hash.update(b"Morrow/managed-ws/cursor/v1\0");
    hash.update(state.cursor_epoch);hash.update(state.approval.approval_epoch);hash.update(state.grant.expected_command().request_sha256);
    hash.update(ordinal.to_be_bytes());hash.update(Sha256::digest(bytes));hash.finalize().into()}
fn field(hash:&mut Sha256,value:&[u8]){hash.update((value.len()as u64).to_be_bytes());hash.update(value);}
fn prepare(a:&Approval,broker:&ChannelBroker)->Result<(Client,Vec<u8>,RawHttpRequest,[u8;32],[u8;32])>{
    a.limits.validate().map_err(|e|Error::Transport(websocket::Error::Transport(e)))?;
    a.quotas.validate().map_err(Error::Transport)?;websocket::validate_protocols(&a.subprotocols).map_err(Error::Transport)?;
    let endpoint=broker.endpoint();
    let remaining=a.deadline.checked_duration_since(Instant::now()).ok_or(Error::Invalid)?;
    if a.approval_epoch==[0;32]||a.request.method!="GET"||!a.request.body.is_empty()||remaining>a.limits.timeout
        ||a.max_encoded_bytes==0||a.max_encoded_bytes>endpoint.budget.max_bytes
        ||a.quotas.max_incoming_wire_bytes>a.limits.max_response_bytes as u64
        ||a.quotas.max_outgoing_wire_bytes>a.limits.max_request_bytes as u64
        ||a.quotas.max_incoming_wire_bytes>morrow_core::plugin_package::io::MAX_JOB_BYTES
        ||a.quotas.max_incoming_messages>endpoint.budget.max_messages
        ||a.quotas.max_outgoing_messages>endpoint.budget.max_messages
        ||a.quotas.max_message_bytes.checked_add(128).is_none_or(|n|n>a.quotas.max_pending_bytes||n>MAX_ENVELOPE_BYTES)
    {return Err(Error::Limit);}
    let raw=&a.request.target;
    if raw.len()>8192||raw.chars().any(|c|c.is_control()||c.is_whitespace()||c=='\\'){return Err(Error::Invalid);}
    let target=url::Url::parse(raw).map_err(|_|Error::Invalid)?;
    if target.as_str()!=raw||!target.username().is_empty()||target.password().is_some()||target.fragment().is_some()
        ||target.host_str().is_none(){return Err(Error::Invalid);}
    let mut http=target.clone();
    match a.profile {
        NetworkProfile::PublicWss|NetworkProfile::LoopbackWss if target.scheme()=="wss"=>http.set_scheme("https").map_err(|_|Error::Invalid)?,
        NetworkProfile::LoopbackWs if target.scheme()=="ws"=>http.set_scheme("http").map_err(|_|Error::Invalid)?,
        _=>return Err(Error::Invalid),
    }
    let origin=http.origin().ascii_serialization();
    let policy=match a.profile{
        NetworkProfile::PublicWss=>EndpointPolicy::new(&origin,&["GET"],false),
        NetworkProfile::LoopbackWs=>EndpointPolicy::new(&origin,&["GET"],true),
        NetworkProfile::LoopbackWss=>EndpointPolicy::local_https(&origin,&["GET"]),
    }.map_err(|e|Error::Transport(websocket::Error::Transport(e)))?;
    let client=Client::new(policy,a.limits).map_err(|e|Error::Transport(websocket::Error::Transport(e)))?;
    for (name,_)in &a.request.headers{let name=name.to_ascii_lowercase();
        if matches!(name.as_str(),"authorization"|"cookie"|"proxy-authorization"|"x-api-key"|"api-key") {return Err(Error::UnsupportedCredentials);}
        if name.starts_with("sec-websocket-"){return Err(Error::Invalid);}}
    let mut http_request=a.request.clone();http_request.target=http.as_str().to_owned();
    let (_,mut opening_headers)=client.validate_request(&http_request).map_err(|e|Error::Transport(websocket::Error::Transport(e)))?;
    // Exact fixed-size host nonce placeholder: approval does not produce random
    // key material or connect, but validates the COMPLETE opening header budget.
    for (name,value) in [("connection","Upgrade"),("upgrade","websocket"),("sec-websocket-version","13"),
        ("sec-websocket-key","AAAAAAAAAAAAAAAAAAAAAA==")] {
        opening_headers.insert(reqwest::header::HeaderName::from_static(name),
            reqwest::header::HeaderValue::from_str(value).map_err(|_|Error::Invalid)?);
    }
    if !a.subprotocols.is_empty(){opening_headers.insert(reqwest::header::HeaderName::from_static("sec-websocket-protocol"),
        reqwest::header::HeaderValue::from_str(&a.subprotocols.join(", ")).map_err(|_|Error::Invalid)?);}
    client.validate_upgrade_headers(&opening_headers).map_err(|e|Error::Transport(websocket::Error::Transport(e)))?;
    let mut target_hash=Sha256::new();target_hash.update(b"Morrow/managed-ws/target/v1\0");target_hash.update([a.profile as u8]);field(&mut target_hash,raw.as_bytes());
    let target_digest:[u8;32]=target_hash.finalize().into();
    let mut relative=target.path().to_owned();if let Some(query)=target.query(){relative.push('?');relative.push_str(query);}
    let endpoint_reference=hex(&target_digest);
    let mut logical_headers:Vec<io::Header>=a.request.headers.iter().map(|(name,value)|io::Header{name:name.clone(),value:value.clone()}).collect();
    logical_headers.push(io::Header{name:"sec-websocket-version".into(),value:b"13".to_vec()});
    if !a.subprotocols.is_empty(){logical_headers.push(io::Header{name:"sec-websocket-protocol".into(),value:a.subprotocols.join(", ").into_bytes()});}
    // The existing schema reuses HttpRequest for WS. Apply its existing bounded
    // field contract before encoding; do not use Unsupported to bypass it.
    let logical=io::HttpSubmission{operation_id:a.operation_id.as_bytes().to_vec(),
        deadline_ms:a.limits.timeout.as_millis().try_into().map_err(|_|Error::Limit)?,endpoint:endpoint_reference.as_bytes().to_vec(),
        method:"GET".into(),relative_target:relative.clone(),headers:logical_headers,body:Vec::new(),credential:Vec::new()};
    io::validate_http_submission(&logical).map_err(storage)?;
    // Preserve the actual existing IO Submission::WebSocketConnect discriminator.
    // The frozen/public Request decoder continues to classify it Unsupported.
    let mut message=capnp::message::Builder::new_default();
    {let mut root=message.init_root::<morrow_core::io_capnp::request::Builder>();root.set_version(io::VERSION);root.set_schema_sha256(&io::schema_digest());root.set_call_id(1);
        let mut submission=root.init_submit();submission.set_operation_id(a.operation_id.as_bytes());
        submission.set_deadline_ms(a.limits.timeout.as_millis().try_into().map_err(|_|Error::Limit)?);
        let mut ws=submission.init_web_socket_connect();ws.set_endpoint(&logical.endpoint);ws.set_method("GET");ws.set_relative_target(&relative);
        ws.set_body(&[]);ws.set_credential(&[]);
        let mut headers=ws.init_headers(logical.headers.len().try_into().map_err(|_|Error::Limit)?);
        for(index,value)in logical.headers.iter().enumerate(){let mut header=headers.reborrow().get(index as u32);header.set_name(&value.name);header.set_value(&value.value);}}
    let request=capnp::serialize::write_message_to_words(&message);
    if request.len()>io::MAX_FRAME_BYTES{return Err(Error::Limit);}
    // Check the original legacy decoder's correlation, without pretending it
    // authorizes/exposes a public WebSocket submission route.
    let decoded=io::Request::decode(&request).map_err(storage)?;
    if decoded.action()!=&io::Action::Unsupported(io::UnsupportedAction::Submit){return Err(Error::Invalid);}
    let mut hash=Sha256::new();hash.update(b"Morrow/managed-ws/approval/v1\0");hash.update(a.approval_epoch);hash.update(target_digest);hash.update(Sha256::digest(&request));
    static CLOCK_ORIGIN:OnceLock<Instant>=OnceLock::new();let origin=*CLOCK_ORIGIN.get_or_init(Instant::now);
    hash.update(a.deadline.checked_duration_since(origin).ok_or(Error::Invalid)?.as_nanos().to_be_bytes());
    for value in [a.limits.max_request_bytes as u64,a.limits.max_response_bytes as u64,a.limits.max_header_bytes as u64,
        a.limits.max_concurrent as u64,a.max_encoded_bytes,a.quotas.max_message_bytes as u64,a.quotas.max_frame_bytes as u64,
        a.quotas.max_incoming_messages,a.quotas.max_outgoing_messages,a.quotas.max_incoming_frames,a.quotas.max_outgoing_frames,
        a.quotas.max_incoming_wire_bytes,a.quotas.max_outgoing_wire_bytes,a.quotas.max_pending_messages as u64,a.quotas.max_pending_bytes as u64]{hash.update(value.to_be_bytes());}
    for protocol in &a.subprotocols{field(&mut hash,protocol.as_bytes());}
    Ok((client,request,http_request,target_digest,hash.finalize().into()))
}

fn hex(bytes:&[u8])->String {use std::fmt::Write;let mut out=String::with_capacity(bytes.len()*2);
    for byte in bytes {write!(&mut out,"{byte:02x}").expect("String write");}out}
