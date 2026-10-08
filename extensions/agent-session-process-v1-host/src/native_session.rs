//! Native canonical session transport on one existing catalog/manager connection.
//! The owner retains this lease. Client handles contain revocation, never a Core or Store.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_NATIVE_ENDPOINTS: usize = 17; // one control plus sixteen approved writers
pub const MAX_NATIVE_FRAME_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NativeEndpointId([u8; 32]);
impl std::fmt::Debug for NativeEndpointId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeEndpointId([redacted])")
    }
}

struct Endpoint {
    id: NativeEndpointId,
    host: Arc<SessionExecHost>,
    admission: Admission,
    revoke: morrow_agent_session_exec_v1_r2::authority::AdmissionRevocation,
    writer: Option<String>,
    created: u64,
    expires: u64,
    closed: AtomicBool,
    uncertain: Arc<AtomicBool>,
    delivery_unknown: Arc<AtomicBool>,
    live: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl Endpoint {
    fn close(&self) -> Result<()> {
        // Reject every clone before touching the original authority. Failed
        // revocation remains retained by NativeSessionLease for explicit cleanup.
        self.closed.store(true, Ordering::Release);
        self.revoke.revoke();
        self.host.revoke(&self.admission)
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.revoke.revoke();
        let _ = self.host.revoke(&self.admission);
    }
}

/// Original-admission revocation can happen off the owner thread. Dispatch cannot.
#[derive(Clone)]
pub struct NativeEndpointAuthority {
    endpoint: Arc<Endpoint>,
    generation: u64,
}
impl NativeEndpointAuthority {
    pub fn id(&self) -> NativeEndpointId {
        self.endpoint.id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn is_closed(&self) -> bool {
        self.endpoint.closed.load(Ordering::Acquire)
    }
    pub fn is_live_at(&self, now: u64) -> bool {
        !self.is_closed()
            && !self.endpoint.revoke.is_revoked()
            && now >= self.endpoint.created
            && now < self.endpoint.expires
            && (self.endpoint.live)()
            && self
                .endpoint
                .host
                .admission_live_at(&self.endpoint.admission, now)
    }
    /// Lost mutation delivery can only reduce this original lease's rights.
    /// There is deliberately no inverse operation or TTL refresh.
    pub fn mark_unknown(&self) {
        self.endpoint
            .delivery_unknown
            .store(true, Ordering::Release);
    }
    pub fn close(&self) -> Result<()> {
        self.endpoint.close()
    }
}

/// Dedicated native admissions on the exact already approved connection.
/// It never changes the persisted approval, expiry, owner or generation.
pub struct NativeSessionLease {
    connection: Arc<Connection>,
    host: Arc<SessionExecHost>,
    owner: HostBinding,
    generation: u64,
    declared: SessionCapabilities,
    sessions: Vec<String>,
    domain: String,
    created: u64,
    expires: u64,
    endpoints: Vec<Arc<Endpoint>>,
    uncertain: Arc<AtomicBool>,
    delivery_unknown: Arc<AtomicBool>,
    live: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl NativeSessionLease {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        runtime: &HostRuntime,
        host: Arc<SessionExecHost>,
        connection: Arc<Connection>,
        generation: u64,
        declared: SessionCapabilities,
        sessions: Vec<String>,
        domain: String,
        created: u64,
        expires: u64,
        live: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> (Self, Result<()>) {
        let mut lease = Self {
            connection,
            host,
            owner: runtime.binding(),
            generation,
            declared,
            sessions,
            domain,
            created,
            expires,
            endpoints: Vec::new(),
            uncertain: Arc::new(AtomicBool::new(false)),
            delivery_unknown: Arc::new(AtomicBool::new(false)),
            live,
        };
        let prepared = lease.admit(runtime, None, created).map(|_| ());
        // Return the actual lease even on a post-admission failure. The catalog
        // retains it until original revocation cleanup has actually succeeded.
        (lease, prepared)
    }
    pub fn control(&self) -> NativeEndpointAuthority {
        NativeEndpointAuthority {
            endpoint: self.endpoints[0].clone(),
            generation: self.generation,
        }
    }
    pub fn close(&self) -> Result<()> {
        let mut first = None;
        for endpoint in &self.endpoints {
            if let Err(error) = endpoint.close() {
                first.get_or_insert(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
    pub fn belongs_to(&self, connection: &Arc<Connection>) -> bool {
        Arc::ptr_eq(&self.connection, connection)
    }
    fn check(&self, runtime: &HostRuntime, now: u64) -> Result<()> {
        if runtime.binding() != self.owner
            || !(self.live)()
            || now < self.created
            || now >= self.expires
            || self.host.generation(runtime)? != self.generation
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn admit(
        &mut self,
        runtime: &HostRuntime,
        writer: Option<String>,
        now: u64,
    ) -> Result<NativeEndpointAuthority> {
        self.check(runtime, now)?;
        if self.endpoints.len() >= MAX_NATIVE_ENDPOINTS {
            return Err(Error::Limit);
        }
        let sessions = match &writer {
            Some(sid) if self.sessions.contains(sid) => vec![sid.clone()],
            Some(_) => return Err(Error::Denied),
            None => self.sessions.clone(),
        };
        let mut id = [0; 32];
        getrandom::fill(&mut id).map_err(|_| Error::Storage)?;
        let admission = self.host.admit(
            runtime,
            &self.connection,
            self.declared,
            SessionCapabilities {
                session_read: true,
                session_write: true,
                propose: false,
                execute: false,
                retire: false,
            },
            sessions,
            self.domain.clone(),
            self.expires,
            now,
        )?;
        let revoke = admission.revocation();
        let endpoint = Arc::new(Endpoint {
            id: NativeEndpointId(id),
            host: self.host.clone(),
            admission,
            revoke,
            writer,
            created: now,
            expires: self.expires,
            closed: AtomicBool::new(false),
            uncertain: self.uncertain.clone(),
            delivery_unknown: self.delivery_unknown.clone(),
            live: self.live.clone(),
        });
        // Retain the original grant even if a post-admission liveness check fails.
        self.endpoints.push(endpoint.clone());
        self.check(runtime, now)?;
        Ok(NativeEndpointAuthority {
            endpoint,
            generation: self.generation,
        })
    }
    pub(crate) fn admit_writer(
        &mut self,
        runtime: &HostRuntime,
        sid: &str,
        now: u64,
    ) -> Result<NativeEndpointAuthority> {
        if self.endpoints[0].closed.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        if self.uncertain.load(Ordering::Acquire) || self.delivery_unknown.load(Ordering::Acquire) {
            return Err(Error::CommitUnknown);
        }
        self.admit(runtime, Some(sid.to_owned()), now)
    }
    pub(crate) fn exchange(
        &self,
        runtime: &mut HostRuntime,
        id: NativeEndpointId,
        bytes: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<Vec<u8>> {
        if bytes.len() > MAX_NATIVE_FRAME_BYTES {
            return Err(Error::Limit);
        }
        let request = Request::decode(bytes)?;
        if request.generation() != self.generation {
            return Err(Error::Correlation);
        }
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.id == id)
            .ok_or(Error::Denied)?;
        if endpoint.closed.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        let action = request.action();
        let reading = matches!(action, Action::List | Action::Snapshot { .. });
        let allowed = match (&endpoint.writer, action) {
            (None, Action::List | Action::Snapshot { .. } | Action::Create { .. }) => true,
            (
                Some(sid),
                Action::Snapshot { session_id, .. }
                | Action::OpenWriter { session_id, .. }
                | Action::Append { session_id, .. }
                | Action::Checkpoint { session_id, .. },
            ) => sid == session_id,
            _ => false,
        };
        if !allowed {
            return Err(Error::Denied);
        }
        if !reading
            && (endpoint.uncertain.load(Ordering::Acquire)
                || endpoint.delivery_unknown.load(Ordering::Acquire))
        {
            return Err(Error::CommitUnknown);
        }
        self.check(runtime, clock())?;
        if !reading {
            endpoint.uncertain.store(true, Ordering::Release);
        }
        let result = self.host.dispatch(
            runtime,
            &self.connection,
            &endpoint.admission,
            bytes,
            || {
                let now = clock();
                if !(self.live)() || endpoint.closed.load(Ordering::Acquire) {
                    // Same original admission only; never recursively acquire the
                    // dispatch fence and never revoke other writers or the control.
                    endpoint.revoke.revoke();
                }
                now
            },
        );
        // No authoritative zero-effect receipt for an outer dispatch error.
        let mut raw = result.map_err(|error| if reading { error } else { Error::CommitUnknown })?;
        if raw.len() > MAX_NATIVE_FRAME_BYTES {
            raw.fill(0);
            return Err(if reading {
                Error::Limit
            } else {
                Error::CommitUnknown
            });
        }
        let reply = Reply::decode_for(&request, &raw)
            .map_err(|error| if reading { error } else { Error::CommitUnknown })?;
        // Storage and CommitUnknown never unlock further mutation. No frame is resent.
        let certain = !matches!(
            reply.outcome,
            Outcome::Rejected(Error::Storage | Error::CommitUnknown)
        );
        if self.check(runtime, clock()).is_err() || endpoint.closed.load(Ordering::Acquire) {
            raw.fill(0);
            return Err(if reading {
                Error::Denied
            } else {
                Error::CommitUnknown
            });
        }
        if !reading && certain {
            endpoint.uncertain.store(false, Ordering::Release);
        }
        Ok(raw)
    }
    pub(crate) fn mark_unknown(&self) {
        self.delivery_unknown.store(true, Ordering::Release);
    }
}
impl Drop for NativeSessionLease {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
