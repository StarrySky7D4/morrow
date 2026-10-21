//! Explicit v8 runtime adapter. Holds one attempt's bounded historical protocol
//! state, never a process/pipe handle or a product cleanup capability.
use super::native_close_protocol::*;
use crate::ipc_framed::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration,Instant};

fn rejected() -> io::Error { io::Error::new(io::ErrorKind::InvalidData, "v8 native-close transport rejected; no replay") }
fn codec<T>(r: Result<T, NativeCloseCodecError>) -> io::Result<T> { r.map_err(|error|match error {
    NativeCloseCodecError::StreamIoError{os_error:Some(code)}=>io::Error::from_raw_os_error(code),_=>rejected(),
}) }
fn protocol<T>(r: Result<T, NativeCloseProtocolError>) -> io::Result<T> { r.map_err(|_| rejected()) }

/// Distinct RUNNER identity, not the business child's identity. Startup captured
/// from the original handle by host and original self pseudo-handle by runner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRunnerIdentity {
    pub process_id: u32,
    pub thread_id: u32,
    pub creation_filetime_100ns: u64,
}
impl RuntimeRunnerIdentity {
    pub fn verified(self) -> io::Result<VerifiedChildProcessIdentity> {
        protocol(VerifiedChildProcessIdentity::try_new(self.process_id, self.thread_id, self.creation_filetime_100ns))
    }
    /// Startup-only: no OpenProcess, PID lookup, duplicate or late native query.
    pub fn capture_current_startup() -> io::Result<Self> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetProcessTimes};
        let zero = FILETIME {dwLowDateTime:0, dwHighDateTime:0};
        let (mut creation,mut exit,mut kernel,mut user)=(zero,zero,zero,zero);
        if unsafe { GetProcessTimes(GetCurrentProcess(), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let value = Self {process_id:unsafe {GetCurrentProcessId()}, thread_id:unsafe {GetCurrentThreadId()},
            creation_filetime_100ns:((creation.dwHighDateTime as u64)<<32)|creation.dwLowDateTime as u64};
        value.verified()?;Ok(value)
    }
}

/// Private bootstrap seed traverses only the already authenticated original named
/// pipe. The actual SessionNonce is derived with runner identity/attempt/schema
/// and is NEVER serialized, put in argv, debug-printed or returned publicly.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBootstrapWire {
    pub schema: [u8;32],
    pub attempt: u64,
    pub runner: RuntimeRunnerIdentity,
    pub seed: [u8;32],
    pub hello: NativeCloseHandshakeHelloWire,
    pub terminal_close_ack_required: bool,
}
impl fmt::Debug for RuntimeBootstrapWire {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("RuntimeBootstrapWire([REDACTED_PRIVATE_BOOTSTRAP])")}
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum RuntimeDirection { HostToRunner, RunnerToHost }
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePacketWire {
    pub schema: [u8;32],
    pub attempt: u64,
    pub sequence: u64,
    pub direction: RuntimeDirection,
    pub event: Option<Box<Version8WireFrame>>,
    pub content: Value,
    /// Present only on the final receipt, binding the ACK actually authenticated
    /// by the original input reader. Not an ACK-writer join or close assertion.
    pub terminal_close_ack: Option<WireAuthTag>,
    pub tag: WireAuthTag,
}

#[derive(Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalAckBody {
    child:WireChildIdentity,
    raw_child_exit_dword:u32,
}
#[derive(Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalAckContent { native_close_ack:TerminalAckBody }
#[derive(Default)]
struct TerminalState {
    exit:Option<i32>,deadline:Option<Instant>,
    ack_attempted:bool,ack_written:bool,ack_received:bool,
    ack_tag:Option<WireAuthTag>,
}
impl fmt::Debug for RuntimePacketWire {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.debug_struct("RuntimePacketWire").field("sequence",&self.sequence)
        .field("direction",&self.direction).finish_non_exhaustive()}
}

pub struct NativeCloseRuntime {
    binding: StartupAttemptBinding,
    runner: RuntimeRunnerIdentity,
    role: RuntimeDirection,
    send_sequence: AtomicU64,
    receive_sequence: AtomicU64,
    controls_written: AtomicU64,
    failed: AtomicBool,
    physical_eof: AtomicBool,
    receipt_attempted: AtomicBool,
    child: Mutex<Option<VerifiedChildProcessIdentity>>,
    receiver: Mutex<NativeCloseReceiverState>,
    terminal:Mutex<TerminalState>,
}
impl fmt::Debug for NativeCloseRuntime {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.debug_struct("NativeCloseRuntime").field("runner",&self.runner)
        .field("attempt",&self.binding.startup_attempt_id()).finish_non_exhaustive()}
}
fn binding(seed:[u8;32],attempt:u64,runner:RuntimeRunnerIdentity)->io::Result<StartupAttemptBinding> {
    runner.verified()?;
    if seed==[0;32] {return Err(rejected());}
    let mut digest=Sha256::new();digest.update(b"c28.v8.live.original-runner-binding\0");digest.update(seed);
    digest.update(attempt.to_le_bytes());digest.update(runner.process_id.to_le_bytes());digest.update(runner.thread_id.to_le_bytes());
    digest.update(runner.creation_filetime_100ns.to_le_bytes());digest.update(NATIVE_CLOSE_PROTOCOL_SCHEMA);
    protocol(StartupAttemptBinding::try_new(protocol(StartupAttemptId::try_new(attempt))?,
        protocol(SessionNonce::try_new(digest.finalize().into()))?,SchemaDigest::v8_native_close(),NativeCloseCapabilityOptIn::ExplicitlyOptedInV8))
}
impl NativeCloseRuntime {
    fn new(binding:StartupAttemptBinding,runner:RuntimeRunnerIdentity,role:RuntimeDirection)->io::Result<Arc<Self>> {
        let mut receiver=NativeCloseReceiverState::new(binding.clone());
        protocol(receiver.bind_original_runner_identity(runner.verified()?))?;
        protocol(receiver.record_handshake_hello(&NativeCloseHandshakeHelloWire::new_signed(&binding)))?;
        Ok(Arc::new(Self {binding,runner,role,send_sequence:AtomicU64::new(0),receive_sequence:AtomicU64::new(0),controls_written:AtomicU64::new(0),failed:AtomicBool::new(false),physical_eof:AtomicBool::new(false),receipt_attempted:AtomicBool::new(false),
            child:Mutex::new(None),receiver:Mutex::new(receiver),terminal:Mutex::new(TerminalState::default())}))
    }
    pub fn host(seed:[u8;32],attempt:u64,runner:RuntimeRunnerIdentity)->io::Result<(Arc<Self>,Version8WireFrame)> {
        let binding=binding(seed,attempt,runner)?;
        let bootstrap=RuntimeBootstrapWire {schema:NATIVE_CLOSE_PROTOCOL_SCHEMA,attempt,runner,seed,
            hello:NativeCloseHandshakeHelloWire::new_signed(&binding),terminal_close_ack_required:true};
        Ok((Self::new(binding,runner,RuntimeDirection::HostToRunner)?,
            Version8WireFrame::new(Version8Message::RuntimeBootstrap(Box::new(bootstrap)))))
    }
    pub fn runner_bootstrap(frame:Version8WireFrame,original:RuntimeRunnerIdentity)->io::Result<Arc<Self>> {
        if frame.version!=NATIVE_CLOSE_IPC_PROTOCOL_VERSION {return Err(rejected());}
        let Version8Message::RuntimeBootstrap(b)=frame.message else {return Err(rejected())};
        if b.schema!=NATIVE_CLOSE_PROTOCOL_SCHEMA || b.runner!=original || !b.terminal_close_ack_required {return Err(rejected());}
        let binding=binding(b.seed,b.attempt,original)?;
        if b.hello!=NativeCloseHandshakeHelloWire::new_signed(&binding) {return Err(rejected());}
        Self::new(binding,original,RuntimeDirection::RunnerToHost)
    }
    pub fn control_hello(&self)->ControlHello {ControlHello {nonce:*self.binding.session_nonce().as_bytes(),schema:CHECKED_CONTROL_SCHEMA}}
    pub fn controls_written(&self)->io::Result<u16> {self.controls_written.load(Ordering::SeqCst).try_into().map_err(|_| rejected())}
    pub fn attempt(&self)->u64 {self.binding.startup_attempt_id().get()}
    pub fn runner_identity(&self)->RuntimeRunnerIdentity {self.runner}
    pub fn is_sealed(&self)->bool {self.failed.load(Ordering::SeqCst)}
    pub fn physical_eof_seen(&self)->bool {self.physical_eof.load(Ordering::SeqCst)}
    /// Original runner shutdown budget is installed before its one Exit writer.
    /// Once armed it cannot be renewed, even on a later explicit observation.
    pub fn arm_terminal_deadline(&self,deadline:Instant)->io::Result<()> {
        let mut state=self.terminal.try_lock().map_err(|_|rejected())?;
        if state.deadline.is_some() || state.exit.is_some() || Instant::now()>=deadline {
            drop(state);self.latch_transport_failure();return Err(rejected());
        }
        state.deadline=Some(deadline);Ok(())
    }
    fn record_terminal_exit(&self,exit:i32)->io::Result<()> {
        let mut state=self.terminal.try_lock().map_err(|_|rejected())?;
        if state.exit.is_some(){return Err(rejected());}
        state.exit=Some(exit);
        // Host starts its ONE terminal budget on authenticated Exit, not after
        // receipt/EOF. Simulated peers use the same bounded fallback; the actual
        // runner arms its existing original shutdown deadline before writing.
        if state.deadline.is_none(){state.deadline=Some(Instant::now()+Duration::from_secs(30));}
        if Instant::now()>=state.deadline.unwrap(){return Err(rejected());}Ok(())
    }
    pub fn terminal_deadline(&self)->io::Result<Instant> {
        self.terminal.try_lock().map_err(|_|rejected())?.deadline.ok_or_else(rejected)
    }
    pub fn terminal_ack_received(&self)->bool {
        !self.is_sealed() && self.terminal.try_lock().is_ok_and(|s|s.ack_received && s.ack_tag.is_some())
    }
    pub(crate) fn terminal_ack_written(&self)->bool {
        !self.is_sealed() && self.terminal.try_lock().is_ok_and(|s|s.ack_written && s.ack_tag.is_some())
    }
    fn latch_transport_failure(&self) {
        self.failed.store(true,Ordering::SeqCst);
        if let Ok(mut receiver)=self.receiver.try_lock() {receiver.record_external_sticky_unknown(ProtocolStickyFailureKind::RuntimeTransportFailed);}
    }

    pub fn set_child(&self,child:VerifiedChildProcessIdentity)->io::Result<()> {
        let mut slot=self.child.try_lock().map_err(|_| rejected())?;
        if slot.is_some() {return Err(rejected());}*slot=Some(child);Ok(())
    }
    fn packet_tag(&self,p:&RuntimePacketWire)->io::Result<WireAuthTag> {
        let body=serde_json::to_vec(&(p.schema,p.attempt,p.sequence,p.direction,&p.event,&p.content,&p.terminal_close_ack)).map_err(|_| rejected())?;
        let mut h=Sha256::new();h.update(b"c28.v8.live.packet\0");h.update(self.binding.session_nonce().as_bytes());
        h.update((body.len() as u64).to_le_bytes());h.update(body);h.update(self.binding.session_nonce().as_bytes());
        Ok(WireAuthTag::from_bytes(h.finalize().into()))
    }
    fn strip_secrets(value:&mut Value) {
        if let Some(payload)=value.get_mut("payload") {
            if let Some(hello)=payload.get_mut("hello").and_then(Value::as_object_mut) {hello.remove("nonce");}
            if let Some(request)=payload.get_mut("request").and_then(Value::as_object_mut) {request.remove("nonce");}
            if let Some(object)=payload.as_object_mut() {object.remove("nonce");}
        }
    }
    fn restore_secrets(&self,value:&mut Value)->io::Result<()> {
        let kind=value.get("type").and_then(Value::as_str).ok_or_else(rejected)?.to_owned();
        let payload=value.get_mut("payload").ok_or_else(rejected)?;
        let nonce=serde_json::to_value(self.control_hello().nonce).map_err(|_| rejected())?;
        match kind.as_str() {
            "hello_ack"=>{payload.get_mut("hello").and_then(Value::as_object_mut).ok_or_else(rejected)?.insert("nonce".into(),nonce);}
            "control_result"=>{payload.get_mut("request").and_then(Value::as_object_mut).ok_or_else(rejected)?.insert("nonce".into(),nonce);}
            "control_request"=>{payload.as_object_mut().ok_or_else(rejected)?.insert("nonce".into(),nonce);}
            _=>{}
        }Ok(())
    }
    fn event(&self,message:&Message)->io::Result<Option<Version8WireFrame>> {
        let event=match message {
            Message::HelloAck{payload}=>{
                if payload.hello!=self.control_hello() || !payload.controls.close_stdin || !payload.controls.resize {return Err(rejected());}
                Version8Message::HandshakeAck(NativeCloseHandshakeAckWire::new_signed(&self.binding))
            }
            Message::SpawnReady{payload}=>{
                let child=self.child.try_lock().map_err(|_| rejected())?.ok_or_else(rejected)?;
                if child.pid.get()!=payload.process_id {return Err(rejected());}
                Version8Message::SpawnReady(NativeCloseSpawnReadyWire::new_signed(&self.binding,child))
            }
            Message::Output{payload}=>Version8Message::Output(NativeCloseOutputFrameWire {
                stream:match payload.stream {OutputStream::Stdout=>NativeCloseOutputStream::Stdout,OutputStream::Stderr=>NativeCloseOutputStream::Stderr},
                byte_len:decode_bytes(&payload.data_b64).map_err(|_| rejected())?.len().try_into().map_err(|_| rejected())?}),
            Message::ControlResult{payload}=>Version8Message::ControlResult(NativeCloseControlResultWire {request_id:payload.request.id,status:payload.status}),
            Message::Exit{payload}=>Version8Message::BusinessExit(NativeCloseBusinessExitWire::from_exit_payload(payload)),
            Message::Error{..}=>return Ok(None),
            _=>return Err(rejected()),
        };Ok(Some(Version8WireFrame::new(event)))
    }
    fn send_packet<W:Write>(&self,w:&mut W,content:Value,event:Option<Version8WireFrame>)->io::Result<()> {
        if self.failed.load(Ordering::SeqCst) {return Err(rejected());}
        let sequence=self.send_sequence.fetch_add(1,Ordering::SeqCst).checked_add(1).ok_or_else(rejected)?;
        if sequence>u64::from(MAX_TOTAL_FRAMES_PER_ATTEMPT)+MAX_CONTROL_FRAMES_PER_ATTEMPT as u64 {return Err(rejected());}
        let mut packet=RuntimePacketWire {schema:NATIVE_CLOSE_PROTOCOL_SCHEMA,attempt:self.attempt(),sequence,
            direction:self.role,event:event.map(Box::new),content,terminal_close_ack:None,tag:WireAuthTag::from_bytes([0;32])};
        if packet.event.as_ref().is_some_and(|e|matches!(e.message,Version8Message::NativeCloseReceipt(_))) {
            let terminal=self.terminal.try_lock().map_err(|_|rejected())?;
            if !terminal.ack_received || !self.physical_eof_seen(){return Err(rejected());}
            packet.terminal_close_ack=Some(terminal.ack_tag.clone().ok_or_else(rejected)?);
        }
        packet.tag=self.packet_tag(&packet)?;
        let written=codec(write_v8_wire_frame_bounded(w,&Version8WireFrame::new(Version8Message::RuntimePacket(Box::new(packet))))).map(|_| ());
        if written.is_err() {self.latch_transport_failure();}written
    }
    pub fn write_message<W:Write>(&self,w:&mut W,message:&FramedMessage)->io::Result<()> {
        if self.failed.load(Ordering::SeqCst) {return Err(rejected());}
        let written=self.write_message_inner(w,message);
        if written.is_err() {self.latch_transport_failure();}written
    }
    fn write_message_inner<W:Write>(&self,w:&mut W,message:&FramedMessage)->io::Result<()> {
        if self.role==RuntimeDirection::HostToRunner && self.terminal.try_lock().map_err(|_|rejected())?.ack_attempted {
            return Err(rejected());
        }
        if self.role==RuntimeDirection::RunnerToHost {
            if let Message::Exit{payload}=&message.message {self.record_terminal_exit(payload.exit_code)?;}
        }
        let event=if self.role==RuntimeDirection::RunnerToHost {self.event(&message.message)?} else {
            if !matches!(message.message,Message::SpawnRequest{..}|Message::Stdin{..}|Message::ControlRequest{..}|Message::Terminate{..}) {return Err(rejected());}None
        };
        let mut frame=message.clone();frame.version=NATIVE_CLOSE_IPC_PROTOCOL_VERSION;
        if let Message::Error{payload}=&mut frame.message {
            // Only exact finite diagnostic atoms may cross the authenticated
            // wire. All free bodies, including near-matches, are still redacted.
            payload.message=crate::RunnerStartupSite::from_wire_message(&payload.message)
                .wire_message().to_owned();
        }
        let mut value=serde_json::to_value(&frame).map_err(|_| rejected())?;Self::strip_secrets(&mut value);
        self.send_packet(w,value,event)?;
        if matches!(message.message, Message::ControlResult{..}) {self.controls_written.fetch_add(1,Ordering::SeqCst);}
        Ok(())
    }
    pub fn write_receipt<W:Write>(&self,w:&mut W,facts:ReportedNativeCloseFacts)->io::Result<()> {
        if self.role!=RuntimeDirection::RunnerToHost || self.receipt_attempted.swap(true,Ordering::SeqCst) {
            self.latch_transport_failure();return Err(rejected());
        }
        let result=self.send_packet(w,Value::Null,Some(Version8WireFrame::new(Version8Message::NativeCloseReceipt(
            Box::new(NativeCloseReceiptWire::new_signed(&self.binding,facts))))));
        if result.is_err(){self.latch_transport_failure();}result
    }
    /// Exactly once, on the SAME original serialized host writer. Success means
    /// ACK bytes flushed only. Its original join and checked close remain later
    /// independent obligations and are never claimed inside this ACK.
    pub fn write_terminal_close_ack<W:Write>(&self,w:&mut W)->io::Result<()> {
        let result=(|| {
            if self.role!=RuntimeDirection::HostToRunner || self.is_sealed(){return Err(rejected());}
            let child=self.child.try_lock().map_err(|_|rejected())?.ok_or_else(rejected)?;
            let (exit,deadline)={let mut state=self.terminal.try_lock().map_err(|_|rejected())?;
                if state.ack_attempted || state.ack_written || state.ack_received{return Err(rejected());}
                state.ack_attempted=true;
                (state.exit.ok_or_else(rejected)?,state.deadline.ok_or_else(rejected)?)
            };
            if Instant::now()>=deadline{return Err(io::Error::new(io::ErrorKind::TimedOut,"original terminal ACK deadline"));}
            let content=serde_json::to_value(TerminalAckContent {native_close_ack:TerminalAckBody {
                child:child.to_wire(),raw_child_exit_dword:exit as u32}}).map_err(|_|rejected())?;
            // Compute the same exact authenticated packet tag that send_packet
            // will write; only this original writer owns the send sequence.
            let mut packet=RuntimePacketWire {schema:NATIVE_CLOSE_PROTOCOL_SCHEMA,attempt:self.attempt(),
                sequence:self.send_sequence.load(Ordering::SeqCst).checked_add(1).ok_or_else(rejected)?,
                direction:self.role,event:None,content:content.clone(),terminal_close_ack:None,tag:WireAuthTag::from_bytes([0;32])};
            packet.tag=self.packet_tag(&packet)?;
            self.send_packet(w,content,None)?;w.flush()?;
            if Instant::now()>=deadline{return Err(io::Error::new(io::ErrorKind::TimedOut,"original terminal ACK completed late"));}
            let mut state=self.terminal.try_lock().map_err(|_|rejected())?;
            state.ack_tag=Some(packet.tag);state.ack_written=true;Ok(())
        })();
        if result.is_err(){self.latch_transport_failure();}result
    }
    /// None is ONLY physical EOF. Some(None) is authenticated metadata: terminal
    /// ACK on runner input, or receipt on host output. Neither is synthetic EOF.
    pub fn read_message<R:Read>(&self,r:&mut R)->io::Result<Option<Option<FramedMessage>>> {
        if self.failed.load(Ordering::SeqCst) {return Err(rejected());}
        let observed=self.read_message_inner(r);
        if observed.is_err() {self.latch_transport_failure();}observed
    }
    fn read_message_inner<R:Read>(&self,r:&mut R)->io::Result<Option<Option<FramedMessage>>> {
        let Some(frame)=codec(read_v8_wire_frame_bounded(r))? else {
            self.physical_eof.store(true,Ordering::SeqCst);
            if self.role==RuntimeDirection::HostToRunner {
                protocol(self.receiver.try_lock().map_err(|_| rejected())?.record_physical_eof())?;
            }else if !self.terminal_ack_received(){return Err(rejected());}
            return Ok(None);
        };
        if frame.version!=NATIVE_CLOSE_IPC_PROTOCOL_VERSION {return Err(rejected());}
        let Version8Message::RuntimePacket(p)=frame.message else {return Err(rejected())};
        let expected=self.receive_sequence.load(Ordering::SeqCst).checked_add(1).ok_or_else(rejected)?;
        if p.schema!=NATIVE_CLOSE_PROTOCOL_SCHEMA || p.attempt!=self.attempt() || p.sequence!=expected
            || p.direction==self.role || self.packet_tag(&p)?!=p.tag {return Err(rejected());}
        let result=if p.content.get("native_close_ack").is_some() {
            if self.role!=RuntimeDirection::RunnerToHost || p.event.is_some() || p.terminal_close_ack.is_some() {return Err(rejected());}
            let body:TerminalAckContent=serde_json::from_value(p.content.clone()).map_err(|_|rejected())?;
            if serde_json::to_value(&body).map_err(|_|rejected())?!=p.content{return Err(rejected());}
            let child=self.child.try_lock().map_err(|_|rejected())?.ok_or_else(rejected)?;
            let mut state=self.terminal.try_lock().map_err(|_|rejected())?;
            if state.ack_received || state.ack_attempted || state.exit.map(|v|v as u32)!=Some(body.native_close_ack.raw_child_exit_dword)
                || child.to_wire()!=body.native_close_ack.child || Instant::now()>=state.deadline.ok_or_else(rejected)? {return Err(rejected());}
            state.ack_received=true;state.ack_tag=Some(p.tag.clone());None
        }else if p.content.is_null() {
            if self.role!=RuntimeDirection::HostToRunner {return Err(rejected());}
            {let state=self.terminal.try_lock().map_err(|_|rejected())?;
                if !state.ack_written || p.terminal_close_ack!=state.ack_tag
                    || Instant::now()>=state.deadline.ok_or_else(rejected)? {return Err(rejected());}
            }
            let event=p.event.as_ref().ok_or_else(rejected)?;
            if !matches!(event.message,Version8Message::NativeCloseReceipt(_)) {return Err(rejected());}
            protocol(self.receiver.try_lock().map_err(|_| rejected())?.apply_wire_frame(event))?;
            None
        }else{
            if p.terminal_close_ack.is_some(){return Err(rejected());}
            if self.role==RuntimeDirection::RunnerToHost && self.terminal.try_lock().map_err(|_|rejected())?.ack_received {return Err(rejected());}
            let mut restored=p.content.clone();self.restore_secrets(&mut restored)?;
            let message:FramedMessage=serde_json::from_value(restored).map_err(|_| rejected())?;
            if message.version!=NATIVE_CLOSE_IPC_PROTOCOL_VERSION {return Err(rejected());}
            let mut exact=serde_json::to_value(&message).map_err(|_| rejected())?;Self::strip_secrets(&mut exact);
            if exact!=p.content {return Err(rejected());} // unknown/default-omitted fields rejected
            if self.role==RuntimeDirection::HostToRunner {
                if let Some(event)=&p.event {
                    if let Version8Message::SpawnReady(ready)=&event.message {
                        let child=protocol(ready.child_identity.validate())?;self.set_child(child)?;
                    }
                    if self.event(&message.message)?.as_ref()!=Some(event.as_ref()) {return Err(rejected());}
                    protocol(self.receiver.try_lock().map_err(|_| rejected())?.apply_wire_frame(event))?;
                    if let Message::Exit{payload}=&message.message {self.record_terminal_exit(payload.exit_code)?;}
                }else if !matches!(message.message,Message::Error{..}) {return Err(rejected());}
            }else if p.event.is_some() || !matches!(message.message,Message::SpawnRequest{..}|Message::Stdin{..}|Message::ControlRequest{..}|Message::Terminate{..}) {
                return Err(rejected());
            }
            Some(message)
        };
        self.receive_sequence.store(expected,Ordering::SeqCst);Ok(Some(result))
    }
    pub(crate) fn observe_physical_close(&self,facts:ReceiverPhysicalCloseFacts)->io::Result<()> {
        let mut receiver=self.receiver.try_lock().map_err(|_| rejected())?;
        if facts.host_keeper_and_desktop_lease!=LeaseRetirementFact::RetiredCleanly
            || facts.host_local_registry_quota_lease!=LeaseRetirementFact::RetiredCleanly {
            protocol(receiver.record_host_native_close_pending_outer(facts))?;
        }else{protocol(receiver.record_receiver_and_runner_native_close(facts))?;}
        Ok(())
    }
    pub fn public_projection(&self)->io::Result<NativeClosePublicProjection> {
        let mut receiver=self.receiver.try_lock().map_err(|_| rejected())?;
        if self.failed.load(Ordering::SeqCst) {receiver.record_external_sticky_unknown(ProtocolStickyFailureKind::RuntimeTransportFailed);}
        Ok(receiver.public_projection())
    }
}

pub fn write_runtime_message<W:Write>(w:&mut W,frame:&FramedMessage,runtime:Option<&NativeCloseRuntime>)->anyhow::Result<()> {
    match runtime {Some(runtime)=>Ok(runtime.write_message(w,frame)?),None=>write_frame(w,frame)}
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use super::super::native_close_protocol::tests::sample_clean_facts;
    // Explicit synthetic peer fixtures. No OS runner/child or release capability
    // is fabricated: these exercise the SAME live codec/FSM called by production.
    fn runner()->RuntimeRunnerIdentity {RuntimeRunnerIdentity {process_id:101,thread_id:102,creation_filetime_100ns:103}}
    fn child()->VerifiedChildProcessIdentity {VerifiedChildProcessIdentity::try_new(201,202,203).unwrap()}
    fn pair()->(Arc<NativeCloseRuntime>,Arc<NativeCloseRuntime>) {
        let (host,bootstrap)=NativeCloseRuntime::host([27;32],271,runner()).unwrap();
        let guest=NativeCloseRuntime::runner_bootstrap(bootstrap,runner()).unwrap();(host,guest)
    }
    fn streamed(raw:u32,receipt:bool)->(Arc<NativeCloseRuntime>,Arc<NativeCloseRuntime>,Vec<u8>,Vec<Option<FramedMessage>>) {
        let (host,guest)=pair();let mut bytes=Vec::new();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
            hello:guest.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}}).unwrap();
        guest.set_child(child()).unwrap();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::SpawnReady {payload:SpawnReady {
            process_id:child().pid.get(),controls:RunnerControlCapabilities {close_stdin:true,resize:false}}}}).unwrap();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::Output {payload:OutputPayload {
            stream:OutputStream::Stdout,data_b64:encode_bytes(b"exact output\0bytes")}}}).unwrap();
        let request=ControlRequest::new(1,guest.control_hello().nonce,ControlOperation::CloseStdin,0,0);
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::ControlResult {payload:
            ControlResult::new(request,ControlStatus::Applied,None)}}).unwrap();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::Exit {payload:ExitPayload {
            exit_code:raw as i32,timed_out:false}}}).unwrap();
        assert!(!bytes.windows(b"\"nonce\"".len()).any(|part|part==b"\"nonce\""));
        let mut prefix=Cursor::new(bytes);let mut seen=Vec::new();
        for _ in 0..5 {seen.push(host.read_message(&mut prefix).unwrap().unwrap());}
        assert_eq!(prefix.position(),prefix.get_ref().len() as u64);
        let mut ack=Vec::new();host.write_terminal_close_ack(&mut ack).unwrap();
        let mut ack=Cursor::new(ack);
        assert!(matches!(guest.read_message(&mut ack),Ok(Some(None))));
        assert!(guest.terminal_ack_received());assert!(!guest.physical_eof_seen());
        assert!(guest.read_message(&mut ack).unwrap().is_none());
        let mut bytes=Vec::new();
        if receipt {guest.write_receipt(&mut bytes,sample_clean_facts(271,child(),raw as i32,1)).unwrap();}
        (host,guest,bytes,seen)
    }
    fn physical(raw:u32)->ReceiverPhysicalCloseFacts {
        ReceiverPhysicalCloseFacts {
            reader_worker_joined_timely_clean:ObservationProof::ProvenOnOriginal,
            writer_worker_joined_timely_clean:ObservationProof::ProvenOnOriginal,
            input_worker_joined_timely_clean:ObservationProof::ProvenOnOriginal,
            runner_process_identity:Some(runner().verified().unwrap()),runner_exit_code_dword:Some(RawProcessExitCodeDword(raw)),
            runner_handle_closed_verified:ObservationProof::ProvenOnOriginal,
            host_input_writer_lease:LeaseRetirementFact::RetiredCleanly,
            host_keeper_and_desktop_lease:LeaseRetirementFact::NotObserved,
            host_local_registry_quota_lease:LeaseRetirementFact::ActiveOrLoaned,
            external_quota_scope:ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
        }
    }
    fn drain(host:&NativeCloseRuntime,bytes:Vec<u8>)->io::Result<Vec<Option<FramedMessage>>> {
        let mut reader=Cursor::new(bytes);let mut seen=Vec::new();
        while let Some(frame)=host.read_message(&mut reader)? {seen.push(frame);}
        Ok(seen)
    }
    fn awaiting_ack(raw:u32)->(Arc<NativeCloseRuntime>,Arc<NativeCloseRuntime>,Vec<u8>) {
        let (host,guest)=pair();let mut bytes=Vec::new();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
            hello:guest.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}}).unwrap();
        guest.set_child(child()).unwrap();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::SpawnReady {payload:SpawnReady {
            process_id:child().pid.get(),controls:RunnerControlCapabilities {close_stdin:true,resize:false}}}}).unwrap();
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::Exit {payload:ExitPayload {exit_code:raw as i32,timed_out:false}}}).unwrap();
        let mut input=Cursor::new(bytes);for _ in 0..3 {assert!(host.read_message(&mut input).unwrap().unwrap().is_some());}
        let mut ack=Vec::new();host.write_terminal_close_ack(&mut ack).unwrap();(host,guest,ack)
    }
    #[test]
    fn terminal_ack_binds_attempt_child_full_dword_schema_direction_sequence_and_authentication() {
        for raw in [0u32,7,256,259,u32::MAX] {
            let (host,guest,ack)=awaiting_ack(raw);
            assert!(host.terminal_ack_written());assert!(!guest.terminal_ack_received());
            assert!(matches!(guest.read_message(&mut Cursor::new(ack)),Ok(Some(None))));
            assert!(guest.terminal_ack_received());assert!(!guest.physical_eof_seen());
            // ACK never says its own writer joined, nor invents receiver/native
            // close. Only original subsequent EOF and outer operations can do so.
            assert!(!host.public_projection().unwrap().local_registry_slot_releasable);
            assert!(guest.read_message(&mut Cursor::new(Vec::<u8>::new())).unwrap().is_none());
        }
        for kind in 0..10 {
            let (host,guest,ack)=awaiting_ack(u32::MAX);
            let mut frame=read_v8_wire_frame_bounded(&mut Cursor::new(ack)).unwrap().unwrap();
            let Version8Message::RuntimePacket(p)=&mut frame.message else{unreachable!()};
            match kind {
                0=>p.attempt+=1,
                1=>p.content["native_close_ack"]["child"]["pid"]=serde_json::json!(999),
                2=>p.content["native_close_ack"]["raw_child_exit_dword"]=serde_json::json!(0),
                3=>p.direction=RuntimeDirection::RunnerToHost,
                4=>p.schema=*b"morrow.c28.native-close.schema.\x08",
                5=>{},6=>p.sequence+=1,
                7=>p.content["unreviewed"]=Value::Bool(true),
                8=>p.content["native_close_ack"]["child"]["thread_id"]=serde_json::json!(999),
                _=>p.content["native_close_ack"]["child"]["creation_filetime_100ns"]=serde_json::json!(999),
            }
            p.tag=if kind==5 {WireAuthTag::from_bytes([0;32])}else{host.packet_tag(p).unwrap()};
            assert!(guest.read_message(&mut Cursor::new(encode_v8_length_prefixed_frame(&frame).unwrap())).is_err());
            assert!(guest.is_sealed());assert!(!guest.terminal_ack_received());
        }
    }
    #[test]
    fn terminal_ack_missing_early_duplicate_replayed_and_late_are_not_eof_or_receipt_success() {
        let (_,guest,ack)=awaiting_ack(7);
        assert!(matches!(guest.read_message(&mut Cursor::new(ack)),Ok(Some(None))));
        assert!(!guest.physical_eof_seen());
        let mut premature=Vec::new();
        assert!(guest.write_receipt(&mut premature,sample_clean_facts(271,child(),7,0)).is_err());
        assert!(premature.is_empty());assert!(guest.is_sealed());
        let (_,guest,_)=awaiting_ack(7);
        assert!(guest.read_message(&mut Cursor::new(Vec::<u8>::new())).is_err());
        assert!(guest.physical_eof_seen());assert!(!guest.terminal_ack_received());
        assert!(guest.write_receipt(&mut Vec::new(),sample_clean_facts(271,child(),7,0)).is_err());
        let (host,guest)=pair();guest.set_child(child()).unwrap();
        let mut early=Vec::new();
        host.send_packet(&mut early,serde_json::to_value(TerminalAckContent {native_close_ack:TerminalAckBody {
            child:child().to_wire(),raw_child_exit_dword:7}}).unwrap(),None).unwrap();
        assert!(guest.read_message(&mut Cursor::new(early)).is_err());assert!(guest.is_sealed());
        for replay in [false,true] {
            let (host,guest,ack)=awaiting_ack(7);
            assert!(matches!(guest.read_message(&mut Cursor::new(ack.clone())),Ok(Some(None))));
            let duplicate=if replay {ack}else{
                let mut duplicate=Vec::new();host.send_packet(&mut duplicate,serde_json::to_value(TerminalAckContent {
                    native_close_ack:TerminalAckBody {child:child().to_wire(),raw_child_exit_dword:7}}).unwrap(),None).unwrap();duplicate
            };
            assert!(guest.read_message(&mut Cursor::new(duplicate)).is_err());assert!(guest.is_sealed());
        }
        let (_,guest,ack)=awaiting_ack(7);
        // Pure state/deadline fixture: no sleep and no renewal of an OS budget.
        guest.terminal.lock().unwrap().deadline=Some(Instant::now());
        assert!(guest.read_message(&mut Cursor::new(ack)).is_err());assert!(guest.is_sealed());
    }
    #[test]
    fn terminal_ack_receipt_binds_actual_observed_ack_and_failed_write_is_not_replayed() {
        let (host,guest,bytes,_)=streamed(7,true);
        let mut frame=read_v8_wire_frame_bounded(&mut Cursor::new(bytes)).unwrap().unwrap();
        let Version8Message::RuntimePacket(p)=&mut frame.message else{unreachable!()};
        p.terminal_close_ack=Some(WireAuthTag::from_bytes([31;32]));p.tag=guest.packet_tag(p).unwrap();
        assert!(host.read_message(&mut Cursor::new(encode_v8_length_prefixed_frame(&frame).unwrap())).is_err());
        assert!(host.is_sealed());
        struct FailedWrite(usize);
        impl Write for FailedWrite {
            fn write(&mut self,_:&[u8])->io::Result<usize>{self.0+=1;Err(io::Error::from_raw_os_error(5))}
            fn flush(&mut self)->io::Result<()>{panic!("failed ACK must not flush")}
        }
        let (host,_,_)=awaiting_ack(7);
        // Fresh labelled host terminal state, not a product/native completion.
        let (failing,_)=NativeCloseRuntime::host([28;32],272,runner()).unwrap();
        failing.set_child(child()).unwrap();failing.record_terminal_exit(7).unwrap();
        let mut writer=FailedWrite(0);
        assert_eq!(failing.write_terminal_close_ack(&mut writer).unwrap_err().raw_os_error(),Some(5));
        assert!(failing.write_terminal_close_ack(&mut writer).is_err());assert_eq!(writer.0,1);
        assert!(!failing.terminal_ack_written());assert!(failing.is_sealed());
        let deadline=host.terminal_deadline().unwrap();
        assert!(host.arm_terminal_deadline(deadline+Duration::from_secs(1)).is_err());
        assert_eq!(host.terminal_deadline().unwrap(),deadline);
    }
    #[test]
    fn explicit_bootstrap_rejects_wrong_version_schema_attempt_nonce_runner_and_missing_capability() {
        let (_,original)=NativeCloseRuntime::host([27;32],271,runner()).unwrap();
        for kind in 0..8 {
            let mut candidate=original.clone();
            if kind==0 {candidate.version=7;}
            let Version8Message::RuntimeBootstrap(value)=&mut candidate.message else{unreachable!()};
            match kind {1=>value.schema=[1;32],2=>value.attempt+=1,3=>value.seed[0]^=1,
                4=>value.runner.creation_filetime_100ns+=1,
                5=>value.hello.capability=NativeCloseCapabilityOptIn::DisabledOrNotRequested,
                6=>value.hello.schema=[2;32],7=>value.terminal_close_ack_required=false,_=>{}}
            assert!(NativeCloseRuntime::runner_bootstrap(candidate,runner()).is_err());
        }
        assert!(NativeCloseRuntime::host([0;32],271,runner()).is_err());
        assert!(NativeCloseRuntime::host([1;32],0,runner()).is_err());
    }
    #[test]
    fn live_data_control_exit_receipt_and_eof_preserve_full_dword_with_outer_scope_pending() {
        for raw in [0u32,7,256,259,u32::MAX] {
            let (host,guest,bytes,mut seen)=streamed(raw,true);
            // Stream includes binary length prefixes; inspect the exact marker
            // bytes without decoding those headers as UTF-8. No nonce field may
            // occur anywhere in the original bounded framed stream.
            assert!(!bytes.windows(b"\"nonce\"".len()).any(|part|part==b"\"nonce\""));
            assert_eq!(guest.controls_written().unwrap(),1);
            seen.extend(drain(&host,bytes).unwrap());assert_eq!(seen.len(),6);assert!(seen[5].is_none());
            let Message::Output{payload}= &seen[2].as_ref().unwrap().message else{panic!("real data frame")};
            assert_eq!(decode_bytes(&payload.data_b64).unwrap(),b"exact output\0bytes");
            let Message::ControlResult{payload}= &seen[3].as_ref().unwrap().message else{panic!("control result")};
            assert_eq!(payload.request.nonce,host.control_hello().nonce);
            let before=host.public_projection().unwrap();
            assert_eq!(before.stage,ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose);
            assert!(!before.local_registry_slot_releasable);
            let facts=physical(raw);host.observe_physical_close(facts).unwrap();
            let pending=host.public_projection().unwrap();
            assert_eq!(pending.stage,ReceiverProtocolStage::HostNativeClosedPendingOuterScope);
            assert!(!pending.local_registry_slot_releasable);
            assert_eq!(pending.validated_receipt.unwrap().raw_child_exit_dword(),RawProcessExitCodeDword(raw));
            // Only a labelled pure fixture supplies these outer facts. Runtime
            // owner leaves them unobserved and cannot mint a product capability.
            let mut completed=facts;completed.host_keeper_and_desktop_lease=LeaseRetirementFact::RetiredCleanly;
            completed.host_local_registry_quota_lease=LeaseRetirementFact::RetiredCleanly;
            host.observe_physical_close(completed).unwrap();
            assert_eq!(host.receiver.lock().unwrap().stage(),ReceiverProtocolStage::RunnerAttemptTransportClosed);
            assert_eq!(host.receiver.lock().unwrap().validated_receipt().unwrap().business_exit_i32(),BusinessExitCodeI32(raw as i32));
        }
    }
    #[test]
    fn missing_or_duplicate_receipt_and_post_exit_output_remain_sticky_not_eof_success() {
        let (host,_,bytes,_)=streamed(7,false);
        assert!(drain(&host,bytes).is_err());assert!(host.physical_eof_seen());
        assert_eq!(host.receiver.lock().unwrap().primary_error(),Some(&NativeCloseProtocolError::MissingReceiptAtPhysicalEof));
        assert!(host.observe_physical_close(physical(7)).is_err());
        assert_eq!(host.receiver.lock().unwrap().primary_error(),Some(&NativeCloseProtocolError::MissingReceiptAtPhysicalEof));
        let (host,guest,mut bytes,_)=streamed(7,true);
        // Deliberately hostile peer bypasses its public one-shot sender API.
        let duplicate=Version8WireFrame::new(Version8Message::NativeCloseReceipt(Box::new(
            NativeCloseReceiptWire::new_signed(&guest.binding,sample_clean_facts(271,child(),7,1)))));
        guest.send_packet(&mut bytes,Value::Null,Some(duplicate)).unwrap();
        assert!(drain(&host,bytes).is_err());
        assert_eq!(host.receiver.lock().unwrap().primary_error(),Some(&NativeCloseProtocolError::DuplicateReceipt));
        let (host,guest,mut bytes,_)=streamed(7,false);
        guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::Output {payload:OutputPayload {
            stream:OutputStream::Stdout,data_b64:encode_bytes(b"late")}}}).unwrap();
        assert!(drain(&host,bytes).is_err());
        assert_eq!(host.receiver.lock().unwrap().primary_error(),Some(&NativeCloseProtocolError::OutputAfterBusinessExit));
    }
    #[test]
    fn authenticated_packets_reject_tamper_replay_and_wrong_attempt_without_resuming() {
        fn greeting()->(Arc<NativeCloseRuntime>,Vec<u8>){
            let (host,guest)=pair();let mut bytes=Vec::new();
            guest.write_message(&mut bytes,&FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
                hello:guest.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}}).unwrap();
            (host,bytes)
        }
        for kind in 0..5 {
            let (host,bytes)=greeting();let mut cursor=Cursor::new(bytes);
            let mut frame=read_v8_wire_frame_bounded(&mut cursor).unwrap().unwrap();
            let Version8Message::RuntimePacket(packet)=&mut frame.message else{unreachable!()};
            match kind {0=>packet.attempt+=1,1=>packet.sequence+=1,2=>packet.schema=[1;32],
                3=>packet.direction=RuntimeDirection::HostToRunner,4=>packet.content["payload"]["controls"]["resize"]=Value::Bool(false),_=>{}}
            let bad=encode_v8_length_prefixed_frame(&frame).unwrap();
            assert!(host.read_message(&mut Cursor::new(bad)).is_err());
            assert!(host.receiver.lock().unwrap().no_replay_sealed());
            assert!(host.read_message(&mut cursor).is_err());
        }
        let (host,bytes)=greeting();let mut cursor=Cursor::new(bytes);
        let first=read_v8_wire_frame_bounded(&mut cursor).unwrap().unwrap();
        let exact=encode_v8_length_prefixed_frame(&first).unwrap();
        assert!(host.read_message(&mut Cursor::new(exact.clone())).is_ok());
        assert!(host.read_message(&mut Cursor::new(exact)).is_err());
        assert!(host.receiver.lock().unwrap().no_replay_sealed());
    }
    #[test]
    fn independent_runner_identity_and_native_dword_are_required_after_receipt_and_eof() {
        for wrong in 0..3 {
            let (host,_,bytes,_)=streamed(259,true);drain(&host,bytes).unwrap();
            let mut facts=physical(259);
            match wrong {0=>facts.runner_process_identity=Some(child()),1=>facts.runner_exit_code_dword=Some(RawProcessExitCodeDword(0)),
                _=>facts.runner_handle_closed_verified=ObservationProof::NotProven}
            assert!(host.observe_physical_close(facts).is_err());
            let first=host.receiver.lock().unwrap().primary_error().cloned();
            assert_eq!(first,Some(NativeCloseProtocolError::RunnerNativeProcessCloseFailed));
            assert!(host.observe_physical_close(physical(259)).is_err());
            assert_eq!(host.receiver.lock().unwrap().primary_error().cloned(),first);
        }
    }
    #[test]
    fn legacy_exit_wire_goldens_remain_exact_and_v7_hello_cannot_negotiate_v8() {
        for (version,golden) in [(6u8,br#"{"version":6,"type":"exit","payload":{"exit_code":-1,"timed_out":false}}"#.as_slice()),
            (7u8,br#"{"version":7,"type":"exit","payload":{"exit_code":-1,"timed_out":false}}"#.as_slice())] {
            let mut wire=Vec::new();write_runtime_message(&mut wire,&FramedMessage {version,message:Message::Exit {
                payload:ExitPayload {exit_code:-1,timed_out:false}}},None).unwrap();
            assert_eq!(&wire[4..],golden);assert_eq!(&wire[..4],&(golden.len() as u32).to_le_bytes());
        }
        let mut old=Vec::new();write_frame(&mut old,&FramedMessage {version:7,message:Message::Hello {payload:ControlHello {
            nonce:[27;32],schema:CHECKED_CONTROL_SCHEMA}}}).unwrap();
        assert!(read_v8_wire_frame_bounded(&mut Cursor::new(old)).is_err());
    }
    #[test]
    fn busy_or_poisoned_protocol_observation_never_returns_clean_defaults() {
        let (host,_)=pair();let guard=host.receiver.lock().unwrap();
        assert!(host.public_projection().is_err());drop(guard);
        assert!(!host.public_projection().unwrap().local_registry_slot_releasable);
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard=host.receiver.lock().unwrap();panic!("pure protocol mutex poison");
        }));
        assert!(host.public_projection().is_err());
    }
}

/// Startup read readiness on the exact original pipe; codec enforces the same
///16KiB/depth bound when consuming it. Does not grant any native ownership power.
pub fn wait_native_runtime_frame(pipe:&std::fs::File,deadline:std::time::Instant)->io::Result<()> {
    if std::time::Instant::now()>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"native frame original deadline expired"));}
    crate::framed_io::wait_for_complete_frame_with_cap(pipe,deadline,MAX_NATIVE_CLOSE_FRAME_BYTES)?;
    if std::time::Instant::now()>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"native frame readiness arrived late"));}
    Ok(())
}
