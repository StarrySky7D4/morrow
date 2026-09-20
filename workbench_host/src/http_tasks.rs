//! Explicit experimental HTTP-forward guest profile on the original owner.
use crate::{
    Result, Workbench,
    io_tasks::{AccessError, PreparedJob, StartOptions, TaskKey},
};
use morrow_core::{
    io::{Header, HttpSubmission, Request},
    outbound_authority::proto::record::Kind,
    plugin_package::io::IoCapability,
};
use morrow_network_node::{managed_http::HttpRouter, stored_http::StoredHttpEndpoint};
use morrow_plugin_runtime::io_jobs::{BrokerRouter, JobLimits, RouteContext, RouterFault};
use std::{
    collections::BTreeSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Opt-in input profile, not inferred from general network capability.
pub const HTTP_FORWARD_PROFILE: &str = "morrow.http.forward.v1";
const MAX_SESSION_SUBMISSIONS: usize = 512;
#[derive(Default)]
pub(crate) struct HttpTasks {
    seen: BTreeSet<[u8; 32]>,
    current: Option<[u8; 32]>,
}
pub struct HttpStart {
    pub submission: [u8; 32],
    pub endpoint: [u8; 32],
    pub endpoint_revision: u64,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub method: String,
    pub target: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
    pub timeout_ms: u64,
}
// The executor owns the runtime along with its router. UI return/close cannot
// destroy the network runtime while the original Storage remains in that worker.
struct OwnedRouter {
    request: Vec<u8>,
    inner: HttpRouter,
    _runtime: tokio::runtime::Runtime,
}
impl BrokerRouter for OwnedRouter {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        if call != 0 || request.bytes() != self.request {
            return Err(RouterFault::Denied);
        }
        self.inner.route(context, call, request)
    }
}
fn utc() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| d.as_millis().try_into().ok())
        .unwrap_or(0)
}
impl Workbench {
    /// Correlate a lost start reply without starting or replaying anything.
    pub fn http_submission(&self) -> Option<[u8; 32]> {
        self.http_tasks.current
    }

    pub fn start_http(&mut self, request: HttpStart) -> Result<TaskKey> {
        let status = self.io_status();
        if status.key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.host.local()?;
        if request.submission == [0; 32]
            || request.endpoint == [0; 32]
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
            || self.http_tasks.seen.contains(&request.submission)
            || self.http_tasks.seen.len() >= MAX_SESSION_SUBMISSIONS
        {
            return Err("invalid, repeated or exhausted HTTP submission identity".into());
        }
        let record = self
            .host
            .local()?
            .store_local()
            .load_outbound_authority(&request.endpoint)?
            .ok_or("endpoint not found")?;
        if record.value().revision != request.endpoint_revision {
            return Err("endpoint changed; refresh before starting".into());
        }
        record.check_time(utc())?;
        let Some(Kind::Endpoint(policy)) = record.value().kind.as_ref() else {
            return Err("not an endpoint".into());
        };
        if policy.package_sha256 != request.package_digest {
            return Err("endpoint package digest changed".into());
        }
        let package = self
            .catalog
            .as_ref()
            .ok_or("catalog unavailable")?
            .load(request.package_digest)?;
        let declaration = package.io_declaration().ok_or("plugin has no IO profile")?;
        if !declaration
            .handlers
            .iter()
            .any(|h| h == HTTP_FORWARD_PROFILE)
        {
            return Err("plugin does not implement the HTTP forward input profile".into());
        }
        let budget = declaration.budget.as_ref().ok_or("missing IO budget")?;
        if request.timeout_ms > budget.max_duration_ms {
            return Err("task exceeds declared duration".into());
        }
        let limits = JobLimits::new(
            1,
            budget.max_job_bytes.min(1024 * 1024),
            budget.max_bytes.min(4 * 1024 * 1024),
        )
        .map_err(|_| "invalid HTTP job budget")?;
        let mut capabilities = BTreeSet::from([IoCapability::HttpRequest]);
        if !policy.credential_reference.is_empty() {
            capabilities.insert(IoCapability::CredentialUse);
        }
        let package_id = policy.package_id.clone();
        let lifetime = Duration::from_millis(request.timeout_ms);
        let stored = StoredHttpEndpoint::resolve(
            self.host.local_mut()?.store_local_mut(),
            &request.endpoint,
            utc,
        )?;
        let credential = stored.credential_reference().unwrap_or_default();
        let operation = request
            .submission
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        let mut input = HttpSubmission {
            operation_id: operation.into_bytes(),
            deadline_ms: request.timeout_ms,
            endpoint: b"pending".to_vec(),
            method: request.method,
            relative_target: request.target,
            headers: request.headers,
            body: request.body,
            credential,
        };
        // Validate untrusted request data before reserving an attempt or touching
        // credentials. Live endpoint validation remains inside actual admission.
        morrow_core::io::validate_http_submission(&input)?;
        self.http_tasks.seen.insert(request.submission);
        self.http_tasks.current = Some(request.submission);
        self.start_io(
            StartOptions {
                package_id,
                digest: request.package_digest,
                revision: request.registry_revision,
                capabilities,
                lifetime,
                limits,
            },
            move |p| {
                let mut secret = [0; 32];
                getrandom::fill(&mut secret)?;
                #[cfg(target_os = "windows")]
                let endpoint = stored
                    .approve_windows(p.manager, p.host, p.instance, p.binding, secret, p.now)?;
                #[cfg(not(target_os = "windows"))]
                let endpoint = stored.approve(
                    p.manager,
                    p.host,
                    p.instance,
                    p.binding,
                    secret,
                    p.now,
                    |_| Err(morrow_network_node::Error::Denied),
                )?;
                input.endpoint = endpoint.endpoint_reference().into_bytes();
                let frame = Request::encode_http_submit(1, &input)?;
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()?;
                let router = OwnedRouter {
                    request: frame.bytes().to_vec(),
                    inner: endpoint.router(runtime.handle().clone()),
                    _runtime: runtime,
                };
                Ok(PreparedJob {
                    input: frame.bytes().to_vec(),
                    router: Box::new(router),
                    timeout: lifetime,
                })
            },
        )
    }
}
