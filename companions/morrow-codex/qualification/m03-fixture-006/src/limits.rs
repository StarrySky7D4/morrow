pub const REQUEST_BODY: usize = 32 * 1024;
pub const RESPONSE_BODY: usize = 64 * 1024;
pub const HEADER_BYTES: usize = 8 * 1024;
pub const HEADER_COUNT: usize = 32;
pub const ERROR_BODY: usize = 4 * 1024;
pub const MAX_CHUNK: usize = 8 * 1024;
pub const MAX_IN_FLIGHT: usize = 16 * 1024;
pub const CONSUMER_EVENTS: usize = 8;
pub const CLEANUP_GRACE_MS: u64 = 2000;

pub fn fixture_base(url: &str) -> Result<(), &'static str> {
    let uri: http::Uri = url.parse().map_err(|_| "invalid fixture URL")?;
    if uri.scheme_str() != Some("http")
        || uri.host() != Some("127.0.0.1")
        || uri.port_u16().is_none_or(|p| p == 0)
        || uri.path() != "/v1"
        || uri.query().is_some()
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
    {
        return Err("only exact loopback fixture base http://127.0.0.1:PORT/v1");
    }
    Ok(())
}
