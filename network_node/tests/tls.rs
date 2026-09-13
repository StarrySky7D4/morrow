//! Actual loopback TLS tests. Trust is explicit; certificate checks are never disabled.
use morrow_network_node::{
    Error, HttpResponse, Limits,
    server::{Handler, Node, Route, TlsIdentity},
};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const TOKEN: &str = "tls-test-local-secret-0123456789abcdef";
struct Identity {
    ca: String,
    chain: String,
    key: String,
}
impl Identity {
    fn new() -> Self {
        let ca_key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec![]).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let ca = params.self_signed(&ca_key).unwrap();
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let leaf = params.signed_by(&key, &ca, &ca_key).unwrap();
        Self {
            ca: ca.pem(),
            chain: format!("{}{}", leaf.pem(), ca.pem()),
            key: key.serialize_pem(),
        }
    }
    fn server(&self) -> TlsIdentity {
        TlsIdentity::from_pem(self.chain.as_bytes(), self.key.as_bytes()).unwrap()
    }
    fn client(&self, address: SocketAddr) -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .add_root_certificate(reqwest::Certificate::from_pem(self.ca.as_bytes()).unwrap())
            .resolve("localhost", address)
            .timeout(Duration::from_secs(4))
            .build()
            .unwrap()
    }
}
fn address() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
fn url(node: &Node) -> String {
    format!("https://localhost:{}/echo", node.local_addr().port())
}
fn route(calls: Arc<AtomicUsize>) -> Route {
    let handler: Handler = Arc::new(move |request, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert!(
                !request
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            );
            Ok(HttpResponse {
                status: 201,
                headers: vec![("x-result".into(), "tls".into())],
                body: request.body,
            })
        })
    });
    Route::new("POST", "/echo", handler).unwrap()
}
async fn start(identity: &Identity, limits: Limits, calls: Arc<AtomicUsize>) -> Node {
    Node::bind_tls(
        address(),
        TOKEN.into(),
        vec![route(calls)],
        limits,
        identity.server(),
    )
    .await
    .unwrap()
}
async fn socket_closed(stream: &mut TcpStream) {
    let mut byte = [0];
    match timeout(Duration::from_secs(4), stream.read(&mut byte)).await {
        Ok(Ok(0)) | Ok(Err(_)) => {}
        other => panic!("TLS half connection did not close: {other:?}"),
    }
}

#[test]
fn pem_input_is_bounded_and_private_key_must_match() {
    let identity = Identity::new();
    assert!(matches!(
        TlsIdentity::from_pem(b"", identity.key.as_bytes()),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        TlsIdentity::from_pem(identity.chain.as_bytes(), b"bad key"),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        TlsIdentity::from_pem(&vec![b'x'; 65_537], identity.key.as_bytes()),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        TlsIdentity::from_pem(identity.chain.as_bytes(), &vec![b'x'; 65_537]),
        Err(Error::Invalid)
    ));
    let other = Identity::new();
    assert!(matches!(
        TlsIdentity::from_pem(identity.chain.as_bytes(), other.key.as_bytes()),
        Err(Error::Invalid)
    ));
    let duplicate = format!("{}{}", identity.key, identity.key);
    assert!(matches!(
        TlsIdentity::from_pem(identity.chain.as_bytes(), duplicate.as_bytes()),
        Err(Error::Invalid)
    ));
    let oversized_chain = identity.chain.repeat(9);
    assert!(matches!(
        TlsIdentity::from_pem(oversized_chain.as_bytes(), identity.key.as_bytes()),
        Err(Error::Invalid)
    ));
}

#[tokio::test]
async fn trusted_https_preserves_authentication_routes_and_exact_response() {
    let identity = Identity::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = start(&identity, Limits::default(), calls.clone()).await;
    let client = identity.client(node.local_addr());
    let target = url(&node);
    for bearer in [None, Some("wrong-bearer")] {
        let mut request = client.post(&target);
        if let Some(bearer) = bearer {
            request = request.bearer_auth(bearer);
        }
        assert_eq!(request.send().await.unwrap().status(), 401);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        client
            .get(&target)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        405
    );
    assert_eq!(
        client
            .post(format!("{target}/missing"))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = client
        .post(&target)
        .bearer_auth(TOKEN)
        .body(vec![0, 255, 3])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(response.headers()["x-result"], "tls");
    assert_eq!(response.bytes().await.unwrap().as_ref(), &[0, 255, 3]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn untrusted_ca_and_wrong_hostname_never_reach_handler() {
    let identity = Identity::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = start(&identity, Limits::default(), calls.clone()).await;
    let untrusted = reqwest::Client::builder()
        .no_proxy()
        .resolve("localhost", node.local_addr())
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap();
    assert!(
        untrusted
            .post(url(&node))
            .bearer_auth(TOKEN)
            .send()
            .await
            .is_err()
    );
    let trusted = identity.client(node.local_addr());
    // The trusted certificate has only the localhost DNS SAN, not the numeric IP SAN.
    assert!(
        trusted
            .post(format!("https://{}/echo", node.local_addr()))
            .bearer_auth(TOKEN)
            .send()
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        trusted
            .post(url(&node))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        201
    );
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn half_handshakes_do_not_block_normal_tls_and_expire_without_shutdown() {
    let identity = Identity::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let limits = Limits {
        max_concurrent: 2,
        timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let node = start(&identity, limits, calls.clone()).await;
    let mut silent = TcpStream::connect(node.local_addr()).await.unwrap();
    let mut partial = TcpStream::connect(node.local_addr()).await.unwrap();
    // Begin a TLS handshake record and deliberately withhold the rest.
    partial.write_all(&[22, 3, 3, 0, 100, 1]).await.unwrap();
    let client = identity.client(node.local_addr());
    let response = timeout(
        Duration::from_millis(800),
        client.post(url(&node)).bearer_auth(TOKEN).send(),
    )
    .await
    .expect("a half handshake serialized listener acceptance")
    .unwrap();
    assert_eq!(response.status(), 201);
    socket_closed(&mut silent).await;
    socket_closed(&mut partial).await;
    // Expiration, not shutdown, returns connection capacity to the listener.
    let fresh = identity.client(node.local_addr());
    assert_eq!(
        fresh
            .post(url(&node))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        201
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_and_drop_close_half_tls_connections_and_release_port() {
    for graceful in [true, false] {
        let identity = Identity::new();
        let node = start(&identity, Limits::default(), Arc::new(AtomicUsize::new(0))).await;
        let bound = node.local_addr();
        let mut partial = TcpStream::connect(bound).await.unwrap();
        partial.write_all(&[22, 3, 3, 0, 100, 1]).await.unwrap();
        // Complete a different handshake to establish that the listener is active.
        let client = identity.client(bound);
        assert_eq!(
            client
                .post(url(&node))
                .bearer_auth(TOKEN)
                .send()
                .await
                .unwrap()
                .status(),
            201
        );
        if graceful {
            node.shutdown().await.unwrap();
        } else {
            drop(node);
        }
        socket_closed(&mut partial).await;
        let rebound = timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(listener) = TcpListener::bind(bound).await {
                    break listener;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        drop(rebound);
    }
}
