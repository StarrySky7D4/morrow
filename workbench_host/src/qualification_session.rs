//! One binary-private slot borrowing the actual main Workbench. Never opens an owner.
use super::{qualification_session_bootstrap as b, qualification_session_policy as policy};
use morrow_workbench_host::{Result, Workbench, agent_task_discovery as ad,
    agent_tasks::{AgentContext, AgentSnapshot, AgentStart, AgentLimits}, io_tasks::{TaskKey, StoragePhase}};
use morrow_core::dispatch::HostBinding;
use capnp::{message::ReaderOptions, serialize};
use std::{collections::BTreeSet, io::Write, time::{Duration, Instant}};

pub const MAGIC: &[u8; 8] = b"MROWQF42";
pub const REPLY_MAGIC: &[u8; 8] = b"MROWQR42";
pub const REQUEST_BYTES: usize = 188;
pub const REPLY_BYTES: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation { Start=1, Stop=2, Poll=3, Ack=4 }
#[derive(Clone, Copy)]
pub struct Request { pub op: Operation, pub id: [u8;32], pub plan: [u8;32], pub nonce: [u8;16],
    pub task: [u8;32], pub barrier: ([u8;32],[u8;32]) }
impl Request {
    pub fn decode(raw: &[u8]) -> Result<Self> {
        if raw.len() != REQUEST_BYTES || &raw[..8] != MAGIC || raw[8] != 1 || raw[10..12] != [0,0] {
            return Err("qualification request shape/version".into());
        }
        let op = match raw[9] {1=>Operation::Start,2=>Operation::Stop,3=>Operation::Poll,4=>Operation::Ack,
            _=>return Err("qualification operation".into())};
        let q = Self {op,id:raw[12..44].try_into()?,plan:raw[44..76].try_into()?,nonce:raw[76..92].try_into()?,
            task:raw[92..124].try_into()?,barrier:(raw[124..156].try_into()?,raw[156..188].try_into()?)};
        if q.id == [0;32] || q.plan == [0;32] || q.nonce == [0;16]
            || (op == Operation::Start) != (q.task == [0;32])
            || matches!(op,Operation::Stop|Operation::Poll) && q.barrier != ([0;32],[0;32])
            || matches!(op,Operation::Start|Operation::Ack) && (q.barrier.0 == [0;32] || q.barrier.1 == [0;32]) {
            return Err("qualification canonical fields".into());
        }
        Ok(q)
    }
}
#[derive(Default)]
struct Policy {
    start: bool, stop: bool, ack: bool, fenced: bool, ids: BTreeSet<[u8;32]>, polls: u32,
    startup: Option<([u8;32],[u8;32])>, idle: bool, retained: Option<([u8;32],[u8;32])>,
}
impl Policy {
    fn reserve(&mut self, q: Request) -> Result<()> {
        if self.ids.len() >= 1024 || !self.ids.insert(q.id) { return Err("qualification ID bound/replay".into()); }
        match q.op {
            Operation::Start => {
                if self.fenced || self.start || self.startup != Some(q.barrier) || !self.idle {
                    return Err("qualification startup receipt/idle/once barrier".into());
                }
                self.start = true; // Permanent before review, context, or worker effect.
            },
            Operation::Stop => {
                if !self.start || self.stop || self.ack { return Err("qualification stop once/order".into()); }
                self.stop = true;
            },
            Operation::Poll => {
                if !self.stop || self.ack || self.polls >= 1000 { return Err("qualification poll bound/order".into()); }
                self.polls += 1;
            },
            Operation::Ack => {
                if self.fenced || !self.stop || self.ack || self.retained != Some(q.barrier) {
                    return Err("qualification retained receipt/ACK once".into());
                }
                self.ack = true; // Never reset after original ACK or delivery failure.
            },
        }
        Ok(())
    }
}
pub struct QualificationSlot {
    admission: b::Admission, binding: HostBinding, incarnation: [u8;32], profile: [u8;32],
    policy: Policy, reviewed: Option<policy::Reviewed>, context: Option<AgentContext>,
    key: Option<TaskKey>, acknowledged: Option<TaskKey>, started: Option<Instant>, stopping: Option<Instant>,
    #[cfg(feature="c28-qualified-startup-rpc-fault-fixture")]
    startup_rpc_fault: morrow_workbench_host::protocol::QualifiedStartupRpcFault,
}
impl QualificationSlot {
    pub fn new(admission: b::Admission, host: &Workbench, record: &serde_json::Value) -> Result<Self> {
        Ok(Self { binding: host.agent_owner_binding(),
            incarnation: b::unhex(record["incarnation"].as_str().ok_or("qualification incarnation")?)?,
            profile: b::unhex(record["profile"].as_str().ok_or("qualification profile")?)?,
            admission, policy: Policy::default(), reviewed: None, context: None, key: None,
            acknowledged: None, started: None, stopping: None,
            #[cfg(feature="c28-qualified-startup-rpc-fault-fixture")]
            startup_rpc_fault: Default::default() })
    }
    // Only a real, admitted binary-private slot selects this fixture response.
    #[cfg(feature="c28-qualified-startup-rpc-fault-fixture")]
    pub fn respond_startup_rpc_fixture(&mut self, host: &mut Workbench, bytes: &[u8]) -> Result<Vec<u8>> {
        self.original(host)?;
        morrow_workbench_host::protocol::respond_qualified_startup_rpc_fixture(
            host, bytes, &mut self.startup_rpc_fault)
    }
    pub fn fence(&mut self, host: &mut Workbench) {
        self.policy.fenced = true;
        host.mark_product_session_delivery_unknown();
    }
    fn original(&self, host: &Workbench) -> Result<()> {
        if self.binding != host.agent_owner_binding() { return Err("qualification original owner changed".into()); }
        host.product_gate().check()
    }
    /// Called only after the original, successful actual write/flush/final gate.
    /// Its bytes remain genuine public responses; this never dispatches a frame.
    pub fn after_delivery(&mut self, host: &Workbench, input: &[u8], response: &[u8]) -> Result<()> {
        self.original(host)?;
        let pair = (b::sha(input), b::sha(response));
        if input.starts_with(ad::MAGIC) {
            let q = ad::Request::decode(input)?;
            let observation = ad::Observation::decode_for(q,input,response)?;
            if !self.policy.start && self.policy.startup.is_some() {
                self.policy.idle = observation.kind == ad::Kind::None && observation.task.is_none();
            }
            if observation.kind == ad::Kind::RetainedAgentExit && observation.task == self.key && self.key.is_some() {
                self.policy.retained = Some(pair);
            }
        } else if self.policy.startup.is_none() && !self.policy.start && startup_response(input,response) {
            self.policy.startup = Some(pair);
        }
        Ok(())
    }
    pub fn before_frame(&self, input: &[u8]) -> Result<()> {
        if input.starts_with(morrow_workbench_host::agent_product_control::MAGIC)
            && (self.policy.fenced || self.started.is_none_or(|t| t.elapsed() >= Duration::from_secs(40))) {
            return Err("qualification product deadline/fence".into());
        }
        Ok(())
    }
    fn require_key(&self, q: Request) -> Result<TaskKey> {
        let key = self.key.ok_or("qualification actual task absent")?;
        if key.as_bytes() != &q.task { return Err("qualification foreign task".into()); }
        Ok(key)
    }
    pub fn respond(&mut self, host: &mut Workbench, raw: &[u8]) -> Result<Vec<u8>> {
        let result = self.respond_inner(host,raw);
        if result.is_err() { self.fence(host); }
        result
    }
    fn respond_inner(&mut self, host: &mut Workbench, raw: &[u8]) -> Result<Vec<u8>> {
        self.original(host)?;
        let q = Request::decode(raw)?;
        if q.plan != self.admission.plan.digest || q.nonce != self.admission.plan.nonce {
            return Err("qualification plan/nonce correlation".into());
        }
        if q.op != Operation::Start { self.require_key(q)?; }
        self.policy.reserve(q)?;
        let snapshot = match q.op {
            Operation::Start => {
                if host.observe_agent_task().kind != ad::Kind::None || host.agent_status()?.is_some()
                    || host.maintenance_warning().is_some() || !host.writable() {
                    return Err("qualification original idle/writable gate".into());
                }
                self.reviewed = Some(policy::review_once(host,&self.admission)?);
                let sid = self.admission.plan.sid();
                let mut scope = vec![sid.clone(),format!("{sid}-child")]; scope.sort();
                self.context = Some(host.prepare_agent_context(|owner| {
                    let authority = owner.session_host()?;
                    let connection = owner.connect()?;
                    let caps = morrow_agent_session_exec_v1_r2::authority::Capabilities {
                        session_read:true,session_write:true,..Default::default()
                    };
                    let expires = owner.expires_after(60000)?;
                    owner.admit(&authority,&connection,caps,caps,scope,"session-only".into(),expires)?;
                    owner.context(Default::default(),None)
                })?);
                let reviewed = self.reviewed.as_ref().ok_or("qualification review retained")?;
                let start = host.start_agent(AgentStart {
                    package_id:self.admission.plan.package_id(),full_sha256:reviewed.full_sha,
                    revisions:reviewed.revisions,lifetime:Duration::from_secs(60),limits:AgentLimits::default(),
                },&mut self.context);
                match start {
                    Ok(key) => { self.key = Some(key); self.started = Some(Instant::now()); },
                    Err(e) => {
                        // Only a consumed context can have installed our attempted task.
                        // No prior task was present; capture immutable original fact before failing.
                        if self.context.is_none() { self.key = host.observe_agent_task().task; }
                        return Err(e);
                    }
                }
                Some(host.agent_status()?.ok_or("qualification started task snapshot absent")?)
            },
            Operation::Stop => {
                self.stopping = Some(Instant::now());
                let key = self.require_key(q)?;
                let status = host.poll_agent(key)?;
                Some(if status.task.storage == StoragePhase::Reclaimed { status }
                else { host.cancel_agent(key)? })
            },
            Operation::Poll => {
                if self.stopping.is_none_or(|t| t.elapsed() >= Duration::from_secs(20)) {
                    return Err("qualification original join deadline".into());
                }
                Some(host.poll_agent(self.require_key(q)?)?)
            },
            Operation::Ack => {
                let key = self.require_key(q)?;
                let status = host.poll_agent(key)?;
                if !healthy(status) || self.context.is_some() || host.observe_agent_task().kind != ad::Kind::RetainedAgentExit {
                    return Err("qualification ACK requires original healthy return".into());
                }
                host.acknowledge_agent(key)?;
                self.acknowledged = Some(key); // Retain before clearing active key or fallible delivery.
                self.key = None;
                None // No synthetic snapshot: original ACK+immutable None is checked below.
            },
        };
        self.original(host)?;
        if snapshot.is_some_and(|v| matches!(v.task.storage,StoragePhase::RecoveryRequired|StoragePhase::Unavailable)
            || v.exit.is_some_and(|e|e.execution.is_err()||e.disconnect.is_err()||e.maintenance.is_err())) {
            return Err("qualification original task failure retained; no success promotion".into());
        }
        if snapshot.is_some_and(|v|v.task.key != self.key) { return Err("qualification actual snapshot key".into()); }
        if q.op == Operation::Ack && host.observe_agent_task().kind != ad::Kind::None {
            return Err("qualification original ACK did not clear task".into());
        }
        let mut out = Vec::with_capacity(REPLY_BYTES);
        out.extend(REPLY_MAGIC); out.extend([1,q.op as u8,0,snapshot.map_or(0,|v|storage(v.task.storage))]);
        out.extend(q.id); out.extend(q.plan); out.extend(q.nonce);
        out.extend(self.key.map_or([0;32],|k|*k.as_bytes()));
        out.extend(self.acknowledged.map_or([0;32],|k|*k.as_bytes()));
        out.extend(b::sha(raw)); out.extend(self.incarnation); out.extend(self.profile);
        out.extend([1,u8::from(snapshot.is_some_and(healthy)),u8::from(self.context.is_some()),u8::from(self.policy.fenced)]);
        if out.len() != REPLY_BYTES { return Err("qualification reply shape".into()); }
        Ok(out)
    }
    pub fn deliver(&mut self, host: &mut Workbench, output: &mut impl Write, response: &[u8]) -> Result<()> {
        let result = deliver_reply(output,response,&host.product_gate());
        if result.is_err() { self.fence(host); }
        result
    }
    /// Cleanup of only this unstarted external context. Never ACK/repair/restart.
    /// This Slot remains retained through original finish_wait, including failures.
    pub fn shutdown_context(&mut self, host: &mut Workbench) -> Result<()> {
        if self.context.is_some() {
            let result = host.dispose_agent_context(&mut self.context);
            if result.is_err() { self.fence(host); }
            result
        } else { Ok(()) }
    }
}
fn deliver_reply(output:&mut impl Write,response:&[u8],gate:&morrow_workbench_host::product_gate::ProductGate)->Result<()> {
    if response.len()!=REPLY_BYTES {return Err("qualification reply bound".into());}
    gate.check()?; output.write_all(&(response.len() as u32).to_le_bytes())?;
    gate.check()?; output.write_all(response)?; gate.check()?;
    output.flush()?; gate.check()?; Ok(())
}
fn storage(v:StoragePhase)->u8 {match v {StoragePhase::Local=>0,StoragePhase::Running=>1,StoragePhase::Stopping=>2,
    StoragePhase::Reclaimed=>3,StoragePhase::RecoveryRequired=>4,StoragePhase::Unavailable=>5}}
fn healthy(v:AgentSnapshot)->bool {v.task.storage==StoragePhase::Reclaimed && v.exit.is_some_and(|e|
    e.execution.is_ok()&&e.disconnect.is_ok()&&e.maintenance.is_ok())}
fn startup_response(input:&[u8],response:&[u8])->bool {
    if input.len()>131072 || response.len()>131072 {return false;}
    // No successful marker for an arbitrary/failed response or a nested action.
    let check = || -> Result<bool> {
        let q=marker_reader(input)?; let q=q.get_root::<morrow_workbench_host::host_capnp::request::Reader>()?;
        let r=marker_reader(response)?; let r=r.get_root::<morrow_workbench_host::host_capnp::response::Reader>()?;
        Ok(q.get_version()==1 && q.get_digest()?==morrow_workbench_host::host_protocol_digest()
            && q.get_action()?==morrow_workbench_host::host_capnp::Action::PageVersioned && q.get_limit()==1
            && r.get_version()==1 && r.get_digest()?==morrow_workbench_host::host_protocol_digest()
            && !r.get_read_only() && r.get_error()?.is_empty() && r.get_maintenance_warning()?.is_empty())
    }; check().unwrap_or(false)
}
fn marker_reader(raw:&[u8])->capnp::Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    let mut rest=raw;
    let r=serialize::read_message_from_flat_slice(&mut rest,ReaderOptions {
        traversal_limit_in_words:Some(65536),nesting_limit:20,
    })?;
    if !rest.is_empty() {return Err(capnp::Error::failed("trailing bytes".into()));} Ok(r)
}

#[cfg(test)]
#[path = "../tests/support/qualification_session.rs"]
mod tests;
