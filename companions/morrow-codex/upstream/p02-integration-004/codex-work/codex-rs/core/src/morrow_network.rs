//! Explicit network ownership for ModelClient's HTTP and WebSocket creation paths.
//! Implementations must enforce their own destination, credential and transport authority.
use codex_api::{ApiError, Provider, ResponsesWebsocketConnection, SharedAuthProvider};
use codex_http_client::{
    HttpTransport, Request, ReqwestTransport, Response, StreamResponse, TransportError,
};
use http::HeaderMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type NetworkFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Inputs at Core's connection boundary, before the default API client merges/authenticates headers.
/// This does not represent a completed WebSocket handshake or a sent response.create frame.
pub struct WebsocketConnectRequest {
    pub provider: Provider,
    pub auth: SharedAuthProvider,
    pub extra_headers: HeaderMap,
    pub default_headers: HeaderMap,
}

/// Caller-owned transport capability; failures must remain failures, without local fallback.
/// Boxed futures allow one object-safe capability to serve cloned ModelClient instances.
pub trait ModelNetworkBackend: std::fmt::Debug + Send + Sync {
    fn execute(&self, request: Request) -> NetworkFuture<'_, Result<Response, TransportError>>;
    fn stream(&self, request: Request)
    -> NetworkFuture<'_, Result<StreamResponse, TransportError>>;
    fn connect_websocket(
        &self,
        request: WebsocketConnectRequest,
    ) -> NetworkFuture<'_, Result<ResponsesWebsocketConnection, ApiError>>;
}

#[derive(Clone, Debug)]
pub(crate) enum ModelHttpTransport {
    Default(ReqwestTransport),
    Injected(Arc<dyn ModelNetworkBackend>),
}

impl HttpTransport for ModelHttpTransport {
    async fn execute(&self, request: Request) -> Result<Response, TransportError> {
        match self {
            Self::Default(transport) => transport.execute(request).await,
            Self::Injected(backend) => backend.execute(request).await,
        }
    }
    async fn stream(&self, request: Request) -> Result<StreamResponse, TransportError> {
        match self {
            Self::Default(transport) => transport.stream(request).await,
            Self::Injected(backend) => backend.stream(request).await,
        }
    }
}
