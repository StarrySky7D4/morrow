//! Trusted in-process, nonblocking session control/writing on one existing AgentWorker.
//! No wire, preparation, replacement owner, executor approval or native Start.
use super::{AgentCommand, AgentCommandHandle, AgentCommandSnapshot, AgentError, AgentReply};
use super::worker::NativeCommandQueue;
use crate::{Workbench, io_tasks::TaskKey};
use morrow_agent_session_exec_v1_r2::{Action, Error, Outcome, Reply, Request};
use morrow_agent_session_process_v1_host::native_session::{
    NativeEndpointAuthority, MAX_NATIVE_FRAME_BYTES,
};
use morrow_core::dispatch::HostBinding;
use zeroize::Zeroizing;

const MAX_COMMANDS: u64 = 16; // Open + writer admission + every exchange, together.
const MAX_EXCHANGES: usize = 15;
static NEXT_CONTROLLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Controller-local correlation only; never an endpoint ID or admission secret.
/// No raw-ID accessor/serialization. Foreign/stale controller slots are invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Writer { namespace: u64, slot: u64 }
struct WriterLease {
    writer: Writer,
    session_id: String,
    authority: NativeEndpointAuthority,
}

/// Local correlation only. It cannot select a worker, endpoint or admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Submission(u64);

#[derive(Clone, Copy, Debug)]
pub struct SessionStatus {
    pub task: TaskKey,
    pub endpoint_generation: Option<u64>,
    pub pending: Option<Submission>,
    pub command_observation: Option<AgentCommandSnapshot>,
    pub original_lease_live: bool,
    /// This controller's sticky observation only. false is not a shared-lease
    /// recovery proof; a new controller cannot clear server delivery Unknown.
    pub delivery_unknown: bool,
    pub last_error: Option<AgentError>,
}

pub enum SessionCompletion {
    Opened { generation: u64 },
    WriterAdmitted { writer: Writer, generation: u64 },
    /// Canonical correlated R2 reply. A definite rejection remains a rejection.
    Reply(Zeroizing<Vec<u8>>),
}
pub enum SessionPoll {
    Pending,
    Complete(SessionCompletion),
}

enum PendingKind {
    Open,
    AdmitWriter { session_id: String },
    Exchange { request: Request, writer: Option<Writer> },
}
struct Pending {
    submission: Submission,
    handle: AgentCommandHandle,
    kind: PendingKind,
}

/// Non-Clone. At most one original command handle is retained at any time.
/// Constructed only from the validated original StateSlot's worker. Dropping
/// this adapter revokes its control/writer endpoints; it does not join, acknowledge or recover.
pub struct ProductSessionController {
    task: TaskKey,
    owner: HostBinding,
    queue: NativeCommandQueue,
    endpoint: Option<NativeEndpointAuthority>,
    namespace: Option<u64>,
    writers: Vec<WriterLease>,
    pending: Option<Pending>,
    next: u64,
    open_submitted: bool,
    seen: Vec<String>,
    delivery_armed: bool,
    sealed: bool,
    last_error: Option<AgentError>,
    #[cfg(test)]
    observer: Option<std::sync::Arc<dyn DispatchObserver>>,
}

impl Workbench {
    /// Attaches only to an already running exact Agent task. This never starts
    /// a task, opens a Store, prepares a context or refreshes any admission.
    pub fn product_session_controller(
        &self, task: TaskKey,
    ) -> crate::Result<ProductSessionController> {
        let worker = self.state.agent_worker(task)?;
        Ok(ProductSessionController::from_original(
            task, self.agent_owner_binding(), worker.native_command_queue(),
        ))
    }
}

impl ProductSessionController {
    pub(super) fn from_original(
        task: TaskKey, owner: HostBinding, queue: NativeCommandQueue,
    ) -> Self {
        Self {
            task, owner, queue, endpoint: None, pending: None, next: 0,
            namespace: NEXT_CONTROLLER.fetch_update(
                std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst,
                |current| current.checked_add(1),
            ).ok(),
            writers: Vec::new(),
            open_submitted: false, seen: Vec::new(), delivery_armed: false,
            sealed: false, last_error: None,
            #[cfg(test)] observer: None,
        }
    }
    pub fn owner_binding(&self) -> HostBinding { self.owner }

    /// Pure observation of the retained queue/authority and finite diagnostics.
    /// NotObserved, closed or pending is not an effect/exit/EOF/join proof.
    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            task: self.task,
            endpoint_generation: self.endpoint.as_ref().map(|e| e.generation()),
            pending: self.pending.as_ref().map(|p| p.submission),
            command_observation: self.pending.as_ref().map(|p| p.handle.diagnostic_snapshot()),
            original_lease_live: self.live(),
            delivery_unknown: self.fenced(),
            last_error: self.last_error,
        }
    }
    fn live(&self) -> bool { self.live_for(None) }
    fn live_for(&self, writer: Option<Writer>) -> bool {
        let now = self.queue.now_millis();
        self.queue.live_at(now)
            && self.endpoint.as_ref().is_none_or(|e| e.is_live_at(now))
            && writer.is_none_or(|writer| self.writer_lease(writer)
                .is_some_and(|lease| lease.authority.is_live_at(now)))
    }
    fn writer_lease(&self, writer: Writer) -> Option<&WriterLease> {
        if self.namespace != Some(writer.namespace) { return None; }
        self.writers.iter().find(|lease| lease.writer == writer)
    }
    fn fenced(&self) -> bool {
        self.sealed || (self.delivery_armed && self.pending.is_none())
    }
    fn ready(&self) -> Result<(), AgentError> {
        if self.namespace.is_none() { return Err(AgentError::Limit); }
        if self.pending.is_some() { return Err(AgentError::Busy); }
        if self.fenced() { return Err(AgentError::Unknown); }
        if !self.live() { return Err(AgentError::Unavailable); }
        Ok(())
    }
    fn seal(&mut self, error: AgentError) {
        self.sealed = true;
        self.last_error = Some(error);
        if let Some(endpoint) = &self.endpoint { endpoint.mark_unknown(); }
        for lease in &self.writers { lease.authority.mark_unknown(); }
    }
    fn enqueue(
        &mut self, command: AgentCommand, kind: PendingKind,
    ) -> Result<Submission, AgentError> {
        let Some(next) = self.next.checked_add(1).filter(|next| *next <= MAX_COMMANDS) else {
            self.delivery_armed = false;
            self.last_error = Some(AgentError::Limit);
            return Err(AgentError::Limit);
        };
        #[cfg(test)]
        if let Some(observer) = &self.observer { observer.enqueued(&command); }
        match self.queue.submit(command) {
            Ok(handle) => {
                self.next = next;
                let submission = Submission(next);
                self.pending = Some(Pending { submission, handle, kind });
                Ok(submission)
            }
            Err(error) => {
                // Existing submit_command proves Busy/Limit did not enqueue.
                // No automatic retry even for these definite rejections.
                if matches!(error, AgentError::Busy | AgentError::Limit) {
                    self.delivery_armed = false;
                } else { self.seal(error); }
                self.last_error = Some(error);
                Err(error)
            }
        }
    }
    pub fn open(&mut self) -> Result<Submission, AgentError> {
        self.ready()?;
        if self.open_submitted { return Err(AgentError::Invalid); }
        // Open issues an admission: losing its reply is not safe to repeat.
        self.delivery_armed = true;
        let submission = self.enqueue(AgentCommand::OpenNativeSession, PendingKind::Open)?;
        self.open_submitted = true;
        Ok(submission)
    }
    /// Issue one genuine exact-session admission on the existing worker. One
    /// issued grant per SID on this controller; close never refreshes that admission.
    pub fn admit_writer(&mut self, session_id: &str) -> Result<Submission, AgentError> {
        self.ready()?;
        self.endpoint.as_ref().ok_or(AgentError::Unavailable)?;
        if session_id.len() > MAX_NATIVE_FRAME_BYTES { return Err(AgentError::Limit); }
        Action::OpenWriter { session_id: session_id.to_owned(), expected_epoch: 0 }
            .validate().map_err(AgentError::Session)?;
        if self.writers.iter().any(|lease| lease.session_id == session_id) {
            return Err(AgentError::Invalid);
        }
        self.delivery_armed = true;
        self.enqueue(AgentCommand::NativeSessionWriter { session_id: session_id.to_owned() },
            PendingKind::AdmitWriter { session_id: session_id.to_owned() })
    }
    pub fn exchange(&mut self, canonical: &[u8]) -> Result<Submission, AgentError> {
        self.exchange_on(None, canonical)
    }
    pub fn writer_exchange(&mut self, writer: Writer, canonical: &[u8]) -> Result<Submission, AgentError> {
        self.exchange_on(Some(writer), canonical)
    }
    fn exchange_on(&mut self, writer: Option<Writer>, canonical: &[u8]) -> Result<Submission, AgentError> {
        self.ready()?;
        if canonical.len() > MAX_NATIVE_FRAME_BYTES { return Err(AgentError::Limit); }
        let request = Request::decode(canonical).map_err(AgentError::Session)?;
        let (endpoint, mutation) = match writer {
            None => {
                let mutation = match request.action() {
                    Action::List | Action::Snapshot { .. } => false,
                    Action::Create { .. } => true,
                    _ => return Err(AgentError::Unsupported),
                };
                (self.endpoint.as_ref().ok_or(AgentError::Unavailable)?, mutation)
            }
            Some(writer) => {
                let lease = self.writer_lease(writer).ok_or(AgentError::Invalid)?;
                let (sid, mutation) = match request.action() {
                    Action::Snapshot { session_id, .. } => (session_id, false),
                    Action::OpenWriter { session_id, .. } | Action::Append { session_id, .. } => (session_id, true),
                    _ => return Err(AgentError::Unsupported),
                };
                if sid != &lease.session_id { return Err(AgentError::Session(Error::Denied)); }
                if !self.live_for(Some(writer)) { return Err(AgentError::Unavailable); }
                (&lease.authority, mutation)
            }
        };
        if request.generation() != endpoint.generation() {
            return Err(AgentError::Session(Error::Correlation));
        }
        if self.seen.len() >= MAX_EXCHANGES { return Err(AgentError::Limit); }
        if self.seen.iter().any(|old| old == request.id()) { return Err(AgentError::Invalid); }
        let id = request.id().to_owned();
        let command = AgentCommand::NativeSessionExchange {
            endpoint: endpoint.id(), canonical: Zeroizing::new(canonical.to_vec()),
        };
        self.delivery_armed = mutation;
        let submission = self.enqueue(command, PendingKind::Exchange { request, writer })?;
        self.seen.push(id);
        Ok(submission)
    }
    /// Exactly one try_read call per poll. Empty retains the same handle. A
    /// terminal result consumes it once; subsequent reads are Invalid, not replay.
    pub fn poll(&mut self, submission: Submission) -> Result<SessionPoll, AgentError> {
        if self.pending.as_ref().map(|p| p.submission) != Some(submission) {
            return Err(AgentError::Invalid);
        }
        let mut pending = self.pending.take().expect("matched above");
        let result = pending.handle.try_read();
        match result {
            Ok(None) => {
                self.pending = Some(pending);
                Ok(SessionPoll::Pending)
            }
            Err(error) => {
                if !self.sealed && definite_rejection(error) {
                    self.delivery_armed = false;
                } else { self.seal(error); }
                self.last_error = Some(error);
                Err(error)
            }
            Ok(Some(reply)) => {
                // AgentReply has Drop (sensitive buffers); borrow it and retain
                // only a clone of the genuine authority or a zeroizing reply.
                let completion = match (&pending.kind, &reply) {
                    (PendingKind::Open, AgentReply::NativeEndpoint(authority)) => {
                        // Retain the actual returned admission, even if delivery
                        // was lost or it expired before this original read.
                        self.endpoint = Some(authority.clone());
                        SessionCompletion::Opened {
                            generation: self.endpoint.as_ref().expect("retained").generation(),
                        }
                    }
                    (PendingKind::AdmitWriter { session_id }, AgentReply::NativeEndpoint(authority)) => {
                        let control = self.endpoint.as_ref().expect("admission requires Open");
                        if authority.generation() != control.generation() || authority.id() == control.id()
                            || self.writers.iter().any(|lease| lease.authority.id() == authority.id()) {
                            authority.mark_unknown();
                            let _ = authority.close();
                            self.seal(AgentError::Unknown);
                            return Err(AgentError::Unknown);
                        }
                        let writer = Writer { namespace: self.namespace.expect("ready checked"),
                            slot: pending.submission.0 };
                        // Retain the actual issued authority even after loss of
                        // delivery; a late response is never a recovery grant.
                        self.writers.push(WriterLease { writer, session_id: session_id.clone(),
                            authority: authority.clone() });
                        SessionCompletion::WriterAdmitted { writer, generation: authority.generation() }
                    }
                    (PendingKind::Exchange { request, .. }, AgentReply::NativeSession(bytes)) => {
                        let decoded = match Reply::decode_for(&request, &bytes) {
                            Ok(decoded) => decoded,
                            Err(_) => { self.seal(AgentError::Unknown); return Err(AgentError::Unknown); }
                        };
                        if let Outcome::Rejected(error @ (Error::CommitUnknown | Error::Storage)) = &decoded.outcome {
                            let error = AgentError::Session(*error);
                            self.seal(error);
                            return Err(error);
                        }
                        // A correlated rejection is preserved in the canonical
                        // response; it is not translated to session success.
                        SessionCompletion::Reply(Zeroizing::new(bytes.to_vec()))
                    }
                    _ => { self.seal(AgentError::Unknown); return Err(AgentError::Unknown); }
                };
                let selected = match &pending.kind {
                    PendingKind::Exchange { writer, .. } => *writer,
                    PendingKind::AdmitWriter { .. } => match &completion {
                        SessionCompletion::WriterAdmitted { writer, .. } => Some(*writer),
                        _ => unreachable!("matched genuine admission"),
                    },
                    PendingKind::Open => None,
                };
                if self.sealed || !self.live_for(selected) {
                    self.seal(AgentError::Unknown);
                    return Err(AgentError::Unknown);
                }
                self.delivery_armed = false;
                self.last_error = None;
                Ok(SessionPoll::Complete(completion))
            }
        }
    }
    /// Called by a trusted outer transport after missing delivery. No inverse,
    /// new request, admission refresh or replacement controller is recovery.
    pub fn mark_delivery_unknown(&mut self) { self.seal(AgentError::Unknown); }
    /// Rights reduction on this controller's exact retained writer only. There
    /// is no R2 CloseWriter action. Keep revoked grants/tombstones for the
    /// original owner's cleanup debt; this is neither join nor session retirement.
    pub fn close_writer(&mut self, writer: Writer) -> Result<(), AgentError> {
        let authority = self.writer_lease(writer).ok_or(AgentError::Invalid)?.authority.clone();
        if authority.is_closed() { return Err(AgentError::Invalid); }
        if self.pending.as_ref().is_some_and(|pending| matches!(&pending.kind,
            PendingKind::Exchange { writer: Some(selected), .. } if *selected == writer)) {
            self.seal(AgentError::Unknown); // retain the same pending handle to drain
        }
        match authority.close().map_err(AgentError::Session) {
            Ok(()) => Ok(()),
            Err(error) => { self.seal(error); Err(error) }
        }
    }
    pub fn retire(&self, _session_id: &str) -> Result<(), AgentError> {
        Err(AgentError::Unsupported)
    }
}

fn definite_rejection(error: AgentError) -> bool {
    matches!(error, AgentError::Busy | AgentError::Limit | AgentError::Session(
        Error::Denied | Error::Invalid | Error::Contract | Error::Correlation |
        Error::Conflict | Error::NotFound | Error::Limit
    ))
}
impl Drop for ProductSessionController {
    fn drop(&mut self) {
        if self.delivery_armed || self.pending.is_some() || self.sealed {
            if let Some(endpoint) = &self.endpoint { endpoint.mark_unknown(); }
        }
        for lease in &self.writers {
            if self.delivery_armed || self.pending.is_some() || self.sealed { lease.authority.mark_unknown(); }
            let _ = lease.authority.close();
        }
        if let Some(endpoint) = &self.endpoint { let _ = endpoint.close(); }
        // Pending's real AgentCommandHandle Drop requests original cancellation.
        // The original worker still owns all join/cleanup/retirement obligations.
    }
}

/// Observes attempted dispatch only, never substitutes a handle or admission.
/// Absent from production builds; the real worker remains the test executor.
#[cfg(test)]
pub(super) trait DispatchObserver: Send + Sync {
    fn enqueued(&self, command: &AgentCommand);
}
#[cfg(test)]
impl ProductSessionController {
    pub(super) fn observe_dispatch(&mut self, observer: std::sync::Arc<dyn DispatchObserver>) {
        self.observer = Some(observer);
    }
}
