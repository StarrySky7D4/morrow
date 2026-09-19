//! Explicit native publication on the original managed plugin instance.
//! Remote authentication never grants content access or additional outbound IO.
use crate::{
    Error, Limits, RawHttpResponse, Result,
    server::{AuthorizedHandler, AuthorizedRoute, Node, Principal, TlsIdentity},
};
use morrow_core::{dispatch::HostRuntime, io::Header, service};
use morrow_plugin_runtime::{
    io_jobs::{BrokerRouter, IoWorker, JobError, Poll},
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
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
        })
    }
    fn bound_route(
        &self,
        grant: ServiceGrant,
        method: &str,
        path: &str,
    ) -> Result<AuthorizedRoute> {
        let scope = grant.service().to_owned();
        let inner = self.inner.clone();
        let handler: AuthorizedHandler = Arc::new(move |request, cancel| {
            let inner = inner.clone();
            let grant = grant.clone();
            Box::pin(async move {
                let (request, principal) = request.into_parts();
                principal.check(grant.service())?;
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let serial = inner
                    .sequence
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                    .map_err(|_| Error::Limit)?;
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
                let headers = request
                    .headers
                    .into_iter()
                    .filter_map(|(name, value)| {
                        let lower = name.to_ascii_lowercase();
                        if lower.starts_with("proxy-")
                            || nominated.contains(&lower)
                            || matches!(
                                lower.as_str(),
                                "authorization"
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
                let invocation = service::Invocation {
                    service: grant.service().into(),
                    handler: grant.handler().into(),
                    principal: principal.id().into(),
                    method: request.method,
                    target: request.target,
                    headers,
                    body: request.body,
                };
                let request =
                    service::Request::encode(serial, &invocation).map_err(|_| Error::Invalid)?;
                let router = (inner.routers)();
                let result = inner
                    .worker
                    .lock()
                    .map_err(|_| Error::Closed)?
                    .submit_service(request, grant.clone(), router, inner.timeout);
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
                if report.cancelled {
                    return Err(Error::Cancelled);
                }
                if report.unknown || report.task.execution.outcome != Ok(0) {
                    return Err(Error::Transport);
                }
                let reply = report.service_response.ok_or(Error::Transport)?;
                Ok(RawHttpResponse {
                    status: reply.status,
                    headers: reply
                        .headers
                        .into_iter()
                        .map(|h| (h.name, h.value))
                        .collect(),
                    body: reply.body,
                })
            })
        });
        AuthorizedRoute::new(&scope, method, path, handler)
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
            approved.push(
                route
                    .host
                    .bound_route(service, &route.method, &route.path)?,
            );
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
