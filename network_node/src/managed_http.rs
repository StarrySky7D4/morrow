//! Native HTTP adapter for an explicitly approved managed plugin instance.
//! Live endpoint authority is host-issued; durable command hashes are evidence,
//! never a replacement for the original approval. No user account is loaded here.
use crate::{
    Error, HttpRequest, Limits, Result,
    client::{Client, EndpointPolicy},
};
use morrow_core::{
    dispatch::HostRuntime,
    io::{self, Action, Header, HttpOutcome, Request, Response, Status},
    io_intent::Command,
    plugin_package::io::IoCapability,
};
use morrow_plugin_runtime::{
    http_io::{HttpCallGuard, HttpGrant},
    io_binding::IoBinding,
    io_execution,
    io_jobs::{BrokerRouter, RouteContext, RouterFault},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use url::Url;

static NEXT_ENDPOINT: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy)]
pub enum NetworkProfile {
    PublicHttps,
    LoopbackHttp,
    LoopbackHttps,
}
/// Host-owned header secret. No Debug or serialization, and never sent to a guest
/// or written into the request material. The provider can still echo it remotely.
pub struct Credential {
    reference: Vec<u8>,
    name: String,
    value: String,
}
impl Credential {
    pub fn header(reference: Vec<u8>, name: &str, value: &str) -> Result<Self> {
        if reference.is_empty()
            || reference.len() > io::MAX_CREDENTIAL_BYTES
            || reference.iter().any(|b| b.is_ascii_control())
            || value.is_empty()
            || value.len() > io::MAX_HEADER_VALUE_BYTES
            || name.len() > io::MAX_HEADER_NAME_BYTES
        {
            return Err(Error::Invalid);
        }
        let parsed =
            http::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::Invalid)?;
        let name = parsed.as_str();
        if matches!(
            name,
            "host"
                | "content-length"
                | "transfer-encoding"
                | "connection"
                | "keep-alive"
                | "upgrade"
                | "te"
                | "trailer"
                | "expect"
        ) || name.starts_with("proxy-")
        {
            return Err(Error::Denied);
        }
        http::header::HeaderValue::from_str(value).map_err(|_| Error::Invalid)?;
        Ok(Self {
            reference,
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}
/// Explicit trusted-host approval input, not plugin-provided configuration.
/// Custom roots add trust while retaining certificate and hostname verification.
pub struct EndpointApproval {
    pub origin: String,
    pub methods: Vec<String>,
    pub profile: NetworkProfile,
    pub limits: Limits,
    pub response_frame_limit: u64,
    pub credential: Option<Credential>,
    pub root_certificate: Option<Vec<u8>>,
}
struct Endpoint {
    grant: HttpGrant,
    subject: String,
    package: [u8; 32],
    origin: String,
    methods: BTreeSet<String>,
    limits: Limits,
    response_frame_limit: u64,
    credential: Option<Credential>,
    client: Client,
    approval_sha256: [u8; 32],
}
/// Clones retain the same original approval/resource lease. Revoke all clones
/// together; old references never authorize a different managed instance.
#[derive(Clone)]
pub struct HttpEndpoint {
    inner: Arc<Endpoint>,
}
impl HttpEndpoint {
    /// The host must supply a fresh unpredictable secret per host session.
    /// This approval occupies one resource, plus one per active broker call.
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        binding: &IoBinding,
        approval: EndpointApproval,
        host_secret: [u8; 32],
        now: u64,
    ) -> Result<Self> {
        let EndpointApproval {
            origin,
            methods,
            profile,
            limits,
            response_frame_limit,
            credential,
            root_certificate,
        } = approval;
        limits.validate()?;
        if host_secret == [0; 32]
            || response_frame_limit == 0
            || response_frame_limit > io::MAX_FRAME_BYTES as u64
            || limits.max_request_bytes > io::MAX_PAYLOAD_BYTES
            || limits.max_response_bytes > io::MAX_PAYLOAD_BYTES
            || limits.max_header_bytes > io::MAX_HEADER_BYTES
            || limits.timeout > Duration::from_millis(io::MAX_SUBMIT_DEADLINE_MS)
        {
            return Err(Error::Limit);
        }
        if credential
            .as_ref()
            .is_some_and(|c| c.name.len() + c.value.len() + 4 > limits.max_header_bytes)
        {
            return Err(Error::Limit);
        }
        let names: Vec<_> = methods.iter().map(String::as_str).collect();
        let policy = match profile {
            NetworkProfile::PublicHttps => EndpointPolicy::new(&origin, &names, false),
            NetworkProfile::LoopbackHttp => {
                if !origin.starts_with("http://") {
                    return Err(Error::Denied);
                }
                EndpointPolicy::new(&origin, &names, true)
            }
            NetworkProfile::LoopbackHttps => EndpointPolicy::local_https(&origin, &names),
        }?;
        let origin = Url::parse(&origin)
            .map_err(|_| Error::Invalid)?
            .origin()
            .ascii_serialization();
        let methods: BTreeSet<String> = methods.into_iter().collect();
        // Length-prefix every variable field. The live policy is immutable and
        // the approval epoch is unique even when the same endpoint is reapproved.
        let mut digest = Sha256::new();
        field(&mut digest, b"morrow.managed-http.policy.v1");
        field(&mut digest, origin.as_bytes());
        for method in &methods {
            field(&mut digest, method.as_bytes());
        }
        field(&mut digest, &[profile as u8]);
        for value in [
            limits.max_request_bytes as u64,
            limits.max_response_bytes as u64,
            limits.max_header_bytes as u64,
            limits.max_concurrent as u64,
            limits.timeout.as_millis() as u64,
            response_frame_limit,
        ] {
            field(&mut digest, &value.to_le_bytes());
        }
        if let Some(ref c) = credential {
            field(&mut digest, &c.reference);
            field(&mut digest, c.name.as_bytes());
        }
        if let Some(ref root) = root_certificate {
            field(&mut digest, &Sha256::digest(root));
        }
        let policy_sha256: [u8; 32] = digest.finalize().into();
        let sequence = NEXT_ENDPOINT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let mut digest = Sha256::new();
        field(&mut digest, b"morrow.managed-http.approval.v1");
        field(&mut digest, &host_secret);
        field(&mut digest, &sequence.to_le_bytes());
        field(&mut digest, &policy_sha256);
        let package = instance.package().package().digest();
        field(&mut digest, &package);
        let endpoint: [u8; 32] = digest.finalize().into();
        let client = match root_certificate {
            Some(root) => Client::with_root_certificate(policy, limits, &root)?,
            None => Client::new(policy, limits)?,
        };
        let grant = HttpGrant::issue(
            manager,
            host,
            instance,
            binding,
            policy_sha256,
            endpoint,
            credential.is_some(),
            now,
        )
        .map_err(|error| match error {
            morrow_plugin_runtime::io_binding::Error::Limit => Error::Limit,
            _ => Error::Denied,
        })?;
        Ok(Self {
            inner: Arc::new(Endpoint {
                grant,
                subject: instance.package().package().manifest().package_id.clone(),
                package,
                origin,
                methods,
                limits,
                response_frame_limit,
                credential,
                client,
                approval_sha256: endpoint,
            }),
        })
    }
    pub fn endpoint_reference(&self) -> String {
        self.inner.grant.endpoint_reference()
    }
    pub fn revoke(&self) {
        self.inner.grant.revoke();
    }
    /// The host's Tokio runtime must remain alive and driven while jobs run.
    /// The managed worker calls it from its native IO thread, never a Tokio task.
    pub fn router(&self, runtime: Handle) -> HttpRouter {
        HttpRouter {
            endpoint: self.clone(),
            runtime,
        }
    }
}
fn field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}
pub struct HttpRouter {
    endpoint: HttpEndpoint,
    runtime: Handle,
}
impl BrokerRouter for HttpRouter {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        _call: u32,
        request: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        let endpoint = &self.endpoint.inner;
        let Action::SubmitHttp(http) = request.action() else {
            return Err(RouterFault::Denied);
        };
        // Authenticate exact live owner/reference before looking up host secrets.
        let guard = context
            .authorize_http(&endpoint.grant)
            .map_err(route_error)?;
        if !endpoint.methods.contains(&http.method)
            || http.body.len() > endpoint.limits.max_request_bytes
        {
            return Err(RouterFault::Denied);
        }
        let mut headers = Vec::with_capacity(http.headers.len() + 1);
        let mut header_bytes = 0usize;
        for header in &http.headers {
            if endpoint
                .credential
                .as_ref()
                .is_some_and(|c| header.name.eq_ignore_ascii_case(&c.name))
            {
                return Err(RouterFault::Denied);
            }
            let value = std::str::from_utf8(&header.value).map_err(|_| RouterFault::Denied)?;
            http::header::HeaderValue::from_str(value).map_err(|_| RouterFault::Denied)?;
            header_bytes = header_bytes
                .checked_add(header.name.len() + value.len() + 4)
                .ok_or(RouterFault::Limit)?;
            headers.push((header.name.clone(), value.to_owned()));
        }
        match &endpoint.credential {
            Some(c) if http.credential == c.reference => {
                header_bytes = header_bytes
                    .checked_add(c.name.len() + c.value.len() + 4)
                    .ok_or(RouterFault::Limit)?;
                headers.push((c.name.clone(), c.value.clone()));
            }
            None if http.credential.is_empty() => {}
            _ => return Err(RouterFault::Denied),
        }
        if header_bytes > endpoint.limits.max_header_bytes {
            return Err(RouterFault::Limit);
        }
        let operation = std::str::from_utf8(&http.operation_id).map_err(|_| RouterFault::Denied)?;
        // Core rejects network-path references and backslashes. Concatenation keeps
        // the approved authority fixed; Client revalidates exact origin and DNS.
        let target = format!("{}{}", endpoint.origin, http.relative_target);
        let command = Command {
            operation_id: operation.to_owned(),
            subject: endpoint.subject.clone(),
            package_sha256: endpoint.package,
            capability: IoCapability::HttpRequest,
            protocol_sha256: io::schema_digest(),
            request_sha256: request.digest(),
            request_bytes: request.bytes().len() as u64,
            response_limit: endpoint.response_frame_limit,
            approval_sha256: endpoint.approval_sha256,
            target_sha256: Sha256::digest(target.as_bytes()).into(),
        };
        let outbound = HttpRequest {
            method: http.method.clone(),
            target,
            headers,
            body: http.body.clone(),
        };
        let duration = if http.deadline_ms == 0 {
            endpoint.limits.timeout
        } else {
            endpoint
                .limits
                .timeout
                .min(Duration::from_millis(http.deadline_ms))
        };
        let result = context.dispatch(&command, |_| {
            guard.check().map_err(|_| ())?;
            self.runtime.block_on(async {
                tokio::time::timeout(duration, send(endpoint, outbound, request, &guard))
                    .await
                    .map_err(|_| ())?
            })
        });
        // Endpoint-only revocation need not revoke the whole managed worker.
        // Do not hand its response to the worker after that independent revocation.
        let response = result.map_err(route_error)?;
        guard.check().map_err(|_| RouterFault::Unknown)?;
        Ok(response)
    }
}
fn route_error(error: io_execution::Error) -> RouterFault {
    match error {
        io_execution::Error::Limit => RouterFault::Limit,
        io_execution::Error::OutcomeUnknown | io_execution::Error::CommitUnknown => {
            RouterFault::Unknown
        }
        _ => RouterFault::Denied,
    }
}
async fn send(
    endpoint: &Endpoint,
    outbound: HttpRequest,
    request: &Request,
    guard: &HttpCallGuard,
) -> std::result::Result<Vec<u8>, ()> {
    let cancel = CancellationToken::new();
    let exchange = endpoint.client.send_raw(outbound, cancel.clone());
    tokio::pin!(exchange);
    let mut tick = tokio::time::interval(Duration::from_millis(5));
    loop {
        tokio::select! {
            biased;
            _ = tick.tick() => {
                if guard.check().is_err() { cancel.cancel(); return Err(()); }
            }
            result = &mut exchange => {
                let response = result.map_err(|_| ())?;
                let outcome = HttpOutcome { status: Status::Completed, http_status: response.status,
                    headers: response.headers.into_iter().map(|(name,value)| Header {name,value}).collect(), body: response.body };
                return Response::encode_http(request, &outcome).map_err(|_| ());
            }
        }
    }
}
