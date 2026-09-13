use morrow_network_node::{
    Error, HttpRequest, Limits,
    client::{Client, EndpointPolicy},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

async fn server(
    response: Vec<u8>,
    delay: Duration,
) -> (String, oneshot::Receiver<Vec<u8>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![];
        let mut buffer = [0; 1024];
        let header_end = loop {
            let n = socket.read(&mut buffer).await.unwrap();
            if n == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..n]);
            assert!(request.len() < 128 * 1024);
            if let Some(n) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break n + 4;
            }
        };
        let text = String::from_utf8_lossy(&request[..header_end]);
        let length = text
            .lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
            .unwrap_or(0);
        while request.len() < header_end + length {
            let n = socket.read(&mut buffer).await.unwrap();
            assert_ne!(n, 0);
            request.extend_from_slice(&buffer[..n]);
        }
        let _ = tx.send(request);
        tokio::time::sleep(delay).await;
        let _ = socket.write_all(&response).await;
        let _ = socket.shutdown().await;
    });
    (origin, rx, task)
}
fn response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut out=format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nX-Repeat: one\r\nX-Repeat: two\r\n\r\n",body.len()).into_bytes();
    out.extend_from_slice(body);
    out
}
fn client(origin: &str, methods: &[&str], limits: Limits) -> Client {
    Client::new(EndpointPolicy::new(origin, methods, true).unwrap(), limits).unwrap()
}
fn request(origin: &str, method: &str) -> HttpRequest {
    HttpRequest {
        method: method.into(),
        target: format!("{origin}/api?q=one&q=two"),
        headers: vec![],
        body: vec![],
    }
}
#[tokio::test]
async fn all_approved_methods_preserve_body_and_duplicate_headers() {
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let (origin, received, task) =
            server(response("200 OK", b"\0\xffreply"), Duration::ZERO).await;
        let c = client(&origin, &[method], Limits::default());
        let mut req = request(&origin, method);
        req.body = b"\0\xffinput".to_vec();
        req.headers = vec![
            ("X-Request".into(), "first".into()),
            ("X-Request".into(), "second".into()),
            ("Authorization".into(), "Bearer synthetic-only".into()),
        ];
        let reply = c.send(req, CancellationToken::new()).await.unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(
            reply.body,
            if method == "HEAD" {
                vec![]
            } else {
                b"\0\xffreply".to_vec()
            }
        );
        assert_eq!(
            reply
                .headers
                .iter()
                .filter(|(k, _)| k == "x-repeat")
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
        let raw = received.await.unwrap();
        assert!(raw.starts_with(format!("{method} /api?q=one&q=two HTTP/1.1\r\n").as_bytes()));
        assert!(raw.ends_with(b"\0\xffinput"));
        let header = String::from_utf8_lossy(&raw).to_ascii_lowercase();
        assert!(header.contains("x-request: first\r\n"));
        assert!(header.contains("x-request: second\r\n"));
        task.await.unwrap();
    }
}
#[tokio::test]
async fn http_error_status_is_a_response_not_transport_failure() {
    for status in [
        "401 Unauthorized",
        "404 Not Found",
        "429 Too Many Requests",
        "500 Internal Server Error",
    ] {
        let (origin, _, task) =
            server(response(status, b"actual error body"), Duration::ZERO).await;
        let reply = client(&origin, &["GET"], Limits::default())
            .send(request(&origin, "GET"), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(reply.status, status[..3].parse::<u16>().unwrap());
        assert_eq!(reply.body, b"actual error body");
        task.await.unwrap();
    }
}
#[tokio::test]
async fn unapproved_origin_method_and_sensitive_framing_fail_before_connect() {
    let (origin, mut received, task) = server(response("200 OK", b"unused"), Duration::ZERO).await;
    let c = client(&origin, &["GET"], Limits::default());
    let mut wrong = request(&origin, "GET");
    wrong.target = "http://127.0.0.1:1/api".into();
    assert_eq!(
        c.send(wrong, CancellationToken::new()).await.err(),
        Some(Error::Denied)
    );
    assert_eq!(
        c.send(request(&origin, "POST"), CancellationToken::new())
            .await
            .err(),
        Some(Error::Denied)
    );
    for name in [
        "Host",
        "Content-Length",
        "Transfer-Encoding",
        "Connection",
        "Proxy-Authorization",
    ] {
        let mut req = request(&origin, "GET");
        req.headers.push((name.into(), "untrusted".into()));
        assert_eq!(
            c.send(req, CancellationToken::new()).await.err(),
            Some(Error::Denied)
        );
    }
    let mut req = request(&origin, "GET");
    req.headers.push(("X-Test".into(), "a\r\nb: c".into()));
    assert_eq!(
        c.send(req, CancellationToken::new()).await.err(),
        Some(Error::Invalid)
    );
    assert_eq!(
        received.try_recv().err(),
        Some(oneshot::error::TryRecvError::Empty)
    );
    task.abort();
}
#[test]
fn origin_policy_is_exact_and_local_http_is_loopback_only() {
    for origin in [
        "https://user:pass@example.com",
        "https://@example.com",
        "https://example.com/a",
        "https://example.com/a/..",
        "https://example.com/?q=1",
        "https://example.com/#frag",
        " https://example.com",
        "https://example.com\\other",
        "ftp://example.com",
    ] {
        assert_eq!(
            EndpointPolicy::new(origin, &["GET"], false).err(),
            Some(Error::Invalid),
            "{origin}"
        );
    }
    for origin in [
        "http://example.com",
        "http://10.0.0.1",
        "http://192.168.1.1",
        "http://169.254.169.254",
        "http://[fc00::1]",
    ] {
        assert_eq!(
            EndpointPolicy::new(origin, &["GET"], true).err(),
            Some(Error::Denied),
            "{origin}"
        );
    }
    for origin in [
        "https://127.0.0.1",
        "https://10.0.0.1",
        "https://100.64.1.1",
        "https://192.0.2.1",
        "https://198.19.0.1",
        "https://224.0.0.1",
        "https://[::1]",
        "https://[::ffff:127.0.0.1]",
        "https://[2001:db8::1]",
        "https://[2002:7f00:1::1]",
    ] {
        assert_eq!(
            EndpointPolicy::new(origin, &["GET"], false).err(),
            Some(Error::Denied),
            "{origin}"
        );
    }
    for origin in [
        "http://127.0.0.1:1234",
        "http://[::1]:1234",
        "http://localhost:1234",
    ] {
        assert!(EndpointPolicy::new(origin, &["GET"], true).is_ok());
        assert_eq!(
            EndpointPolicy::new(origin, &["GET"], false).err(),
            Some(Error::Denied)
        );
    }
    assert!(EndpointPolicy::new("https://example.com/", &["GET", "POST"], false).is_ok());
    for methods in [vec![], vec!["GET", "GET"], vec!["CONNECT"], vec!["get"]] {
        assert_eq!(
            EndpointPolicy::new("https://example.com", &methods, false).err(),
            Some(Error::Invalid)
        );
    }
}
#[tokio::test]
async fn redirect_is_returned_without_contacting_second_origin() {
    let (other, mut other_received, other_task) =
        server(response("200 OK", b"secret"), Duration::ZERO).await;
    let data=format!("HTTP/1.1 302 Found\r\nLocation: {other}/new\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes();
    let (origin, _, task) = server(data, Duration::ZERO).await;
    let reply = client(&origin, &["GET"], Limits::default())
        .send(request(&origin, "GET"), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(reply.status, 302);
    assert!(reply.body.is_empty());
    assert!(
        reply
            .headers
            .iter()
            .any(|(n, v)| n == "location" && v == &format!("{other}/new"))
    );
    assert_eq!(
        other_received.try_recv().err(),
        Some(oneshot::error::TryRecvError::Empty)
    );
    task.await.unwrap();
    other_task.abort();
}
#[tokio::test]
async fn request_and_response_budgets_cover_known_and_streaming_lengths() {
    let (origin, mut received, task) = server(response("200 OK", b"unused"), Duration::ZERO).await;
    let c = client(
        &origin,
        &["POST"],
        Limits {
            max_request_bytes: 3,
            max_header_bytes: 16,
            ..Limits::default()
        },
    );
    let mut req = request(&origin, "POST");
    req.body = vec![1; 4];
    assert_eq!(
        c.send(req, CancellationToken::new()).await.err(),
        Some(Error::Limit)
    );
    let mut req = request(&origin, "POST");
    req.headers = vec![("X-Long".into(), "abcdefghijklmnop".into())];
    assert_eq!(
        c.send(req, CancellationToken::new()).await.err(),
        Some(Error::Limit)
    );
    assert_eq!(
        received.try_recv().err(),
        Some(oneshot::error::TryRecvError::Empty)
    );
    task.abort();
    for raw in [response("200 OK",b"12345"),b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\n123\r\n2\r\n45\r\n0\r\n\r\n".to_vec()] {
  let (origin,_,task)=server(raw,Duration::ZERO).await;assert_eq!(client(&origin,&["GET"],Limits{max_response_bytes:4,..Limits::default()}).send(request(&origin,"GET"),CancellationToken::new()).await.err(),Some(Error::Limit));task.await.unwrap();
 }
    let (origin, _, task) = server(response("200 OK", b"ok"), Duration::ZERO).await;
    assert_eq!(
        client(
            &origin,
            &["GET"],
            Limits {
                max_header_bytes: 8,
                ..Limits::default()
            }
        )
        .send(request(&origin, "GET"), CancellationToken::new())
        .await
        .err(),
        Some(Error::Limit)
    );
    task.await.unwrap();
}
#[tokio::test]
async fn cancellation_timeout_and_concurrency_are_bounded_and_release_permits() {
    let (origin, received, task) =
        server(response("200 OK", b"later"), Duration::from_secs(60)).await;
    let c = Arc::new(client(
        &origin,
        &["GET"],
        Limits {
            max_concurrent: 1,
            ..Limits::default()
        },
    ));
    let token = CancellationToken::new();
    let worker = {
        let c = c.clone();
        let token = token.clone();
        let req = request(&origin, "GET");
        tokio::spawn(async move { c.send(req, token).await })
    };
    received.await.unwrap();
    assert_eq!(
        c.send(request(&origin, "GET"), CancellationToken::new())
            .await
            .err(),
        Some(Error::Limit)
    );
    token.cancel();
    assert_eq!(worker.await.unwrap().err(), Some(Error::Cancelled));
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert_eq!(
        c.send(request(&origin, "GET"), cancelled).await.err(),
        Some(Error::Cancelled)
    );
    task.abort();
    let _ = task.await;
    // The released permit admits another request (it now reaches a closed local server).
    assert_eq!(
        c.send(request(&origin, "GET"), CancellationToken::new())
            .await
            .err(),
        Some(Error::Transport)
    );
    let (origin, _, task) = server(response("200 OK", b"late"), Duration::from_secs(60)).await;
    assert_eq!(
        client(
            &origin,
            &["GET"],
            Limits {
                timeout: Duration::from_millis(50),
                ..Limits::default()
            }
        )
        .send(request(&origin, "GET"), CancellationToken::new())
        .await
        .err(),
        Some(Error::Timeout)
    );
    task.abort();
}
#[tokio::test]
async fn default_dns_policy_rejects_localhost_even_for_https_before_network_io() {
    let policy = EndpointPolicy::new("https://localhost:443", &["GET"], false).unwrap();
    let c = Client::new(policy, Limits::default()).unwrap();
    let req = HttpRequest {
        method: "GET".into(),
        target: "https://localhost:443/".into(),
        headers: vec![],
        body: vec![],
    };
    assert_eq!(
        c.send(req, CancellationToken::new()).await.err(),
        Some(Error::Denied)
    );
}

#[tokio::test]
async fn connection_closed_without_response_does_not_retry_or_leak_diagnostic() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        assert!(socket.read(&mut buffer).await.unwrap() > 0);
        drop(socket);
        tokio::time::timeout(Duration::from_millis(150), listener.accept())
            .await
            .is_ok()
    });
    let mut req = request(&origin, "POST");
    req.target.push_str("&secret=synthetic-marker");
    req.body = b"private-test-body".to_vec();
    let error = client(&origin, &["POST"], Limits::default())
        .send(req, CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert_eq!(error, Error::Transport);
    assert!(!error.to_string().contains("synthetic-marker"));
    assert!(!error.to_string().contains("private-test-body"));
    assert!(!task.await.unwrap());
}
