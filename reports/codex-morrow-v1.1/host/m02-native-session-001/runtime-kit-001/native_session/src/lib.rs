#![forbid(unsafe_code)]
//! Reusable native host owner; no client frame can create an admission.
//! Uses the existing core HostPolicy instance/revocation lifecycle. This control-only
//! transport does not expose content grants or a second data-plane approval system.
use morrow_core::lifecycle::{HostPolicy, Instance, InstancePhase};
pub use morrow_native_session_wire as wire;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{mpsc, oneshot},
    time::Instant as TokioInstant,
};
use wire::{Frame, Kind};
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub slot: String,
    pub executable: PathBuf,
    pub artifact_sha256: [u8; 32],
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub ttl_ms: u64,
    pub handshake_ms: u64,
    pub frame_ms: u64,
    pub close_ms: u64,
    pub request_budget: u64,
}
/// Only a trusted host call can construct this, and it is consumed by launch.
/// There is no Deserialize implementation or fixture approval boolean.
pub struct Admission {
    spec: LaunchSpec,
    created: Instant,
    config: [u8; 32],
    nonce: [u8; 32],
    environment: BTreeMap<String, String>,
}
impl Admission {
    pub fn authorize(spec: LaunchSpec) -> Result<Self> {
        let created = Instant::now();
        if spec.slot.is_empty()
            || spec.slot.len() > 64
            || !spec.executable.is_absolute()
            || !spec.cwd.is_absolute()
            || !(100..=60_000).contains(&spec.ttl_ms)
            || !(50..=10_000).contains(&spec.handshake_ms)
            || !(50..=5_000).contains(&spec.frame_ms)
            || !(20..=5_000).contains(&spec.close_ms)
            || !(1..=128).contains(&spec.request_budget)
            || spec.args.len() > 8
            || spec.args.iter().any(|a| a.len() > 256)
        {
            return Err("invalid launch limits".into());
        }
        if !spec.cwd.is_dir()
            || std::fs::read_dir(&spec.cwd)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
        {
            return Err("requires existing empty isolated cwd".into());
        }
        let meta = std::fs::metadata(&spec.executable).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > 256 * 1024 * 1024 {
            return Err("artifact limit".into());
        }
        if wire::digest(&std::fs::read(&spec.executable).map_err(|e| e.to_string())?)
            != spec.artifact_sha256
        {
            return Err("artifact digest mismatch".into());
        }
        // Capture the exact allowlisted environment at admission, bind it, and use
        // that same snapshot at spawn. Later host environment changes cannot alter it.
        let mut environment = BTreeMap::new();
        for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
            match std::env::var(key) {
                Ok(value) => { environment.insert(key.to_owned(), value); }
                Err(std::env::VarError::NotPresent) => {},
                Err(_) => return Err("non-Unicode allowlisted environment".into()),
            }
        }
        let config=wire::digest(&serde_json::to_vec(&json!({"launch":spec,"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"wire":wire::hex(&wire::schema_digest()),"environment":environment})).unwrap());
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        Ok(Self {
            spec,
            created,
            config,
            nonce,
            environment,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub session: u64,
    pub epoch: u64,
    pub pid: u32,
    pub generation: u64,
    pub phase: String,
    pub owner_retained: bool,
    pub exit_code: Option<i32>,
    pub exit_observed: bool,
    pub stdout_eof: bool,
    pub stderr_eof: bool,
    pub event_overflow: u64,
    pub events: Vec<Value>,
}
struct Shared {
    state: Mutex<Snapshot>,
    created: Instant,
}
impl Shared {
    fn event(&self, name: &str, value: Value) {
        let mut s = self.state.lock().unwrap();
        // Admission request limit ensures frame evidence cannot exhaust this bound.
        if s.events.len() < 1024 {
            let ordinal = s.events.len();
            s.events.push(json!({"ordinal":ordinal,"at_us":self.created.elapsed().as_micros(),"event":name,"detail":value}));
        } else {
            s.event_overflow += 1;
        }
    }
    fn phase(&self, phase: &str) {
        self.state.lock().unwrap().phase = phase.into();
        self.event("phase", json!(phase));
    }
}
enum Control {
    Revoke(oneshot::Sender<()>),
    Stop(oneshot::Sender<()>),
}
#[derive(Clone)]
pub struct Session {
    shared: Arc<Shared>,
    control: mpsc::Sender<Control>,
}
impl Session {
    pub fn snapshot(&self) -> Snapshot {
        self.shared.state.lock().unwrap().clone()
    }
    pub async fn revoke(&self) -> Result<()> {
        self.control(false).await
    }
    pub async fn stop(&self) -> Result<()> {
        self.control(true).await
    }
    async fn control(&self, stop: bool) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.control
            .try_send(if stop {
                Control::Stop(tx)
            } else {
                Control::Revoke(tx)
            })
            .map_err(|e| format!("control unavailable: {e}"))?;
        tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .map_err(|_| "control acknowledgement unconfirmed")?
            .map_err(|_| "supervisor ended".into())
    }
}
/// Registry retains all unconfirmed owners. Same slot cannot launch a second child.
pub struct NativeHost {
    policy: Arc<Mutex<HostPolicy>>,
    sessions: BTreeMap<String, Session>,
    next_epoch: u64,
}
impl NativeHost {
    pub fn new() -> Result<Self> {
        Ok(Self {
            policy: Arc::new(Mutex::new(HostPolicy::new().map_err(|e| e.to_string())?)),
            sessions: BTreeMap::new(),
            next_epoch: 1,
        })
    }
    pub fn launch(&mut self, admission: Admission) -> Result<Session> {
        if self
            .sessions
            .get(&admission.spec.slot)
            .is_some_and(|s| s.snapshot().owner_retained)
        {
            return Err("slot owner retained".into());
        }
        if self.sessions.len() >= 64 && !self.sessions.contains_key(&admission.spec.slot) {
            return Err("slot limit".into());
        }
        let spec = &admission.spec;
        let expires = admission.created + Duration::from_millis(spec.ttl_ms);
        // Recheck immediately before spawn. This is NOT an image-loader TOCTOU closure.
        if wire::digest(&std::fs::read(&spec.executable).map_err(|e| e.to_string())?)
            != spec.artifact_sha256
        {
            return Err("artifact changed before spawn".into());
        }
        if Instant::now() >= expires {
            return Err("admission expired before spawn".into());
        }
        let instance = self
            .policy
            .lock()
            .unwrap()
            .activate()
            .map_err(|e| e.to_string())?;
        let mut command = Command::new(&spec.executable);
        command
            .arg("--morrow-native-session-v2")
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.envs(&admission.environment);
        #[cfg(windows)]
        {
            command.creation_flags(0x08000000);
        }
        let child = match command.spawn() {
            Ok(v) => v,
            Err(e) => {
                let mut p = self.policy.lock().unwrap();
                let _ = p.safety_stop(instance);
                let _ = p.stop(instance);
                let _ = p.retire(instance);
                return Err(format!("spawn failed: {e}"));
            }
        };
        let pid = child.id().ok_or("missing child pid")?;
        let session = u64::from_le_bytes(admission.nonce[..8].try_into().unwrap()).max(1);
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.checked_add(1).ok_or("epoch exhausted")?;
        let shared = Arc::new(Shared {
            created: admission.created,
            state: Mutex::new(Snapshot {
                session,
                epoch,
                pid,
                generation: 1,
                phase: "Preparing".into(),
                owner_retained: true,
                exit_code: None,
                exit_observed: false,
                stdout_eof: false,
                stderr_eof: false,
                event_overflow: 0,
                events: vec![],
            }),
        });
        shared.event("spawn",json!({"pid":pid,"epoch":epoch,"artifact_sha256":wire::hex(&spec.artifact_sha256),"config_sha256":wire::hex(&admission.config),"schema_sha256":wire::hex(&wire::schema_digest()),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"path_image_race_closed":false}));
        let (tx, rx) = mpsc::channel(8);
        let handle = Session {
            shared: shared.clone(),
            control: tx,
        };
        self.sessions.insert(spec.slot.clone(), handle.clone());
        tokio::spawn(supervise(
            child,
            admission,
            instance,
            self.policy.clone(),
            shared,
            rx,
        ));
        Ok(handle)
    }
}

fn revoke(policy: &Arc<Mutex<HostPolicy>>, instance: Instance, shared: &Shared, reason: u32) {
    let mut state = shared.state.lock().unwrap();
    if state.generation == 1 {
        let _ = policy.lock().unwrap().safety_stop(instance);
        state.generation = 2;
        drop(state);
        shared.event("revoked", json!({"generation":2,"reason":reason}));
    }
}
struct Output {
    bytes: Vec<u8>,
    offset: usize,
    deadline: Instant,
}
fn queue(frame: Frame, frame_ms: u64) -> Output {
    Output {
        bytes: frame.encode(),
        offset: 0,
        deadline: Instant::now() + Duration::from_millis(frame_ms),
    }
}
fn reply(
    initial: &Frame,
    kind: Kind,
    seq: u64,
    code: u32,
    shared: &Shared,
    expires: Instant,
    budget: u64,
) -> Frame {
    let mut f = initial.request(kind, seq);
    f.code = code;
    f.generation = shared.state.lock().unwrap().generation;
    f.remaining_ms = expires
        .saturating_duration_since(Instant::now())
        .as_millis() as u64;
    f.budget = budget;
    f
}
async fn supervise(
    mut child: Child,
    admission: Admission,
    instance: Instance,
    policy: Arc<Mutex<HostPolicy>>,
    shared: Arc<Shared>,
    mut control: mpsc::Receiver<Control>,
) {
    let spec = &admission.spec;
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let expires = admission.created + Duration::from_millis(spec.ttl_ms);
    let handshake_deadline =
        (admission.created + Duration::from_millis(spec.handshake_ms)).min(expires);
    let s = shared.state.lock().unwrap().clone();
    let initial = Frame {
        kind: Kind::Challenge,
        sequence: 0,
        session: s.session,
        epoch: s.epoch,
        generation: 1,
        pid: s.pid,
        code: 0,
        remaining_ms: expires
            .saturating_duration_since(Instant::now())
            .as_millis() as u64,
        nonce: admission.nonce,
        schema: wire::schema_digest(),
        artifact: spec.artifact_sha256,
        config: admission.config,
        budget: spec.request_budget,
        capabilities: 1,
    };
    let mut output = Some(queue(initial.clone(), spec.frame_ms));
    let mut input = Vec::with_capacity(wire::MAX_FRAME);
    let mut input_target = 4;
    let mut in_deadline = None;
    let mut scratch = [0u8; wire::MAX_FRAME];
    let mut errbuf = [0u8; 4096];
    let mut errbytes = 0usize;
    let mut errchunks = 0u64;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut exited = false;
    let mut ready = false;
    let mut last_seq = 0u64;
    let mut remaining = spec.request_budget;
    let mut closing: Option<Instant> = None;
    let mut killed = false;
    let mut unconfirmed = false;
    let mut reason = 0u32;
    let mut control_open = true;
    loop {
        if reason != 0 && closing.is_none() {
            if !input.is_empty() {
                shared.event("frame_received_partial", json!({"raw_hex":wire::hex(&input),"expected_total":input_target,"complete":false}));
            }
            revoke(&policy, instance, &shared, reason);
            shared.phase("Closing");
            closing = Some(Instant::now() + Duration::from_millis(spec.close_ms));
            shared.event("stop_requested", json!({"reason":reason}));
            if output.as_ref().is_some_and(|o| o.offset > 0) {
                let out=output.as_ref().unwrap();
                shared.event("frame_sent_partial",json!({"raw_hex":wire::hex(&out.bytes[..out.offset]),"expected_total":out.bytes.len(),"complete":false}));
                output = None;
                stdin = None;
                shared.event("partial_output_cancelled", json!(true));
            } else if stdin.is_some() {
                output = Some(queue(
                    reply(&initial, Kind::Stop, 0, reason, &shared, expires, remaining),
                    spec.frame_ms,
                ));
            }
        }
        if exited && stdout_done && stderr_done {
            let closed = {
                let st = shared.state.lock().unwrap();
                st.exit_observed && st.stdout_eof && st.stderr_eof
            };
            if closed {
                revoke(&policy, instance, &shared, 24);
                let mut p = policy.lock().unwrap();
                let _ = p.stop(instance);
                let _ = p.retire(instance);
                drop(p);
                shared.state.lock().unwrap().owner_retained = false;
                shared.phase("Released");
                break;
            }
        }
        let now = Instant::now();
        let timer = if let Some(c) = closing {
            if unconfirmed {
                now + Duration::from_secs(3600)
            } else {
                c
            }
        } else {
            let mut d = if ready { expires } else { handshake_deadline };
            if let Some(t) = in_deadline {
                d = d.min(t);
            }
            if let Some(o) = &output {
                d = d.min(o.deadline);
            }
            d
        };
        let can_write = output.is_some() && stdin.is_some();
        let read_limit = if closing.is_some() {
            wire::MAX_FRAME
        } else {
            input_target - input.len()
        };
        tokio::select! {
            biased;
            command=control.recv(), if control_open => {
                match command {
                    Some(Control::Revoke(ack))=>{
                        revoke(&policy,instance,&shared,19);
                        if closing.is_none(){shared.phase("Revoked");}
                        // Discard unsent success; partial writes close instead of frame splicing.
                        if output.as_ref().is_some_and(|o|o.offset>0){output=None;stdin=None;reason=19;}
                        else if output.as_ref().is_some_and(|o|matches!(Frame::decode(&o.bytes).unwrap().kind,Kind::Welcome|Kind::State)){
                            let seq=Frame::decode(&output.as_ref().unwrap().bytes).unwrap().sequence;
                            output=Some(queue(reply(&initial,Kind::Denied,seq,19,&shared,expires,remaining),spec.frame_ms));
                            shared.event("pending_response_denied",json!({"sequence":seq}));
                        }
                        shared.event("control_ack",json!({"action":"revoke","generation":2}));let _=ack.send(());
                    }
                    Some(Control::Stop(ack))=>{revoke(&policy,instance,&shared,25);reason=25;shared.event("control_ack",json!({"action":"stop"}));let _=ack.send(());}
                    None=>{control_open=false;reason=25;}
                }
            }
            _=tokio::time::sleep_until(TokioInstant::from_std(timer))=>{
                if closing.is_some(){
                    if !killed {killed=true;stdin=None;output=None;let result=child.start_kill();shared.event("kill_requested",json!({"ok":result.is_ok(),"error":result.err().map(|e|e.to_string())}));closing=Some(Instant::now()+Duration::from_millis(spec.close_ms));}
                    else {unconfirmed=true;shared.phase("ClosingUnconfirmed");shared.event("owner_retained",json!(true));}
                }else {let observed=Instant::now();reason=if observed>=expires{20}else if !ready && observed>=handshake_deadline{23}else{16};}
            }
            result=async {stdin.as_mut().unwrap().write(&output.as_ref().unwrap().bytes[output.as_ref().unwrap().offset..]).await}, if can_write=>{
                match result {
                    Ok(0)|Err(_)=>{stdin=None;output=None;reason=24;shared.event("stdin_failed",json!(true));}
                    Ok(n)=>{let out=output.as_mut().unwrap();out.offset+=n;if out.offset==out.bytes.len() {shared.event("frame_sent",json!({"raw_hex":wire::hex(&out.bytes)}));let stop=Frame::decode(&out.bytes).unwrap().kind==Kind::Stop;output=None;if stop{stdin=None;}}}
                }
            }
            status=child.wait(), if !exited=>{
                match status {
                    Ok(status)=>{exited=true;let mut st=shared.state.lock().unwrap();st.exit_observed=true;st.exit_code=status.code();drop(st);shared.event("exit",json!({"code":status.code(),"business_success":false}));reason=24;}
                    Err(e)=>{exited=true;shared.event("wait_failed",json!(e.to_string()));reason=24;}
                }
            }
            result=stdout.read(&mut scratch[..read_limit]), if !stdout_done && (output.is_none() || closing.is_some())=>{
                match result {
                    Ok(0)=>{stdout_done=true;shared.state.lock().unwrap().stdout_eof=true;shared.event("stdout_eof",json!(true));reason=24;}
                    Err(e)=>{stdout_done=true;shared.event("stdout_error",json!(e.to_string()));reason=24;}
                    Ok(n)=>{
                        if closing.is_some(){continue;}
                        if input.is_empty(){in_deadline=Some(Instant::now()+Duration::from_millis(spec.frame_ms));}
                        input.extend_from_slice(&scratch[..n]);
                        if input.len()!=input_target {continue;}
                        if input_target==4 {match wire::payload_length(&input){Ok(n)=>{input_target=n+4;continue;},Err(_)=>{shared.event("malformed_prefix",json!({"raw_hex":wire::hex(&input)}));reason=16;continue;}}}
                        shared.event("frame_received",json!({"raw_hex":wire::hex(&input)}));
                        let decoded=Frame::decode(&input);input.clear();input_target=4;in_deadline=None;
                        let frame=match decoded{Ok(v)=>v,Err(_)=>{reason=16;continue;}};
                        let revoked=policy.lock().unwrap().phase(instance).ok()==Some(InstancePhase::Revoked);
                        let code=if Instant::now()>=expires{20}else if revoked{19}else if frame.code!=0||!frame.matches_admission(&initial){17}else if frame.sequence!=last_seq+1{18}else if !ready&&frame.kind!=Kind::Hello{23}else if ready&&frame.kind==Kind::Hello{23}else{0};
                        if code!=0 {
                            output=Some(queue(reply(&initial,Kind::Denied,frame.sequence,code,&shared,expires,remaining),spec.frame_ms));
                            shared.event("request_denied",json!({"sequence":frame.sequence,"code":code}));
                            // Revoked requests are bounded by the same original budget; no renewal.
                            if code!=19 {reason=code;}else if remaining==0{reason=21;}else{remaining-=1;}
                            continue;
                        }
                        last_seq=frame.sequence;
                        if !ready {
                            if policy.lock().unwrap().ready(instance).is_err(){reason=19;continue;}
                            ready=true;shared.phase("Active");output=Some(queue(reply(&initial,Kind::Welcome,last_seq,2,&shared,expires,remaining),spec.frame_ms));
                        }else if frame.kind==Kind::Close {reason=25;}
                        else if frame.kind!=Kind::Query {reason=22;}
                        else if remaining==0 {reason=21;}
                        else {remaining-=1;output=Some(queue(reply(&initial,Kind::State,last_seq,2,&shared,expires,remaining),spec.frame_ms));shared.event("state_read",json!({"sequence":last_seq,"generation":1}));}
                    }
                }
            }
            result=stderr.read(&mut errbuf), if !stderr_done=>{
                match result {
                    Ok(0)=>{stderr_done=true;shared.state.lock().unwrap().stderr_eof=true;shared.event("stderr_eof",json!({"bytes":errbytes,"chunks":errchunks,"omitted_chunk_details":errchunks.saturating_sub(16)}));}
                    Err(e)=>{stderr_done=true;shared.event("stderr_error",json!(e.to_string()));reason=24;}
                    Ok(n)=>{errbytes=errbytes.saturating_add(n);errchunks+=1;if errchunks<=16{shared.event("stderr_chunk",json!({"bytes":n,"sha256":wire::hex(&wire::digest(&errbuf[..n]))}));}if errbytes>65536{reason=21;}}
                }
            }
        }
    }
}
