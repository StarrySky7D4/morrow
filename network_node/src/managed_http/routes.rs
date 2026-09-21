//! Exact selection among explicitly approved live outbound endpoints.
use super::*;
use morrow_plugin_runtime::io_jobs::RouteStart;
use std::collections::BTreeMap;

pub const MAX_SERVICE_ENDPOINTS: usize = morrow_core::service_resources::MAX_ENDPOINTS;

/// Explicitly selected set of approved live outbound endpoints.
/// Cloned by service factories; each import resolves via exact reference.
#[derive(Clone)]
pub struct HttpRouteSet {
    endpoints: BTreeMap<String, HttpEndpoint>,
    runtime: Handle,
}

impl HttpRouteSet {
    /// The caller keeps this Tokio runtime alive and driven through all calls.
    /// Membership does not replace the original endpoint/instance authorization.
    pub fn new(endpoints: Vec<HttpEndpoint>, runtime: Handle) -> crate::Result<Self> {
        if endpoints.is_empty() {
            return Err(Error::Invalid);
        }
        if endpoints.len() > MAX_SERVICE_ENDPOINTS {
            return Err(Error::Limit);
        }
        let mut map: BTreeMap<String, HttpEndpoint> = BTreeMap::new();
        for endpoint in endpoints {
            let reference = endpoint.endpoint_reference();
            if map.contains_key(&reference) {
                return Err(Error::Invalid);
            }
            map.insert(reference, endpoint);
        }
        Ok(Self {
            endpoints: map,
            runtime,
        })
    }

    /// Sorted, exact endpoint references available for selection.
    pub fn references(&self) -> impl Iterator<Item = &str> {
        self.endpoints.keys().map(|k| k.as_str())
    }

    /// Metadata from this exact approved set. It is not a live grant and must
    /// be delivered only to a guest admitting the service-resources-v1 profile.
    pub fn resources(
        &self,
        scope_sha256: [u8; 32],
    ) -> crate::Result<morrow_core::service_resources::Directory> {
        use morrow_core::service_resources::{Directory, Endpoint};
        let endpoints = self
            .endpoints
            .iter()
            .map(|(reference, endpoint)| {
                let endpoint = &endpoint.inner;
                Endpoint {
                    reference: reference.clone(),
                    credential: endpoint
                        .credential
                        .as_ref()
                        .map_or_else(Vec::new, |c| c.reference().to_vec()),
                    methods: endpoint.methods.iter().cloned().collect(),
                    max_request_bytes: endpoint.limits.max_request_bytes as u64,
                    max_response_bytes: endpoint.limits.max_response_bytes as u64,
                    timeout_ms: endpoint.limits.timeout.as_millis() as u64,
                    response_frame_limit: endpoint.response_frame_limit,
                }
            })
            .collect();
        let resources = Directory {
            scope_sha256,
            endpoints,
        };
        resources.encode().map_err(|_| Error::Invalid)?;
        Ok(resources)
    }

    fn select(&self, request: &Request) -> std::result::Result<HttpRouter, RouterFault> {
        let Action::SubmitHttp(http) = request.action() else {
            return Err(RouterFault::Denied);
        };
        let reference = std::str::from_utf8(&http.endpoint).map_err(|_| RouterFault::Denied)?;
        let endpoint = self.endpoints.get(reference).ok_or(RouterFault::Denied)?;
        Ok(endpoint.router(self.runtime.clone()))
    }
}

impl BrokerRouter for HttpRouteSet {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        let mut selected = self.select(request)?;
        selected.route(context, call, request)
    }

    fn begin(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> morrow_plugin_runtime::io_jobs::RouteStart {
        match self.select(request) {
            Ok(mut selected) => selected.begin(context, call, request),
            Err(fault) => RouteStart::Ready(Err(fault)),
        }
    }
}
