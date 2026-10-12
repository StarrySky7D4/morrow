//! Independent original protected-owner/public-PS30 qualification, source only.
//! A carrier must retain this object on error. No action is performed by Drop.
use crate::{
    formal_cleanup::{WorkbenchAdapter, WorkflowTracker},
    preflight,
    product,
    product_session_wire::{self as codec, Pending, PendingKind},
    protected_owner::ProtectedOwner,
    provisioning::GuestLifecycle,
    sealed_preparation::{self, PreparedSessionGuest},
    wrapper::{ReviewedWrapper, SelectionStep},
};
use anyhow::{Result, anyhow, ensure};
use morrow_agent_session_exec_v1_r2 as r2;
use morrow_core::dispatch::HostBinding;
use morrow_workbench_host::{
    agent_product_control::{self as ps, Request, Reply},
    agent_product_control_capnp::{Action, Operation, Status},
    agent_task_discovery::{self as ad, Kind},
    agent_tasks::{AgentLimits, AgentSnapshot, AgentStart},
    io_tasks::{StoragePhase, TaskKey},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf, time::{Duration, Instant}};

const FIXTURE: &[u8] = include_bytes!(
    "../../c28-basic-fixtures/public-session-r2-v1/morrow_codex_session_exec_guest_r2.wasm"
);
const FIXTURE_SHA: &str = "cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b";
const MAX_EVIDENCE: usize = 8 * 1024 * 1024;
const MAX_RECORDS: usize = 4200;
const MAX_POLLS: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Fresh, OwnerOpened, Reviewed, Prepared, Running, Attached, Opened,
    Created, Listed, WriterAdmitted, WriterOpened, Appended, Snapshotted,
    WriterClosed, Detached, Stopping, Reclaimed, RetainedExit, Acknowledged,
    Finished, Fenced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Completion { Open, Create, List, AdmitWriter, OpenWriter, Append, Snapshot }
#[derive(Default)]
struct Once { attempted: BTreeSet<&'static str> }
impl Once {
    fn take(&mut self, label: &'static str) -> Result<()> {
        ensure!(self.attempted.insert(label), "PS30 effect already attempted; no replay");
        Ok(())
    }
}
struct Policy { phase: Phase, once: Once, primary_failure: Option<&'static str> }
impl Policy {
    fn require(&self, phase: Phase) -> Result<()> {
        ensure!(self.primary_failure.is_none() && self.phase == phase,
            "PS30 business phase unavailable; retain original state");
        Ok(())
    }
    fn fence(&mut self, label: &'static str) {
        self.primary_failure.get_or_insert(label);
        self.phase = Phase::Fenced;
    }
}

/// Public frames/observations only. No authority objects or connection IDs.
#[derive(Serialize)]
pub struct ExchangeEvidence {
    pub profile: &'static str,
    pub step: &'static str,
    pub request_sha256: String,
    pub response_sha256: String,
    pub gate_closed_before: bool,
    pub gate_closed_after: bool,
    pub owner_binding_unchanged: bool,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadProgress { Pending, Complete }

#[must_use = "retain this same flow on errors; receipts cannot restore its owner"]
pub struct ProductSessionFlow<'a> {
    // The real provisioning/lock owner must outlive this retained flow. Safe
    // Rust cannot drop it or call its &mut cleanup while this borrow is live.
    _lifecycle: &'a GuestLifecycle,
    root: PathBuf,
    nonce: [u8; 16],
    sid: String,
    package_id: String,
    owner: Option<ProtectedOwner>,
    original_binding: Option<HostBinding>,
    wrapper: Option<ReviewedWrapper>,
    prepared: Option<PreparedSessionGuest>,
    tracker: WorkflowTracker,
    key: Option<TaskKey>,
    acknowledged_key: Option<TaskKey>,
    attachment: Option<u64>,
    generation: Option<u64>,
    writer: Option<u64>,
    pending: Option<Pending>,
    completion: Option<Completion>,
    accepted_info: Option<r2::SessionInfo>,
    last_outcome: Option<r2::Outcome>,
    last_completion: Option<Value>,
    last_snapshot: Option<AgentSnapshot>,
    expected_event: r2::Event,
    sequence: u64,
    policy: Policy,
    started: Option<Instant>,
    stopping: Option<Instant>,
    acknowledged_observed: bool,
    evidence: Vec<ExchangeEvidence>,
    evidence_bytes: usize,
    facts: Vec<Value>,
    cleanup_errors: Vec<&'static str>,
    join_polls: u32,
    cleanup_polls: u32,
}
impl<'a> ProductSessionFlow<'a> {
    /// Reuse the original non-deserializable provisioning object's actual phase.
    /// This is not a UUID/probe receipt or an alternative CLI permission flag.
    /// No owner/root is opened here. The disabled binary never calls this.
    pub fn new(lifecycle: &'a GuestLifecycle, nonce: [u8; 16]) -> Result<Self> {
        let runner = lifecycle.runner()?; // Exact original RunnerMaterialized gate.
        drop(runner);
        ensure!(nonce != [0; 16], "PS30 distinct nonce required");
        let suffix = preflight::hex(&nonce);
        let root = lifecycle.synthetic_root().join(format!("ps30-session-{suffix}"));
        preflight::plain_absolute(&root)?;
        preflight::no_reparse_ancestors(lifecycle.synthetic_root())?;
        ensure!(!root.try_exists()?, "PS30 fresh independent root required");
        let sid = format!("vm-product-session-{suffix}");
        r2::Action::Create { session_id: sid.clone(), parent: None, parent_tail: 0 }.validate()?;
        let expected_event = r2::Event {
            event_id: format!("vm-product-event-{suffix}"),
            body: b"stage39 independent public PS30 synthetic event\n".to_vec(),
        };
        expected_event.validate()?;
        Ok(Self {
            _lifecycle: lifecycle, root, nonce, sid, package_id: format!("vm.product.ps30.session.v1.{suffix}"),
            owner: None, original_binding: None, wrapper: None, prepared: None,
            tracker: WorkflowTracker::default(), key: None, acknowledged_key: None,
            attachment: None, generation: None, writer: None, pending: None, completion: None,
            accepted_info: None, last_outcome: None, last_completion: None, last_snapshot: None, expected_event,
            sequence: 0, policy: Policy { phase: Phase::Fresh, once: Once::default(), primary_failure: None },
            started: None, stopping: None, acknowledged_observed: false,
            evidence: Vec::new(), evidence_bytes: 0, facts: Vec::new(), cleanup_errors: Vec::new(),
            join_polls: 0, cleanup_polls: 0,
        })
    }
    pub fn phase(&self) -> Phase { self.policy.phase }
    pub fn evidence(&self) -> &[ExchangeEvidence] { &self.evidence }
    pub fn facts(&self) -> &[Value] { &self.facts }
    pub fn status(&self) -> Value {
        json!({"scope":"ORIGINAL_RUST_PUBLIC_RESPONDER_ONLY_NOT_DART_MAIN_SUPERVISOR",
            "phase":format!("{:?}",self.policy.phase), "root":self.root, "sid":self.sid,
            "task":self.key.map(|k|preflight::hex(k.as_bytes())),
            "acknowledged_task":self.acknowledged_key.map(|k|preflight::hex(k.as_bytes())),
            "attachment":self.attachment,"generation":self.generation,"writer_slot":self.writer,
            "pending":self.pending.as_ref().map(|p|p.token),
            "primary_failure":self.policy.primary_failure,"cleanup_errors":self.cleanup_errors,
            "last_consumed_completion":self.last_completion,
            "production_qualified":false,"release_eligible":false,"sdk_gate":"SDK26_G04_OPEN"})
    }
    /// Carrier calls this before returning an evidence persistence error. Never
    /// replay an effect because its successful in-memory receipt was not saved.
    pub fn evidence_persistence_failed(&mut self) { self.fence("evidence-persistence"); }
    fn fence(&mut self, label: &'static str) {
        self.policy.fence(label);
        if let Some(owner) = &mut self.owner {
            owner.workbench.mark_product_session_delivery_unknown();
        }
    }
    fn effect<T>(&mut self, label: &'static str, phase: Phase,
        body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.policy.require(phase)?;
        self.policy.once.take(label)?; // Permanent, before any effect/IO.
        let result = body(self);
        if result.is_err() { self.fence(label); }
        result
    }
    fn cleanup<T>(&mut self, label: &'static str,
        body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.policy.once.take(label)?;
        self.fence("explicit-cleanup"); // Successful cleanup never resumes business.
        let result = body(self);
        if result.is_err() {
            self.cleanup_errors.push(label);
            self.fence(label);
        }
        result
    }
    fn original(&self) -> Result<()> {
        let owner = self.owner.as_ref().ok_or_else(|| anyhow!("PS30 original owner absent"))?;
        ensure!(self.original_binding == Some(owner.workbench.agent_owner_binding()),
            "PS30 original owner binding changed");
        Ok(())
    }
    fn gate(&self) -> Result<()> {
        self.original()?;
        ensure!(!self.owner.as_ref().unwrap().workbench.product_gate().closed(),
            "PS30 original product gate closed");
        Ok(())
    }
    fn business_time(&self) -> Result<()> {
        if let Some(started) = self.started {
            ensure!(started.elapsed() < Duration::from_secs(30),
                "PS30 scenario deadline; no admission refresh");
        }
        Ok(())
    }
    fn id(&mut self, label: &str) -> Result<[u8; 32]> {
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| anyhow!("PS30 ID overflow"))?;
        let mut h = Sha256::new();
        for part in [b"stage39-ps30-original-owner-v1".as_slice(), self.nonce.as_slice(), label.as_bytes()] {
            h.update((part.len() as u64).to_le_bytes()); h.update(part);
        }
        h.update(self.sequence.to_le_bytes());
        if let Some(k) = self.key.or(self.acknowledged_key) { h.update(k.as_bytes()); }
        let value: [u8; 32] = h.finalize().into();
        ensure!(value != [0; 32], "PS30 nonzero ID"); Ok(value)
    }
    fn request(&mut self, label: &str, operation: Operation) -> Result<Request> {
        let key = self.key.ok_or_else(|| anyhow!("PS30 known original task absent"))?;
        let attachment = if operation == Operation::Attach { 0 } else {
            self.attachment.ok_or_else(|| anyhow!("PS30 known attachment absent"))?
        };
        Ok(Request::empty(self.id(label)?, *key.as_bytes(), attachment, operation))
    }
    fn reserve_evidence(&self) -> Result<()> {
        ensure!(self.evidence.len() < MAX_RECORDS && self.evidence_bytes
            .checked_add(ps::MAX_REQUEST_BYTES + ps::MAX_RESPONSE_BYTES)
            .is_some_and(|n| n <= MAX_EVIDENCE), "PS30 evidence budget before dispatch");
        Ok(())
    }
    fn record(&mut self, profile: &'static str, label: &'static str, input: Vec<u8>, output: Vec<u8>) {
        let gate_closed_after = self.owner.as_ref().is_none_or(|o|o.workbench.product_gate().closed());
        let owner_binding_unchanged = self.owner.as_ref().is_some_and(|o|
            self.original_binding == Some(o.workbench.agent_owner_binding()));
        self.evidence_bytes += input.len() + output.len();
        self.evidence.push(ExchangeEvidence { profile, step: label,
            request_sha256: preflight::hex(&Sha256::digest(&input)),
            response_sha256: preflight::hex(&Sha256::digest(&output)),
            gate_closed_before: false, // The actual gate check passed immediately before dispatch.
            gate_closed_after, owner_binding_unchanged, request: input, response: output });
    }
    fn call(&mut self, label: &'static str, q: &Request) -> Result<Reply> {
        self.gate()?;
        self.reserve_evidence()?;
        let input = q.encode().map_err(|_| anyhow!("PS30 original request encode"))?;
        let result = morrow_workbench_host::protocol::respond(
            &mut self.owner.as_mut().unwrap().workbench, &input);
        let output = match result {
            Ok(output) => output,
            Err(_) => {
                // Preserve the attempted original frame even with no reply.
                self.record("PS30", label, input, Vec::new());
                return Err(anyhow!("PS30 original public response missing; no replay"));
            }
        };
        ensure!(output.len() <= ps::MAX_RESPONSE_BYTES, "PS30 original response exceeds budget");
        let decoded = codec::decode_for(q, &input, self.pending.as_ref(), &output);
        // Actual bytes are retained before fallible checks; carrier IO follows.
        self.record("PS30", label, input, output);
        // The caller must first retain attachment/pending/consumed state from a
        // decoded response, then perform the post-delivery gate check. A gate
        // closing here must never discard known original correlation slots.
        decoded
    }
    fn observe(&mut self, label: &'static str, kind: Kind, expected: Option<TaskKey>) -> Result<()> {
        self.gate()?; self.reserve_evidence()?;
        let q = ad::Request { id: self.id(label)? };
        let input = q.encode()?;
        let result = morrow_workbench_host::protocol::respond(
            &mut self.owner.as_mut().unwrap().workbench, &input);
        let output = match result {
            Ok(output) => output,
            Err(_) => { self.record("AD31", label, input, Vec::new());
                return Err(anyhow!("AD31 public response missing")); }
        };
        ensure!(output.len() <= ad::MAX_FRAME_BYTES, "AD31 output bounds");
        let observed = ad::Observation::decode_for(q, &input, &output);
        self.record("AD31", label, input, output);
        self.gate()?;
        let observed = observed?;
        ensure!(observed.kind == kind && observed.task == expected, "AD31 exact kind/key");
        Ok(())
    }
    fn healthy(reply: &Reply) -> Result<()> {
        ensure!(!reply.delivery_unknown, "PS30 delivery Unknown; no replay"); Ok(())
    }
    pub fn open_owner_once(&mut self) -> Result<()> {
        self.effect("open-owner", Phase::Fresh, |s| {
            preflight::no_reparse_ancestors(s.root.parent().ok_or_else(|| anyhow!("PS30 root parent"))?)?;
            std::fs::create_dir(&s.root)?;
            preflight::no_reparse_ancestors(&s.root)?;
            let owner = ProtectedOwner::open(&s.root)?;
            s.original_binding = Some(owner.workbench.agent_owner_binding());
            s.owner = Some(owner); // Retain before status/maintenance checks.
            s.owner.as_mut().unwrap().require_idle()?;
            ensure!(s.owner.as_ref().unwrap().workbench.maintenance_warning().is_none(),
                "PS30 original protected maintenance warning");
            s.observe("before-start", Kind::None, None)?;
            s.policy.phase = Phase::OwnerOpened; Ok(())
        })
    }
    pub fn review_once(&mut self) -> Result<()> {
        self.effect("review", Phase::OwnerOpened, |s| {
            s.gate()?;
            ensure!(FIXTURE.len() == 425912 && preflight::hex(&Sha256::digest(FIXTURE)) == FIXTURE_SHA,
                "PS30 immutable public synthetic fixture pin");
            let mut seed = u128::from_le_bytes(s.nonce);
            // Leave a fixed, checked range for all catalog requests/pages.
            seed &= u128::MAX >> 1; seed = seed.checked_add(1).unwrap();
            let wrapper = ReviewedWrapper::session_public_r2_v1(
                &mut s.owner.as_mut().unwrap().workbench, s.root.join("ps30-session.wrapper"),
                &s.package_id, FIXTURE, &s.sid, seed)?;
            s.wrapper = Some(wrapper);
            for (label, step) in [
                ("install", SelectionStep::Install), ("base-select", SelectionStep::BaseSelect),
                ("base-enable", SelectionStep::BaseEnable), ("wrapper-select", SelectionStep::WrapperSelect),
                ("approve", SelectionStep::Approve), ("wrapper-enable", SelectionStep::WrapperEnable),
            ] {
                s.policy.once.take(label)?;
                s.wrapper.as_mut().unwrap().step(&mut s.owner.as_mut().unwrap().workbench, step)?;
            }
            let wrapper = s.wrapper.as_ref().unwrap();
            wrapper.approved_revisions()?;
            ensure!(wrapper.approval.session_bits == 3 && wrapper.approval.process_bits == 0
                && wrapper.approval.domain == "session-only", "PS30 session-only genuine review");
            s.facts.push(json!({"step":"review","package":wrapper.id,
                "full_sha256":preflight::hex(&wrapper.full_sha256),
                "base_sha256":preflight::hex(&wrapper.base_sha256),"session_bits":3,"process_bits":0}));
            s.policy.phase = Phase::Reviewed; Ok(())
        })
    }
    pub fn prepare_once(&mut self) -> Result<()> {
        self.effect("prepare", Phase::Reviewed, |s| {
            s.gate()?;
            let prepared = sealed_preparation::prepare_session_guest(
                &mut s.owner.as_mut().unwrap().workbench, &s.sid)?;
            s.prepared = Some(prepared); // Original owner retains any failed preparation debt.
            s.policy.phase = Phase::Prepared; Ok(())
        })
    }
    pub fn start_once(&mut self) -> Result<()> {
        self.effect("start", Phase::Prepared, |s| {
            s.gate()?;
            let wrapper = s.wrapper.as_ref().ok_or_else(|| anyhow!("PS30 wrapper absent"))?;
            let pair = ReviewedWrapper::current_pair(&mut s.owner.as_mut().unwrap().workbench,
                &[wrapper], 1u128 << 127)?;
            let options = AgentStart { package_id: wrapper.id.clone(), full_sha256: wrapper.full_sha256,
                revisions: pair, lifetime: Duration::from_secs(60), limits: AgentLimits::default() };
            // Tracker captures the original task on a failed start as well.
            let key = s.tracker.start_once(&mut WorkbenchAdapter(&mut s.owner.as_mut().unwrap().workbench),
                options, &mut s.prepared.as_mut().ok_or_else(|| anyhow!("PS30 context absent"))?.context,
                &mut s.key)?;
            s.key = Some(key); s.started = Some(Instant::now());
            s.facts.push(json!({"step":"actual-start","task":preflight::hex(key.as_bytes())}));
            s.observe("running", Kind::RunningAgent, Some(key))?;
            s.policy.phase = Phase::Running; Ok(())
        })
    }
    pub fn attach_once(&mut self) -> Result<()> {
        self.effect("attach", Phase::Running, |s| {
            s.business_time()?;
            let q = s.request("attach", Operation::Attach)?;
            let reply = s.call("attach", &q)?;
            if reply.status == Status::Attached && reply.attachment != 0 { s.attachment = Some(reply.attachment); }
            s.gate()?;
            Self::healthy(&reply)?;
            ensure!(reply.status == Status::Attached, "PS30 Attach rejected");
            s.policy.phase = Phase::Attached; Ok(())
        })
    }
    fn enqueue(&mut self, label: &'static str, q: Request, kind: PendingKind, complete: Completion) -> Result<()> {
        ensure!(self.pending.is_none(), "PS30 original command still pending");
        self.business_time()?;
        let reply = self.call(label, &q)?;
        if reply.status == Status::Submitted && reply.submission != 0 {
            self.pending = Some(Pending { token: reply.submission, kind,
                generation: if complete == Completion::Open { None } else { self.generation }, polls: 0 });
            self.completion = Some(complete); // Retain original Submit before subsequent checks.
        }
        self.gate()?;
        Self::healthy(&reply)?;
        ensure!(reply.status == Status::Submitted, "PS30 command rejected; no retry");
        ensure!(reply.lease_live, "PS30 submitted original lease not live");
        if complete != Completion::Open {
            ensure!(Some(reply.generation) == self.generation, "PS30 submitted generation mismatch");
        }
        Ok(())
    }
    pub fn open_once(&mut self) -> Result<()> {
        self.effect("open", Phase::Attached, |s| {
            let q = s.request("open", Operation::Open)?;
            s.enqueue("open", q, PendingKind::Open, Completion::Open)
        })
    }
    fn submit(&mut self, label: &'static str, mut q: Request, complete: Completion) -> Result<()> {
        q.generation = self.generation.ok_or_else(|| anyhow!("PS30 Opened generation absent"))?;
        let original = q.canonical_request().map_err(|_| anyhow!("PS30 original R2 request"))?;
        self.enqueue(label, q, PendingKind::Exchange(original), complete)
    }
    pub fn create_once(&mut self) -> Result<()> {
        self.effect("create", Phase::Opened, |s| {
            let mut q = s.request("create", Operation::Submit)?;
            q.action = Action::Create; q.session_id = s.sid.clone();
            s.submit("create", q, Completion::Create)
        })
    }
    pub fn list_once(&mut self) -> Result<()> {
        self.effect("list", Phase::Created, |s| {
            let mut q = s.request("list", Operation::Submit)?; q.action = Action::List;
            s.submit("list", q, Completion::List)
        })
    }
    pub fn admit_writer_once(&mut self) -> Result<()> {
        self.effect("admit-writer", Phase::Listed, |s| {
            let mut q = s.request("admit-writer", Operation::AdmitWriter)?;
            q.session_id = s.sid.clone();
            s.enqueue("admit-writer", q, PendingKind::AdmitWriter, Completion::AdmitWriter)
        })
    }
    pub fn open_writer_once(&mut self) -> Result<()> {
        self.effect("open-writer", Phase::WriterAdmitted, |s| {
            let mut q = s.request("open-writer", Operation::Submit)?;
            q.action = Action::OpenWriter; q.session_id = s.sid.clone();
            q.writer_slot = s.writer.ok_or_else(|| anyhow!("PS30 writer slot absent"))?;
            q.epoch = s.accepted_info.as_ref().ok_or_else(|| anyhow!("PS30 Create info absent"))?.epoch;
            s.submit("open-writer", q, Completion::OpenWriter)
        })
    }
    pub fn append_once(&mut self) -> Result<()> {
        self.effect("append", Phase::WriterOpened, |s| {
            let info = s.accepted_info.as_ref().ok_or_else(|| anyhow!("PS30 writer info absent"))?.clone();
            let mut q = s.request("append", Operation::Submit)?;
            q.action = Action::Append; q.session_id = s.sid.clone();
            q.writer_slot = s.writer.ok_or_else(|| anyhow!("PS30 writer slot absent"))?;
            q.epoch = info.epoch; q.expected_tail = info.tail; q.events.push(s.expected_event.clone());
            s.submit("append", q, Completion::Append)
        })
    }
    pub fn snapshot_once(&mut self) -> Result<()> {
        self.effect("snapshot", Phase::Appended, |s| {
            let mut q = s.request("snapshot", Operation::Submit)?;
            q.action = Action::Snapshot; q.session_id = s.sid.clone(); q.after = 0; q.limit = 16;
            s.submit("snapshot", q, Completion::Snapshot)
        })
    }
    /// One original read only. A correlated Pending permits another Read with
    /// a new request ID and this same pending token, never another submission.
    pub fn read_once(&mut self) -> Result<ReadProgress> {
        ensure!(self.policy.primary_failure.is_none(), "PS30 uncertain Read is not replayable");
        let result = self.read_inner();
        if result.is_err() { self.fence("read"); }
        result
    }
    fn read_inner(&mut self) -> Result<ReadProgress> {
        self.business_time()?;
        let pending = self.pending.as_mut().ok_or_else(|| anyhow!("PS30 original pending absent"))?;
        pending.polls = pending.polls.checked_add(1).ok_or_else(|| anyhow!("PS30 poll overflow"))?;
        ensure!(pending.polls <= MAX_POLLS, "PS30 bounded Read polling deadline");
        let token = pending.token;
        let mut q = self.request("read", Operation::Read)?; q.submission = token;
        let reply = self.call("read", &q)?;
        if reply.status == Status::Pending {
            self.gate()?;
            Self::healthy(&reply)?; return Ok(ReadProgress::Pending);
        }
        if matches!(reply.status, Status::Opened | Status::WriterAdmitted | Status::Reply) {
            // The real original handle was consumed: record local state first.
            self.pending.take();
            if reply.status == Status::Opened { self.generation = Some(reply.generation); }
            if reply.status == Status::WriterAdmitted { self.writer = Some(reply.writer_slot); }
            self.last_outcome = reply.outcome.clone();
            self.last_completion = Some(json!({"status":format!("{:?}",reply.status),
                "submission":reply.submission,"generation":reply.generation,
                "writer_slot":reply.writer_slot,"lease_live":reply.lease_live,
                "delivery_unknown":reply.delivery_unknown,
                "canonical_request_sha256":preflight::hex(&reply.canonical_request_sha256),
                "canonical_reply_sha256":preflight::hex(&Sha256::digest(&reply.canonical_reply))}));
            let completed = self.completion.take().ok_or_else(|| anyhow!("PS30 completion kind absent"))?;
            self.gate()?;
            Self::healthy(&reply)?;
            ensure!(reply.lease_live, "PS30 original completed lease not live");
            self.accept(completed, reply)?;
            Ok(ReadProgress::Complete)
        } else { Err(anyhow!("PS30 Read rejected; keep original state, no replay")) }
    }
    fn accept(&mut self, completed: Completion, reply: Reply) -> Result<()> {
        use r2::Outcome;
        let before = self.accepted_info.clone();
        match completed {
            Completion::Open => {
                ensure!(reply.status == Status::Opened && Some(reply.generation)
                    == self.prepared.as_ref().map(|p|p.generation), "PS30 original Open generation");
                self.policy.phase = Phase::Opened;
            }
            Completion::AdmitWriter => {
                ensure!(reply.status == Status::WriterAdmitted && reply.writer_slot != 0
                    && Some(reply.generation) == self.generation, "PS30 actual writer admission");
                self.policy.phase = Phase::WriterAdmitted;
            }
            Completion::Create => {
                let Some(Outcome::Session(info)) = reply.outcome else { return Err(anyhow!("PS30 Create rejected/outcome")); };
                ensure!(info.session_id == self.sid && info.parent.is_none() && info.epoch == 0
                    && info.tail == 0 && info.floor == 1 && !info.archived && !info.checkpoint_sealed
                    && info.checkpoint_sha256 == r2::hash(&[]), "PS30 exact fresh session");
                self.accepted_info = Some(info); self.policy.phase = Phase::Created;
            }
            Completion::List => {
                let Some(Outcome::Sessions(infos)) = reply.outcome else { return Err(anyhow!("PS30 List rejected/outcome")); };
                ensure!(infos.len() == 1 && Some(&infos[0]) == before.as_ref(), "PS30 exact scoped list");
                self.policy.phase = Phase::Listed;
            }
            Completion::OpenWriter | Completion::Append => {
                let Some(Outcome::Session(info)) = reply.outcome else { return Err(anyhow!("PS30 writer mutation rejected/outcome")); };
                let old = before.ok_or_else(|| anyhow!("PS30 accepted prior info absent"))?;
                let mut expected = old.clone();
                expected.revision = old.revision.checked_add(1).ok_or_else(|| anyhow!("PS30 revision overflow"))?;
                if completed == Completion::OpenWriter {
                    expected.epoch = old.epoch.checked_add(1).ok_or_else(|| anyhow!("PS30 epoch overflow"))?;
                } else { expected.tail = old.tail.checked_add(1).ok_or_else(|| anyhow!("PS30 tail overflow"))?; }
                ensure!(info == expected, "PS30 exact writer epoch/tail/ancestry/checkpoint");
                self.accepted_info = Some(info);
                self.policy.phase = if completed == Completion::OpenWriter { Phase::WriterOpened } else { Phase::Appended };
            }
            Completion::Snapshot => {
                let Some(Outcome::Snapshot(snapshot)) = reply.outcome else { return Err(anyhow!("PS30 Snapshot rejected/outcome")); };
                ensure!(Some(&snapshot.info) == before.as_ref() && snapshot.events.len() == 1
                    && snapshot.events[0].sequence == 1 && snapshot.events[0].event == self.expected_event
                    && !snapshot.gap && snapshot.checkpoint.is_empty(), "PS30 exact persisted event Snapshot");
                self.facts.push(json!({"step":"snapshot","event_id":self.expected_event.event_id,
                    "body_sha256":preflight::hex(&r2::hash(&self.expected_event.body)),
                    "epoch":snapshot.info.epoch,"tail":snapshot.info.tail,"revision":snapshot.info.revision}));
                self.policy.phase = Phase::Snapshotted;
            }
        }
        Ok(())
    }
    fn close_writer_inner(&mut self) -> Result<bool> {
        let mut q = self.request("close-writer", Operation::CloseWriter)?;
        q.writer_slot = self.writer.ok_or_else(|| anyhow!("PS30 exact writer absent"))?;
        let reply = self.call("close-writer", &q)?;
        if reply.status == Status::Observed { self.writer = None; }
        self.gate()?;
        ensure!(reply.status == Status::Observed, "PS30 CloseWriter rejected");
        Ok(reply.delivery_unknown)
    }
    pub fn close_writer_once(&mut self) -> Result<()> {
        self.effect("close-writer", Phase::Snapshotted, |s| {
            ensure!(s.pending.is_none(), "PS30 command pending before close");
            let unknown = s.close_writer_inner()?;
            ensure!(!unknown, "PS30 CloseWriter delivery Unknown");
            // Close may seal an original controller if a pending writer existed.
            s.policy.phase = Phase::WriterClosed; Ok(())
        })
    }
    fn detach_inner(&mut self) -> Result<bool> {
        let q = self.request("detach", Operation::Detach)?;
        let reply = self.call("detach", &q)?;
        ensure!(reply.status == Status::Detached, "PS30 Detach rejected");
        self.attachment = None; self.writer = None; self.pending = None; self.completion = None;
        self.gate()?;
        Ok(reply.delivery_unknown)
    }
    pub fn detach_once(&mut self) -> Result<()> {
        self.effect("detach", Phase::WriterClosed, |s| {
            ensure!(s.pending.is_none(), "PS30 command pending before Detach");
            let unknown = s.detach_inner()?;
            ensure!(!unknown, "PS30 Detached Unknown");
            s.policy.phase = Phase::Detached; Ok(())
        })
    }
    fn save_snapshot(&mut self, snapshot: AgentSnapshot, step: &'static str) -> Result<AgentSnapshot> {
        self.last_snapshot = Some(snapshot); // Retain even when identity/cleanup checks fail.
        // Keep only terminal/control facts; repeated nonterminal polls overwrite
        // last_snapshot instead of accumulating an unbounded observation log.
        if !matches!(step, "poll-join" | "cleanup-poll") || snapshot.task.storage == StoragePhase::Reclaimed {
            ensure!(self.facts.len() < 128, "PS30 finite lifecycle facts");
            self.facts.push(json!({"step":step,"actual_snapshot":format!("{snapshot:?}")}));
        }
        ensure!(snapshot.task.key == self.key, "PS30 original task identity changed");
        Ok(snapshot)
    }
    fn stop_inner(&mut self) -> Result<()> {
        self.original()?;
        let key = self.key.ok_or_else(|| anyhow!("PS30 original task unknown; cannot adopt"))?;
        self.stopping = Some(Instant::now());
        let snapshot = product::stop_original(&mut self.owner.as_mut().unwrap().workbench, key)?;
        self.save_snapshot(snapshot, "stop")?; Ok(())
    }
    pub fn stop_once(&mut self) -> Result<()> {
        self.effect("stop", Phase::Detached, |s| {
            s.stop_inner()?; s.policy.phase = Phase::Stopping; Ok(())
        })
    }
    fn poll_inner(&mut self) -> Result<AgentSnapshot> {
        self.original()?;
        self.join_polls = self.join_polls.checked_add(1).ok_or_else(|| anyhow!("PS30 join poll overflow"))?;
        ensure!(self.join_polls <= MAX_POLLS && self.facts.len() < 128, "PS30 finite join polling");
        ensure!(self.stopping.is_some_and(|t|t.elapsed() < Duration::from_secs(20)),
            "PS30 actual join deadline; explicit cleanup required");
        let key = self.key.ok_or_else(|| anyhow!("PS30 original task absent"))?;
        let snapshot = product::poll_original(&mut self.owner.as_mut().unwrap().workbench, key)?;
        self.save_snapshot(snapshot, "poll-join")
    }
    pub fn poll_join_once(&mut self) -> Result<bool> {
        self.policy.require(Phase::Stopping)?;
        let result = (|| -> Result<bool> {
            let snapshot = self.poll_inner()?;
            if snapshot.task.storage != StoragePhase::Reclaimed { return Ok(false); }
            let exit = snapshot.exit.ok_or_else(|| anyhow!("PS30 actual joined exit absent"))?;
            ensure!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok(),
                "PS30 original execution/disconnect/maintenance failed");
            self.policy.phase = Phase::Reclaimed; Ok(true)
        })();
        if result.is_err() { self.fence("poll-join"); } result
    }
    pub fn observe_retained_once(&mut self) -> Result<()> {
        self.effect("observe-retained", Phase::Reclaimed, |s| {
            let key = s.key.ok_or_else(|| anyhow!("PS30 known key absent"))?;
            s.observe("retained-before-ack", Kind::RetainedAgentExit, Some(key))?;
            s.policy.phase = Phase::RetainedExit; Ok(())
        })
    }
    fn ack_inner(&mut self) -> Result<()> {
        self.original()?;
        let key = self.key.ok_or_else(|| anyhow!("PS30 original ACK key absent"))?;
        self.owner.as_mut().unwrap().workbench.acknowledge_agent(key)
            .map_err(|_| anyhow!("PS30 original ACK failed; retain key"))?;
        self.acknowledged_key = Some(key); self.key = None; // Before observation or carrier IO.
        self.facts.push(json!({"step":"actual-ack","task":preflight::hex(key.as_bytes())}));
        Ok(())
    }
    pub fn ack_once(&mut self) -> Result<()> {
        self.effect("ack", Phase::RetainedExit, |s| {
            s.ack_inner()?;
            s.observe("after-ack", Kind::None, None)?;
            s.acknowledged_observed = true; s.policy.phase = Phase::Acknowledged; Ok(())
        })
    }
    fn finish_inner(&mut self) -> Result<()> {
        self.original()?;
        ensure!(self.key.is_none() && self.pending.is_none() && self.attachment.is_none(),
            "PS30 original authority/task cleanup incomplete");
        ensure!(self.prepared.as_ref().is_none_or(|p|p.context.is_none()),
            "PS30 external original context not disposed");
        self.owner.as_mut().unwrap().require_idle()?;
        self.owner.as_mut().unwrap().finish()?;
        self.prepared.take();
        self.facts.push(json!({"step":"actual-finish","scope":"PROTECTED_OWNER_RUST_ONLY"}));
        Ok(())
    }
    pub fn finish_once(&mut self) -> Result<()> {
        self.effect("finish", Phase::Acknowledged, |s| {
            ensure!(s.acknowledged_observed, "PS30 after-ACK observation required");
            s.finish_inner()?; s.policy.phase = Phase::Finished; Ok(())
        })
    }

    // Explicit rights reduction/cleanup remains available after Fenced. These
    // latches share normal action labels, so a failed normal action is not replayed.
    pub fn cleanup_close_writer_once(&mut self) -> Result<()> {
        self.cleanup("close-writer", |s| { s.close_writer_inner()?; Ok(()) })
    }
    pub fn cleanup_detach_once(&mut self) -> Result<()> {
        self.cleanup("detach", |s| { s.detach_inner()?; Ok(()) })
    }
    pub fn cleanup_stop_once(&mut self) -> Result<()> { self.cleanup("stop", |s| s.stop_inner()) }
    pub fn cleanup_poll_once(&mut self) -> Result<AgentSnapshot> {
        self.fence("explicit-cleanup");
        self.original()?;
        self.cleanup_polls = self.cleanup_polls.checked_add(1).ok_or_else(|| anyhow!("PS30 cleanup poll overflow"))?;
        ensure!(self.cleanup_polls <= MAX_POLLS && self.facts.len() < 128, "PS30 finite cleanup polling");
        let key = self.key.ok_or_else(|| anyhow!("PS30 exact cleanup key absent"))?;
        let snapshot = product::poll_original(&mut self.owner.as_mut().unwrap().workbench, key)?;
        self.save_snapshot(snapshot, "cleanup-poll")
    }
    pub fn cleanup_recover_once(&mut self) -> Result<()> {
        self.cleanup("recover-original-cleanup", |s| {
            s.original()?;
            let key = s.key.ok_or_else(|| anyhow!("PS30 exact recovery key absent"))?;
            let snapshot = product::recover_original(&mut s.owner.as_mut().unwrap().workbench, key)?;
            s.save_snapshot(snapshot, "cleanup-recover")?; Ok(())
        })
    }
    pub fn cleanup_ack_once(&mut self) -> Result<()> {
        self.cleanup("ack", |s| {
            let snapshot = s.last_snapshot.ok_or_else(|| anyhow!("PS30 actual cleanup snapshot absent"))?;
            ensure!(snapshot.task.key == s.key && snapshot.task.storage == StoragePhase::Reclaimed,
                "PS30 exact joined cleanup required before ACK");
            s.ack_inner() // Original ACK alone decides whether current repair is complete.
        })
    }
    pub fn cleanup_dispose_context_once(&mut self) -> Result<()> {
        self.cleanup("dispose-context", |s| {
            s.original()?;
            s.tracker.precheck(&mut WorkbenchAdapter(&mut s.owner.as_mut().unwrap().workbench), s.key)?;
            let p = s.prepared.as_mut().ok_or_else(|| anyhow!("PS30 external context absent"))?;
            ensure!(p.context.is_some(), "PS30 context already consumed/disposed");
            s.owner.as_mut().unwrap().workbench.dispose_agent_context(&mut p.context)
                .map_err(|_| anyhow!("PS30 original context cleanup retained"))
        })
    }
    pub fn cleanup_repair_preparation_once(&mut self) -> Result<()> {
        self.cleanup("repair-original-preparation", |s| {
            s.original()?;
            s.owner.as_mut().unwrap().workbench.repair_agent_preparation()
                .map_err(|_| anyhow!("PS30 original preparation debt remains"))
        })
    }
    pub fn cleanup_finish_once(&mut self) -> Result<()> {
        // Cleanup completion never changes the primary failed classification.
        self.cleanup("finish", |s| s.finish_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attempted_effect_is_permanent_even_after_failure_or_success() {
        let mut once = Once::default();
        once.take("append").unwrap();
        assert!(once.take("append").is_err());
        once.take("stop").unwrap();
        assert!(once.take("stop").is_err()); // Normal/cleanup share the same latch.
    }
    #[test]
    fn fencing_keeps_first_failure_and_blocks_business_even_if_phase_is_changed() {
        let mut p = Policy { phase: Phase::WriterOpened, once: Once::default(), primary_failure: None };
        p.fence("lost-append"); p.fence("later-cleanup");
        assert_eq!(p.primary_failure, Some("lost-append"));
        p.phase = Phase::WriterOpened;
        assert!(p.require(Phase::WriterOpened).is_err());
        p.once.take("detach").unwrap(); // Reduction latch remains available.
    }
    #[test]
    fn ack_phase_requires_the_retained_exit_observation() {
        let mut p = Policy { phase: Phase::Reclaimed, once: Once::default(), primary_failure: None };
        assert!(p.require(Phase::RetainedExit).is_err());
        p.phase = Phase::RetainedExit;
        assert!(p.require(Phase::RetainedExit).is_ok());
        p.once.take("ack").unwrap();
        assert!(p.once.take("ack").is_err());
    }
}
