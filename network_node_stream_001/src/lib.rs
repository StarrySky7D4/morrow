#![deny(unsafe_code)]
//! Versioned experimental trusted outbound HTTP stream transport.
//! The optional managed adapter binds explicit host-selected HTTP resources to
//! original plugin authority; the transport alone never grants guest networking.
use std::time::Duration;

#[forbid(unsafe_code)]
pub mod client;
#[forbid(unsafe_code)]
pub mod sse;
#[forbid(unsafe_code)]
pub mod stream;

#[cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
#[forbid(unsafe_code)]
pub mod managed_sse;
#[cfg(all(feature = "managed-channel", not(target_arch = "wasm32")))]
#[allow(clippy::all)]
// Capnpc-generated RAW_SCHEMA metadata only; all handwritten modules remain forbidden.
#[allow(unsafe_code)]
mod sse_event_capnp {
    include!(concat!(env!("OUT_DIR"), "/sse_event_capnp.rs"));
}

#[cfg(all(feature = "managed-websocket", not(target_arch = "wasm32")))]
#[forbid(unsafe_code)]
pub mod websocket;
#[cfg(all(feature = "managed-websocket", not(target_arch = "wasm32")))]
#[forbid(unsafe_code)]
pub mod managed_ws;
#[cfg(all(feature = "managed-websocket", not(target_arch = "wasm32")))]
#[allow(clippy::all)]
// Only generated RAW_SCHEMA metadata; handwritten WS modules still forbid unsafe.
#[allow(unsafe_code)]
mod ws_message_capnp {
    include!(concat!(env!("OUT_DIR"), "/ws_message_capnp.rs"));
}

// Deliberately no Debug: headers and bodies can contain credentials or user content.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    /// Absolute URL for outbound calls; origin-form path/query for inbound dispatch.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
/// HTTP response with exact header value bytes, including legal HTTP obs-text.
/// Deliberately no Debug: response headers and bodies can contain secrets.
#[derive(Clone, PartialEq, Eq)]
pub struct RawHttpResponse {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_header_bytes: usize,
    pub max_concurrent: usize,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_request_bytes: 1024 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
            max_header_bytes: 16 * 1024,
            max_concurrent: 16,
            timeout: Duration::from_secs(30),
        }
    }
}
impl Limits {
    pub fn validate(self) -> Result<()> {
        if self.max_request_bytes == 0
            || self.max_request_bytes > 64 * 1024 * 1024
            || self.max_response_bytes == 0
            || self.max_response_bytes > 64 * 1024 * 1024
            || self.max_header_bytes == 0
            || self.max_header_bytes > 64 * 1024
            || self.max_concurrent == 0
            || self.max_concurrent > 128
            || self.timeout.is_zero()
            || self.timeout > Duration::from_secs(300)
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Denied,
    Limit,
    Cancelled,
    Timeout,
    Transport,
    Closed,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "network: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Raw prepared request: preserves legal non-UTF-8 header values and duplicate entries.
#[derive(Clone, PartialEq, Eq)]
pub struct RawHttpRequest {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}
impl From<HttpRequest> for RawHttpRequest {
    fn from(r: HttpRequest) -> Self {
        Self {
            method: r.method,
            target: r.target,
            headers: r
                .headers
                .into_iter()
                .map(|(n, v)| (n, v.into_bytes()))
                .collect(),
            body: r.body,
        }
    }
}
