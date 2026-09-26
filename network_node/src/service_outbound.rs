//! Configured service wiring from one exact original-Store endpoint selection.
//! Keeps live dependencies, replay scope, router and guest directory together.
use crate::{
    Error, Result,
    managed_http::{Credential, HttpEndpoint, HttpRouteSet, MAX_SERVICE_ENDPOINTS},
    managed_service::{RouterFactory, ServiceHost, ServiceHostFailure},
    stored_http::StoredHttpEndpoint,
};
use morrow_core::{
    dispatch::HostRuntime, io::Request, outbound_authority::Record,
    plugin_package::io::IoCapability, service_resources::Directory, store::Store,
};
use morrow_plugin_runtime::{
    io_binding::IoBinding,
    io_jobs::{BrokerRouter, HostOwner, IoWorker, RouteContext, RouterFault},
    manager::{ManagedInstance, Manager},
    service_authority::{ConfiguredService, ResolvedService},
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::runtime::Handle;

/// Explicit host selection; knowing a reference or revision never grants access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceEndpointSelection {
    pub reference: [u8; 32],
    pub revision: u64,
}
impl ServiceEndpointSelection {
    /// Validate before resolving records or invoking a trusted clock/provider.
    pub fn validate(selections: &[Self]) -> Result<()> {
        if selections.len() > MAX_SERVICE_ENDPOINTS {
            return Err(Error::Limit);
        }
        let mut references = BTreeSet::new();
        for selected in selections {
            if selected.reference == [0; 32]
                || selected.revision == 0
                || !references.insert(selected.reference)
            {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
}

/// Resolved but unapproved state. No sockets or credential values are opened.
/// Private fields prevent substituting a directory or omitting a live dependency.
pub struct SelectedService {
    resolved: ResolvedService,
    endpoints: Vec<StoredHttpEndpoint>,
    scope: Option<[u8; 32]>,
}
impl SelectedService {
    /// The cloneable trusted UTC clock must be bounded and non-reentrant.
    /// Every selected record is pinned on the original Store, including credentials.
    pub fn resolve(
        store: &mut Store,
        resolved: ResolvedService,
        selections: &[ServiceEndpointSelection],
        clock: impl FnMut() -> u64 + Clone + Send + 'static,
    ) -> Result<Self> {
        ServiceEndpointSelection::validate(selections)?;
        let mut endpoints = Vec::with_capacity(selections.len());
        for selected in selections {
            let endpoint = StoredHttpEndpoint::resolve(store, &selected.reference, clock.clone())?;
            if endpoint.revision() != selected.revision {
                return Err(Error::Denied);
            }
            endpoints.push(endpoint);
        }
        let scope = StoredHttpEndpoint::selection_digest(&endpoints)?;
        let dependencies = endpoints
            .iter()
            .map(|e| e.service_dependency(store))
            .collect::<Result<Vec<_>>>()?;
        let resolved = resolved
            .with_dependencies(store, dependencies)
            .map_err(|_| Error::Denied)?;
        Ok(Self {
            resolved,
            endpoints,
            scope,
        })
    }
    /// Required live capabilities only. Registry/package approval remains separate.
    pub fn capabilities(&self) -> BTreeSet<IoCapability> {
        let mut caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        if !self.endpoints.is_empty() {
            caps.insert(IoCapability::HttpRequest);
        }
        if self
            .endpoints
            .iter()
            .any(|e| e.credential_reference().is_some())
        {
            caps.insert(IoCapability::CredentialUse);
        }
        caps
    }
    /// Rechecks managed identity and every saved policy before opening credentials.
    /// The provider is trusted host code, called only for selected credentials.
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        now: u64,
        runtime: Handle,
        mut fresh_secret: impl FnMut() -> Result<[u8; 32]>,
        mut credentials: impl FnMut(&Record) -> Result<Credential>,
    ) -> Result<PreparedService> {
        self.approve_with(manager, host, instance, binding, now, runtime, |endpoint| {
            endpoint.approve_persistent(
                manager,
                host,
                instance,
                binding,
                fresh_secret()?,
                now,
                &mut credentials,
            )
        })
    }
    /// Production Windows provider; unsupported platforms have no plaintext fallback.
    #[cfg(target_os = "windows")]
    #[allow(clippy::too_many_arguments)]
    pub fn approve_windows(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        now: u64,
        runtime: Handle,
        mut fresh_secret: impl FnMut() -> Result<[u8; 32]>,
    ) -> Result<PreparedService> {
        self.approve_with(manager, host, instance, binding, now, runtime, |endpoint| {
            endpoint.approve_persistent_windows(
                manager,
                host,
                instance,
                binding,
                fresh_secret()?,
                now,
            )
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn approve_with(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        now: u64,
        runtime: Handle,
        mut approve: impl FnMut(StoredHttpEndpoint) -> Result<HttpEndpoint>,
    ) -> Result<PreparedService> {
        let configured = self
            .resolved
            .issue(manager, host, instance, binding, now)
            .map_err(|_| Error::Denied)?;
        let mut approved = Vec::with_capacity(self.endpoints.len());
        for endpoint in self.endpoints {
            approved.push(approve(endpoint)?);
        }
        let directory_enabled = instance
            .package()
            .package()
            .manifest()
            .required_features
            .iter()
            .any(|feature| feature == morrow_core::service_resources::FEATURE);
        let mut resources = None;
        let routers: RouterFactory = if approved.is_empty() {
            Arc::new(|| Box::new(Deny))
        } else {
            let routes = HttpRouteSet::new(approved, runtime)?;
            if directory_enabled {
                resources = Some(routes.resources(self.scope.ok_or(Error::Invalid)?)?);
            }
            Arc::new(move || Box::new(routes.clone()))
        };
        configured.check().map_err(|_| Error::Denied)?;
        Ok(PreparedService {
            configured,
            routers,
            scope: self.scope,
            resources,
        })
    }
}
struct Deny;
impl BrokerRouter for Deny {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}
/// Approved state; attachment preserves the exact selection without caller wiring.
/// This is not a listener. Caller must bind_configured and reclaim the original worker.
pub struct PreparedService {
    configured: ConfiguredService,
    routers: RouterFactory,
    scope: Option<[u8; 32]>,
    resources: Option<Directory>,
}
impl PreparedService {
    /// On failure return the same live worker for explicit stop/join/reclamation.
    pub fn attach<O: HostOwner>(
        self,
        worker: IoWorker<O>,
        timeout: Duration,
    ) -> std::result::Result<(ServiceHost<O>, ConfiguredService), ServiceHostFailure<O>> {
        if self.configured.check().is_err()
            || worker.check_service(self.configured.grant()).is_err()
        {
            return Err(ServiceHostFailure {
                worker,
                error: Error::Denied,
            });
        }
        let host = ServiceHost::new_owned_with_resources(
            worker,
            timeout,
            self.routers,
            self.scope,
            self.resources,
        )?;
        Ok((host, self.configured))
    }
}
