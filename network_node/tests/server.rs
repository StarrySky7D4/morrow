//! Actual loopback HTTP qualification; no mocked HTTP transport.
use morrow_network_node::{
    Error, HttpRequest, HttpResponse, Limits,
    server::{Handler, Node, Route},
};
use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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

const TOKEN: &str = "local-qualification-secret-123456789";
fn handler<F, T>(f: F) -> Handler
where
    F: Fn(HttpRequest, CancellationToken) -> T + Send + Sync + 'static,
    T: Future<Output = morrow_network_node::Result<HttpResponse>> + Send + 'static,
{
    Arc::new(move |request, token| Box::pin(f(request, token)))
}
fn ok(body: impl Into<Vec<u8>>) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![],
        body: body.into(),
    }
}
fn limits() -> Limits {
    Limits {
        timeout: Duration::from_secs(2),
        ..Default::default()
    }
}
fn address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap()
}
fn url(node: &Node, path: &str) -> String {
    format!("http://{}{path}", node.local_addr())
}
async fn start(routes: Vec<Route>, limits: Limits) -> Node {
    Node::bind(address(), TOKEN.into(), routes, limits)
        .await
        .unwrap()
}
fn route(method: &str, path: &str, callback: Handler) -> Route {
    Route::new(method, path, callback).unwrap()
}

#[tokio::test]
async fn validation_denies_public_listening_and_ambiguous_routes() {
    let callback = handler(|_, _| async { Ok(ok(b"ok")) });
    for (method, path) in [
        ("", "/"),
        ("bad method", "/"),
        ("GET", "/*"),
        ("GET", "/{id}"),
        ("GET", "http://host/"),
        ("GET", "/x?q=1"),
    ] {
        assert!(matches!(
            Route::new(method, path, callback.clone()),
            Err(Error::Invalid)
        ));
    }
    for ip in [
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        "192.0.2.1".parse().unwrap(),
    ] {
        assert!(matches!(
            Node::bind(
                SocketAddr::new(ip, 0),
                TOKEN.into(),
                vec![route("GET", "/", callback.clone())],
                limits()
            )
            .await,
            Err(Error::Denied)
        ));
    }
    assert!(matches!(
        Node::bind(
            address(),
            TOKEN.into(),
            vec![
                route("GET", "/", callback.clone()),
                route("GET", "/", callback.clone())
            ],
            limits()
        )
        .await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        Node::bind(
            address(),
            "".into(),
            vec![route("GET", "/", callback.clone())],
            limits()
        )
        .await,
        Err(Error::Invalid)
    ));
    assert!(matches!(
        Node::bind(
            address(),
            TOKEN.into(),
            vec![route("GET", "/", callback)],
            Limits {
                max_concurrent: 0,
                ..limits()
            }
        )
        .await,
        Err(Error::Limit)
    ));
}

#[tokio::test]
async fn bearer_precedes_handler_and_exact_routes_preserve_custom_response() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let node = start(
        vec![route(
            "POST",
            "/echo",
            handler(move |request, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                async move {
                    assert_eq!(request.method, "POST");
                    assert_eq!(request.target, "/echo?kind=binary");
                    assert!(
                        !request
                            .headers
                            .iter()
                            .any(|(name, _)| name.eq_ignore_ascii_case("authorization")
                                || name.eq_ignore_ascii_case("proxy-authorization"))
                    );
                    assert!(
                        request
                            .headers
                            .iter()
                            .any(|(name, value)| name == "x-input" && value == "test")
                    );
                    Ok(HttpResponse {
                        status: 201,
                        headers: vec![
                            ("x-result".into(), "yes".into()),
                            ("content-type".into(), "application/octet-stream".into()),
                        ],
                        body: request.body,
                    })
                }
            }),
        )],
        limits(),
    )
    .await;
    let client = client();
    for token in [None, Some("wrong"), Some("Basic abc")] {
        let mut request = client.post(url(&node, "/echo?kind=binary"));
        if let Some(token) = token {
            request = request.header("authorization", token);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 401);
        assert!(!response.text().await.unwrap().contains(TOKEN));
    }
    let duplicated = client
        .post(url(&node, "/echo"))
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicated.status(), 401);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let missing = client
        .get(url(&node, "/echo/more"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    let method = client
        .get(url(&node, "/echo"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(method.status(), 405);
    assert_eq!(method.headers()["allow"], "POST");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let bytes = vec![0, 255, 42];
    let response = client
        .post(url(&node, "/echo?kind=binary"))
        .bearer_auth(TOKEN)
        .header("proxy-authorization", "must-not-reach-handler")
        .header("x-input", "test")
        .body(bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(response.headers()["x-result"], "yes");
    assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn request_stream_and_header_limits_prevent_handler_execution() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let node = start(
        vec![route(
            "POST",
            "/",
            handler(move |_, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                async { Ok(ok(b"ok")) }
            }),
        )],
        Limits {
            max_request_bytes: 3,
            max_header_bytes: 256,
            ..limits()
        },
    )
    .await;
    let client = client();
    let response = client
        .post(url(&node, "/"))
        .bearer_auth(TOKEN)
        .body(vec![0; 4])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    let chunks = futures_util::stream::iter([Ok::<_, std::io::Error>(vec![1, 2]), Ok(vec![3, 4])]);
    let response = client
        .post(url(&node, "/"))
        .bearer_auth(TOKEN)
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 413);
    let response = client
        .post(url(&node, "/"))
        .bearer_auth(TOKEN)
        .header("x-large", "x".repeat(300))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 431);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        client
            .post(url(&node, "/"))
            .bearer_auth(TOKEN)
            .body(vec![0; 3])
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn bad_handler_headers_bodies_and_statuses_never_reach_the_client() {
    let variants = [
        HttpResponse {
            status: 200,
            headers: vec![],
            body: vec![9; 5],
        },
        HttpResponse {
            status: 200,
            headers: vec![("content-length".into(), "99".into())],
            body: vec![],
        },
        HttpResponse {
            status: 200,
            headers: vec![("connection".into(), "keep-alive".into())],
            body: vec![],
        },
        HttpResponse {
            status: 200,
            headers: vec![("x-test".into(), "value\r\ninjected: true".into())],
            body: vec![],
        },
        HttpResponse {
            status: 204,
            headers: vec![],
            body: vec![9],
        },
        HttpResponse {
            status: 101,
            headers: vec![],
            body: vec![],
        },
        HttpResponse {
            status: 200,
            headers: vec![("x-large".into(), "x".repeat(300))],
            body: vec![],
        },
    ];
    let routes = variants
        .into_iter()
        .enumerate()
        .map(|(index, response)| {
            route(
                "GET",
                &format!("/{index}"),
                handler(move |_, _| {
                    let response = response.clone();
                    async { Ok(response) }
                }),
            )
        })
        .collect();
    let node = start(
        routes,
        Limits {
            max_response_bytes: 4,
            max_header_bytes: 256,
            ..limits()
        },
    )
    .await;
    let client = client();
    for index in 0..7 {
        let response = client
            .get(url(&node, &format!("/{index}")))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        assert!(response.headers().get("x-test").is_none());
        assert_eq!(response.text().await.unwrap(), "invalid handler response");
    }
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_handler_limit_rejects_without_queuing_then_recovers() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let (entry, gate, observed) = (entered.clone(), release.clone(), calls.clone());
    let node = start(
        vec![route(
            "GET",
            "/",
            handler(move |_, _| {
                let first = observed.fetch_add(1, Ordering::SeqCst) == 0;
                let (entry, gate) = (entry.clone(), gate.clone());
                async move {
                    if first {
                        entry.notify_one();
                        gate.notified().await;
                    }
                    Ok(ok(b"ok"))
                }
            }),
        )],
        Limits {
            max_concurrent: 1,
            ..limits()
        },
    )
    .await;
    let client = client();
    let target = url(&node, "/");
    let first_client = client.clone();
    let first_target = target.clone();
    let first = tokio::spawn(async move {
        first_client
            .get(first_target)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status()
    });
    timeout(Duration::from_secs(1), entered.notified())
        .await
        .unwrap();
    assert_eq!(
        client
            .get(&target)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        429
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    release.notify_one();
    assert_eq!(first.await.unwrap(), 200);
    assert_eq!(
        client
            .get(&target)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    node.shutdown().await.unwrap();
}

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn timeout_cancels_and_drops_pending_handler_and_rejects_late_output() {
    let cancelled = Arc::new(Notify::new());
    let dropped = Arc::new(AtomicBool::new(false));
    let (signal, flag) = (cancelled.clone(), dropped.clone());
    let node = start(
        vec![route(
            "GET",
            "/",
            handler(move |_, token| {
                let (signal, flag) = (signal.clone(), flag.clone());
                async move {
                    let _drop = Dropped(flag);
                    tokio::spawn(async move {
                        token.cancelled().await;
                        signal.notify_one();
                    });
                    std::future::pending::<()>().await;
                    Ok(ok(b"must never be delivered"))
                }
            }),
        )],
        Limits {
            timeout: Duration::from_millis(60),
            ..limits()
        },
    )
    .await;
    let response = client()
        .get(url(&node, "/"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 504);
    assert_eq!(response.text().await.unwrap(), "request timed out");
    timeout(Duration::from_secs(1), cancelled.notified())
        .await
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    node.shutdown().await.unwrap();

    let node = start(
        vec![route(
            "GET",
            "/",
            handler(|_, token| async move {
                token.cancel();
                Ok(ok(b"late response"))
            }),
        )],
        limits(),
    )
    .await;
    let response = client()
        .get(url(&node, "/"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(!response.text().await.unwrap().contains("late response"));
    node.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_active_handler_and_incomplete_header_socket_then_releases_port() {
    let entered = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let (entry, signal) = (entered.clone(), cancelled.clone());
    let node = start(
        vec![route(
            "GET",
            "/",
            handler(move |_, token| {
                let (entry, signal) = (entry.clone(), signal.clone());
                async move {
                    tokio::spawn(async move {
                        token.cancelled().await;
                        signal.notify_one();
                    });
                    entry.notify_one();
                    std::future::pending::<()>().await;
                    Ok(ok(b"late"))
                }
            }),
        )],
        limits(),
    )
    .await;
    let address = node.local_addr();
    let target = url(&node, "/");
    let request = tokio::spawn(async move { client().get(target).bearer_auth(TOKEN).send().await });
    timeout(Duration::from_secs(1), entered.notified())
        .await
        .unwrap();
    let mut slow = TcpStream::connect(address).await.unwrap();
    slow.write_all(b"GET / HTTP/1.1\r\nHost:").await.unwrap();
    timeout(Duration::from_secs(3), node.shutdown())
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_secs(1), cancelled.notified())
        .await
        .unwrap();
    if let Ok(response) = request.await.unwrap() {
        assert_ne!(response.status(), 200);
    }
    let mut byte = [0u8; 1];
    let closed = timeout(Duration::from_secs(1), slow.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(closed, Ok(0) | Err(_)));
    let listener = TcpListener::bind(address).await.unwrap();
    drop(listener);
}

#[tokio::test]
async fn ipv6_loopback_and_drop_release_actual_listening_ports() {
    let node = Node::bind(
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
        TOKEN.into(),
        vec![route("GET", "/", handler(|_, _| async { Ok(ok(b"ipv6")) }))],
        limits(),
    )
    .await
    .unwrap();
    let response = client()
        .get(url(&node, "/"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "ipv6");
    let address = node.local_addr();
    drop(node);
    timeout(Duration::from_secs(1), async {
        loop {
            match TcpListener::bind(address).await {
                Ok(listener) => {
                    drop(listener);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn incomplete_unauthenticated_connections_expire_and_capacity_recovers_without_shutdown() {
    let node = start(
        vec![route(
            "GET",
            "/",
            handler(|_, _| async { Ok(ok(b"recovered")) }),
        )],
        Limits {
            max_concurrent: 1,
            timeout: Duration::from_millis(150),
            ..limits()
        },
    )
    .await;
    // Fill the separate 2x accepted-connection ceiling without complete headers/auth.
    let mut first = TcpStream::connect(node.local_addr()).await.unwrap();
    let mut second = TcpStream::connect(node.local_addr()).await.unwrap();
    first.write_all(b"GET / HTTP/1.1\r\nHost:").await.unwrap();
    second.write_all(b"G").await.unwrap();
    let mut byte = [0u8; 1];
    for stream in [&mut first, &mut second] {
        let closed = timeout(Duration::from_secs(2), stream.read(&mut byte))
            .await
            .unwrap();
        assert!(matches!(closed, Ok(0) | Err(_)));
    }
    let response = client()
        .get(url(&node, "/"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "recovered");
    node.shutdown().await.unwrap();
}
