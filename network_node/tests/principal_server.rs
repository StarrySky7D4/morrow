//! Real loopback authorization with synthetic identities and test-local TLS keys.
//! Principal scopes authorize the named route; request headers never name the caller.
use morrow_network_node::{
    Error, Limits, RawHttpResponse,
    server::{AuthorizedHandler, AuthorizedRequest, AuthorizedRoute, Node, Principal, TlsIdentity},
};
use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    time::timeout,
};
use tokio_util::sync::CancellationToken;
const ALICE: &str = "synthetic-principal-alice-token-123456789";
const BOB: &str = "synthetic-principal-bob-token-12345678901";
const WAIT: Duration = Duration::from_secs(4);
fn address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}
fn limits() -> Limits {
    Limits {
        timeout: Duration::from_secs(2),
        ..Limits::default()
    }
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(WAIT)
        .build()
        .unwrap()
}
fn url(node: &Node, path: &str) -> String {
    format!("http://{}{path}", node.local_addr())
}
fn principal(id: &str, token: &str, scopes: &[&str]) -> Principal {
    Principal::new(id, token, scopes, Duration::from_secs(60)).unwrap()
}
fn handler<F, T>(callback: F) -> AuthorizedHandler
where
    F: Fn(AuthorizedRequest, CancellationToken) -> T + Send + Sync + 'static,
    T: Future<Output = morrow_network_node::Result<RawHttpResponse>> + Send + 'static,
{
    Arc::new(move |request, cancel| Box::pin(callback(request, cancel)))
}
fn ok(body: impl Into<Vec<u8>>) -> RawHttpResponse {
    RawHttpResponse {
        status: 200,
        headers: vec![],
        body: body.into(),
    }
}
fn route(service: &str, path: &str, callback: AuthorizedHandler) -> AuthorizedRoute {
    AuthorizedRoute::new(service, "POST", path, callback).unwrap()
}
fn counted(calls: Arc<AtomicUsize>) -> AuthorizedHandler {
    handler(move |request, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        let (request, principal) = request.into_parts();
        async move {
            assert!(
                !request
                    .headers
                    .iter()
                    .any(|(name, _)| name == "authorization")
            );
            let mut body = principal.id().as_bytes().to_vec();
            body.push(b':');
            body.extend_from_slice(&request.body);
            Ok(ok(body))
        }
    })
}
#[test]
fn principal_inputs_are_bounded_and_clones_share_revocation() {
    for id in ["", "bad identity", "bad/name", "\u{4e2d}"] {
        assert!(Principal::new(id, ALICE, &["service.a"], Duration::from_secs(1)).is_err());
    }
    assert!(
        Principal::new(
            &"a".repeat(129),
            ALICE,
            &["service.a"],
            Duration::from_secs(1)
        )
        .is_err()
    );
    for scopes in [
        vec![],
        vec!["service.a", "service.a"],
        vec!["bad/scope"],
        vec!["service.a"; 65],
    ] {
        assert!(Principal::new("alice", ALICE, &scopes, Duration::from_secs(1)).is_err());
    }
    for ttl in [Duration::ZERO, Duration::from_secs(24 * 60 * 60 + 1)] {
        assert!(Principal::new("alice", ALICE, &["service.a"], ttl).is_err());
    }
    assert!(Principal::new("alice", "short", &["service.a"], Duration::from_secs(1)).is_err());
    let original = principal("alice", ALICE, &["service.a"]);
    let clone = original.clone();
    assert_eq!(clone.id(), "alice");
    assert!(clone.allows("service.a"));
    assert!(!clone.allows("service.b"));
    original.revoke();
    assert_eq!(clone.check("service.a"), Err(Error::Denied));
}
#[tokio::test]
async fn authenticated_id_and_disjoint_scopes_cannot_be_overridden_by_headers() {
    let calls = Arc::new(AtomicUsize::new(0));
    let callback = counted(calls.clone());
    let node = Node::bind_authorized(
        address(),
        vec![
            principal("alice", ALICE, &["service.a"]),
            principal("bob", BOB, &["service.b"]),
        ],
        vec![
            route("service.a", "/a", callback.clone()),
            route("service.b", "/b", callback),
        ],
        limits(),
    )
    .await
    .unwrap();
    let c = client();
    for (token, path, expected) in [(ALICE, "/a", "alice:payload"), (BOB, "/b", "bob:payload")] {
        let response = c
            .post(url(&node, path))
            .bearer_auth(token)
            .header("x-principal-id", "forged-admin")
            .body("payload")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.text().await.unwrap(), expected);
    }
    for (token, path) in [(ALICE, "/b"), (BOB, "/a")] {
        let response = c
            .post(url(&node, path))
            .bearer_auth(token)
            .header("x-principal-id", "bob")
            .header("x-service", "service.b")
            .body("service.b")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn duplicate_or_missing_authorization_never_reaches_handler() {
    let calls = Arc::new(AtomicUsize::new(0));
    let node = Node::bind_authorized(
        address(),
        vec![principal("alice", ALICE, &["service.a"])],
        vec![route("service.a", "/", counted(calls.clone()))],
        limits(),
    )
    .await
    .unwrap();
    let c = client();
    let response = c
        .post(url(&node, "/"))
        .header("x-principal-id", "alice")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let response = c
        .post(url(&node, "/"))
        .header("authorization", format!("Bearer {ALICE}"))
        .header("authorization", format!("Bearer {ALICE}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let response = c
        .post(url(&node, "/"))
        .bearer_auth(BOB)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn forbidden_scope_is_rejected_before_waiting_for_declared_body() {
    let calls = Arc::new(AtomicUsize::new(0));
    let node = Node::bind_authorized(
        address(),
        vec![principal("alice", ALICE, &["service.a"])],
        vec![route("service.b", "/", counted(calls.clone()))],
        limits(),
    )
    .await
    .unwrap();
    let mut socket = TcpStream::connect(node.local_addr()).await.unwrap();
    socket.write_all(format!("POST / HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {ALICE}\r\nContent-Length: 10000000\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut received = [0; 4096];
    let n = timeout(Duration::from_millis(750), socket.read(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert!(received[..n].starts_with(b"HTTP/1.1 403"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(socket);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn revocation_while_body_is_incomplete_prevents_handler_entry() {
    let caller = principal("alice", ALICE, &["service.a"]);
    let calls = Arc::new(AtomicUsize::new(0));
    let node = Node::bind_authorized(
        address(),
        vec![caller.clone()],
        vec![route("service.a", "/", counted(calls.clone()))],
        limits(),
    )
    .await
    .unwrap();
    let mut socket = TcpStream::connect(node.local_addr()).await.unwrap();
    socket.write_all(format!("POST / HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {ALICE}\r\nContent-Length: 4\r\nConnection: close\r\n\r\na").as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    caller.revoke();
    socket.write_all(b"bcd").await.unwrap();
    let mut received = vec![];
    timeout(WAIT, socket.read_to_end(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert!(received.starts_with(b"HTTP/1.1 403") || received.starts_with(b"HTTP/1.1 401"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn expired_and_revoked_principals_cannot_authenticate() {
    for expire in [false, true] {
        let caller =
            Principal::new("alice", ALICE, &["service.a"], Duration::from_millis(150)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let node = Node::bind_authorized(
            address(),
            vec![caller.clone()],
            vec![route("service.a", "/", counted(calls.clone()))],
            limits(),
        )
        .await
        .unwrap();
        if expire {
            tokio::time::sleep(Duration::from_millis(180)).await;
        } else {
            caller.revoke();
        }
        let response = client()
            .post(url(&node, "/"))
            .bearer_auth(ALICE)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        node.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn late_handler_result_is_suppressed_after_revocation_or_expiry() {
    for expire in [false, true] {
        let caller =
            Principal::new("alice", ALICE, &["service.a"], Duration::from_secs(1)).unwrap();
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let seen = entered.clone();
        let gate = release.clone();
        let callback = handler(move |request, _| {
            let seen = seen.clone();
            let gate = gate.clone();
            async move {
                let (_, identity) = request.into_parts();
                assert_eq!(identity.id(), "alice");
                seen.notify_one();
                gate.notified().await;
                Ok(ok(b"private-late-result"))
            }
        });
        let node = Node::bind_authorized(
            address(),
            vec![caller.clone()],
            vec![route("service.a", "/", callback)],
            limits(),
        )
        .await
        .unwrap();
        let target = url(&node, "/");
        let call = tokio::spawn(async move {
            client()
                .post(target)
                .bearer_auth(ALICE)
                .send()
                .await
                .unwrap()
        });
        timeout(WAIT, entered.notified()).await.unwrap();
        if expire {
            tokio::time::sleep(Duration::from_millis(1100)).await;
        } else {
            caller.revoke();
        }
        release.notify_one();
        let response = timeout(WAIT, call).await.unwrap().unwrap();
        assert_eq!(response.status(), 403);
        assert!(
            !response
                .text()
                .await
                .unwrap()
                .contains("private-late-result")
        );
        node.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn duplicate_identity_token_and_route_configs_fail_before_socket_binding() {
    for bad in 0..3 {
        let sentinel = TcpListener::bind(address()).await.unwrap();
        let occupied = sentinel.local_addr().unwrap();
        let callback = counted(Arc::new(AtomicUsize::new(0)));
        let principals = match bad {
            0 => vec![
                principal("same", ALICE, &["service.a"]),
                principal("same", BOB, &["service.a"]),
            ],
            1 => vec![
                principal("alice", ALICE, &["service.a"]),
                principal("bob", ALICE, &["service.a"]),
            ],
            _ => vec![principal("alice", ALICE, &["service.a"])],
        };
        let mut routes = vec![route("service.a", "/", callback.clone())];
        if bad == 2 {
            routes.push(route("service.b", "/", callback));
        }
        // An occupied port would produce Transport if the implementation bound
        // before validating. The exact Invalid proves configuration wins first.
        assert!(matches!(
            Node::bind_authorized(occupied, principals, routes, limits()).await,
            Err(Error::Invalid)
        ));
        drop(sentinel);
        let recovered = TcpListener::bind(occupied).await.unwrap();
        drop(recovered);
    }
    assert!(matches!(
        Node::bind_authorized(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            vec![principal("alice", ALICE, &["service.a"])],
            vec![route(
                "service.a",
                "/",
                counted(Arc::new(AtomicUsize::new(0)))
            )],
            limits()
        )
        .await,
        Err(Error::Denied)
    ));
}
#[tokio::test]
async fn authorized_raw_header_values_keep_obs_text_without_lossy_conversion() {
    let callback = handler(|_, _| async {
        Ok(RawHttpResponse {
            status: 201,
            headers: vec![
                ("x-label".into(), vec![0xe9]),
                ("x-repeat".into(), b"one".to_vec()),
                ("x-repeat".into(), b"two".to_vec()),
            ],
            body: b"\0\xffbody".to_vec(),
        })
    });
    let node = Node::bind_authorized(
        address(),
        vec![principal("alice", ALICE, &["service.a"])],
        vec![route("service.a", "/", callback)],
        limits(),
    )
    .await
    .unwrap();
    let response = client()
        .post(url(&node, "/"))
        .bearer_auth(ALICE)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(
        response.headers().get("x-label").unwrap().as_bytes(),
        &[0xe9]
    );
    assert_eq!(
        response
            .headers()
            .get_all("x-repeat")
            .iter()
            .map(|v| v.as_bytes())
            .collect::<Vec<_>>(),
        vec![b"one".as_slice(), b"two".as_slice()]
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"\0\xffbody");
    node.shutdown().await.unwrap();
}
#[tokio::test]
async fn raw_response_still_rejects_framing_and_control_byte_headers() {
    for (name, value) in [
        ("content-length", b"999".to_vec()),
        ("x-bad", b"value\r\ninjected: true".to_vec()),
    ] {
        let callback = handler(move |_, _| {
            let value = value.clone();
            async move {
                Ok(RawHttpResponse {
                    status: 200,
                    headers: vec![(name.into(), value)],
                    body: b"private".to_vec(),
                })
            }
        });
        let node = Node::bind_authorized(
            address(),
            vec![principal("alice", ALICE, &["service.a"])],
            vec![route("service.a", "/", callback)],
            limits(),
        )
        .await
        .unwrap();
        let response = client()
            .post(url(&node, "/"))
            .bearer_auth(ALICE)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        assert!(!response.text().await.unwrap().contains("private"));
        node.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn authorized_tls_uses_the_same_authenticated_principal_dispatch() {
    let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let identity = TlsIdentity::from_pem(
        key.cert.pem().as_bytes(),
        key.key_pair.serialize_pem().as_bytes(),
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let node = Node::bind_authorized_tls(
        address(),
        vec![principal("alice", ALICE, &["service.a"])],
        vec![route("service.a", "/", counted(calls.clone()))],
        limits(),
        identity,
    )
    .await
    .unwrap();
    let c = reqwest::Client::builder()
        .no_proxy()
        .timeout(WAIT)
        .add_root_certificate(reqwest::Certificate::from_der(key.cert.der().as_ref()).unwrap())
        .build()
        .unwrap();
    let target = format!("https://localhost:{}/", node.local_addr().port());
    let denied = c.post(&target).bearer_auth(BOB).send().await.unwrap();
    assert_eq!(denied.status(), 401);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = c
        .post(&target)
        .bearer_auth(ALICE)
        .body("tls")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "alice:tls");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}
