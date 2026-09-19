//! Host-issued service and listener approvals. Neither grant chooses a socket
//! address, authenticates a remote principal, or authorizes unrelated IO effects.
use crate::{
    Cancellation,
    io_binding::{self, Error, IoBinding, IoResourceLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability, service};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
struct Resource {
    lease: IoResourceLease,
    cancel: Cancellation,
}
impl Resource {
    fn check(&self, now: u64) -> io_binding::Result<()> {
        if self.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        let result = self.lease.binding().check_liveness(now);
        if result.is_err() {
            self.cancel.cancel();
        }
        result?;
        if self.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
struct ServiceState {
    resource: Arc<Resource>,
    listener: Option<ListenerGrant>,
    service: String,
    handler: String,
}
/// One registered handler on one actual managed instance. Clones share both the
/// resource reservation and the revocation signal used by every submitted job.
#[derive(Clone)]
pub struct ServiceGrant {
    state: Arc<ServiceState>,
}
impl ServiceGrant {
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        service: &str,
        handler: &str,
        now: u64,
    ) -> io_binding::Result<Self> {
        let valid = |text: &str| {
            !text.is_empty()
                && text.len() <= 256
                && !text
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        };
        if !valid(service) || !valid(handler) {
            return Err(Error::Denied);
        }
        let declaration = instance
            .package()
            .package()
            .io_declaration()
            .ok_or(Error::Denied)?;
        if declaration.service_schema_sha256 != service::schema_digest()
            || !declaration.handlers.iter().any(|value| value == handler)
        {
            return Err(Error::Denied);
        }
        let lease =
            binding.admit_resource(manager, host, instance, &[IoCapability::HttpPublish], now)?;
        Ok(Self {
            state: Arc::new(ServiceState {
                resource: Arc::new(Resource {
                    lease,
                    cancel: Cancellation::default(),
                }),
                listener: None,
                service: service.into(),
                handler: handler.into(),
            }),
        })
    }
    /// Bind this service to one actual listener without duplicating either
    /// resource reservation. Revocation of either original grant stops jobs.
    /// A bound grant cannot be rebound, including to the same listener.
    pub fn bound_to_listener(&self, listener: &ListenerGrant) -> io_binding::Result<Self> {
        if self.state.listener.is_some() {
            return Err(Error::Denied);
        }
        let binding = self.state.resource.lease.binding();
        self.validate_binding(binding)?;
        listener.validate_binding(binding)?;
        Ok(Self {
            state: Arc::new(ServiceState {
                resource: self.state.resource.clone(),
                listener: Some(listener.clone()),
                service: self.state.service.clone(),
                handler: self.state.handler.clone(),
            }),
        })
    }
    pub(crate) fn validate_instance(
        &self,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> io_binding::Result<()> {
        self.state
            .resource
            .lease
            .binding()
            .validate_owner(host, instance)
    }
    pub(crate) fn same_service(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state.resource, &other.state.resource)
            && self.state.service == other.state.service
            && self.state.handler == other.state.handler
    }
    pub fn service(&self) -> &str {
        &self.state.service
    }
    pub fn handler(&self) -> &str {
        &self.state.handler
    }
    pub fn revoke(&self) {
        self.state.resource.cancel.cancel();
    }
    pub fn check(&self, now: u64) -> io_binding::Result<()> {
        self.state.resource.check(now)?;
        if let Some(listener) = &self.state.listener {
            listener.check(now)?;
        }
        Ok(())
    }
    pub(crate) fn cancellation(&self) -> Cancellation {
        let service = self.state.resource.cancel.clone();
        match &self.state.listener {
            Some(listener) => Cancellation::linked(service, listener.resource.cancel.clone()),
            None => service,
        }
    }
    pub(crate) fn validate_binding(&self, binding: &IoBinding) -> io_binding::Result<()> {
        self.state
            .resource
            .lease
            .binding()
            .validate_same_owner(binding)?;
        binding.require_capability(IoCapability::HttpPublish)?;
        if let Some(listener) = &self.state.listener {
            listener.validate_binding(binding)?;
        }
        if self.state.resource.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn validate_job(
        &self,
        binding: &IoBinding,
        request: &service::Request,
    ) -> io_binding::Result<()> {
        self.validate_binding(binding)?;
        if request.invocation().service != self.state.service
            || request.invocation().handler != self.state.handler
            || self.state.resource.cancel.fault().is_some()
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
/// Separate permission to bind a native listener. The trusted native adapter
/// fixes its address/TLS/authentication policy before opening the socket.
#[derive(Clone)]
pub struct ListenerGrant {
    resource: Arc<Resource>,
    activated: Arc<AtomicBool>,
}
impl ListenerGrant {
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        now: u64,
    ) -> io_binding::Result<Self> {
        let lease =
            binding.admit_resource(manager, host, instance, &[IoCapability::HttpListen], now)?;
        Ok(Self {
            resource: Arc::new(Resource {
                lease,
                cancel: Cancellation::default(),
            }),
            activated: Arc::new(AtomicBool::new(false)),
        })
    }
    pub(crate) fn validate_binding(&self, binding: &IoBinding) -> io_binding::Result<()> {
        self.resource.lease.binding().validate_same_owner(binding)?;
        if self.resource.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// Claim this one resource exactly once across all clones. A failed native
    /// bind still consumes the claim; retry requires a newly approved grant.
    pub fn activate(&self) -> io_binding::Result<()> {
        self.resource
            .lease
            .binding()
            .validate_same_owner(self.resource.lease.binding())?;
        if self.resource.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        self.activated
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Denied)?;
        if self.resource.cancel.fault().is_some() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn revoke(&self) {
        self.resource.cancel.cancel();
    }
    pub fn check(&self, now: u64) -> io_binding::Result<()> {
        self.resource.check(now)
    }
}
