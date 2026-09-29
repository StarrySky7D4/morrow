#![forbid(unsafe_code)]
//! Reusable native host owner; no client frame can create an admission.
//! Uses existing Core HostPolicy and pinned Store, with separately bounded native
//! HTTP approvals. Persisted records do not reconstruct live authority.
use morrow_core::lifecycle::{HostPolicy, Instance};
pub use morrow_native_http_stream_wire as wire;
mod http_authority;
mod pipe_driver;
#[cfg(feature = "qualification-pipe-fault")]
mod qualification_pipe;
mod supervisor;
mod write_state;
#[cfg(feature = "qualification-pipe-fault")]
pub use qualification_pipe::{PartialFrameWitness, PipeFaultPlan};
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
    process::Command,
    sync::{mpsc, oneshot},
};
pub mod authority;
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
    pub http_origin: String,
}
/// Only a trusted host call can construct this, and it is consumed by launch.
/// There is no Deserialize implementation or fixture approval boolean.
pub(crate) struct Admission {
    spec: LaunchSpec,
    created: Instant,
    config: [u8; 32],
    nonce: [u8; 32],
    environment: BTreeMap<String, String>,
    #[cfg(feature = "qualification-pipe-fault")]
    pipe_fault: Option<PipeFaultPlan>,
}
impl Admission {
    #[cfg(feature = "qualification-pipe-fault")]
    fn bind_pipe_fault(&mut self, plan: PipeFaultPlan) -> Result<()> {
        if self.pipe_fault.is_some() {
            return Err("qualification admission plan already bound".into());
        }
        plan.validate(&self.spec.args)?;
        let mut bytes = b"Morrow/qualification-pipe-fault/v1\0".to_vec();
        bytes.extend_from_slice(&self.config);
        bytes.extend_from_slice(&plan.canonical());
        self.config = wire::digest(&bytes);
        self.pipe_fault = Some(plan);
        Ok(())
    }
    pub(crate) fn bind_authority(&mut self, context: &[u8]) {
        let mut bytes = b"Morrow/native-admission/v1\0".to_vec();
        bytes.extend_from_slice(&self.config);
        bytes.extend_from_slice(context);
        self.config = wire::digest(&bytes);
    }
    pub fn authorize(spec: LaunchSpec) -> Result<Self> {
        let created = Instant::now();
        let origin =
            url::Url::parse(&spec.http_origin).map_err(|_| "invalid HTTP fixture origin")?;
        if origin.scheme() != "http"
            || origin.host_str() != Some("127.0.0.1")
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !origin.username().is_empty()
            || origin.password().is_some()
        {
            return Err("only exact local fixture origin permitted".into());
        }
        if spec.slot.is_empty()
            || spec.slot.len() > 64
            || !spec.executable.is_absolute()
            || !spec.cwd.is_absolute()
            || !(100..=10_000).contains(&spec.ttl_ms)
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
                Ok(value) => {
                    environment.insert(key.to_owned(), value);
                }
                Err(std::env::VarError::NotPresent) => {}
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
            #[cfg(feature = "qualification-pipe-fault")]
            pipe_fault: None,
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
    pub http: Value,
}
struct Shared {
    state: Mutex<Snapshot>,
    gate: pipe_driver::Gate,
    created: Instant,
}
impl Shared {
    fn phase_if_changed(&self, phase: &str) {
        if self.state.lock().unwrap().phase != phase {
            self.phase(phase);
        }
    }
    fn event(&self, name: &str, value: Value) {
        let mut s = self.state.lock().unwrap();
        // Admission request limit ensures frame evidence cannot exhaust this bound.
        if s.events.len() < 1024 {
            let ordinal = s.events.len();
            let elapsed = self.created.elapsed();
            let domain = format!("host-authority-monotonic:{}:{}", s.session, s.epoch);
            s.events.push(json!({"ordinal":ordinal,"at_us":elapsed.as_micros(),"at_ns":elapsed.as_nanos(),"clock_domain":domain,"event":name,"detail":value}));
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
    #[cfg(feature = "qualification-pipe-fault")]
    CloseDataAfterWitness {
        witness: PartialFrameWitness,
        ack: oneshot::Sender<Result<Value>>,
    },
    Revoke(oneshot::Sender<()>),
    Stop(oneshot::Sender<()>),
    ApproveHttp {
        proposal_ref: Vec<u8>,
        expected_hash: Vec<u8>,
        response_limit: u32,
        ack: oneshot::Sender<Result<Value>>,
    },
}
#[derive(Clone)]
pub(crate) struct Session {
    shared: Arc<Shared>,
    control: mpsc::Sender<Control>,
}
impl Session {
    #[cfg(feature = "qualification-pipe-fault")]
    async fn close_data_after_witness(&self, witness: PartialFrameWitness) -> Result<Value> {
        let (ack, reply) = oneshot::channel();
        self.control
            .try_send(Control::CloseDataAfterWitness { witness, ack })
            .map_err(|_| "qualification control queue unavailable")?;
        tokio::time::timeout(Duration::from_secs(2), reply)
            .await
            .map_err(|_| "qualification close acceptance unknown")?
            .map_err(|_| "supervisor ended")?
    }
    pub async fn approve_http(
        &self,
        proposal_ref: Vec<u8>,
        expected_hash: Vec<u8>,
        response_limit: u32,
    ) -> Result<Value> {
        let (ack, reply) = oneshot::channel();
        self.control
            .try_send(Control::ApproveHttp {
                proposal_ref,
                expected_hash,
                response_limit,
                ack,
            })
            .map_err(|_| "HTTP operator queue unavailable")?;
        tokio::time::timeout(Duration::from_secs(2), reply)
            .await
            .map_err(|_| "HTTP approval acknowledgement unknown")?
            .map_err(|_| "supervisor ended")?
    }
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
        self.shared.gate.lock().map_err(|_| "gate poison")?.revoked = true;
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
pub(crate) struct NativeHost {
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
    pub fn launch(
        &mut self,
        admission: Admission,
        mut parent: http_authority::Parent,
    ) -> Result<Session> {
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
            .arg("--morrow-native-http-v3")
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
        parent.pid = pid;
        parent.session = session;
        parent.epoch = epoch;
        self.next_epoch = self.next_epoch.checked_add(1).ok_or("epoch exhausted")?;
        let shared = Arc::new(Shared {
            created: admission.created,
            gate: parent.gate.clone(),
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
                http: json!({"phase":"Idle"}),
            }),
        });
        shared.event("spawn",json!({"pid":pid,"epoch":epoch,"artifact_sha256":wire::hex(&spec.artifact_sha256),"config_sha256":wire::hex(&admission.config),"schema_sha256":wire::hex(&wire::schema_digest()),"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"path_image_race_closed":false}));
        let gate_matches =
            parent.gate.lock().map_err(|_| "gate poison")?.deadline == parent.deadline;
        shared.event("authority_deadline_bound",json!({"created_offset_ns":0,"original_deadline_offset_ns":parent.deadline.saturating_duration_since(admission.created).as_nanos(),"lifetime_ms":spec.ttl_ms,"parent_deadline_matches_gate":gate_matches,"expiry_not_renewed":true}));
        let (tx, rx) = mpsc::channel(8);
        let handle = Session {
            shared: shared.clone(),
            control: tx,
        };
        self.sessions.insert(spec.slot.clone(), handle.clone());
        tokio::spawn(supervisor::run(
            child,
            admission,
            instance,
            self.policy.clone(),
            shared,
            rx,
            parent,
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
