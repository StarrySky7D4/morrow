//! Real TLS qualification with synthetic, test-local keys; certificate validation is never disabled.
use morrow_network_node::{
    Error, HttpRequest, HttpResponse, Limits,
    client::{Client, EndpointPolicy},
    server::{Handler, Node, Route, TlsIdentity},
};
use rcgen::{CertifiedKey, generate_simple_self_signed};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
const TOKEN: &str = "synthetic-tls-test-bearer-1234567890";
fn limits() -> Limits {
    Limits {
        timeout: Duration::from_secs(3),
        ..Limits::default()
    }
}
fn policy(origin: &str) -> EndpointPolicy {
    EndpointPolicy::local_https(origin, &["POST"]).unwrap()
}
fn request(origin: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".into(),
        target: format!("{origin}/echo"),
        headers: vec![
            ("Authorization".into(), format!("Bearer {TOKEN}")),
            ("Content-Type".into(), "application/octet-stream".into()),
        ],
        body: b"actual TLS body\0\xff".to_vec(),
    }
}
async fn node(key: &CertifiedKey, calls: Arc<AtomicUsize>) -> Node {
    let handler: Handler = Arc::new(move |request, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(HttpResponse {
                status: 201,
                headers: vec![
                    ("X-TLS-Result".into(), "one".into()),
                    ("X-TLS-Result".into(), "two".into()),
                ],
                body: request.body,
            })
        })
    });
    let identity = TlsIdentity::from_pem(
        key.cert.pem().as_bytes(),
        key.key_pair.serialize_pem().as_bytes(),
    )
    .unwrap();
    Node::bind_tls(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        TOKEN.into(),
        vec![Route::new("POST", "/echo", handler).unwrap()],
        limits(),
        identity,
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn explicit_root_and_matching_hostname_preserve_actual_https_body_and_status() {
    let key = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = node(&key, calls.clone()).await;
    let origin = format!("https://localhost:{}", node.local_addr().port());
    let client =
        Client::with_root_certificate(policy(&origin), limits(), key.cert.der().as_ref()).unwrap();
    let input = request(&origin);
    let expected = input.body.clone();
    let reply = client.send(input, CancellationToken::new()).await.unwrap();
    assert_eq!(reply.status, 201);
    assert_eq!(reply.body, expected);
    assert_eq!(
        reply
            .headers
            .iter()
            .filter(|(name, _)| name == "x-tls-result")
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn untrusted_certificate_and_wrong_root_fail_before_node_handler() {
    let key = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let wrong = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = node(&key, calls.clone()).await;
    let origin = format!("https://localhost:{}", node.local_addr().port());
    let ordinary = Client::new(policy(&origin), limits()).unwrap();
    assert_eq!(
        ordinary
            .send(request(&origin), CancellationToken::new())
            .await
            .err(),
        Some(Error::Transport)
    );
    let wrong_client =
        Client::with_root_certificate(policy(&origin), limits(), wrong.cert.der().as_ref())
            .unwrap();
    assert_eq!(
        wrong_client
            .send(request(&origin), CancellationToken::new())
            .await
            .err(),
        Some(Error::Transport)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let trusted =
        Client::with_root_certificate(policy(&origin), limits(), key.cert.der().as_ref()).unwrap();
    assert_eq!(
        trusted
            .send(request(&origin), CancellationToken::new())
            .await
            .unwrap()
            .status,
        201
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn adding_root_does_not_disable_certificate_hostname_validation() {
    let key = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = node(&key, calls.clone()).await;
    let wrong_host = format!("https://127.0.0.1:{}", node.local_addr().port());
    let client =
        Client::with_root_certificate(policy(&wrong_host), limits(), key.cert.der().as_ref())
            .unwrap();
    assert_eq!(
        client
            .send(request(&wrong_host), CancellationToken::new())
            .await
            .err(),
        Some(Error::Transport)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let right_host = format!("https://localhost:{}", node.local_addr().port());
    let correct =
        Client::with_root_certificate(policy(&right_host), limits(), key.cert.der().as_ref())
            .unwrap();
    assert_eq!(
        correct
            .send(request(&right_host), CancellationToken::new())
            .await
            .unwrap()
            .status,
        201
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}
#[test]
fn local_https_is_explicit_and_single_root_input_is_bounded() {
    for origin in [
        "https://localhost:8443",
        "https://127.0.0.1:8443",
        "https://[::1]:8443",
    ] {
        assert!(EndpointPolicy::local_https(origin, &["GET"]).is_ok());
    }
    for origin in [
        "http://localhost:8443",
        "https://example.com",
        "https://10.0.0.1",
        "https://192.168.0.1",
        "https://[fc00::1]",
    ] {
        assert_eq!(
            EndpointPolicy::local_https(origin, &["GET"]).err(),
            Some(Error::Denied)
        );
    }
    assert_eq!(
        EndpointPolicy::new("https://127.0.0.1:8443", &["GET"], true).err(),
        Some(Error::Denied)
    );
    assert_eq!(
        EndpointPolicy::local_https("https://localhost/a/..", &["GET"]).err(),
        Some(Error::Invalid)
    );
    let key = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = key.cert.der().as_ref();
    assert!(Client::with_root_certificate(policy("https://localhost:8443"), limits(), der).is_ok());
    for bytes in [
        &[][..],
        b"not a certificate".as_slice(),
        &der[..der.len() - 1],
    ] {
        assert_eq!(
            Client::with_root_certificate(policy("https://localhost:8443"), limits(), bytes).err(),
            Some(Error::Invalid)
        );
    }
    let mut joined = der.to_vec();
    joined.extend_from_slice(der);
    assert_eq!(
        Client::with_root_certificate(policy("https://localhost:8443"), limits(), &joined).err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        Client::with_root_certificate(
            policy("https://localhost:8443"),
            limits(),
            &vec![0; 64 * 1024 + 1]
        )
        .err(),
        Some(Error::Limit)
    );
}
