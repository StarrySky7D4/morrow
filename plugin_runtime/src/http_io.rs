//! Live HTTP resource approval for one exact managed instance. This module
//! neither chooses an origin nor executes a request. A trusted host must issue
//! the grant only after approving its immutable endpoint and credential policy.
use crate::{
    Cancellation, Fault,
    io_binding::{self, IoBinding, IoJobLease, IoResourceLease},
    io_execution,
    manager::{ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::HostRuntime,
    io::{Action, Request},
    plugin_package::io::IoCapability,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) type SharedClock = Arc<Mutex<Box<dyn FnMut() -> u64 + Send>>>;
struct GrantState {
    resource: IoResourceLease,
    policy_sha256: [u8; 32],
    endpoint: [u8; 32],
    credential: bool,
    revoked: AtomicBool,
}
/// An opaque, revocable host resource approval. Copies retain the same resource
/// and revocation state, and cannot establish a new approval or managed owner.
#[derive(Clone)]
pub struct HttpGrant {
    state: Arc<GrantState>,
}
impl HttpGrant {
    /// The digest identifies the host-approved immutable policy; the digest alone
    /// is not proof of approval. This trusted host call performs the actual issuance.
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        policy_sha256: [u8; 32],
        endpoint: [u8; 32],
        credential: bool,
        now: u64,
    ) -> io_binding::Result<Self> {
        if policy_sha256 == [0; 32] || endpoint == [0; 32] {
            return Err(io_binding::Error::Denied);
        }
        let capabilities: &[IoCapability] = if credential {
            &[IoCapability::HttpRequest, IoCapability::CredentialUse]
        } else {
            &[IoCapability::HttpRequest]
        };
        let resource = binding.admit_resource(manager, host, instance, capabilities, now)?;
        Ok(Self {
            state: Arc::new(GrantState {
                resource,
                policy_sha256,
                endpoint,
                credential,
                revoked: AtomicBool::new(false),
            }),
        })
    }
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    pub fn policy_sha256(&self) -> [u8; 32] {
        self.state.policy_sha256
    }
    /// Lowercase hexadecimal is the wire representation of the opaque endpoint
    /// nonce. Raw random bytes are not valid textual IO endpoint references.
    pub fn endpoint_reference(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut value = String::with_capacity(64);
        for byte in self.state.endpoint {
            value.push(HEX[(byte >> 4) as usize] as char);
            value.push(HEX[(byte & 15) as usize] as char);
        }
        value
    }
    pub(crate) fn authorize(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
        job: &Arc<IoJobLease>,
        request: &Request,
        cancel: Cancellation,
        clock: SharedClock,
    ) -> io_execution::Result<HttpCallGuard> {
        use io_execution::Error;
        // Reject foreign owners and mismatched references before sampling the
        // shared monotonic clock or mutating any legitimate instance accounting.
        self.state
            .resource
            .binding()
            .validate_owner(host, instance)?;
        job.binding().validate_owner(host, instance)?;
        let Action::SubmitHttp(http) = request.action() else {
            return Err(Error::Denied);
        };
        if self.state.revoked.load(Ordering::Acquire)
            || http.endpoint != self.endpoint_reference().as_bytes()
            || (!http.credential.is_empty() && !self.state.credential)
        {
            return Err(Error::Denied);
        }
        let guard = HttpCallGuard {
            service_validity: None,
            service_content: None,
            inner: Arc::new(CallState {
                grant: Arc::clone(&self.state),
                job: Arc::clone(job),
                cancel,
                clock,
            }),
        };
        guard.check()?;
        Ok(guard)
    }
}
struct CallState {
    grant: Arc<GrantState>,
    // An outstanding backend check retains the exact shared job reservation.
    job: Arc<IoJobLease>,
    cancel: Cancellation,
    clock: SharedClock,
}
/// A monitor for an already authorized call. It exposes no resource constructor,
/// request dispatch or host identity, and stays tied to the original job lifetime.
#[derive(Clone)]
pub struct HttpCallGuard {
    inner: Arc<CallState>,
    service_validity: Option<crate::service_history::Validity>,
    service_content: Option<crate::service_content::ServiceContentAccess>,
}
impl HttpCallGuard {
    pub(crate) fn with_service_validity(
        mut self,
        validity: Option<&crate::service_history::Validity>,
    ) -> io_execution::Result<Self> {
        self.service_validity = validity.cloned();
        self.check()?;
        Ok(self)
    }

    pub(crate) fn with_service_content(
        mut self,
        content: Option<&crate::service_content::ServiceContentAccess>,
    ) -> io_execution::Result<Self> {
        self.service_content = content.cloned();
        self.check()?;
        Ok(self)
    }
    pub fn policy_sha256(&self) -> [u8; 32] {
        self.inner.grant.policy_sha256
    }
    /// Sample the original worker clock afresh. This method acquires no worker-state lock,
    /// so a blocking transport can monitor this guard without blocking revocation.
    pub fn check(&self) -> io_execution::Result<()> {
        Self::check_all(std::slice::from_ref(self))
    }
    /// One final instant for all resources of a job, so a later clock sample
    /// cannot expire an earlier resource after it was already checked.
    pub(crate) fn check_all(guards: &[Self]) -> io_execution::Result<()> {
        let Some(first) = guards.first() else {
            return Ok(());
        };
        for guard in guards {
            if !Arc::ptr_eq(&first.inner.clock, &guard.inner.clock) {
                return Err(io_execution::Error::Denied);
            }
            guard.check_cancel()?;
        }
        let mut clock = first
            .inner
            .clock
            .lock()
            .map_err(|_| io_execution::Error::Denied)?;
        Self::check_all_at(guards, clock())
    }
    /// Caller holds the original managed worker clock and has sampled one instant
    /// for service, job and all endpoint grants at the delivery boundary.
    pub(crate) fn check_all_at(guards: &[Self], now: u64) -> io_execution::Result<()> {
        for guard in guards {
            guard.check_cancel()?;
            guard.inner.grant.resource.binding().check_liveness(now)?;
            guard.inner.job.binding().check_liveness(now)?;
            if let Some(content) = &guard.service_content {
                content.check_at(now)?;
            }
            guard.check_cancel()?;
        }
        Ok(())
    }
    fn check_cancel(&self) -> io_execution::Result<()> {
        if let Some(content) = &self.service_content {
            content.check()?;
        }
        if let Some(validity) = &self.service_validity {
            validity.check()?;
        }
        if self.inner.grant.revoked.load(Ordering::Acquire) {
            return Err(io_execution::Error::Denied);
        }
        match self.inner.cancel.fault() {
            None => Ok(()),
            Some(Fault::Deadline) => Err(io_execution::Error::Expired),
            Some(_) => Err(io_execution::Error::Cancelled),
        }
    }
}
