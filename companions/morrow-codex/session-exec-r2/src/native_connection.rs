//! Canonical, correlated client frames on a reviewed native connection.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use chrono::{DateTime, Utc};
use codex_thread_store::{ThreadStoreError, ThreadStoreResult};
use morrow_agent_session_exec_v1_r2::{Action, Outcome, Reply, Request};

use crate::thread_store::{SessionControl, SessionWriter};

/// Original native endpoint identity, established by reviewed package import.
/// The endpoint server owns the live Core/SDK admission. A request frame carries
/// data only: it cannot select another connection or reconstruct authority.
pub trait OriginalNativeEndpoint: Send + Sync {
    /// Pinned when this original admission is created, never refreshed from
    /// guest bytes or silently changed during a retry.
    fn generation(&self) -> u64;
    fn exchange(&self, canonical_request: &[u8]) -> ThreadStoreResult<Vec<u8>>;
    /// Must revoke the original server admission and all retained endpoint
    /// clones. It is idempotent and must not create a replacement admission.
    fn revoke(&self) -> ThreadStoreResult<()>;
}

/// Trusted host control plane, kept outside the request codec. Package
/// declarations are ceilings, and approval/scope/expiry come from host review.
pub trait ReviewedNativeSessionPort: Send + Sync {
    fn control(&self) -> Arc<dyn OriginalNativeEndpoint>;
    /// Establish a new dedicated writer admission on the original connection,
    /// within the already reviewed finite session scope and lifetime.
    fn admit_writer(&self, session_id: &str) -> ThreadStoreResult<Arc<dyn OriginalNativeEndpoint>>;
    /// Performs reviewed generation/revision/payload-digest CAS retirement after
    /// proving all original writers inactive and related tools safely retired.
    fn retire(&self, session_id: &str) -> ThreadStoreResult<()>;
    fn now(&self) -> DateTime<Utc>;
}
struct CanonicalEndpoint {
    original: Arc<dyn OriginalNativeEndpoint>,
    generation: u64,
    prefix: String,
    next: AtomicU64,
}
impl CanonicalEndpoint {
    fn new(original: Arc<dyn OriginalNativeEndpoint>) -> ThreadStoreResult<Self> {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(internal)?;
        let generation = original.generation();
        if generation == 0 {
            return Err(internal("original endpoint has no admitted generation"));
        }
        Ok(Self {
            original,
            generation,
            prefix: nonce.iter().map(|byte| format!("{byte:02x}")).collect(),
            next: AtomicU64::new(0),
        })
    }
    fn request(&self, action: Action) -> ThreadStoreResult<Outcome> {
        let next = self
            .next
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_add(1)
            })
            .map_err(|_| internal("request identity exhausted"))?;
        let request = Request::new_for_generation(
            format!("codex-{}-{next}", self.prefix),
            self.generation,
            action,
        )
        .map_err(internal)?;
        // A transport retry must resend this exact frame inside exchange.
        // This layer never retries CommitUnknown with a new request identity.
        let reply = self.original.exchange(request.raw())?;
        Ok(Reply::decode_for(&request, &reply)
            .map_err(internal)?
            .outcome)
    }
}
impl SessionWriter for CanonicalEndpoint {
    fn request(&self, action: Action) -> ThreadStoreResult<Outcome> {
        self.request(action)
    }
    fn close(&self) -> ThreadStoreResult<()> {
        self.original.revoke()
    }
}
pub struct CanonicalSessionControl {
    port: Arc<dyn ReviewedNativeSessionPort>,
    control: CanonicalEndpoint,
}
impl CanonicalSessionControl {
    pub fn new(port: Arc<dyn ReviewedNativeSessionPort>) -> ThreadStoreResult<Self> {
        Ok(Self {
            control: CanonicalEndpoint::new(port.control())?,
            port,
        })
    }
}
impl SessionControl for CanonicalSessionControl {
    fn request(&self, action: Action) -> ThreadStoreResult<Outcome> {
        self.control.request(action)
    }
    fn writer(&self, session_id: &str) -> ThreadStoreResult<Arc<dyn SessionWriter>> {
        Ok(Arc::new(CanonicalEndpoint::new(
            self.port.admit_writer(session_id)?,
        )?))
    }
    fn retire(&self, session_id: &str) -> ThreadStoreResult<()> {
        self.port.retire(session_id)
    }
    fn now(&self) -> DateTime<Utc> {
        self.port.now()
    }
}
fn internal(message: impl ToString) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: message.to_string(),
    }
}
