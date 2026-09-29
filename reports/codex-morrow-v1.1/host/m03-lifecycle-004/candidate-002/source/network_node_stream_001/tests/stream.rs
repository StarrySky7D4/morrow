use morrow_network_node_stream::{
    Error, Limits, RawHttpRequest,
    client::{Client, EndpointPolicy},
    stream::{MAX_DELIVERY_CHUNK, SendContext, StreamGuard},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::{Instant, timeout},
};
use tokio_util::sync::CancellationToken;

fn limits() -> Limits {
    Limits {
        max_concurrent: 1,
        max_request_bytes: 32 * 1024,
        max_response_bytes: 64 * 1024,
        timeout: Duration::from_secs(5),
        ..Limits::default()
    }
}
fn client(origin: &str, limit: Limits) -> Client {
    Client::new(EndpointPolicy::new(origin, &["POST"], true).unwrap(), limit).unwrap()
}
fn request(origin: &str) -> RawHttpRequest {
    RawHttpRequest {
        method: "POST".into(),
        target: format!("{origin}/responses"),
        headers: vec![("Content-Type".into(), b"application/json".to_vec())],
        body: b"{\"fixture\":true}".to_vec(),
    }
}
fn context() -> SendContext {
    SendContext::trusted(
        Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    )
}
async fn listen() -> (TcpListener, String) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    (l, origin)
}
async fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut all = Vec::new();
    let mut buf = [0; 4096];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        all.extend_from_slice(&buf[..n]);
        assert!(all.len() < 48 * 1024);
        if let Some(p) = all.windows(4).position(|w| w == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&all[..p]);
            let length = text
                .lines()
                .filter_map(|s| s.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if all.len() >= p + 4 + length {
                return all;
            }
        }
    }
}

#[tokio::test]
async fn first_chunk_precedes_server_eof_and_permit_survives_head_and_eof() {
    let (listener, origin) = listen().await;
    let (gate_tx, gate_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        let received = read_request(&mut s).await;
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nX-Repeat: one\r\nX-Repeat: two\r\nX-Raw: \xff\r\n\r\n3\r\none\r\n").await.unwrap();
        // EOF is causally impossible until actual client chunk consumption.
        gate_rx.await.unwrap();
        s.write_all(b"3\r\ntwo\r\n0\r\n\r\n").await.unwrap();
        received
    });
    let c = client(&origin, limits());
    let mut r = request(&origin);
    r.headers.push(("X-Request-Raw".into(), vec![0xff]));
    r.headers.push(("X-Duplicate".into(), b"a".to_vec()));
    r.headers.push(("X-Duplicate".into(), b"b".to_vec()));
    let mut lease = c.send_stream(r, context()).await.unwrap();
    assert_eq!(lease.head().status, 200);
    assert!(
        lease
            .head()
            .headers
            .iter()
            .any(|(n, v)| n == "x-raw" && v == &[0xff])
    );
    assert_eq!(
        lease
            .head()
            .headers
            .iter()
            .filter(|(n, _)| n == "x-repeat")
            .count(),
        2
    );
    assert_eq!(
        c.send_stream(request(&origin), context())
            .await
            .err()
            .unwrap(),
        Error::Limit
    );
    let first = timeout(Duration::from_secs(1), lease.next_chunk())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(first, b"one"[..]);
    assert!(lease.terminal().is_none());
    gate_tx.send(()).unwrap();
    let mut tail = Vec::new();
    while let Some(bytes) = lease.next_chunk().await.unwrap() {
        tail.extend_from_slice(&bytes);
    }
    assert_eq!(tail, b"two");
    assert_eq!(
        c.send_stream(request(&origin), context())
            .await
            .err()
            .unwrap(),
        Error::Limit
    );
    let done = lease.finish().await;
    assert!(done.http_eof && done.worker_joined);
    assert_eq!(done.outcome, Ok(()));
    assert_eq!((done.received_bytes, done.delivered_bytes), (6, 6));
    let received = server.await.unwrap();
    assert!(
        received
            .windows(18)
            .any(|w| w == b"x-request-raw: \xff\r\n")
    );
    assert!(received.ends_with(b"{\"fixture\":true}"));
    assert_eq!(
        c.send_stream(request(&origin), context())
            .await
            .err()
            .unwrap(),
        Error::Transport
    );
}

#[tokio::test]
async fn original_deadline_expires_while_waiting_for_demand_and_does_not_renew_at_head() {
    let (listener, origin) = listen().await;
    let (head_tx, head_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read_request(&mut s).await;
        tokio::time::sleep(Duration::from_millis(90)).await;
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        head_tx.send(()).unwrap();
        let mut buf = [0; 64];
        let _ = s.read(&mut buf).await;
    });
    let c = client(&origin, limits());
    let deadline = Instant::now() + Duration::from_millis(220);
    let mut lease = c
        .send_stream(
            request(&origin),
            SendContext::trusted(deadline, CancellationToken::new()),
        )
        .await
        .unwrap();
    head_rx.await.unwrap();
    assert_eq!(lease.deadline(), deadline);
    tokio::time::sleep_until(deadline + Duration::from_millis(30)).await;
    assert_eq!(lease.next_chunk().await.err(), Some(Error::Timeout));
    let done = lease.finish().await;
    assert_eq!(done.outcome, Err(Error::Timeout));
    assert!(done.worker_joined && !done.http_eof);
    timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    let expired = SendContext::trusted(
        Instant::now() - Duration::from_millis(1),
        CancellationToken::new(),
    );
    assert_eq!(
        c.send_stream(request(&origin), expired)
            .await
            .err()
            .unwrap(),
        Error::Timeout
    );
}

#[tokio::test]
async fn cancel_pending_body_read_and_cancelled_next_future_do_not_lose_demand() {
    let (listener, origin) = listen().await;
    let (gate_tx, gate_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read_request(&mut s).await;
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        gate_rx.await.unwrap();
        s.write_all(b"1\r\nx\r\n").await.unwrap();
        let mut buf = [0; 64];
        let _ = s.read(&mut buf).await;
    });
    let c = client(&origin, limits());
    let token = CancellationToken::new();
    let mut lease = c
        .send_stream(
            request(&origin),
            SendContext::trusted(Instant::now() + Duration::from_secs(3), token.clone()),
        )
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_millis(30), lease.next_chunk())
            .await
            .is_err()
    );
    gate_tx.send(()).unwrap();
    assert_eq!(lease.next_chunk().await.unwrap().unwrap(), b"x"[..]);
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        token.cancel();
    });
    assert_eq!(lease.next_chunk().await.err(), Some(Error::Cancelled));
    cancel_task.await.unwrap();
    let done = lease.finish().await;
    assert!(done.worker_joined && !done.http_eof);
    assert_eq!(done.outcome, Err(Error::Cancelled));
    timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn response_limit_streaming_unknown_length_and_small_delivery_chunks() {
    for excess in [false, true] {
        let (listener, origin) = listen().await;
        let length = if excess { 20001 } else { 20000 };
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read_request(&mut s).await;
            s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            let mut body = format!("{length:x}\r\n").into_bytes();
            body.extend(vec![b'z'; length]);
            body.extend_from_slice(b"\r\n0\r\n\r\n");
            let _ = s.write_all(&body).await;
        });
        let c = client(
            &origin,
            Limits {
                max_response_bytes: 20000,
                ..limits()
            },
        );
        let mut lease = c.send_stream(request(&origin), context()).await.unwrap();
        let mut n = 0;
        let outcome;
        loop {
            match lease.next_chunk().await {
                Ok(Some(bytes)) => {
                    assert!(bytes.len() <= MAX_DELIVERY_CHUNK);
                    n += bytes.len();
                }
                Ok(None) => {
                    outcome = Ok(());
                    break;
                }
                Err(e) => {
                    outcome = Err(e);
                    break;
                }
            }
        }
        assert_eq!(outcome, if excess { Err(Error::Limit) } else { Ok(()) });
        if !excess {
            assert_eq!(n, 20000);
        } else {
            assert!(n <= 20000);
        }
        let done = lease.finish().await;
        assert_eq!(done.outcome, outcome);
        assert_eq!(done.http_eof, !excess);
        assert!(done.worker_joined);
        server.await.unwrap();
    }
}

struct Guard(AtomicBool);
impl StreamGuard for Guard {
    fn check(&self) -> morrow_network_node_stream::Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn live_guard_revocation_is_checked_even_without_consumer_demand() {
    let (listener, origin) = listen().await;
    let server = tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.unwrap();
        read_request(&mut s).await;
        s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        let mut b = [0; 64];
        let _ = s.read(&mut b).await;
    });
    let guard = Arc::new(Guard(AtomicBool::new(false)));
    let c = client(&origin, limits());
    let mut lease = c
        .send_stream(
            request(&origin),
            SendContext::guarded(
                Instant::now() + Duration::from_secs(3),
                CancellationToken::new(),
                guard.clone(),
            ),
        )
        .await
        .unwrap();
    guard.0.store(true, Ordering::SeqCst);
    timeout(Duration::from_secs(1), async {
        loop {
            if lease.terminal().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(lease.next_chunk().await.err(), Some(Error::Denied));
    let done = lease.finish().await;
    assert_eq!(done.outcome, Err(Error::Denied));
    assert!(done.worker_joined);
    timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn dropping_head_future_and_lease_cancels_owned_worker_without_false_join_receipt() {
    for after_head in [false, true] {
        let (listener, origin) = listen().await;
        let (received_tx, received_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read_request(&mut s).await;
            if after_head {
                s.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                    .await
                    .unwrap();
            }
            received_tx.send(()).unwrap();
            let mut b = [0; 64];
            let _ = s.read(&mut b).await;
        });
        let c = client(&origin, limits());
        if after_head {
            let lease = c.send_stream(request(&origin), context()).await.unwrap();
            received_rx.await.unwrap();
            drop(lease);
        } else {
            let owned = c.clone();
            let r = request(&origin);
            let future = tokio::spawn(async move { owned.send_stream(r, context()).await });
            received_rx.await.unwrap();
            future.abort();
            assert!(matches!(future.await,Err(error) if error.is_cancelled()));
        }
        timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        // A closed listener and non-Limit error show eventual permit release, not an explicit join receipt.
        timeout(Duration::from_secs(1), async {
            loop {
                let error = c
                    .send_stream(
                        request(&origin),
                        SendContext::trusted(
                            Instant::now() + Duration::from_millis(100),
                            CancellationToken::new(),
                        ),
                    )
                    .await
                    .err()
                    .unwrap();
                if error != Error::Limit {
                    // Windows closed-port connect can outlast the fixture's one-second
                    // wait. Either outcome proves admission after the old permit released.
                    assert!(matches!(error, Error::Transport | Error::Timeout));
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
