use morrow_network_node_stream::{
    Error as TransportError, Limits, RawHttpRequest,
    client::{Client, EndpointPolicy},
    sse::{DecoderLimits, Error, SseLease, decoder::Error as DecoderError},
    stream::{SendContext, StreamGuard},
};
use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::{Instant, timeout},
};
use tokio_util::sync::CancellationToken;

fn decoder_limits() -> DecoderLimits {
    DecoderLimits {
        max_line_bytes: 1024,
        max_event_bytes: 4096,
        max_total_bytes: 32768,
        max_events: 16,
        max_id_bytes: 128,
        max_retry_digits: 10,
    }
}
fn client(origin: &str, response_bytes: usize) -> Client {
    Client::new(
        EndpointPolicy::new(origin, &["POST"], true).unwrap(),
        Limits {
            max_concurrent: 1,
            max_response_bytes: response_bytes,
            timeout: Duration::from_secs(3),
            ..Limits::default()
        },
    ).unwrap()
}
fn request(origin: &str) -> RawHttpRequest {
    RawHttpRequest {
        method: "POST".into(),
        target: format!("{origin}/synthetic-events"),
        headers: vec![("Content-Type".into(), b"application/json".to_vec())],
        body: b"{\"synthetic\":true}".to_vec(),
    }
}
fn context() -> SendContext {
    SendContext::trusted(Instant::now() + Duration::from_secs(3), CancellationToken::new())
}
async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (listener, origin)
}
async fn read_request(socket: &mut TcpStream) {
    let mut bytes = Vec::new();
    let mut block = [0; 4096];
    loop {
        let n = socket.read(&mut block).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&block[..n]);
        assert!(bytes.len() <= 4096);
        if let Some(start) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..start]);
            assert!(head.starts_with("POST /synthetic-events HTTP/1.1"));
            let length = head.lines().filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, length)| length.trim().parse::<usize>().unwrap()).unwrap_or(0);
            if bytes.len() >= start + 4 + length {
                assert_eq!(&bytes[start + 4..start + 4 + length], b"{\"synthetic\":true}");
                return;
            }
        }
    }
}
async fn chunk(socket: &mut TcpStream, bytes: &[u8]) -> bool {
    socket.write_all(format!("{:x}\r\n", bytes.len()).as_bytes()).await.is_ok()
        && socket.write_all(bytes).await.is_ok()
        && socket.write_all(b"\r\n").await.is_ok()
}
fn serve(listener: TcpListener, head: &'static str, parts: Vec<Vec<u8>>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        if socket.write_all(head.as_bytes()).await.is_err() { return; }
        for part in parts {
            if !chunk(&mut socket, &part).await { return; }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    })
}
const HEAD: &str = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream; charset=utf-8\r\n\r\n";
async fn join(server: JoinHandle<()>) {
    timeout(Duration::from_secs(1), server).await.unwrap().unwrap();
}
async fn open(client: &Client, origin: &str, limits: DecoderLimits, context: SendContext) -> SseLease {
    let stream = client.send_stream(request(origin), context).await.unwrap();
    SseLease::new(stream, limits).await.unwrap()
}

#[tokio::test]
async fn first_event_precedes_allowed_eof_and_slow_consumer_does_not_pull() {
    let (listener, origin) = listen().await;
    let (end_tx, end_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(HEAD.as_bytes()).await.unwrap();
        assert!(chunk(&mut socket, b"data: first\n\n").await);
        end_rx.await.unwrap();
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(lease.terminal().is_none());
    assert_eq!(lease.delivered_events(), 0);
    let first = timeout(Duration::from_secs(1), lease.next_event()).await.unwrap().unwrap().unwrap();
    assert_eq!(first.data, "first");
    // Server cannot emit HTTP EOF until this successful event assertion.
    assert!(lease.terminal().is_none());
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(lease.terminal().is_none());
    assert_eq!(c.send_stream(request(&origin), context()).await.err(), Some(TransportError::Limit));
    // Explicit finish does not issue demand/drain; EOF remains gated on the server.
    let done = lease.finish().await;
    assert_eq!(done.transport.network_polls, 1);
    assert_eq!(done.delivered_events, 1);
    assert!(done.transport.worker_joined && !done.transport.http_eof && done.truncated);
    assert_eq!(done.outcome, Err(Error::Transport(TransportError::Cancelled)));
    end_tx.send(()).unwrap();
    join(server).await;
}

#[tokio::test]
async fn utf8_crlf_multiline_empty_data_done_and_unterminated_eof() {
    let (listener, origin) = listen().await;
    let parts: Vec<Vec<u8>> = [
        &b"id: original\r\nevent: delta\r\nretry: 12\r\ndata: \xe4"[..],
        &b"\xbd\xa0\r"[..], &b"\ndata: second\r"[..], &b"\n\r"[..], &b"\n"[..],
        &b"data:\n\n"[..], &b"data: [DONE]\n\n"[..], &b"data: tail"[..],
    ].into_iter().map(Vec::from).collect();
    let server = serve(listener, HEAD, parts);
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    let first = lease.next_event().await.unwrap().unwrap();
    assert_eq!(first.kind, "delta");
    assert_eq!(first.data, "你\nsecond");
    assert_eq!(first.id, "original");
    assert_eq!(first.retry, Some(12));
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "");
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "[DONE]");
    assert!(lease.next_event().await.unwrap().is_none());
    let done = lease.finish().await;
    assert_eq!(done.outcome, Ok(()));
    assert!(done.transport.http_eof && done.transport.worker_joined);
    assert!(done.decoder_reached_http_eof && done.decoder.unwrap().truncated && done.truncated);
    assert_eq!(done.delivered_events, 3);
    join(server).await;
}

#[tokio::test]
async fn dropped_next_event_future_preserves_partial_decoder_and_pending_demand() {
    let (listener, origin) = listen().await;
    let (tail_tx, tail_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(HEAD.as_bytes()).await.unwrap();
        assert!(chunk(&mut socket, b"data: par").await);
        tail_rx.await.unwrap();
        assert!(chunk(&mut socket, b"tial\n\ndata: second\n\n").await);
        socket.write_all(b"0\r\n\r\n").await.unwrap();
    });
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert!(timeout(Duration::from_millis(30), lease.next_event()).await.is_err());
    tail_tx.send(()).unwrap();
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "partial");
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "second");
    assert!(lease.next_event().await.unwrap().is_none());
    let done = lease.finish().await;
    assert_eq!(done.delivered_events, 2);
    assert_eq!(done.outcome, Ok(()));
    assert!(done.transport.worker_joined && done.transport.http_eof && !done.truncated);
    join(server).await;
}

struct Guard(AtomicBool);
impl StreamGuard for Guard {
    fn check(&self) -> morrow_network_node_stream::Result<()> {
        if self.0.load(Ordering::SeqCst) { Err(TransportError::Denied) } else { Ok(()) }
    }
}
#[tokio::test]
async fn buffered_second_event_rechecks_revocation_cancellation_and_absolute_expiry() {
    for cause in 0..3 {
        let (listener, origin) = listen().await;
        let server = serve(listener, HEAD, vec![b"data: one\n\ndata: two\n\n".to_vec()]);
        let c = client(&origin, 32768);
        let guard = Arc::new(Guard(AtomicBool::new(false)));
        let cancel = CancellationToken::new();
        let deadline = Instant::now() + Duration::from_millis(400);
        let mut lease = open(&c, &origin, decoder_limits(),
            SendContext::guarded(deadline, cancel.clone(), guard.clone())).await;
        assert_eq!(lease.deadline(), deadline);
        assert_eq!(lease.next_event().await.unwrap().unwrap().data, "one");
        assert!(lease.buffered_bytes() > 0, "fixture must actually exercise the buffered path");
        let expected = match cause {
            0 => { guard.0.store(true, Ordering::SeqCst); TransportError::Denied },
            1 => { cancel.cancel(); TransportError::Cancelled },
            _ => {
                tokio::time::sleep_until(deadline + Duration::from_millis(10)).await;
                TransportError::Timeout
            },
        };
        assert_eq!(lease.next_event().await.err(), Some(Error::Transport(expected)));
        assert_eq!(lease.next_event().await.err(), Some(Error::Transport(expected)));
        let done = lease.finish().await;
        assert_eq!(done.outcome, Err(Error::Transport(expected)));
        assert_eq!(done.delivered_events, 1);
        assert!(done.buffered_bytes > 0 && done.truncated && done.transport.worker_joined);
        join(server).await;
    }
}

#[tokio::test]
async fn rejected_status_or_mime_returns_real_join_receipt_and_first_cause() {
    let cases = [
        ("HTTP/1.1 400 Bad Request\r\n", "Content-Type: text/event-stream\r\n", Error::HttpStatus(400)),
        ("HTTP/1.1 500 Server Error\r\n", "Content-Type: text/event-stream\r\n", Error::HttpStatus(500)),
        ("HTTP/1.1 204 No Content\r\n", "Content-Type: text/event-stream\r\n", Error::HttpStatus(204)),
        ("HTTP/1.1 200 OK\r\n", "", Error::InvalidMime),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/event-stream\r\nContent-Encoding: gzip\r\n", Error::InvalidEncoding),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/event-stream\r\nContent-Encoding: identity\r\nContent-Encoding: identity\r\n", Error::InvalidEncoding),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/plain\r\n", Error::InvalidMime),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/event-stream\r\nContent-Type: text/event-stream\r\n", Error::InvalidMime),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/event-stream; charset=latin1\r\n", Error::InvalidMime),
        ("HTTP/1.1 200 OK\r\n", "Content-Type: text/event-stream; charset=utf-8; charset=utf-8\r\n", Error::InvalidMime),
    ];
    for (status, mime, expected) in cases {
        let (listener, origin) = listen().await;
        let head = format!("{status}Content-Length: 0\r\n{mime}\r\n");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            let _ = socket.write_all(head.as_bytes()).await;
        });
        let c = client(&origin, 32768);
        let stream = c.send_stream(request(&origin), context()).await.unwrap();
        let failed = SseLease::new(stream, decoder_limits()).await.err().unwrap();
        assert_eq!(failed.cause, expected);
        assert!(failed.transport.worker_joined);
        join(server).await;
        let after = c.send_stream(request(&origin), context()).await.err().unwrap();
        assert_ne!(after, TransportError::Limit);
    }
}

#[tokio::test]
async fn decoder_failures_cancel_transport_but_keep_first_parser_cause() {
    let cases = [
        (b"data: toolong\n\n".to_vec(), 0, DecoderError::LineLimit),
        (b": one\n: two\n: three\n: four\n\n".to_vec(), 1, DecoderError::EventLimit),
        (b"data: first\n\ndata: second\n\n".to_vec(), 2, DecoderError::TotalLimit),
        (b"id: toolong\ndata: x\n\n".to_vec(), 3, DecoderError::IdLimit),
        (b"retry: 12345\ndata: x\n\n".to_vec(), 4, DecoderError::RetryLimit),
        (b"data: first\n\ndata: second\n\n".to_vec(), 5, DecoderError::EventCountLimit),
        (b"data: \xff\n\n".to_vec(), 6, DecoderError::InvalidUtf8),
    ];
    for (bytes, variant, expected) in cases {
        let (listener, origin) = listen().await;
        let server = serve(listener, HEAD, vec![bytes]);
        let c = client(&origin, 32768);
        let mut bounds = decoder_limits();
        match variant {
            0 => bounds.max_line_bytes = 8,
            1 => { bounds.max_line_bytes = 8; bounds.max_event_bytes = 20; },
            2 => { bounds.max_line_bytes = 16; bounds.max_event_bytes = 16; bounds.max_total_bytes = 20; },
            3 => bounds.max_id_bytes = 3,
            4 => bounds.max_retry_digits = 3,
            5 => bounds.max_events = 1,
            _ => {},
        }
        bounds.max_id_bytes = bounds.max_id_bytes.min(bounds.max_line_bytes);
        let mut lease = open(&c, &origin, bounds, context()).await;
        loop {
            match lease.next_event().await {
                Ok(Some(_)) => {},
                Err(error) => { assert_eq!(error, Error::Decoder(expected)); break; },
                Ok(None) => panic!("missing expected bounded decoder error"),
            }
        }
        assert_eq!(lease.next_event().await.err(), Some(Error::Decoder(expected)));
        let done = lease.finish().await;
        assert_eq!(done.outcome, Err(Error::Decoder(expected)));
        assert_eq!(done.decoder.err(), Some(expected));
        assert!(done.transport.worker_joined);
        join(server).await;
    }
}

#[tokio::test]
async fn raw_transport_budget_remains_independent_of_decoder_budget() {
    let (listener, origin) = listen().await;
    let server = serve(listener, HEAD, vec![b"data: beyondtransport\n\n".to_vec()]);
    let c = client(&origin, 8);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert_eq!(lease.next_event().await.err(), Some(Error::Transport(TransportError::Limit)));
    let done = lease.finish().await;
    assert_eq!(done.outcome, Err(Error::Transport(TransportError::Limit)));
    assert!(done.transport.worker_joined && !done.transport.http_eof);
    join(server).await;
}

#[tokio::test]
async fn clean_http_eof_does_not_release_permit_before_explicit_finish() {
    let (listener, origin) = listen().await;
    let server = serve(listener, HEAD, vec![b"data: ordinary\n\n".to_vec()]);
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "ordinary");
    assert!(lease.next_event().await.unwrap().is_none());
    assert_eq!(c.send_stream(request(&origin), context()).await.err(), Some(TransportError::Limit));
    let done = lease.finish().await;
    assert_eq!(done.outcome, Ok(()));
    assert!(done.transport.http_eof && done.transport.worker_joined && !done.truncated);
    assert_eq!(done.delivered_events, 1);
    join(server).await;
    assert_ne!(c.send_stream(request(&origin), context()).await.err(), Some(TransportError::Limit));
}

#[tokio::test]
async fn cancel_and_wait_and_drop_keep_original_worker_cleanup_semantics() {
    for explicit in [true, false] {
        let (listener, origin) = listen().await;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_request(&mut socket).await;
            socket.write_all(HEAD.as_bytes()).await.unwrap();
            let mut bytes = [0; 1];
            let _ = socket.read(&mut bytes).await;
        });
        let c = client(&origin, 32768);
        let lease = open(&c, &origin, decoder_limits(), context()).await;
        assert_eq!(c.send_stream(request(&origin), context()).await.err(), Some(TransportError::Limit));
        if explicit {
            let done = lease.cancel_and_wait().await;
            assert!(done.transport.worker_joined && done.truncated && !done.transport.http_eof);
            assert_eq!(done.delivered_events, 0);
            assert_eq!(done.transport.network_polls, 0);
        } else {
            drop(lease); // No joined receipt is fabricated.
        }
        join(server).await;
        timeout(Duration::from_secs(1), async {
            loop {
                let result = c.send_stream(request(&origin),
                    SendContext::trusted(Instant::now() + Duration::from_millis(100), CancellationToken::new())).await;
                if result.err() != Some(TransportError::Limit) { break; }
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
    }
}

#[tokio::test]
async fn identity_encoding_and_bounded_quoted_charset_are_accepted() {
    let (listener, origin) = listen().await;
    let server = serve(listener,
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: Text/Event-Stream; charset=\"UTF-8\"\r\nContent-Encoding: identity\r\n\r\n",
        vec![b"data: accepted\n\n".to_vec()]);
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert_eq!(lease.next_event().await.unwrap().unwrap().data, "accepted");
    assert!(lease.next_event().await.unwrap().is_none());
    let done = lease.finish().await;
    assert_eq!(done.outcome, Ok(()));
    assert!(done.transport.http_eof && done.transport.worker_joined && !done.truncated);
    join(server).await;
}

#[tokio::test]
async fn invalid_decoder_limits_join_already_open_transport_without_body_poll() {
    let (listener, origin) = listen().await;
    let server = serve(listener, HEAD, vec![b"data: unopened\n\n".to_vec()]);
    let c = client(&origin, 32768);
    let stream = c.send_stream(request(&origin), context()).await.unwrap();
    let mut bounds = decoder_limits();
    bounds.max_events = 0;
    let failed = SseLease::new(stream, bounds).await.err().unwrap();
    assert_eq!(failed.cause, Error::Decoder(DecoderError::InvalidLimits));
    assert!(failed.transport.worker_joined);
    assert_eq!(failed.transport.network_polls, 0);
    join(server).await;
}

#[tokio::test]
async fn finish_keeps_partial_utf8_fault_before_cleanup_cancellation() {
    let (listener, origin) = listen().await;
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        socket.write_all(HEAD.as_bytes()).await.unwrap();
        assert!(chunk(&mut socket, b"data: \xe4").await);
        // No remaining UTF-8 bytes, SSE delimiter or HTTP EOF can arrive.
        let mut bytes = [0; 1];
        assert_eq!(socket.read(&mut bytes).await.unwrap(), 0);
    });
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert!(timeout(Duration::from_millis(30), lease.next_event()).await.is_err());
    assert_eq!(lease.delivered_events(), 0);
    let done = lease.finish().await;
    assert_eq!(done.outcome, Err(Error::Decoder(DecoderError::InvalidUtf8)));
    assert_eq!(done.decoder.err(), Some(DecoderError::InvalidUtf8));
    assert_eq!(done.transport.outcome, Err(TransportError::Cancelled));
    assert!(done.transport.worker_joined && !done.transport.http_eof);
    assert!(!done.decoder_reached_http_eof && done.truncated);
    assert_eq!(done.delivered_events, 0);
    join(server).await;
}

#[tokio::test]
async fn http_eof_with_partial_utf8_remains_truncated_parser_failure() {
    let (listener, origin) = listen().await;
    let server = serve(listener, HEAD, vec![b"data: \xe4\xbd".to_vec()]);
    let c = client(&origin, 32768);
    let mut lease = open(&c, &origin, decoder_limits(), context()).await;
    assert_eq!(lease.next_event().await.err(), Some(Error::Decoder(DecoderError::InvalidUtf8)));
    let done = lease.finish().await;
    assert_eq!(done.outcome, Err(Error::Decoder(DecoderError::InvalidUtf8)));
    assert_eq!(done.decoder.err(), Some(DecoderError::InvalidUtf8));
    assert_eq!(done.transport.outcome, Ok(()));
    assert!(done.transport.http_eof && done.transport.worker_joined);
    assert!(done.decoder_reached_http_eof && done.truncated);
    assert_eq!(done.delivered_events, 0);
    join(server).await;
}
