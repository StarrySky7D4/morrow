//! Trusted native outbound HTTP. No plugin approval or reusable guest capability is created here.
use crate::{Error, HttpRequest, HttpResponse, Limits, RawHttpResponse, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

const MAX_TARGET_BYTES: usize = 8192;
const MAX_DNS_ADDRESSES: usize = 32;
const METHODS: &[&str] = &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

/// Exact origin and method allowlist, chosen by a trusted host. This is not a guest grant.
/// Deliberately no Debug: origins may name private services.
pub struct EndpointPolicy {
    origin: Url,
    methods: BTreeSet<String>,
    local: bool,
}
impl EndpointPolicy {
    pub fn new(origin: &str, methods: &[&str], allow_local_http: bool) -> Result<Self> {
        Self::prepare(origin, methods, allow_local_http, false)
    }
    /// Explicit trusted-host exception for loopback HTTPS, never a private-network wildcard.
    /// Certificate and hostname validation remain mandatory.
    pub fn local_https(origin: &str, methods: &[&str]) -> Result<Self> {
        Self::prepare(origin, methods, false, true)
    }
    fn prepare(
        origin: &str,
        methods: &[&str],
        allow_local_http: bool,
        local_https: bool,
    ) -> Result<Self> {
        let parsed = parse_target(origin)?;
        // URL parsing normalizes dot segments. Check the original authority suffix as well.
        let authority = origin.split_once("://").ok_or(Error::Invalid)?.1;
        let suffix = authority
            .find(['/', '?', '#'])
            .map(|n| &authority[n..])
            .unwrap_or("");
        if !matches!(suffix, "" | "/")
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(Error::Invalid);
        }
        let local = local_origin(&parsed)
            && ((parsed.scheme() == "http" && allow_local_http)
                || (parsed.scheme() == "https" && local_https));
        if local_https && !local {
            return Err(Error::Denied);
        }
        if !local && parsed.scheme() != "https" {
            return Err(Error::Denied);
        }
        if !local
            && let Some(ip) = literal_ip(&parsed)
            && !public_ip(ip)
        {
            return Err(Error::Denied);
        }
        if methods.is_empty() || methods.len() > METHODS.len() {
            return Err(Error::Invalid);
        }
        let mut approved = BTreeSet::new();
        for &method in methods {
            if !METHODS.contains(&method) || !approved.insert(method.to_owned()) {
                return Err(Error::Invalid);
            }
        }
        Ok(Self {
            origin: parsed,
            methods: approved,
            local,
        })
    }
}

pub struct Client {
    policy: EndpointPolicy,
    limits: Limits,
    concurrent: Arc<Semaphore>,
    root_certificate: Option<reqwest::Certificate>,
}
impl Client {
    pub fn new(policy: EndpointPolicy, limits: Limits) -> Result<Self> {
        limits.validate()?;
        Ok(Self {
            policy,
            limits,
            concurrent: Arc::new(Semaphore::new(limits.max_concurrent)),
            root_certificate: None,
        })
    }
    /// Add exactly one bounded DER trust anchor chosen by the trusted host.
    /// Built-in roots and ordinary certificate/hostname checks remain enabled.
    pub fn with_root_certificate(
        policy: EndpointPolicy,
        limits: Limits,
        der: &[u8],
    ) -> Result<Self> {
        if der.is_empty() {
            return Err(Error::Invalid);
        }
        if der.len() > 64 * 1024 {
            return Err(Error::Limit);
        }
        let mut result = Self::new(policy, limits)?;
        // reqwest's rustls from_der stores bytes without parsing; fail closed here.
        rustls::RootCertStore::empty()
            .add(rustls::pki_types::CertificateDer::from(der))
            .map_err(|_| Error::Invalid)?;
        result.root_certificate =
            Some(reqwest::Certificate::from_der(der).map_err(|_| Error::Invalid)?);
        Ok(result)
    }
    /// No redirects or automatic retries. HTTP failures (including 4xx/5xx) are responses.
    /// Cancellation/timeout cannot undo a request which has already reached a server.
    pub async fn send(
        &self,
        request: HttpRequest,
        cancel: CancellationToken,
    ) -> Result<HttpResponse> {
        let response = self.send_raw(request, cancel).await?;
        let headers = response
            .headers
            .into_iter()
            .map(|(name, value)| {
                // Preserve the existing text API's strict HeaderValue semantics.
                // In particular, never replace non-ASCII bytes with lossy text.
                let value = HeaderValue::from_bytes(&value).map_err(|_| Error::Transport)?;
                Ok((
                    name,
                    value.to_str().map_err(|_| Error::Transport)?.to_owned(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(HttpResponse {
            status: response.status,
            headers,
            body: response.body,
        })
    }
    /// Preserve exact response header value bytes, including legal obs-text.
    /// Admission, destination controls, quotas and cancellation match `send`.
    pub async fn send_raw(
        &self,
        request: HttpRequest,
        cancel: CancellationToken,
    ) -> Result<RawHttpResponse> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let target = parse_target(&request.target)?;
        if target.origin() != self.policy.origin.origin()
            || !self.policy.methods.contains(&request.method)
        {
            return Err(Error::Denied);
        }
        if request.body.len() > self.limits.max_request_bytes {
            return Err(Error::Limit);
        }
        let mut headers = HeaderMap::new();
        let mut header_bytes = 0usize;
        for (name, value) in &request.headers {
            header_bytes = header_cost(header_bytes, name.len(), value.len())?;
            if header_bytes > self.limits.max_header_bytes {
                return Err(Error::Limit);
            }
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::Invalid)?;
            if matches!(
                name.as_str(),
                "host"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "keep-alive"
                    | "upgrade"
                    | "proxy-authorization"
                    | "proxy-authenticate"
                    | "proxy-connection"
                    | "te"
                    | "trailer"
                    | "expect"
            ) {
                return Err(Error::Denied);
            }
            let value = HeaderValue::from_str(value).map_err(|_| Error::Invalid)?;
            headers.append(name, value);
        }
        let _permit = self
            .concurrent
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Limit)?;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(Error::Cancelled),
            result = tokio::time::timeout(self.limits.timeout, self.execute(target, request, headers)) => {
                if cancel.is_cancelled() { return Err(Error::Cancelled); }
                result.map_err(|_| Error::Timeout)?
            }
        }
    }
    async fn execute(
        &self,
        target: Url,
        request: HttpRequest,
        headers: HeaderMap,
    ) -> Result<RawHttpResponse> {
        let host = target.host_str().ok_or(Error::Invalid)?;
        let port = target.port_or_known_default().ok_or(Error::Invalid)?;
        let mut addresses = Vec::new();
        if let Some(ip) = literal_ip(&target) {
            addresses.push(SocketAddr::new(ip, port));
        } else {
            for address in tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| Error::Transport)?
            {
                if addresses.len() == MAX_DNS_ADDRESSES {
                    return Err(Error::Denied);
                }
                addresses.push(address);
            }
        }
        if addresses.is_empty()
            || addresses.iter().any(|a| {
                if self.policy.local {
                    !a.ip().is_loopback()
                } else {
                    !public_ip(a.ip())
                }
            })
        {
            return Err(Error::Denied);
        }
        // A fresh connector is pinned to this verified answer set. No proxy, redirect,
        // pooled connection from another request, or transparent retry may resolve elsewhere.
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .resolve_to_addrs(host, &addresses);
        if let Some(root) = &self.root_certificate {
            builder = builder.add_root_certificate(root.clone());
        }
        let client = builder.build().map_err(|_| Error::Transport)?;
        let method =
            reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| Error::Invalid)?;
        let is_head = method == reqwest::Method::HEAD;
        let response = client
            .request(method, target)
            .headers(headers)
            .body(request.body)
            .send()
            .await
            .map_err(|_| Error::Transport)?;
        // Defense in depth only; pre-connect pinning above is the destination control.
        if !response
            .remote_addr()
            .is_some_and(|actual| addresses.contains(&actual))
        {
            return Err(Error::Denied);
        }
        let status = response.status().as_u16();
        if !is_head
            && !matches!(status, 204 | 304)
            && response
                .content_length()
                .is_some_and(|n| n > self.limits.max_response_bytes as u64)
        {
            return Err(Error::Limit);
        }
        let mut output_headers = Vec::new();
        let mut bytes = 0usize;
        for (name, value) in response.headers() {
            bytes = header_cost(bytes, name.as_str().len(), value.as_bytes().len())?;
            if bytes > self.limits.max_header_bytes {
                return Err(Error::Limit);
            }
            output_headers.push((name.as_str().to_owned(), value.as_bytes().to_vec()));
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| Error::Transport)?;
            if body
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > self.limits.max_response_bytes)
            {
                return Err(Error::Limit);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(RawHttpResponse {
            status,
            headers: output_headers,
            body,
        })
    }
}
fn header_cost(current: usize, name: usize, value: usize) -> Result<usize> {
    current
        .checked_add(name)
        .and_then(|n| n.checked_add(value))
        .and_then(|n| n.checked_add(4))
        .ok_or(Error::Limit)
}
fn parse_target(input: &str) -> Result<Url> {
    if input.len() > MAX_TARGET_BYTES {
        return Err(Error::Limit);
    }
    if input.is_empty()
        || input
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return Err(Error::Invalid);
    }
    let value = Url::parse(input).map_err(|_| Error::Invalid)?;
    let authority = input
        .split_once("://")
        .ok_or(Error::Invalid)?
        .1
        .split(['/', '?', '#'])
        .next()
        .ok_or(Error::Invalid)?;
    if !matches!(value.scheme(), "http" | "https")
        || value.host().is_none()
        || authority.contains('@')
        || !value.username().is_empty()
        || value.password().is_some()
        || value.fragment().is_some()
    {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn literal_ip(value: &Url) -> Option<IpAddr> {
    match value.host()? {
        Host::Ipv4(ip) => Some(IpAddr::V4(ip)),
        Host::Ipv6(ip) => Some(IpAddr::V6(ip)),
        Host::Domain(_) => None,
    }
}
fn local_origin(value: &Url) -> bool {
    literal_ip(value).is_some_and(|ip| ip.is_loopback()) || value.host_str() == Some("localhost")
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}
fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    // Conservative special-use exclusions. This does not allow private addresses merely
    // because an explicitly chosen hostname resolves to them.
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && matches!(c, 0 | 2)) || (b == 88 && c == 99)))
        || (a == 198 && ((18..=19).contains(&b) || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}
fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    // Only global-unicast 2000::/3, excluding special-use, documentation and 6to4.
    // Mapped IPv4, ULA, link-local, multicast and translation ranges are not admitted.
    (s[0] & 0xe000) == 0x2000
        && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
        && s[0] != 0x2002
        && !(s[0] == 0x3fff && s[1] < 0x1000)
}
