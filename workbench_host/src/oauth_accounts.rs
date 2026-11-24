//! Host-owned Windows OAuth public-client flow. Provider enrollment and card
//! permissions are separate decisions. A new account is started by user review.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use morrow_audit::oauth_credentials::Tokens;
use morrow_network_node::{
    HttpRequest, Limits,
    client::{Client, EndpointPolicy},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, SocketAddrV4},
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use url::Url;
use zeroize::{Zeroize, Zeroizing};

pub mod accounts;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    NotApproved,
    Expired,
    Busy,
    Reauthorize,
    Unknown,
    Limit,
    Storage,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OAuth account: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
const CALLBACK_PATH: &str = "/morrow/oauth/callback";
const MAX_TOKEN: usize = 4096;
const SESSION_LIFETIME: Duration = Duration::from_secs(180);

/// Review both the issuer and the exact endpoints. OpenID discovery, arbitrary
/// browser links and requests from guest code cannot register a service here.
#[derive(Clone)]
pub struct Provider {
    issuer: String,
    authorize: Url,
    token: Url,
    resource: Url,
    client_id: String,
    scopes: BTreeSet<String>,
    fixture: bool,
}
fn canonical_scopes(text: &str) -> Result<BTreeSet<String>> {
    if text.is_empty() || text.len() > 512 {
        return Err(Error::Invalid);
    }
    let mut result = BTreeSet::new();
    for entry in text.split(' ') {
        if entry.is_empty()
            || entry.len() > 128
            || result.len() >= 32
            || !entry
                .bytes()
                .all(|b| b == 0x21 || (0x23..=0x5b).contains(&b) || (0x5d..=0x7e).contains(&b))
            || !result.insert(entry.to_owned())
        {
            return Err(Error::Invalid);
        }
    }
    Ok(result)
}
fn opaque(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TOKEN
        && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
}
fn endpoint(text: &str, local_fixture: bool) -> Result<Url> {
    if text.is_empty()
        || text.len() > 2048
        || text.contains('\\')
        || text.bytes().any(|b| b <= 32 || b == 127)
    {
        return Err(Error::Invalid);
    }
    let url = Url::parse(text).map_err(|_| Error::Invalid)?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || text
            .split_once("://")
            .is_none_or(|(_, suffix)| suffix.split('/').next().unwrap_or("").contains('@'))
    {
        return Err(Error::Invalid);
    }
    if local_fixture && (url.scheme() != "http" || url.host_str() != Some("127.0.0.1")) {
        return Err(Error::NotApproved);
    }
    EndpointPolicy::new(&url.origin().ascii_serialization(), &["GET"], local_fixture)
        .map_err(|_| Error::NotApproved)?;
    Ok(url)
}
impl Provider {
    pub(crate) fn review_fields(&self) -> (&str, &str, &str, String) {
        (
            &self.issuer,
            self.authorize.as_str(),
            self.resource.as_str(),
            self.scopes.iter().cloned().collect::<Vec<_>>().join(" "),
        )
    }
    pub(crate) fn identity(&self, library: &[u8; 32], package: &[u8; 32]) -> Result<[u8; 32]> {
        self.binding(
            library,
            package,
            &Sha256::digest(b"Morrow/OAuth/approved-service/v1").into(),
        )
    }
    pub fn reviewed(
        issuer: &str,
        authorize: &str,
        token: &str,
        resource_origin: &str,
        client_id: &str,
        scopes: &str,
    ) -> Result<Self> {
        Self::build(
            issuer,
            authorize,
            token,
            resource_origin,
            client_id,
            scopes,
            false,
        )
    }
    #[cfg(test)]
    pub(crate) fn fixture(origin: &str) -> Self {
        Self::build(
            origin,
            &format!("{origin}/authorize"),
            &format!("{origin}/token"),
            origin,
            "morrow-windows-test",
            "cards.read",
            true,
        )
        .unwrap()
    }
    fn build(
        issuer: &str,
        authorize: &str,
        token: &str,
        resource: &str,
        client_id: &str,
        scopes: &str,
        fixture: bool,
    ) -> Result<Self> {
        endpoint(issuer, fixture)?;
        let authorize = endpoint(authorize, fixture)?;
        let token = endpoint(token, fixture)?;
        let resource = endpoint(resource, fixture)?;
        if !opaque(client_id)
            || client_id.len() > 256
            || resource.path() != "/"
            || authorize == token
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            issuer: issuer.into(),
            authorize,
            token,
            resource,
            client_id: client_id.into(),
            scopes: canonical_scopes(scopes)?,
            fixture,
        })
    }
    fn binding(
        &self,
        library: &[u8; 32],
        package: &[u8; 32],
        account: &[u8; 32],
    ) -> Result<[u8; 32]> {
        if [library, package, account].iter().any(|id| **id == [0; 32]) {
            return Err(Error::Invalid);
        }
        let mut hash = Sha256::new();
        hash.update(b"Morrow/OAuth/account/v1\0");
        for id in [library, package, account] {
            hash.update(id);
        }
        let scopes = self.scopes.iter().cloned().collect::<Vec<_>>().join(" ");
        for field in [
            &self.issuer,
            self.authorize.as_str(),
            self.token.as_str(),
            self.resource.as_str(),
            &self.client_id,
            &scopes,
        ] {
            hash.update((field.len() as u32).to_be_bytes());
            hash.update(field.as_bytes());
        }
        Ok(hash.finalize().into())
    }
    fn token_client(&self) -> Result<Client> {
        let policy = EndpointPolicy::new(
            &self.token.origin().ascii_serialization(),
            &["POST"],
            self.fixture,
        )
        .map_err(|_| Error::NotApproved)?;
        Client::new(
            policy,
            Limits {
                max_request_bytes: 32768,
                max_response_bytes: 32768,
                max_header_bytes: 8192,
                max_concurrent: 1,
                timeout: Duration::from_secs(12),
            },
        )
        .map_err(|_| Error::Invalid)
    }
    async fn exchange(
        &self,
        fields: &[(&str, &str)],
        previous_refresh: Option<&str>,
        now_ms: u64,
        account_expires_ms: u64,
        cancel: CancellationToken,
    ) -> Result<Tokens> {
        if now_ms == 0 || now_ms >= account_expires_ms {
            return Err(Error::Expired);
        }
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("client_id", &self.client_id)
            .extend_pairs(fields.iter().copied())
            .finish()
            .into_bytes();
        let result = self
            .token_client()?
            .send(
                HttpRequest {
                    method: "POST".into(),
                    target: self.token.as_str().into(),
                    headers: vec![
                        (
                            "content-type".into(),
                            "application/x-www-form-urlencoded".into(),
                        ),
                        ("accept".into(), "application/json".into()),
                    ],
                    body,
                },
                cancel,
            )
            .await
            .map_err(|_| Error::Unknown)?;
        let bytes = Zeroizing::new(result.body);
        if result.status != 200 {
            #[derive(Deserialize)]
            struct Failure {
                error: String,
            }
            if result.status == 400
                && serde_json::from_slice::<Failure>(&bytes)
                    .is_ok_and(|v| v.error == "invalid_grant")
            {
                return Err(Error::Reauthorize);
            }
            return Err(Error::Unknown);
        }
        let mut content_types = result
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"));
        if !content_types.next().is_some_and(|(_, v)| {
            v.split(';')
                .next()
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
        }) || content_types.next().is_some()
        {
            return Err(Error::Unknown);
        }
        #[derive(Deserialize)]
        struct Response {
            token_type: String,
            access_token: String,
            expires_in: u64,
            refresh_token: Option<String>,
            scope: Option<String>,
        }
        impl Drop for Response {
            fn drop(&mut self) {
                self.access_token.zeroize();
                self.refresh_token.zeroize();
            }
        }
        let mut v: Response = serde_json::from_slice(&bytes).map_err(|_| Error::Unknown)?;
        let refresh = v
            .refresh_token
            .as_deref()
            .or(previous_refresh)
            .ok_or(Error::Reauthorize)?;
        if !v.token_type.eq_ignore_ascii_case("bearer")
            || !opaque(&v.access_token)
            || !opaque(refresh)
            || v.expires_in == 0
            || v.scope
                .as_deref()
                .map(canonical_scopes)
                .transpose()?
                .is_some_and(|s| s != self.scopes)
        {
            return Err(Error::Reauthorize);
        }
        let access_expires_ms = now_ms
            .checked_add(v.expires_in.checked_mul(1000).ok_or(Error::Invalid)?)
            .ok_or(Error::Invalid)?
            .min(account_expires_ms);
        let refresh = Zeroizing::new(refresh.to_owned());
        Ok(Tokens {
            access: Zeroizing::new(std::mem::take(&mut v.access_token)),
            refresh,
            access_expires_ms,
        })
    }
}
fn random_secret() -> Result<Zeroizing<String>> {
    let mut seed = Zeroizing::new([0; 32]);
    getrandom::fill(&mut *seed).map_err(|_| Error::Unknown)?;
    Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(seed.as_slice())))
}

/// The socket stays bound until this login completes or is dropped. The host
/// opens browser_url only after the user reviews the package and service.
pub struct Login {
    provider: Provider,
    listener: TcpListener,
    redirect: Url,
    verifier: Zeroizing<String>,
    state: Zeroizing<String>,
    deadline: Instant,
}
impl Login {
    pub async fn new(provider: Provider) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| Error::Unknown)?;
        let port = listener.local_addr().map_err(|_| Error::Unknown)?.port();
        let redirect = Url::parse(&format!("http://127.0.0.1:{port}{CALLBACK_PATH}"))
            .map_err(|_| Error::Invalid)?;
        Ok(Self {
            provider,
            listener,
            redirect,
            verifier: random_secret()?,
            state: random_secret()?,
            deadline: Instant::now() + SESSION_LIFETIME,
        })
    }
    pub fn browser_url(&self) -> Url {
        let mut url = self.provider.authorize.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.provider.client_id)
            .append_pair("redirect_uri", self.redirect.as_str())
            .append_pair(
                "scope",
                &self
                    .provider
                    .scopes
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .append_pair("state", &self.state)
            .append_pair("code_challenge_method", "S256")
            .append_pair(
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes())),
            );
        url
    }
    fn code(&self, target: &Url) -> Result<Zeroizing<String>> {
        if Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        let mut base = target.clone();
        base.set_query(None);
        if base != self.redirect || target.as_str().len() > 8192 {
            return Err(Error::NotApproved);
        }
        let mut code = None;
        let mut state = None;
        let mut issuer = None;
        let mut failure = None;
        for (key, value) in target.query_pairs() {
            let slot = match key.as_ref() {
                "code" => &mut code,
                "state" => &mut state,
                "iss" => &mut issuer,
                "error" => &mut failure,
                _ => continue,
            };
            if slot.replace(Zeroizing::new(value.into_owned())).is_some() {
                return Err(Error::Invalid);
            }
        }
        if state.as_deref() != Some(&*self.state)
            || issuer.as_deref() != Some(&self.provider.issuer)
        {
            return Err(Error::NotApproved);
        }
        if failure.is_some() {
            return if code.is_none() {
                Err(Error::Reauthorize)
            } else {
                Err(Error::Invalid)
            };
        }
        code.filter(|s| opaque(s)).ok_or(Error::Invalid)
    }
    pub async fn complete(
        self,
        now_ms: u64,
        account_expires_ms: u64,
        cancel: CancellationToken,
    ) -> Result<Tokens> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let until = tokio::time::Instant::from_std(self.deadline);
        let receive = async {
            loop {
                let (mut stream, peer) =
                    self.listener.accept().await.map_err(|_| Error::Unknown)?;
                if !peer.ip().is_loopback() {
                    continue;
                }
                let read = async {
                    let mut data = Zeroizing::new(Vec::new());
                    let mut chunk = Zeroizing::new([0u8; 1024]);
                    while data.len() < 8192 && !data.windows(4).any(|v| v == b"\r\n\r\n") {
                        let n = stream.read(&mut *chunk).await.map_err(|_| Error::Invalid)?;
                        if n == 0 {
                            return Err(Error::Invalid);
                        }
                        data.extend_from_slice(&chunk[..n]);
                    }
                    if data.len() > 8192 || !data.ends_with(b"\r\n\r\n") {
                        return Err(Error::Invalid);
                    }
                    let text = std::str::from_utf8(&data).map_err(|_| Error::Invalid)?;
                    let mut lines = text.split("\r\n");
                    let request = lines.next().ok_or(Error::Invalid)?;
                    let target = request
                        .strip_prefix("GET ")
                        .and_then(|v| v.strip_suffix(" HTTP/1.1"))
                        .ok_or(Error::Invalid)?;
                    let (path, query) = target.split_once('?').ok_or(Error::Invalid)?;
                    if path != CALLBACK_PATH || query.contains('#') || target.contains(' ') {
                        return Err(Error::Invalid);
                    }
                    let mut hosts = lines
                        .filter_map(|line| line.split_once(':'))
                        .filter(|(k, _)| k.eq_ignore_ascii_case("host"));
                    let required =
                        format!("127.0.0.1:{}", self.redirect.port().ok_or(Error::Invalid)?);
                    if !hosts.next().is_some_and(|(_, v)| v.trim() == required)
                        || hosts.next().is_some()
                    {
                        return Err(Error::Invalid);
                    }
                    let mut target = self.redirect.clone();
                    target.set_query(Some(query));
                    self.code(&target)
                };
                let code = tokio::time::timeout(Duration::from_secs(2), read).await;
                let accepted = matches!(&code, Ok(Ok(_)));
                let reply: &[u8] = if accepted {
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nReturn to Morrow to finish signing in."
                } else {
                    b"HTTP/1.1 400 Bad Request\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                };
                let _ = tokio::time::timeout(Duration::from_secs(1), stream.write_all(reply)).await;
                if let Ok(Ok(code)) = code {
                    return Ok(code);
                }
                if matches!(code, Ok(Err(Error::Reauthorize))) {
                    return Err(Error::Reauthorize);
                }
            }
        };
        let code = tokio::select! { _ = cancel.cancelled()=>return Err(Error::Expired), result = tokio::time::timeout_at(until, receive)=>result.map_err(|_|Error::Expired)?? };
        self.provider
            .exchange(
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", self.redirect.as_str()),
                    ("code_verifier", &self.verifier),
                ],
                None,
                now_ms,
                account_expires_ms,
                cancel,
            )
            .await
    }
}
