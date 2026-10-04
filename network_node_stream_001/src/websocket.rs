//! Trusted native RFC6455 transport. No guest IO grant or business completion.
//! Reqwest performs the original pinned HTTP/TLS opening; tungstenite owns the
//! actual WebSocket protocol. No redirects, reconnect, compression or retry.
use crate::{client::Client, stream::{SendContext, ResponseHead}, RawHttpRequest};
use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderName, HeaderValue};
use std::{future::Future, io, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll}};
use tokio::{io::{AsyncRead, AsyncWrite, ReadBuf}, sync::{mpsc, oneshot, watch, OwnedSemaphorePermit}, task::JoinHandle};
use tokio_tungstenite::{WebSocketStream, tungstenite::{self, protocol::{Role, WebSocketConfig, CloseFrame, frame::coding::CloseCode}}};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind { Text, Binary, Ping, Pong, Close }
// Debug exposes only kind, length and code, never private payload bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Message { pub kind: MessageKind, pub payload: Vec<u8>, pub close_code: Option<u16> }
impl Message {
    pub fn validate(&self, max: usize) -> Result<()> {
        if self.payload.len() > max { return Err(Error::Limit); }
        if self.kind != MessageKind::Close && self.close_code.is_some() { return Err(Error::Invalid); }
        match self.kind {
            MessageKind::Text => { std::str::from_utf8(&self.payload).map_err(|_| Error::Utf8)?; }
            MessageKind::Ping | MessageKind::Pong if self.payload.len() > 125 => return Err(Error::Limit),
            MessageKind::Close => {
                std::str::from_utf8(&self.payload).map_err(|_| Error::Utf8)?;
                if self.payload.len() > 123 || (self.close_code.is_none() && !self.payload.is_empty())
                    || self.close_code.is_some_and(|code| !CloseCode::from(code).is_allowed())
                { return Err(Error::Invalid); }
            }
            _ => {}
        }
        Ok(())
    }
    fn into_ws(self) -> Result<tungstenite::Message> {
        self.validate(64 * 1024)?;
        Ok(match self.kind {
            MessageKind::Text => tungstenite::Message::Text(String::from_utf8(self.payload).map_err(|_| Error::Utf8)?.into()),
            MessageKind::Binary => tungstenite::Message::Binary(self.payload.into()),
            MessageKind::Ping => tungstenite::Message::Ping(self.payload.into()),
            MessageKind::Pong => tungstenite::Message::Pong(self.payload.into()),
            MessageKind::Close => tungstenite::Message::Close(self.close_code.map(|code| CloseFrame {
                code: code.into(), reason: String::from_utf8(self.payload).expect("validated close UTF8").into(),
            })),
        })
    }
    fn from_ws(value: tungstenite::Message) -> Result<Self> {
        let (kind, payload, close_code) = match value {
            tungstenite::Message::Text(v) => (MessageKind::Text, v.as_bytes().to_vec(), None),
            tungstenite::Message::Binary(v) => (MessageKind::Binary, v.to_vec(), None),
            tungstenite::Message::Ping(v) => (MessageKind::Ping, v.to_vec(), None),
            tungstenite::Message::Pong(v) => (MessageKind::Pong, v.to_vec(), None),
            tungstenite::Message::Close(v) => match v {
                Some(v) => (MessageKind::Close, v.reason.as_bytes().to_vec(), Some(v.code.into())),
                None => (MessageKind::Close, Vec::new(), None),
            },
            tungstenite::Message::Frame(_) => return Err(Error::Protocol),
        };
        Ok(Self { kind, payload, close_code })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Quotas {
    pub max_message_bytes: usize,
    pub max_frame_bytes: usize,
    pub max_incoming_messages: u64,
    pub max_outgoing_messages: u64,
    pub max_incoming_frames: u64,
    pub max_outgoing_frames: u64,
    /// Decrypted RFC6455 bytes including frame headers and masks; TLS overhead excluded.
    pub max_incoming_wire_bytes: u64,
    pub max_outgoing_wire_bytes: u64,
    /// Native staging only. Original channel admits one unacknowledged frame.
    pub max_pending_messages: usize,
    pub max_pending_bytes: usize,
}
impl Default for Quotas {
    fn default() -> Self { Self { max_message_bytes: 32*1024, max_frame_bytes: 32*1024,
        max_incoming_messages: 64, max_outgoing_messages: 64, max_incoming_frames: 256,
        max_outgoing_frames: 256, max_incoming_wire_bytes: 1024*1024,
        max_outgoing_wire_bytes: 1024*1024, max_pending_messages: 4, max_pending_bytes: 64*1024 } }
}
impl Quotas {
    pub fn validate(self) -> Result<()> {
        if self.max_message_bytes == 0 || self.max_message_bytes > 64*1024
            || self.max_frame_bytes == 0 || self.max_frame_bytes > self.max_message_bytes
            || self.max_incoming_messages == 0 || self.max_outgoing_messages == 0
            || self.max_incoming_frames == 0 || self.max_outgoing_frames == 0
            || self.max_incoming_frames > 1_000_000 || self.max_outgoing_frames > 1_000_000
            || self.max_incoming_messages > self.max_incoming_frames
            || self.max_outgoing_messages > self.max_outgoing_frames
            || self.max_incoming_wire_bytes == 0 || self.max_outgoing_wire_bytes == 0
            || self.max_incoming_wire_bytes > 64*1024*1024 || self.max_outgoing_wire_bytes > 64*1024*1024
            || self.max_pending_messages == 0 || self.max_pending_messages > 8
            || self.max_pending_bytes == 0 || self.max_pending_bytes > 64*1024
        { return Err(Error::Limit); }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Invalid, Limit, Utf8, Protocol, Handshake, Transport(crate::Error), Closed, Worker }
impl std::fmt::Display for Error { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { write!(f,"WebSocket: {self:?}") } }
impl std::error::Error for Error {}
impl std::fmt::Debug for Message {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
    f.debug_struct("Message").field("kind",&self.kind).field("payload_bytes",&self.payload.len()).field("close_code",&self.close_code).finish()}}

pub type Result<T> = std::result::Result<T,Error>;
#[derive(Clone)]
pub struct Completion {
    pub outcome: Result<()>, pub peer_close: Option<Message>, pub transport_eof: bool,
    pub worker_joined: bool, pub incoming_messages: u64, pub outgoing_messages: u64,
    pub incoming_frames: u64, pub outgoing_frames: u64,
    pub incoming_wire_bytes: u64, pub outgoing_wire_bytes: u64,
}
impl std::fmt::Debug for Completion {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
    f.debug_struct("Completion").field("outcome",&self.outcome).field("peer_close",&self.peer_close)
        .field("transport_eof",&self.transport_eof).field("worker_joined",&self.worker_joined)
        .field("incoming_messages",&self.incoming_messages).field("outgoing_messages",&self.outgoing_messages)
        .field("incoming_frames",&self.incoming_frames).field("outgoing_frames",&self.outgoing_frames)
        .field("incoming_wire_bytes",&self.incoming_wire_bytes).field("outgoing_wire_bytes",&self.outgoing_wire_bytes).finish()}}
impl Completion {
    fn empty() -> Self { Self { outcome: Ok(()),peer_close:None,transport_eof:false,worker_joined:false,
        incoming_messages:0,outgoing_messages:0,incoming_frames:0,outgoing_frames:0,
        incoming_wire_bytes:0,outgoing_wire_bytes:0 } }
}
#[derive(Clone)]
pub enum OpeningCleanup { NoWorker, Worker(Completion) }
impl std::fmt::Debug for OpeningCleanup {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {match self {Self::NoWorker=>f.write_str("NoWorker"),Self::Worker(v)=>f.debug_tuple("Worker").field(v).finish()}}}
pub struct OpeningError { pub cause: Error, pub cleanup: OpeningCleanup }
impl std::fmt::Debug for OpeningError { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.debug_struct("OpeningError").field("cause",&self.cause).finish_non_exhaustive() } }
impl std::fmt::Display for OpeningError { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { std::fmt::Display::fmt(&self.cause,f) } }
impl std::error::Error for OpeningError {}
struct Outgoing { message: Message, reply: oneshot::Sender<Result<()>> }
struct Exit { completion: Completion, _permit: OwnedSemaphorePermit }
pub struct WebSocketLease {
    head: Option<ResponseHead>, context: Arc<SendContext>, cancel: CancellationToken,
    incoming: mpsc::Receiver<Message>, outgoing: mpsc::Sender<Outgoing>,
    terminal: watch::Receiver<Option<Completion>>, worker: Option<JoinHandle<Exit>>,
    demands: mpsc::Sender<()>, pending_demand: bool,
}
impl std::fmt::Debug for WebSocketLease {fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
    f.debug_struct("WebSocketLease").field("pending_demand",&self.pending_demand).finish_non_exhaustive()}}
impl WebSocketLease {
    pub fn head(&self)->&ResponseHead { self.head.as_ref().expect("head installed before return") }
    fn check(&self)->Result<()> {
        if self.cancel.is_cancelled() { return Err(Error::Transport(crate::Error::Cancelled)); }
        self.context.check().map_err(|error| { self.cancel.cancel(); Error::Transport(error) })
    }
    pub fn terminal(&self)->Option<Completion> { self.terminal.borrow().clone() }
    pub fn cancel(&self) { self.cancel.cancel(); }
    /// Cancellation-safe receive; decoder state lives in the sole worker.
    pub async fn next_message(&mut self)->Result<Option<Message>> {
        self.check()?;
        if !self.pending_demand {
            if !self.demands.is_closed() { self.demands.try_send(()).map_err(|_|Error::Closed)?; }
            self.pending_demand=true;
        }
        let value = run(&self.context,&self.cancel,async { Ok(self.incoming.recv().await) }).await.map_err(Error::Transport)?;
        self.pending_demand=false;
        self.check()?;
        match value { Some(value)=>Ok(Some(value)),None=>loop {
            if let Some(done)=self.terminal.borrow().as_ref() { return done.outcome.map(|()|None); }
            self.terminal.changed().await.map_err(|_|Error::Worker)?;
        } }
    }
    /// Success means the mature sink flush actually completed, never merely admission.
    pub async fn send_message(&self,message:Message)->Result<()> {
        self.check()?;
        let (reply,recv)=oneshot::channel();
        self.outgoing.try_send(Outgoing {message,reply}).map_err(|_|Error::Limit)?;
        let result=run(&self.context,&self.cancel,async { recv.await.map_err(|_|crate::Error::Closed) }).await.map_err(Error::Transport)?;
        self.check()?;
        result
    }
    pub async fn finish(self)->Completion { self.join(false).await }
    pub async fn cancel_and_wait(self)->Completion { self.join(true).await }
    async fn join(mut self,cancel:bool)->Completion {
        if cancel || self.terminal.borrow().is_none() { self.cancel.cancel(); }
        match self.worker.take().expect("sole worker").await {
            Ok(mut exit)=> { exit.completion.worker_joined=true; exit.completion }
            Err(_)=> { let mut done=self.terminal.borrow().clone().unwrap_or_else(Completion::empty);
                if done.outcome.is_ok() { done.outcome=Err(Error::Worker); } done }
        }
    }
}
impl Drop for WebSocketLease { fn drop(&mut self) { self.cancel.cancel(); } }
/// Explicit host GET opening. Caller must have durably won its dispatch claim.
/// Request target is the exact approved http(s) mapping; policy is still original Client.
pub async fn open(client:&Client,request:RawHttpRequest,subprotocols:Vec<String>,quotas:Quotas,
    mut context:SendContext)->std::result::Result<WebSocketLease,OpeningError> {
    let no_worker=|cause|OpeningError{cause,cleanup:OpeningCleanup::NoWorker};
    quotas.validate().map_err(no_worker)?;
    if request.method!="GET" || !request.body.is_empty() { return Err(no_worker(Error::Invalid)); }
    context.check().map_err(|e|no_worker(Error::Transport(e)))?;
    let (target,mut headers)=client.validate_request(&request).map_err(|e|no_worker(Error::Transport(e)))?;
    validate_protocols(&subprotocols).map_err(no_worker)?;
    for (name,_) in &request.headers {
        if name.to_ascii_lowercase().starts_with("sec-websocket-") || matches!(name.to_ascii_lowercase().as_str(),
            "authorization"|"cookie"|"x-api-key"|"api-key") { return Err(no_worker(Error::Invalid)); }
    }
    let key=tungstenite::handshake::client::generate_key();
    for (name,value) in [("connection","Upgrade"),("upgrade","websocket"),("sec-websocket-version","13"),("sec-websocket-key",key.as_str())] {
        headers.insert(HeaderName::from_static(name),HeaderValue::from_str(value).map_err(|_|no_worker(Error::Invalid))?);
    }
    if !subprotocols.is_empty() { headers.insert(HeaderName::from_static("sec-websocket-protocol"),
        HeaderValue::from_str(&subprotocols.join(", ")).map_err(|_|no_worker(Error::Invalid))?); }
    client.validate_upgrade_headers(&headers).map_err(|e|no_worker(Error::Transport(e)))?;
    let permit=client.try_acquire_upgrade().map_err(|e|no_worker(Error::Transport(e)))?;
    context.cap_deadline(tokio::time::Instant::now()+client.upgrade_timeout());
    let cancel=CancellationToken::new();
    // The source context is independently cancellable; never renew its deadline.
    let context=Arc::new(context);
    let (event_tx,incoming)=mpsc::channel(1);
    let (outgoing,commands)=mpsc::channel(1);
    let (demands,demand_rx)=mpsc::channel(1);
    let (term_tx,terminal)=watch::channel(None);
    let (head_tx,head_rx)=oneshot::channel();
    let worker_client=client.clone(); let worker_context=Arc::clone(&context); let worker_cancel=cancel.clone();
    let worker=tokio::spawn(async move {
        let mut done=Completion::empty();
        let result=worker(&worker_client,target,request,headers,key,subprotocols,quotas,
            &worker_context,&worker_cancel,head_tx,event_tx,commands,demand_rx,&mut done).await;
        done.outcome=result;
        let _=term_tx.send(Some(done.clone()));
        Exit { completion:done,_permit:permit }
    });
    // Own cancellation before awaiting the opening result, as in StreamLease.
    // Dropping the opening future cancels its sole worker without inventing a join.
    let mut lease=WebSocketLease{head:None,context,cancel,incoming,outgoing,terminal,worker:Some(worker),demands,pending_demand:false};
    let opened=match head_rx.await {
        Ok(Ok(head))=>lease.check().map(|()|head),
        Ok(Err(error))=>Err(error),
        Err(_)=>Err(Error::Worker),
    };
    match opened {
        Ok(head)=>{lease.head=Some(head);Ok(lease)},
        Err(cause)=>Err(OpeningError{cause,cleanup:OpeningCleanup::Worker(lease.cancel_and_wait().await)}),
    }
}
pub fn validate_protocols(protocols:&[String])->Result<()> {
    if protocols.len()>16 { return Err(Error::Limit); }
    let mut seen=std::collections::BTreeSet::new();
    for protocol in protocols {
        if protocol.is_empty() || protocol.len()>128 || !protocol.bytes().all(|b| b.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&b))
            || !seen.insert(protocol) { return Err(Error::Invalid); }
    }
    Ok(())
}
fn verify(head:&ResponseHead,key:&str,protocols:&[String])->Result<()> {
    fn one<'a>(head:&'a ResponseHead,name:&str)->Result<Option<&'a [u8]>> {
        let mut matching=head.headers.iter().filter(|(n,_)|n.eq_ignore_ascii_case(name));
        let value=matching.next().map(|(_,v)|v.as_slice());
        if matching.next().is_some() { return Err(Error::Handshake); } Ok(value)
    }
    if head.status!=101 || one(head,"sec-websocket-extensions")?.is_some()
        || one(head,"content-encoding")?.is_some() || one(head,"transfer-encoding")?.is_some()
        || one(head,"content-length")?.is_some() { return Err(Error::Handshake); }
    if !one(head,"upgrade")?.is_some_and(|v|v.eq_ignore_ascii_case(b"websocket")) { return Err(Error::Handshake); }
    let connection=one(head,"connection")?.ok_or(Error::Handshake)?;
    let connection=std::str::from_utf8(connection).map_err(|_|Error::Handshake)?;
    let tokens:Vec<_>=connection.split(',').map(|part|part.trim()).collect();
    if tokens.iter().any(|part|part.is_empty()||!part.bytes().all(|b|b.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&b)))
        || !tokens.iter().any(|part|part.eq_ignore_ascii_case("upgrade")) { return Err(Error::Handshake); }
    let expected=tungstenite::handshake::derive_accept_key(key.as_bytes());
    if one(head,"sec-websocket-accept")?!=Some(expected.as_bytes()) { return Err(Error::Handshake); }
    match one(head,"sec-websocket-protocol")? {
        Some(value) if protocols.iter().any(|p|p.as_bytes()==value)=>{},
        None if protocols.is_empty()=>{}, _=>return Err(Error::Handshake),
    }
    Ok(())
}
// Preserve the context's fixed deadline and authority while also observing the
// lease-local token during opening, sink flush, event delivery and idle waits.
async fn run<T>(context:&SendContext,cancel:&CancellationToken,future:impl Future<Output=crate::Result<T>>)->crate::Result<T> {
    context.run(async {
        tokio::select! { biased; _=cancel.cancelled()=>Err(crate::Error::Cancelled), result=future=>result }
    }).await
}
#[allow(clippy::too_many_arguments)]
async fn worker(client:&Client,target:url::Url,request:RawHttpRequest,headers:reqwest::header::HeaderMap,
    key:String,protocols:Vec<String>,quotas:Quotas,context:&SendContext,cancel:&CancellationToken,
    head_tx:oneshot::Sender<Result<ResponseHead>>,events:mpsc::Sender<Message>,mut commands:mpsc::Receiver<Outgoing>,mut demands:mpsc::Receiver<()>,done:&mut Completion)->Result<()> {
    let opening=run(context,cancel,client.execute_upgrade_head(target,request,headers,context)).await.map_err(Error::Transport);
    let (head,response)=match opening {Ok(v)=>v,Err(e)=>{let _=head_tx.send(Err(e));return Err(e);}};
    if response.version()!=reqwest::Version::HTTP_11 {let _=head_tx.send(Err(Error::Handshake));return Err(Error::Handshake);}
    if let Err(e)=verify(&head,&key,&protocols) {let _=head_tx.send(Err(e));return Err(e);}
    let upgraded=match run(context,cancel,async{response.upgrade().await.map_err(|_|crate::Error::Transport)}).await {
        Ok(v)=>v,Err(e)=>{let e=Error::Transport(e);let _=head_tx.send(Err(e));return Err(e);}
    };
    let meter=Arc::new(Mutex::new(Meters::new(quotas)));
    let socket=Metered {inner:upgraded,meters:Arc::clone(&meter)};
    let config=WebSocketConfig::default().read_buffer_size(4096).write_buffer_size(0)
        .max_write_buffer_size(quotas.max_message_bytes+128).max_message_size(Some(quotas.max_message_bytes))
        .max_frame_size(Some(quotas.max_frame_bytes)).accept_unmasked_frames(false);
    let mut socket=WebSocketStream::from_raw_socket(socket,Role::Client,Some(config)).await;
    if head_tx.send(Ok(head)).is_err() { return Err(Error::Transport(crate::Error::Cancelled)); }
    let result=async {
        enum Activity { In(Option<std::result::Result<tungstenite::Message,tungstenite::Error>>), Out(Option<Outgoing>), Demand(Option<()>) }
        let mut reading=false;
        loop {
            let activity=run(context,cancel,async {
                tokio::select! { biased; _=cancel.cancelled()=>Err(crate::Error::Cancelled),
                    command=commands.recv()=>Ok(Activity::Out(command)), demand=demands.recv(),if !reading=>Ok(Activity::Demand(demand)),
                    message=socket.next(),if reading=>Ok(Activity::In(message)) }
            }).await.map_err(Error::Transport)?;
            match activity {
                Activity::Demand(Some(()))=>{reading=true;},
                Activity::Demand(None)=>return Err(Error::Transport(crate::Error::Cancelled)),
                Activity::Out(Some(command))=> {
                    let result=async {
                        command.message.validate(quotas.max_message_bytes)?;
                        if meter.lock().unwrap_or_else(|e|e.into_inner()).write.messages>=quotas.max_outgoing_messages { return Err(Error::Limit); }
                        let message=command.message.into_ws()?;
                        run(context,cancel,async {socket.send(message).await.map_err(|_|crate::Error::Transport)}).await.map_err(Error::Transport)?;
                        done.outgoing_messages+=1; Ok(())
                    }.await;
                    let result=if meter.lock().unwrap_or_else(|e|e.into_inner()).limited {Err(Error::Limit)}else{result};
                    let _=command.reply.send(result);
                    result?;
                }
                Activity::Out(None)=>return Err(Error::Transport(crate::Error::Cancelled)),
                Activity::In(Some(Ok(message)))=> {
                    reading=false;
                    let message=Message::from_ws(message)?;
                    message.validate(quotas.max_message_bytes)?;
                    if done.incoming_messages>=quotas.max_incoming_messages {return Err(Error::Limit);}
                    done.incoming_messages+=1;
                    if message.kind==MessageKind::Close {done.peer_close=Some(message.clone());}
                    // Flush library-generated Pong/Close under the same authority,
                    // deadline and outgoing frame/wire budget, even while guest ACK waits.
                    run(context,cancel,async {socket.flush().await.map_err(|_|crate::Error::Transport)}).await.map_err(Error::Transport)?;
                    context.check().map_err(Error::Transport)?;
                    // Bounded pending message, never an unbounded worker event queue.
                    run(context,cancel,async {tokio::select!{_=cancel.cancelled()=>Err(crate::Error::Cancelled),
                        result=events.send(message)=>result.map_err(|_|crate::Error::Closed)}}).await.map_err(Error::Transport)?;
                }
                Activity::In(Some(Err(e)))=>return Err(ws_error(e)),
                Activity::In(None)=>return if done.peer_close.is_some(){Ok(())}else{Err(Error::Protocol)},
            }
        }
    }.await;
    drop(socket);
    let meters=meter.lock().unwrap_or_else(|e|e.into_inner());
    done.incoming_frames=meters.read.frames; done.outgoing_frames=meters.write.frames;
    done.outgoing_messages=meters.write.messages;
    done.incoming_wire_bytes=meters.read.bytes;done.outgoing_wire_bytes=meters.write.bytes;
    done.transport_eof=meters.eof;
    if meters.limited { Err(Error::Limit) } else { result }
}
fn ws_error(error:tungstenite::Error)->Error { match error {
    tungstenite::Error::Capacity(_)=>Error::Limit,tungstenite::Error::Utf8(_)=>Error::Utf8,
    tungstenite::Error::Protocol(_)=>Error::Protocol,_=>Error::Transport(crate::Error::Transport),
} }
// This observer only meters frame lengths/counts. It does not decode messages,
// validate protocol flags, unmask, reassemble fragments or implement crypto.
// Partial-header state is retained across all AsyncRead/Write polls.
#[derive(Clone)]
struct Counter { bytes:u64,frames:u64,messages:u64,max_messages:u64,header:[u8;14],used:usize,need:usize,left:u64,max_bytes:u64,max_frames:u64,max_payload:usize }
impl Counter {
    fn new(max_bytes:u64,max_frames:u64,max_messages:u64,max_payload:usize)->Self {Self{bytes:0,frames:0,messages:0,max_messages,header:[0;14],used:0,need:2,left:0,max_bytes,max_frames,max_payload}}
    fn charge(&mut self,mut bytes:&[u8])->io::Result<()> {
        self.bytes=self.bytes.checked_add(bytes.len() as u64).ok_or_else(limited)?;
        if self.bytes>self.max_bytes {return Err(limited());}
        while !bytes.is_empty() {
            if self.left>0 {let take=self.left.min(bytes.len() as u64) as usize;self.left-=take as u64;bytes=&bytes[take..];continue;}
            let take=(self.need-self.used).min(bytes.len());self.header[self.used..self.used+take].copy_from_slice(&bytes[..take]);
            self.used+=take;bytes=&bytes[take..];
            if self.used<self.need {continue;}
            if self.need==2 {
                let extended=match self.header[1]&127 {126=>2,127=>8,_=>0};
                self.need=2+extended+if self.header[1]&128!=0{4}else{0};
                if self.used<self.need {continue;}
            }
            let payload=match self.header[1]&127 {126=>u16::from_be_bytes(self.header[2..4].try_into().expect("length")) as u64,
                127=>u64::from_be_bytes(self.header[2..10].try_into().expect("length")),n=>n as u64};
            self.frames=self.frames.checked_add(1).ok_or_else(limited)?;
            if self.header[0]&15!=0 {
                self.messages=self.messages.checked_add(1).ok_or_else(limited)?;
                if self.messages>self.max_messages {return Err(limited());}
            }
            if payload>self.max_payload as u64 || self.frames>self.max_frames {return Err(limited());}
            self.left=payload;self.used=0;self.need=2;
        }
        Ok(())
    }
}
fn limited()->io::Error {io::Error::other("WebSocket transport quota")}
struct Meters {read:Counter,write:Counter,eof:bool,limited:bool}
impl Meters {fn new(q:Quotas)->Self {Self{read:Counter::new(q.max_incoming_wire_bytes,q.max_incoming_frames,q.max_incoming_messages,q.max_frame_bytes),
    write:Counter::new(q.max_outgoing_wire_bytes,q.max_outgoing_frames,q.max_outgoing_messages,q.max_frame_bytes),eof:false,limited:false}}}
struct Metered {inner:reqwest::Upgraded,meters:Arc<Mutex<Meters>>}
impl AsyncRead for Metered {
    fn poll_read(mut self:Pin<&mut Self>,cx:&mut Context<'_>,buf:&mut ReadBuf<'_>)->Poll<io::Result<()>> {
        let mut bytes=[0u8;4096];let cap=buf.remaining().min(bytes.len());
        if cap==0{return Poll::Ready(Ok(()));}
        let mut part=ReadBuf::new(&mut bytes[..cap]);
        match Pin::new(&mut self.inner).poll_read(cx,&mut part) {
            Poll::Ready(Ok(()))=> {
                let mut meter=self.meters.lock().unwrap_or_else(|e|e.into_inner());
                if part.filled().is_empty(){meter.eof=true;}
                if let Err(e)=meter.read.charge(part.filled()){meter.limited=true;return Poll::Ready(Err(e));}
                buf.put_slice(part.filled());Poll::Ready(Ok(()))
            }
            other=>other,
        }
    }
}
impl AsyncWrite for Metered {
    fn poll_write(mut self:Pin<&mut Self>,cx:&mut Context<'_>,bytes:&[u8])->Poll<io::Result<usize>> {
        {let mut meters=self.meters.lock().unwrap_or_else(|e|e.into_inner());
            let mut candidate=meters.write.clone();if let Err(e)=candidate.charge(bytes){meters.limited=true;return Poll::Ready(Err(e));}}
        match Pin::new(&mut self.inner).poll_write(cx,bytes) {
            Poll::Ready(Ok(n))=> {let mut meters=self.meters.lock().unwrap_or_else(|e|e.into_inner());
                if let Err(e)=meters.write.charge(&bytes[..n]){meters.limited=true;return Poll::Ready(Err(e));}Poll::Ready(Ok(n))}
            other=>other,
        }
    }
    fn poll_flush(mut self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {Pin::new(&mut self.inner).poll_flush(cx)}
    fn poll_shutdown(mut self:Pin<&mut Self>,cx:&mut Context<'_>)->Poll<io::Result<()>> {Pin::new(&mut self.inner).poll_shutdown(cx)}
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[tokio::test]
    async fn lease_local_cancel_rejects_already_buffered_message() {
        let (events,incoming)=mpsc::channel(1);
        events.send(Message {kind:MessageKind::Text,payload:b"buffered".to_vec(),close_code:None}).await.unwrap();
        let (outgoing,_commands)=mpsc::channel(1);let (demands,_demand_rx)=mpsc::channel(1);
        let (_terminal_tx,terminal)=watch::channel(None);
        let mut lease=WebSocketLease {head:None,
            context:Arc::new(SendContext::trusted(tokio::time::Instant::now()+std::time::Duration::from_secs(30),CancellationToken::new())),
            cancel:CancellationToken::new(),incoming,outgoing,terminal,worker:None,demands,pending_demand:true};
        lease.cancel();
        assert!(matches!(lease.next_message().await,Err(Error::Transport(crate::Error::Cancelled))));
        assert_eq!(lease.incoming.len(),1,"cancel must not consume or deliver buffered private payload");
    }
    #[tokio::test]
    async fn lease_local_cancel_interrupts_in_progress_transport_wait() {
        let context=SendContext::trusted(tokio::time::Instant::now()+std::time::Duration::from_secs(30),CancellationToken::new());
        let cancel=CancellationToken::new();let other=cancel.clone();
        let wait=run(&context,&cancel,async {
            other.cancel();
            std::future::pending::<crate::Result<()>>().await
        });
        let outcome=tokio::time::timeout(std::time::Duration::from_millis(200),wait).await.expect("local cancellation wakes active IO");
        assert_eq!(outcome,Err(crate::Error::Cancelled));
    }
}
