//! Bounded HTTP/TLS node. Listener configuration belongs to the trusted native host.
//! Plain HTTP is loopback-only; TLS may explicitly bind another configured address.
use crate::{Error, HttpRequest, HttpResponse, Limits, RawHttpResponse, Result};
use axum::{Router, body::Body, extract::State};
use futures_util::StreamExt;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response, StatusCode, Uri};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
    time::{Instant, sleep_until, timeout},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use tokio_util::sync::CancellationToken;

pub type Handler = Arc<
    dyn Fn(
            HttpRequest,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Result<HttpResponse>> + Send>>
        + Send
        + Sync,
>;

pub struct Route {
    method: Method,
    path: String,
    handler: Handler,
}
fn route_key(method: &str, path: &str) -> Result<(Method, String)> {
    if method.is_empty()
        || method.len() > 32
        || !method
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'-')
        || !path.starts_with('/')
        || path.len() > 8192
        || path.contains(['*', '{', '}', '#', '?'])
    {
        return Err(Error::Invalid);
    }
    let method = Method::from_bytes(method.as_bytes()).map_err(|_| Error::Invalid)?;
    let uri: Uri = path.parse().map_err(|_| Error::Invalid)?;
    if uri.scheme().is_some() || uri.authority().is_some() || uri.path() != path {
        return Err(Error::Invalid);
    }
    Ok((method, path.into()))
}
impl Route {
    pub fn new(method: &str, path: &str, handler: Handler) -> Result<Self> {
        let (method, path) = route_key(method, path)?;
        Ok(Self {
            method,
            path,
            handler,
        })
    }
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
struct PrincipalState {
    id: String,
    token_digest: [u8; 32],
    services: BTreeSet<String>,
    expires: Instant,
    revoked: AtomicBool,
}
/// A host-configured authenticated remote identity. Clones share revocation and
/// the original deadline; neither cloning nor a request header renews authority.
/// No bearer text or secret-bearing Debug implementation is retained.
#[derive(Clone)]
pub struct Principal {
    inner: Arc<PrincipalState>,
}
impl Principal {
    pub fn new(id: &str, token: &str, services: &[&str], ttl: Duration) -> Result<Self> {
        validate_bearer_token(token)?;
        if !safe_id(id)
            || services.is_empty()
            || services.len() > 64
            || ttl.is_zero()
            || ttl > Duration::from_secs(24 * 60 * 60)
        {
            return Err(Error::Invalid);
        }
        let mut scopes = BTreeSet::new();
        for service in services {
            if !safe_id(service) || !scopes.insert((*service).to_owned()) {
                return Err(Error::Invalid);
            }
        }
        Ok(Self {
            inner: Arc::new(PrincipalState {
                id: id.into(),
                token_digest: Sha256::digest(token.as_bytes()).into(),
                services: scopes,
                expires: Instant::now().checked_add(ttl).ok_or(Error::Invalid)?,
                revoked: AtomicBool::new(false),
            }),
        })
    }
    pub fn id(&self) -> &str {
        &self.inner.id
    }
    pub fn revoke(&self) {
        self.inner.revoked.store(true, Ordering::Release);
    }
    fn active(&self) -> bool {
        !self.inner.revoked.load(Ordering::Acquire) && Instant::now() < self.inner.expires
    }
    pub fn allows(&self, service: &str) -> bool {
        self.active() && self.inner.services.contains(service)
    }
    pub fn check(&self, service: &str) -> Result<()> {
        if self.allows(service) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
/// Only the server constructs this after authenticating the bearer and checking
/// the route's fixed service scope. HTTP headers cannot supply its principal.
pub struct AuthorizedRequest {
    request: HttpRequest,
    principal: Principal,
}
impl AuthorizedRequest {
    pub fn into_parts(self) -> (HttpRequest, Principal) {
        (self.request, self.principal)
    }
}
pub type AuthorizedHandler = Arc<
    dyn Fn(
            AuthorizedRequest,
            CancellationToken,
        ) -> Pin<Box<dyn Future<Output = Result<RawHttpResponse>> + Send>>
        + Send
        + Sync,
>;
pub struct AuthorizedRoute {
    method: Method,
    path: String,
    service: String,
    handler: AuthorizedHandler,
}
impl AuthorizedRoute {
    pub fn new(
        service: &str,
        method: &str,
        path: &str,
        handler: AuthorizedHandler,
    ) -> Result<Self> {
        if !safe_id(service) {
            return Err(Error::Invalid);
        }
        let (method, path) = route_key(method, path)?;
        Ok(Self {
            method,
            path,
            service: service.into(),
            handler,
        })
    }
}
#[derive(Clone)]
enum Authenticated {
    Legacy,
    Principal(Principal),
}
enum Authentication {
    Legacy([u8; 32]),
    Principals(Vec<Principal>),
}
impl Authentication {
    fn authenticate(&self, headers: &HeaderMap) -> Option<Authenticated> {
        let received = bearer_digest(headers)?;
        match self {
            Self::Legacy(expected) => {
                digest_eq(&received, expected).then_some(Authenticated::Legacy)
            }
            Self::Principals(principals) => {
                let mut matched = None;
                // Bound the table and compare every digest, with no early exit
                // within digest bytes. Do not expose configured IDs on failure.
                for principal in principals {
                    if digest_eq(&received, &principal.inner.token_digest) && principal.active() {
                        matched = Some(Authenticated::Principal(principal.clone()));
                    }
                }
                matched
            }
        }
    }
}
#[derive(Clone)]
enum BoundHandler {
    Legacy(Handler),
    Authorized {
        service: String,
        handler: AuthorizedHandler,
    },
}
impl BoundHandler {
    fn check(&self, identity: &Authenticated) -> Result<()> {
        match (self, identity) {
            (Self::Legacy(_), Authenticated::Legacy) => Ok(()),
            (Self::Authorized { service, .. }, Authenticated::Principal(principal)) => {
                principal.check(service)
            }
            _ => Err(Error::Denied),
        }
    }
    async fn invoke(
        &self,
        input: HttpRequest,
        identity: Authenticated,
        token: CancellationToken,
    ) -> Result<RawHttpResponse> {
        self.check(&identity)?;
        match (self, identity) {
            (Self::Legacy(handler), Authenticated::Legacy) => {
                let value = handler(input, token).await?;
                Ok(RawHttpResponse {
                    status: value.status,
                    body: value.body,
                    headers: value
                        .headers
                        .into_iter()
                        .map(|(name, value)| (name, value.into_bytes()))
                        .collect(),
                })
            }
            (Self::Authorized { handler, .. }, Authenticated::Principal(principal)) => {
                handler(
                    AuthorizedRequest {
                        request: input,
                        principal,
                    },
                    token,
                )
                .await
            }
            _ => Err(Error::Denied),
        }
    }
}
type Routes = BTreeMap<String, BTreeMap<String, BoundHandler>>;
fn route_table(
    entries: impl IntoIterator<Item = (Method, String, BoundHandler)>,
) -> Result<Routes> {
    let mut table = Routes::new();
    let mut count = 0;
    for (method, path, handler) in entries {
        count += 1;
        if count > 1024
            || table
                .entry(path)
                .or_default()
                .insert(method.to_string(), handler)
                .is_some()
        {
            return Err(Error::Invalid);
        }
    }
    if count == 0 {
        return Err(Error::Invalid);
    }
    Ok(table)
}

struct Shared {
    routes: Routes,
    authentication: Authentication,
    limits: Limits,
    requests: Semaphore,
    stop: CancellationToken,
}

/// Validate before creating any service state. Tokens must be generated with sufficient entropy.
/// Length is an input floor, not a claim that arbitrary user text is a secure secret.
pub fn validate_bearer_token(token: &str) -> Result<()> {
    if !(32..=4096).contains(&token.len()) || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(Error::Invalid);
    }
    Ok(())
}

/// A validated server identity. Neither the configuration nor its private key is exposed.
/// Both PEM inputs are capped at 64 KiB; certificate chains contain at most 16 certificates.
pub struct TlsIdentity {
    config: Arc<rustls::ServerConfig>,
}
impl TlsIdentity {
    pub fn from_pem(cert_chain: &[u8], private_key: &[u8]) -> Result<Self> {
        if cert_chain.is_empty()
            || private_key.is_empty()
            || cert_chain.len() > 65_536
            || private_key.len() > 65_536
        {
            return Err(Error::Invalid);
        }
        let mut certificates = Vec::new();
        for certificate in CertificateDer::pem_slice_iter(cert_chain) {
            if certificates.len() == 16 {
                return Err(Error::Invalid);
            }
            certificates.push(certificate.map_err(|_| Error::Invalid)?);
        }
        if certificates.is_empty() {
            return Err(Error::Invalid);
        }
        let mut keys = PrivateKeyDer::pem_slice_iter(private_key);
        let key = keys
            .next()
            .ok_or(Error::Invalid)?
            .map_err(|_| Error::Invalid)?;
        if keys.next().is_some() {
            return Err(Error::Invalid);
        }
        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| Error::Invalid)?
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|_| Error::Invalid)?;
        // The node serves HTTP/1.1; do not negotiate an unsupported protocol.
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Self {
            config: Arc::new(config),
        })
    }
}

pub struct Node {
    address: SocketAddr,
    stop: CancellationToken,
    task: Option<JoinHandle<io::Result<()>>>,
}
impl Node {
    pub async fn bind(
        address: SocketAddr,
        bearer_token: String,
        routes: Vec<Route>,
        limits: Limits,
    ) -> Result<Self> {
        if !address.ip().is_loopback() {
            return Err(Error::Denied);
        }
        Self::bind_inner(address, bearer_token, routes, limits, None).await
    }
    /// Explicit trusted-host TLS configuration; unlike plain HTTP, remote bind addresses
    /// are permitted. Calling this does not authorize any guest or bypass bearer checks.
    pub async fn bind_tls(
        address: SocketAddr,
        bearer_token: String,
        routes: Vec<Route>,
        limits: Limits,
        identity: TlsIdentity,
    ) -> Result<Self> {
        Self::bind_inner(
            address,
            bearer_token,
            routes,
            limits,
            Some(TlsAcceptor::from(identity.config)),
        )
        .await
    }
    async fn bind_inner(
        address: SocketAddr,
        bearer_token: String,
        routes: Vec<Route>,
        limits: Limits,
        tls: Option<TlsAcceptor>,
    ) -> Result<Self> {
        limits.validate()?;
        validate_bearer_token(&bearer_token)?;
        let table = route_table(routes.into_iter().map(|route| {
            (
                route.method,
                route.path,
                BoundHandler::Legacy(route.handler),
            )
        }))?;
        let authentication = Authentication::Legacy(Sha256::digest(bearer_token.as_bytes()).into());
        drop(bearer_token);
        Self::listen(address, table, authentication, limits, tls).await
    }
    /// Plain HTTP retains the original loopback-only restriction.
    pub async fn bind_authorized(
        address: SocketAddr,
        principals: Vec<Principal>,
        routes: Vec<AuthorizedRoute>,
        limits: Limits,
    ) -> Result<Self> {
        if !address.ip().is_loopback() {
            return Err(Error::Denied);
        }
        Self::authorized_inner(address, principals, routes, limits, None).await
    }
    pub async fn bind_authorized_tls(
        address: SocketAddr,
        principals: Vec<Principal>,
        routes: Vec<AuthorizedRoute>,
        limits: Limits,
        identity: TlsIdentity,
    ) -> Result<Self> {
        Self::authorized_inner(
            address,
            principals,
            routes,
            limits,
            Some(TlsAcceptor::from(identity.config)),
        )
        .await
    }
    async fn authorized_inner(
        address: SocketAddr,
        principals: Vec<Principal>,
        routes: Vec<AuthorizedRoute>,
        limits: Limits,
        tls: Option<TlsAcceptor>,
    ) -> Result<Self> {
        limits.validate()?;
        if principals.is_empty() || principals.len() > 1024 {
            return Err(Error::Invalid);
        }
        let mut ids = BTreeSet::new();
        let mut digests = BTreeSet::new();
        for principal in &principals {
            if !principal.active()
                || !ids.insert(principal.id().to_owned())
                || !digests.insert(principal.inner.token_digest)
            {
                return Err(Error::Invalid);
            }
        }
        let table = route_table(routes.into_iter().map(|route| {
            (
                route.method,
                route.path,
                BoundHandler::Authorized {
                    service: route.service,
                    handler: route.handler,
                },
            )
        }))?;
        Self::listen(
            address,
            table,
            Authentication::Principals(principals),
            limits,
            tls,
        )
        .await
    }
    async fn listen(
        address: SocketAddr,
        table: Routes,
        authentication: Authentication,
        limits: Limits,
        tls: Option<TlsAcceptor>,
    ) -> Result<Self> {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|_| Error::Transport)?;
        let address = listener.local_addr().map_err(|_| Error::Transport)?;
        let stop = CancellationToken::new();
        let shared = Arc::new(Shared {
            routes: table,
            authentication,
            limits,
            requests: Semaphore::new(limits.max_concurrent),
            stop: stop.clone(),
        });
        let listener = NodeListener {
            listener,
            stop: stop.clone(),
            connections: Arc::new(Semaphore::new(limits.max_concurrent * 2)),
            lifetime: limits.timeout.saturating_mul(2),
            tls,
        };
        let router = Router::new().fallback(dispatch).with_state(shared);
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
        });
        Ok(Self {
            address,
            stop,
            task: Some(task),
        })
    }
    #[cfg(feature = "plugin-adapter")]
    pub(crate) fn stop_token(&self) -> CancellationToken {
        self.stop.clone()
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    /// Await destruction of the task owning the listening socket, including after
    /// forced cancellation. Timeout does not certify that every separately spawned
    /// connection task has exited; their socket IO still observes the stop token.
    /// Callers releasing a listener lease must await this future, not merely drop it.
    pub async fn shutdown(mut self) -> Result<()> {
        self.stop.cancel();
        let Some(mut task) = self.task.take() else {
            return Err(Error::Closed);
        };
        match timeout(Duration::from_secs(2), &mut task).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(_) => Err(Error::Transport),
            Err(_) => {
                task.abort();
                // abort() requests cancellation; awaiting is what confirms that the
                // listener owner has actually been dropped before its lease is released.
                // Independent connection tasks retain the shared cancellation token.
                let _ = task.await;
                Err(Error::Timeout)
            }
        }
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        // Best-effort cancellation only: synchronous Drop cannot confirm task exit.
        // Managed listener owners use and await shutdown() before releasing a lease.
        self.stop.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

// Axum starts connection tasks independently. Cancellation is enforced at every
// socket read/write, including connections that never supplied a complete header.
// A fixed lifetime of twice the request timeout also bounds unauthenticated idle or
// trickling sockets. Keep-alive connections share this lifetime; it is not refreshed.
struct NodeListener {
    listener: TcpListener,
    stop: CancellationToken,
    connections: Arc<Semaphore>,
    lifetime: Duration,
    tls: Option<TlsAcceptor>,
}
struct NodeStream {
    stream: TcpStream,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
    expires: Pin<Box<tokio::time::Sleep>>,
    _lease: OwnedSemaphorePermit,
}
impl axum::serve::Listener for NodeListener {
    type Io = Box<dyn Connection>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, address)) => {
                    let Ok(lease) = self.connections.clone().try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    let stream = NodeStream {
                        stream,
                        cancelled: Box::pin(self.stop.clone().cancelled_owned()),
                        expires: Box::pin(tokio::time::sleep(self.lifetime)),
                        _lease: lease,
                    };
                    let stream: Box<dyn Connection> = match &self.tls {
                        None => Box::new(stream),
                        Some(acceptor) => {
                            let acceptor = acceptor.clone();
                            Box::new(LazyTlsStream {
                                state: TlsState::Handshake(Box::pin(async move {
                                    acceptor.accept(stream).await
                                })),
                            })
                        }
                    };
                    // Axum polls each handshake in its own bounded connection task.
                    // Never await a peer handshake inside this listener's accept loop.
                    return (stream, address);
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }
}
trait Connection: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Connection for T {}

type Handshake = Pin<Box<dyn Future<Output = io::Result<TlsStream<NodeStream>>> + Send>>;
enum TlsState {
    Handshake(Handshake),
    Ready(Box<TlsStream<NodeStream>>),
    Failed,
}
struct LazyTlsStream {
    state: TlsState,
}
impl LazyTlsStream {
    fn handshake(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let TlsState::Handshake(future) = &mut self.state {
            match future.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Ok(stream)) => self.state = TlsState::Ready(Box::new(stream)),
                Poll::Ready(Err(_)) => self.state = TlsState::Failed,
            }
        }
        match &self.state {
            TlsState::Ready(_) => Poll::Ready(Ok(())),
            _ => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "TLS connection failed",
            ))),
        }
    }
}
impl AsyncRead for LazyTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        std::task::ready!(this.handshake(cx))?;
        let TlsState::Ready(stream) = &mut this.state else {
            unreachable!()
        };
        // Check the lower transport even when rustls has buffered plaintext to deliver.
        if stream.get_mut().0.stopped(cx) {
            return Poll::Ready(Err(NodeStream::closed()));
        }
        Pin::new(stream.as_mut()).poll_read(cx, buffer)
    }
}
impl AsyncWrite for LazyTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        std::task::ready!(this.handshake(cx))?;
        let TlsState::Ready(stream) = &mut this.state else {
            unreachable!()
        };
        if stream.get_mut().0.stopped(cx) {
            return Poll::Ready(Err(NodeStream::closed()));
        }
        Pin::new(stream.as_mut()).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        std::task::ready!(this.handshake(cx))?;
        let TlsState::Ready(stream) = &mut this.state else {
            unreachable!()
        };
        if stream.get_mut().0.stopped(cx) {
            return Poll::Ready(Err(NodeStream::closed()));
        }
        Pin::new(stream.as_mut()).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        std::task::ready!(this.handshake(cx))?;
        let TlsState::Ready(stream) = &mut this.state else {
            unreachable!()
        };
        if stream.get_mut().0.stopped(cx) {
            return Poll::Ready(Err(NodeStream::closed()));
        }
        Pin::new(stream.as_mut()).poll_shutdown(cx)
    }
}

impl NodeStream {
    fn stopped(&mut self, cx: &mut Context<'_>) -> bool {
        self.cancelled.as_mut().poll(cx).is_ready() || self.expires.as_mut().poll(cx).is_ready()
    }
    fn closed() -> io::Error {
        io::Error::new(io::ErrorKind::ConnectionAborted, "node closed")
    }
}
impl AsyncRead for NodeStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.stopped(cx) {
            return Poll::Ready(Err(Self::closed()));
        }
        Pin::new(&mut this.stream).poll_read(cx, buffer)
    }
}
impl AsyncWrite for NodeStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.stopped(cx) {
            return Poll::Ready(Err(Self::closed()));
        }
        Pin::new(&mut this.stream).poll_write(cx, bytes)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.stopped(cx) {
            return Poll::Ready(Err(Self::closed()));
        }
        Pin::new(&mut this.stream).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.stopped(cx) {
            return Poll::Ready(Err(Self::closed()));
        }
        Pin::new(&mut this.stream).poll_shutdown(cx)
    }
}

fn fixed(status: StatusCode, text: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(text));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
fn failure(error: Error) -> Response<Body> {
    match error {
        Error::Invalid => fixed(StatusCode::BAD_REQUEST, "invalid request"),
        Error::Denied => fixed(StatusCode::FORBIDDEN, "request denied"),
        Error::Limit => fixed(StatusCode::PAYLOAD_TOO_LARGE, "request limit"),
        Error::Cancelled | Error::Closed => {
            fixed(StatusCode::SERVICE_UNAVAILABLE, "node unavailable")
        }
        Error::Timeout => fixed(StatusCode::GATEWAY_TIMEOUT, "request timed out"),
        Error::Transport => fixed(StatusCode::BAD_GATEWAY, "handler failed"),
    }
}
fn header_size(headers: &HeaderMap) -> Option<usize> {
    headers.iter().try_fold(0usize, |cost, (name, value)| {
        cost.checked_add(name.as_str().len())?
            .checked_add(value.as_bytes().len())?
            .checked_add(4)
    })
}
fn bearer_digest(headers: &HeaderMap) -> Option<[u8; 32]> {
    let mut values = headers.get_all(http::header::AUTHORIZATION).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let (scheme, token) = value.to_str().ok()?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") || validate_bearer_token(token).is_err() {
        return None;
    }
    Some(Sha256::digest(token.as_bytes()).into())
}
fn digest_eq(received: &[u8; 32], expected: &[u8; 32]) -> bool {
    received
        .iter()
        .zip(expected)
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}
fn forbidden_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "proxy-connection"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}
fn response(value: RawHttpResponse, limits: Limits) -> Result<Response<Body>> {
    if value.body.len() > limits.max_response_bytes {
        return Err(Error::Limit);
    }
    let status = StatusCode::from_u16(value.status).map_err(|_| Error::Invalid)?;
    if status.as_u16() < 200
        || status.as_u16() >= 600
        || (matches!(value.status, 204 | 205 | 304) && !value.body.is_empty())
    {
        return Err(Error::Invalid);
    }
    let mut headers = HeaderMap::new();
    let mut cost = 0usize;
    for (name, value) in value.headers {
        cost = cost
            .checked_add(name.len())
            .and_then(|v| v.checked_add(value.len()))
            .and_then(|v| v.checked_add(4))
            .ok_or(Error::Limit)?;
        if cost > limits.max_header_bytes {
            return Err(Error::Limit);
        }
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::Invalid)?;
        if forbidden_response_header(&name) {
            return Err(Error::Denied);
        }
        let value = HeaderValue::from_bytes(&value).map_err(|_| Error::Invalid)?;
        headers.append(name, value);
    }
    let mut response = Response::new(Body::from(value.body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

async fn dispatch(State(state): State<Arc<Shared>>, request: Request<Body>) -> Response<Body> {
    if state.stop.is_cancelled() {
        return failure(Error::Closed);
    }
    let deadline = Instant::now() + state.limits.timeout;
    let target = request
        .uri()
        .path_and_query()
        .map_or("/", |value| value.as_str());
    if request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || target.len() > state.limits.max_header_bytes
    {
        return failure(Error::Invalid);
    }
    if header_size(request.headers()).is_none_or(|size| size > state.limits.max_header_bytes) {
        return fixed(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE, "header limit");
    }
    let Some(identity) = state.authentication.authenticate(request.headers()) else {
        let mut response = fixed(StatusCode::UNAUTHORIZED, "authentication required");
        response.headers_mut().insert(
            http::header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Bearer"),
        );
        return response;
    };
    let Some(methods) = state.routes.get(request.uri().path()) else {
        return fixed(StatusCode::NOT_FOUND, "route not found");
    };
    let Some(handler) = methods.get(request.method().as_str()).cloned() else {
        let mut response = fixed(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
        if let Ok(allow) =
            HeaderValue::from_str(&methods.keys().cloned().collect::<Vec<_>>().join(", "))
        {
            response.headers_mut().insert(http::header::ALLOW, allow);
        }
        return response;
    };
    if let Err(error) = handler.check(&identity) {
        return failure(error);
    }
    let Ok(_permit) = state.requests.try_acquire() else {
        return fixed(StatusCode::TOO_MANY_REQUESTS, "node busy");
    };
    let mut input = HttpRequest {
        method: request.method().to_string(),
        target: target.into(),
        headers: Vec::new(),
        body: Vec::new(),
    };
    for (name, value) in request.headers() {
        if name == http::header::AUTHORIZATION || name == http::header::PROXY_AUTHORIZATION {
            continue;
        }
        let Ok(value) = value.to_str() else {
            return failure(Error::Invalid);
        };
        input.headers.push((name.as_str().into(), value.into()));
    }
    let token = state.stop.child_token();
    // Also cancels if the HTTP peer disconnects and drops this request future.
    let _cancel_on_drop = token.clone().drop_guard();
    let execution = async {
        let mut stream = request.into_body().into_data_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| Error::Invalid)?;
            if chunk.len()
                > state
                    .limits
                    .max_request_bytes
                    .saturating_sub(input.body.len())
            {
                return Err(Error::Limit);
            }
            input.body.extend_from_slice(&chunk);
        }
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        handler.invoke(input, identity.clone(), token.clone()).await
    };
    let value = tokio::select! {
        biased;
        _ = token.cancelled() => return failure(Error::Cancelled),
        _ = sleep_until(deadline) => { token.cancel(); return failure(Error::Timeout); }
        value = execution => value,
    };
    if token.is_cancelled() || state.stop.is_cancelled() {
        return failure(Error::Cancelled);
    }
    if Instant::now() >= deadline {
        token.cancel();
        return failure(Error::Timeout);
    }
    if let Err(error) = handler.check(&identity) {
        return failure(error);
    }
    match value {
        Ok(value) => match response(value, state.limits) {
            Ok(response) => response,
            Err(_) => fixed(StatusCode::BAD_GATEWAY, "invalid handler response"),
        },
        Err(error) => failure(error),
    }
}
