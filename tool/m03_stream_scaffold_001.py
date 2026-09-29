"""Create only the new transport source directory and provenance; never edit its origin."""
import hashlib,json,pathlib
ROOT=pathlib.Path(__file__).resolve().parents[1]
DST=ROOT/'network_node_stream_001'
assert not DST.exists()
DST.mkdir();(DST/'src').mkdir();(DST/'tests').mkdir();(DST/'provenance').mkdir()
origins=['network_node/src/client.rs','network_node/src/lib.rs','network_node/Cargo.toml','network_node/Cargo.lock','network_node/tests/client.rs']
pins={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in origins}
(DST/'provenance/origin.json').write_text(json.dumps({'origins':pins,'scope':'versioned outbound-only extraction; no server or managed authority copied'},indent=2)+'\n')
for p in origins:
    (DST/'provenance'/p.replace('/','__')).write_bytes((ROOT/p).read_bytes())
lib=(ROOT/'network_node/src/lib.rs').read_text()
a=lib.index('pub mod client;');b=lib.index('// Deliberately no Debug:')
lib=lib[:a]+'pub mod client;\npub mod stream;\n\n'+lib[b:]
lib=lib.replace('Experimental trusted native HTTP client and API node transport.','Versioned experimental trusted outbound HTTP stream transport.')
lib+='''
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
        Self { method: r.method, target: r.target,
            headers: r.headers.into_iter().map(|(n,v)| (n,v.into_bytes())).collect(), body: r.body }
    }
}
'''
(DST/'src/lib.rs').write_text(lib)
(DST/'tests/client.rs').write_text((ROOT/'network_node/tests/client.rs').read_text().replace('morrow_network_node','morrow_network_node_stream'))
source=(ROOT/'network_node/src/client.rs').read_text()
source=source.replace('HttpResponse, Limits, RawHttpResponse, Result','HttpResponse, Limits, RawHttpRequest, RawHttpResponse, Result')
source=source.replace('use futures_util::StreamExt;','use crate::stream::{SendContext, StreamLease, ResponseHead};')
source=source.replace('pub struct EndpointPolicy {','#[derive(Clone)]\npub struct EndpointPolicy {')
source=source.replace('pub struct Client {','#[derive(Clone)]\npub struct Client {')
start=source.index('        if cancel.is_cancelled() {',source.index('    pub async fn send_raw('))
end=source.index('        let target = parse_target',start)
source=source[:start]+'''        let context = SendContext::trusted(tokio::time::Instant::now() + self.limits.timeout, cancel);
        self.collect(request.into(), context).await
    }
    /// Trusted transport only: the caller must perform its durable dispatch claim first.
    /// The parent deadline is absolute and is never renewed at head/body boundaries.
    pub async fn send_stream(&self, request: RawHttpRequest, mut context: SendContext) -> Result<StreamLease> {
        context.cap_deadline(tokio::time::Instant::now() + self.limits.timeout);
        context.check()?;
        let (target, headers) = self.validate_request(&request)?;
        let permit = self.concurrent.clone().try_acquire_owned().map_err(|_| Error::Limit)?;
        crate::stream::start(self.clone(), target, request, headers, context, permit).await
    }
    /// Collection uses exactly the streaming validation/connector/body path.
    pub async fn collect(&self, request: RawHttpRequest, context: SendContext) -> Result<RawHttpResponse> {
        let mut lease = self.send_stream(request, context).await?;
        let mut body = Vec::new();
        loop {
            match lease.next_chunk().await {
                Ok(Some(chunk)) => body.extend_from_slice(&chunk),
                Ok(None) => break,
                Err(error) => { let _ = lease.cancel_and_wait().await; return Err(error); }
            }
        }
        let head = lease.head().clone();
        lease.finish().await.outcome?;
        Ok(RawHttpResponse { status: head.status, headers: head.headers, body })
    }
    fn validate_request(&self, request: &RawHttpRequest) -> Result<(Url, HeaderMap)> {
'''+source[end:]
source=source.replace('HeaderValue::from_str(value)', 'HeaderValue::from_bytes(value)')
start=source.index('        let _permit = self');end=source.index('    async fn execute(',start)
source=source[:start]+'''        Ok((target, headers))
    }
    pub(crate) fn response_limit(&self) -> usize { self.limits.max_response_bytes }
'''+source[end:]
source=source.replace('    async fn execute(', '    pub(crate) async fn execute_head(')
source=source.replace('        request: HttpRequest,\n        headers: HeaderMap,\n    ) -> Result<RawHttpResponse>', '        request: RawHttpRequest,\n        headers: HeaderMap,\n        context: &SendContext,\n    ) -> Result<(ResponseHead, reqwest::Response)>')
source=source.replace('        let host = target.host_str()', '        context.check()?;\n        let host = target.host_str()')
source=source.replace('        // A fresh connector', '        context.check()?;\n        // A fresh connector')
source=source.replace('        let response = client', '        context.check()?;\n        let response = client')
source=source.replace('        // Defense in depth', '        context.check()?;\n        // Defense in depth')
start=source.index('        let mut stream = response.bytes_stream();');end=source.index('\n    }\n}\nfn header_cost',start)
source=source[:start]+'''        let head = ResponseHead { status, headers: output_headers,
            remote_addr: response.remote_addr().ok_or(Error::Denied)? };
        Ok((head, response))'''+source[end:]
(DST/'src/client.rs').write_text(source)
(DST/'Cargo.toml').write_text('''[workspace]
resolver = "2"

[package]
name = "morrow-network-node-stream"
version = "0.1.0"
edition = "2024"
publish = false
license = "AGPL-3.0-only"

[dependencies]
reqwest = { version = "=0.12.28", default-features = false, features = ["rustls-tls", "stream"] }
tokio = { version = "=1.53.1", features = ["full"] }
tokio-util = { version = "=0.7.19", features = ["rt"] }
url = "=2.5.8"
futures-util = "=0.3.33"
bytes = "=1.12.1"
rustls = { version = "=0.23.43", default-features = false, features = ["ring", "std", "tls12"] }

[dev-dependencies]
''')
