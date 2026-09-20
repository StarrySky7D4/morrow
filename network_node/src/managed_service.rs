//! Explicit native publication on the original managed plugin instance.
//! Content routes require a separate host-issued scope policy for each actual
//! authenticated principal. Authentication alone grants no content or outbound IO.
use crate::{
    Error, Limits, RawHttpResponse, Result,
    server::{AuthorizedHandler, AuthorizedRoute, Node, Principal, TlsIdentity},
};
use morrow_core::{dispatch::HostRuntime, io::Header, service, service_record};
use morrow_plugin_runtime::{
    io_jobs::{BrokerRouter, IoWorker, JobError, Poll, ServicePersistenceStatus},
    service_content::{ContentScope, ServiceContentPolicy},
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
/// Trusted host factory. Its routers must enforce every outbound resource approval.
pub type RouterFactory = Arc<dyn Fn() -> Box<dyn BrokerRouter> + Send + Sync>;
struct Execution {
    worker: Mutex<IoWorker>,
    sequence: AtomicU64,
    timeout: Duration,
    routers: RouterFactory,
}
impl Drop for Execution {
    fn drop(&mut self) {
        if let Ok(worker) = self.worker.get_mut() {
            worker.stop();
        }
    }
}
/// Clones share one worker, queue, instance and lifetime budgets.
#[derive(Clone)]
pub struct ServiceHost {
    inner: Arc<Execution>,
}
/// Opaque route keeps the actual worker and publication owner until listener binding.
pub struct ManagedRoute {
    host: ServiceHost,
    grant: ServiceGrant,
    method: String,
    path: String,
    journal: Option<ServiceJournal>,
    content: Option<Arc<ContentRoute>>,
    query_target: Option<String>,
}
struct ContentRoute {
    policy: ServiceContentPolicy,
    principals: BTreeMap<String, Vec<ContentScope>>,
}
impl ServiceHost {
    pub fn new(worker: IoWorker, timeout: Duration, routers: RouterFactory) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(Error::Invalid);
        }
        Ok(Self {
            inner: Arc::new(Execution {
                worker: Mutex::new(worker),
                sequence: AtomicU64::new(1),
                timeout,
                routers,
            }),
        })
    }
    pub fn route(&self, grant: ServiceGrant, method: &str, path: &str) -> Result<ManagedRoute> {
        self.inner
            .worker
            .lock()
            .map_err(|_| Error::Closed)?
            .check_service(&grant)
            .map_err(|_| Error::Denied)?;
        // Validate route syntax now; the real closure is built after listener binding.
        AuthorizedRoute::new(
            grant.service(),
            method,
            path,
            Arc::new(|_, _| Box::pin(async { Err(Error::Closed) })),
        )?;
        Ok(ManagedRoute {
            host: self.clone(),
            grant,
            method: method.into(),
            path: path.into(),
            journal: None,
            content: None,
            query_target: None,
        })
    }
    /// Requires one Idempotency-Key and persists a single execution boundary.
    /// Namespace must remain stable for the same published service after restart.
    pub fn durable_route(
        &self,
        grant: ServiceGrant,
        method: &str,
        path: &str,
        journal: ServiceJournal,
    ) -> Result<ManagedRoute> {
        let mut route = self.route(grant, method, path)?;
        route.journal = Some(journal);
        Ok(route)
    }
    /// Publish a durable content service with a fixed host-approved scope table.
    /// A principal missing from the table is denied, even if it may use the service.
    pub fn content_route(
        &self,
        grant: ServiceGrant,
        method: &str,
        path: &str,
        journal: ServiceJournal,
        policy: ServiceContentPolicy,
        principal_scopes: BTreeMap<String, Vec<ContentScope>>,
    ) -> Result<ManagedRoute> {
        policy.validate_grant(&grant).map_err(|_| Error::Denied)?;
        let mut route = self.durable_route(grant, method, path, journal)?;
        route.content = Some(Arc::new(ContentRoute {
            policy,
            principals: principal_scopes,
        }));
        Ok(route)
    }
    /// Query retained state without preparing, dispatching or retrying an operation.
    /// Repeat the original method, query string, business headers, body and key at
    /// this distinct path. The original operation target is fixed by the host.
    pub fn query_route(&self, original: &ManagedRoute, query_path: &str) -> Result<ManagedRoute> {
        if !Arc::ptr_eq(&self.inner, &original.host.inner) {
            return Err(Error::Denied);
        }
        if original.journal.is_none()
            || original.query_target.is_some()
            || original.path == query_path
        {
            return Err(Error::Invalid);
        }
        let mut route = self.route(original.grant.clone(), &original.method, query_path)?;
        route.journal = original.journal.clone();
        route.content = original.content.clone();
        route.query_target = Some(original.path.clone());
        Ok(route)
    }
    fn bound_route(&self, grant: ServiceGrant, route: ManagedRoute) -> Result<AuthorizedRoute> {
        let ManagedRoute {
            method,
            path,
            journal,
            content,
            query_target,
            ..
        } = route;
        let scope = grant.service().to_owned();
        let inner = self.inner.clone();
        let handler: AuthorizedHandler = Arc::new(move |request, cancel| {
            let inner = inner.clone();
            let grant = grant.clone();
            let journal = journal.clone();
            let content = content.clone();
            let query_target = query_target.clone();
            Box::pin(async move {
                let (request, principal) = request.into_parts();
                principal.check(grant.service())?;
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let key = if journal.is_some() {
                    let mut keys = request
                        .headers
                        .iter()
                        .filter(|(name, _)| name.eq_ignore_ascii_case("idempotency-key"));
                    let key = keys.next().ok_or(Error::Invalid)?.1.clone();
                    if keys.next().is_some() {
                        return Err(Error::Invalid);
                    }
                    Some(key)
                } else {
                    None
                };
                let access = if let Some(content) = &content {
                    let scopes = content
                        .principals
                        .get(principal.id())
                        .ok_or(Error::Denied)?
                        .clone();
                    let live_principal = principal.clone();
                    let service = grant.service().to_owned();
                    Some(
                        content
                            .policy
                            .authorize(principal.id(), scopes, move || {
                                live_principal.allows(&service)
                            })
                            .map_err(|_| Error::Denied)?,
                    )
                } else {
                    None
                };
                // Strip transport/credential fields, including Connection-nominated fields.
                let nominated: Vec<String> = request
                    .headers
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case("connection"))
                    .flat_map(|(_, value)| {
                        value
                            .split(',')
                            .map(|part| part.trim().to_ascii_lowercase())
                    })
                    .collect();
                if key.is_some() && nominated.iter().any(|name| name == "idempotency-key") {
                    return Err(Error::Invalid);
                }
                let mut headers: Vec<Header> = request
                    .headers
                    .into_iter()
                    .filter_map(|(name, value)| {
                        let lower = name.to_ascii_lowercase();
                        if (key.is_some() && lower == "idempotency-key")
                            || lower.starts_with("proxy-")
                            || nominated.contains(&lower)
                            || matches!(
                                lower.as_str(),
                                "morrow-content-scope"
                                    | "authorization"
                                    | "cookie"
                                    | "host"
                                    | "content-length"
                                    | "transfer-encoding"
                                    | "connection"
                                    | "keep-alive"
                                    | "upgrade"
                                    | "te"
                                    | "trailer"
                                    | "expect"
                            )
                        {
                            None
                        } else {
                            Some(Header {
                                name: lower,
                                value: value.into_bytes(),
                            })
                        }
                    })
                    .collect();
                if let Some(access) = &access {
                    let digest = access.scope_digest();
                    let value = digest
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>();
                    headers.push(Header {
                        name: "morrow-content-scope".into(),
                        value: value.into_bytes(),
                    });
                }
                // Normalize name order, preserving the order of repeated values.
                if journal.is_some() {
                    headers.sort_by(|a, b| a.name.cmp(&b.name));
                }
                let querying = query_target.is_some();
                let target = if let Some(original) = query_target {
                    match request.target.split_once('?') {
                        Some((_, query)) => format!("{original}?{query}"),
                        None => original,
                    }
                } else {
                    request.target
                };
                let invocation = service::Invocation {
                    service: grant.service().into(),
                    handler: grant.handler().into(),
                    principal: principal.id().into(),
                    method: request.method,
                    target,
                    headers,
                    body: request.body,
                };
                let serial = if let (Some(journal), Some(key)) = (&journal, &key) {
                    service_record::call_id(journal.policy(), key, &invocation)
                        .map_err(|_| Error::Invalid)?
                } else {
                    inner
                        .sequence
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                        .map_err(|_| Error::Limit)?
                };
                let request =
                    service::Request::encode(serial, &invocation).map_err(|_| Error::Invalid)?;
                // Factory callbacks remain outside the worker lock and are never
                // constructed for read-only queries.
                let router = (!querying).then(|| (inner.routers)());
                let result = {
                    let worker = inner.worker.lock().map_err(|_| Error::Closed)?;
                    if let (Some(journal), Some(key)) = (journal, key) {
                        if querying {
                            worker.query_service_history(
                                request,
                                grant.clone(),
                                journal,
                                &key,
                                access.clone(),
                                inner.timeout,
                            )
                        } else if let Some(access) = &access {
                            worker.submit_service_content(
                                request,
                                grant.clone(),
                                journal,
                                &key,
                                access.clone(),
                                router.ok_or(Error::Closed)?,
                                inner.timeout,
                            )
                        } else {
                            worker.submit_service_durable(
                                request,
                                grant.clone(),
                                journal,
                                &key,
                                router.ok_or(Error::Closed)?,
                                inner.timeout,
                            )
                        }
                    } else {
                        worker.submit_service(
                            request,
                            grant.clone(),
                            router.ok_or(Error::Closed)?,
                            inner.timeout,
                        )
                    }
                };
                let mut job = match result {
                    Ok(job) => job,
                    Err(JobError::Busy) => {
                        return Ok(RawHttpResponse {
                            status: 429,
                            headers: vec![],
                            body: b"Service busy".to_vec(),
                        });
                    }
                    Err(JobError::Limit) => return Err(Error::Limit),
                    Err(_) => return Err(Error::Denied),
                };
                loop {
                    principal.check(grant.service())?;
                    if let Some(access) = &access {
                        access.check().map_err(|_| Error::Denied)?;
                    }
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    match job.poll() {
                        Poll::Ready => break,
                        Poll::Pending => {}
                        _ => return Err(Error::Closed),
                    }
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(Error::Cancelled),
                        _ = tokio::time::sleep(Duration::from_millis(2)) => {}
                    }
                }
                // Read consumes only after the worker rechecks original authority.
                let report = job
                    .read(4 * service::MAX_FRAME_BYTES)
                    .map_err(|_| Error::Transport)?
                    .ok_or(Error::Closed)?;
                principal.check(grant.service())?;
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                inner
                    .worker
                    .lock()
                    .map_err(|_| Error::Closed)?
                    .check_service(&grant)
                    .map_err(|_| Error::Denied)?;
                if let Some(access) = &access {
                    access.check().map_err(|_| Error::Denied)?;
                    inner
                        .worker
                        .lock()
                        .map_err(|_| Error::Closed)?
                        .check_content_access(access)
                        .map_err(|_| Error::Denied)?;
                }
                if !report.service_retention_valid() {
                    return Ok(service_state_reply(
                        querying,
                        "expired",
                        410,
                        b"Service request expired",
                    ));
                }
                if report.cancelled {
                    return Err(Error::Cancelled);
                }
                match report.service_persistence {
                    Some(ServicePersistenceStatus::Missing) => {
                        return Ok(service_state_reply(
                            querying,
                            "missing",
                            404,
                            b"Service request not found",
                        ));
                    }
                    Some(ServicePersistenceStatus::Prepared) => {
                        return Ok(service_state_reply(
                            querying,
                            "prepared",
                            202,
                            b"Service request prepared; not dispatched",
                        ));
                    }
                    Some(ServicePersistenceStatus::Cancelled) => {
                        return Ok(service_state_reply(
                            querying,
                            "cancelled",
                            409,
                            b"Service request cancelled before dispatch",
                        ));
                    }
                    Some(ServicePersistenceStatus::Unknown) => {
                        return Ok(service_state_reply(
                            querying,
                            "unknown",
                            409,
                            b"Service outcome requires reconciliation",
                        ));
                    }
                    Some(ServicePersistenceStatus::Conflict) => {
                        return Ok(service_state_reply(
                            querying,
                            "conflict",
                            409,
                            b"Idempotency key conflict",
                        ));
                    }
                    Some(ServicePersistenceStatus::Expired) => {
                        return Ok(service_state_reply(
                            querying,
                            "expired",
                            410,
                            b"Service request expired",
                        ));
                    }
                    Some(ServicePersistenceStatus::Unavailable) => return Err(Error::Closed),
                    _ => {}
                }
                if report.unknown || report.task.execution.outcome != Ok(0) {
                    return Err(Error::Transport);
                }
                let reply = report.service_response.ok_or(Error::Transport)?;
                // Observed results preserve the exact stored response. Appending
                // metadata here could exceed an originally valid header budget.
                let headers = reply
                    .headers
                    .into_iter()
                    .map(|h| (h.name, h.value))
                    .collect();
                Ok(RawHttpResponse {
                    status: reply.status,
                    headers,
                    body: reply.body,
                })
            })
        });
        AuthorizedRoute::new(&scope, &method, &path, handler)
    }
    /// Stops all clones and returns the original host to its owner exactly once.
    pub async fn shutdown(&self) -> Result<HostRuntime> {
        self.inner.worker.lock().map_err(|_| Error::Closed)?.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(host) = self
                .inner
                .worker
                .lock()
                .map_err(|_| Error::Closed)?
                .try_finish()
                .map_err(|_| Error::Closed)?
            {
                return Ok(host);
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}
/// Listener supervisor retains its resource lease through actual socket shutdown.
/// Dropping the handle signals shutdown; it does not abort the lease owner.
pub struct ManagedNode {
    address: SocketAddr,
    stop: CancellationToken,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
}
impl ManagedNode {
    pub async fn bind(
        address: SocketAddr,
        grant: ListenerGrant,
        principals: Vec<Principal>,
        routes: Vec<ManagedRoute>,
        limits: Limits,
    ) -> Result<Self> {
        Self::bind_inner(address, grant, principals, routes, limits, None).await
    }
    pub async fn bind_tls(
        address: SocketAddr,
        grant: ListenerGrant,
        principals: Vec<Principal>,
        routes: Vec<ManagedRoute>,
        limits: Limits,
        identity: TlsIdentity,
    ) -> Result<Self> {
        Self::bind_inner(address, grant, principals, routes, limits, Some(identity)).await
    }
    async fn bind_inner(
        address: SocketAddr,
        grant: ListenerGrant,
        principals: Vec<Principal>,
        routes: Vec<ManagedRoute>,
        limits: Limits,
        identity: Option<TlsIdentity>,
    ) -> Result<Self> {
        let owner = routes.first().ok_or(Error::Invalid)?.host.clone();
        let mut approved = Vec::with_capacity(routes.len());
        for route in routes {
            if !Arc::ptr_eq(&owner.inner, &route.host.inner) {
                return Err(Error::Denied);
            }
            owner
                .inner
                .worker
                .lock()
                .map_err(|_| Error::Closed)?
                .check_listener(&grant)
                .map_err(|_| Error::Denied)?;
            let service = route
                .grant
                .bound_to_listener(&grant)
                .map_err(|_| Error::Denied)?;
            approved.push(route.host.clone().bound_route(service, route)?);
        }
        grant.activate().map_err(|_| Error::Denied)?;
        let node = if let Some(identity) = identity {
            Node::bind_authorized_tls(address, principals, approved, limits, identity).await?
        } else {
            Node::bind_authorized(address, principals, approved, limits).await?
        };
        if owner
            .inner
            .worker
            .lock()
            .map_err(|_| Error::Closed)?
            .check_listener(&grant)
            .is_err()
        {
            let _ = node.shutdown().await;
            return Err(Error::Denied);
        }
        let address = node.local_addr();
        let stop = node.stop_token();
        let observed = stop.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = observed.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(5)) => {
                        if owner.inner.worker.lock().map_or(true, |worker| worker.check_listener(&grant).is_err()) { break; }
                    }
                }
            }
            let result = node.shutdown().await;
            drop(grant);
            result
        });
        Ok(Self {
            address,
            stop,
            task: Some(task),
        })
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub async fn shutdown(mut self) -> Result<()> {
        self.stop.cancel();
        self.task
            .take()
            .ok_or(Error::Closed)?
            .await
            .map_err(|_| Error::Transport)?
    }
}
impl Drop for ManagedNode {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

fn fixed_service_reply(status: u16, body: &[u8]) -> RawHttpResponse {
    RawHttpResponse {
        status,
        headers: vec![("content-type".into(), b"text/plain; charset=utf-8".to_vec())],
        body: body.to_vec(),
    }
}

fn service_state_reply(querying: bool, state: &str, status: u16, body: &[u8]) -> RawHttpResponse {
    let mut reply = fixed_service_reply(status, body);
    if querying {
        reply
            .headers
            .push(("morrow-service-state".into(), state.as_bytes().to_vec()));
    }
    reply
}
