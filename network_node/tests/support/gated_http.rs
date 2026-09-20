//! Shared real TCP barrier used by native managed HTTP tests.
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};
const WAIT: Duration = Duration::from_secs(10);
fn raw_response(_: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
pub(crate) struct GatedServer {
    pub(crate) origin: String,
    handle: JoinHandle<()>,
    pub(crate) calls: Arc<AtomicUsize>,
    entered: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
}
impl GatedServer {
    pub(crate) async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (entered_tx, entered) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let handle = tokio::spawn(async move {
            tokio::time::timeout(WAIT, async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                let mut raw = Vec::new();
                let mut buffer = [0; 1024];
                let end = loop {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    raw.extend_from_slice(&buffer[..n]);
                    assert!(raw.len() <= 128 * 1024);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                // Body is binary; decode headers only.
                let text = std::str::from_utf8(&raw[..end]).unwrap();
                let length: usize = text
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.trim().parse().unwrap());
                assert!(end + length <= 128 * 1024);
                while raw.len() < end + length {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    raw.extend_from_slice(&buffer[..n]);
                    assert!(raw.len() <= 128 * 1024);
                }
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
                let _ = socket.write_all(&raw_response("200 OK", b"ok")).await;
                let _ = socket.shutdown().await;
            })
            .await
            .expect("bounded gated server");
        });
        Self {
            origin,
            handle,
            calls,
            entered,
            release: Some(release),
        }
    }
    pub(crate) async fn entered(&mut self) {
        tokio::time::timeout(WAIT, &mut self.entered)
            .await
            .unwrap()
            .unwrap();
    }
    pub(crate) async fn finish(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
        tokio::time::timeout(WAIT, &mut self.handle)
            .await
            .unwrap()
            .unwrap();
    }
}
impl Drop for GatedServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
