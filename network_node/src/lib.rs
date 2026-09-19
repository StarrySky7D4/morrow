#![forbid(unsafe_code)]
//! Experimental trusted native HTTP client and API node transport.
//! The optional managed adapter binds explicit host-selected HTTP resources to
//! original plugin authority; the transport alone never grants guest networking.
use std::time::Duration;

pub mod client;
#[cfg(feature = "plugin-adapter")]
pub mod managed_http;
#[cfg(feature = "plugin-adapter")]
pub mod plugin;
pub mod server;

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
