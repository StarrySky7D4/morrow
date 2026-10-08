//! Real production reader/observer/wait branches; no StartedProcess or facts fabrication.
use super::*;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use tokio::io::AsyncWriteExt;
use tokio::io::ReadBuf;

struct FailingReader {
    bytes: io::Cursor<Vec<u8>>,
    code: Option<i32>,
}
impl Read for FailingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = Read::read(&mut self.bytes, buf)?;
        if n != 0 {
            return Ok(n);
        }
        Err(self
            .code
            .map(io::Error::from_raw_os_error)
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "no Windows EOF witness")))
    }
}
impl AsyncRead for FailingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut bytes = [0u8; 32];
        let limit = bytes.len().min(buf.remaining());
        if limit == 0 {
            return Poll::Ready(Ok(()));
        }
        match Read::read(&mut *self, &mut bytes[..limit]) {
            Ok(n) => {
                buf.put_slice(&bytes[..n]);
                Poll::Ready(Ok(()))
            }
            Err(error) => Poll::Ready(Err(error)),
        }
    }
}
fn failing(code: Option<i32>) -> FailingReader {
    FailingReader {
        bytes: io::Cursor::new(b"observed prefix".to_vec()),
        code,
    }
}
async fn receive_all(output: &mut mpsc::Receiver<Vec<u8>>) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut bytes = Vec::new();
        while let Some(chunk) = output.recv().await {
            bytes.extend(chunk);
        }
        bytes
    })
    .await
    .expect("reader observer must terminate")
}

#[tokio::test]
async fn actual_duplex_eof_preserves_bytes_and_failure_source() {
    let lifecycle = Lifecycle::new();
    let failure = lifecycle.receiver();
    let (mut writer, reader) = tokio::io::duplex(64);
    let (sender, mut output) = mpsc::channel(4);
    let task = spawn_pipe_reader(reader, sender, &lifecycle);
    writer.write_all(b"real duplex bytes").await.unwrap();
    writer.shutdown().await.unwrap();
    assert_eq!(receive_all(&mut output).await, b"real duplex bytes");
    task.await.unwrap();
    assert_eq!(*failure.borrow(), None);
    assert!(
        failure.has_changed().is_ok(),
        "session lifecycle keeps source alive after reader joins"
    );
}

#[tokio::test]
async fn pipe_raw_broken_pipe_is_eof_after_real_prefix() {
    let lifecycle = Lifecycle::new();
    let (sender, mut output) = mpsc::channel(4);
    let task = spawn_pipe_reader(failing(Some(109)), sender, &lifecycle);
    assert_eq!(receive_all(&mut output).await, b"observed prefix");
    task.await.unwrap();
    assert_eq!(*lifecycle.receiver().borrow(), None);
}

#[tokio::test]
async fn pipe_cancelled_os_read_latches_before_stream_closes() {
    let lifecycle = Lifecycle::new();
    let failure = lifecycle.receiver();
    let (sender, mut output) = mpsc::channel(4);
    let task = spawn_pipe_reader(failing(Some(995)), sender, &lifecycle);
    assert_eq!(receive_all(&mut output).await, b"observed prefix");
    assert_eq!(
        failure.borrow().as_deref(),
        Some("ordinary output read failed")
    );
    task.await.unwrap();
}

#[tokio::test]
async fn generic_broken_pipe_without_os_witness_is_failure() {
    let lifecycle = Lifecycle::new();
    let (sender, mut output) = mpsc::channel(4);
    let task = spawn_pipe_reader(failing(None), sender, &lifecycle);
    assert_eq!(receive_all(&mut output).await, b"observed prefix");
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output read failed")
    );
    task.await.unwrap();
}

#[tokio::test]
async fn blocking_pty_reader_distinguishes_eof_and_io_failure() {
    for code in [109, 995] {
        let lifecycle = Lifecycle::new();
        let (sender, mut output) = mpsc::channel(4);
        let task = spawn_pty_reader(failing(Some(code)), sender, &lifecycle);
        assert_eq!(receive_all(&mut output).await, b"observed prefix");
        task.await.unwrap();
        assert_eq!(lifecycle.receiver().borrow().is_some(), code == 995);
    }
}

struct PanicReader;
impl AsyncRead for PanicReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        panic!("actual reader task panicked");
    }
}
#[tokio::test]
async fn reader_panic_cannot_close_stream_before_failure_latch() {
    let lifecycle = Lifecycle::new();
    let (sender, mut output) = mpsc::channel(4);
    let task = spawn_pipe_reader(PanicReader, sender, &lifecycle);
    assert!(receive_all(&mut output).await.is_empty());
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output task failed")
    );
    task.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn observer_abort_before_first_poll_preserves_failure_before_channel_end() {
    let lifecycle = Lifecycle::new();
    let (sender, mut output) = mpsc::channel(4);
    let (_, reader) = tokio::io::duplex(64);
    let task = spawn_pipe_reader(reader, sender, &lifecycle);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(receive_all(&mut output).await.is_empty());
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output observer abandoned")
    );
}

#[tokio::test]
async fn stderr_failure_is_not_blocked_by_pending_stdout_reader() {
    let lifecycle = Lifecycle::new();
    let (_writer, reader) = tokio::io::duplex(64);
    let (out_sender, mut stdout) = mpsc::channel(4);
    let stdout_task = spawn_pipe_reader(reader, out_sender, &lifecycle);
    let (err_sender, mut stderr) = mpsc::channel(4);
    let stderr_task = spawn_pipe_reader(failing(Some(5)), err_sender, &lifecycle);
    assert_eq!(receive_all(&mut stderr).await, b"observed prefix");
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output read failed")
    );
    assert_eq!(stdout.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    stdout_task.abort();
    let _ = stdout_task.await;
    stderr_task.await.unwrap();
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output read failed")
    );
}

struct TrackedDuplexReader {
    reader: tokio::io::DuplexStream,
    entered: Arc<tokio::sync::Notify>,
    dropped: Arc<std::sync::atomic::AtomicUsize>,
}

impl AsyncRead for TrackedDuplexReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.entered.notify_one();
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl Drop for TrackedDuplexReader {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn observer_abort_after_reader_poll_releases_actual_async_reader() {
    let lifecycle = Lifecycle::new();
    let (sender, mut output) = mpsc::channel(4);
    let (_writer, reader) = tokio::io::duplex(64);
    let entered = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let task = spawn_pipe_reader(
        TrackedDuplexReader {
            reader,
            entered: Arc::clone(&entered),
            dropped: Arc::clone(&dropped),
        },
        sender,
        &lifecycle,
    );
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .expect("real duplex reader must have been polled");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), async {
        while dropped.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("inner async reader must not detach and retain its I/O");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert!(receive_all(&mut output).await.is_empty());
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary output observer abandoned")
    );
}

#[tokio::test]
async fn failed_wait_does_not_publish_minus_one_or_exited_status() {
    let lifecycle = Lifecycle::new();
    let (exit, receiver) = oneshot::channel();
    let exited = Arc::new(AtomicBool::new(false));
    let code = Arc::new(Mutex::new(None));
    publish_wait(
        Err(io::Error::other("wait failed")),
        lifecycle.wait_witness(exit),
        &exited,
        &code,
    );
    assert!(receiver.await.is_err());
    assert!(!exited.load(Ordering::SeqCst));
    assert_eq!(*code.lock().unwrap(), None);
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary process wait failed")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_wait_latches_before_exit_receiver_cancels() {
    let lifecycle = Lifecycle::new();
    let (exit, receiver) = oneshot::channel();
    let witness = lifecycle.wait_witness(exit);
    let task = tokio::spawn(async move {
        let _witness = witness;
        std::future::pending::<()>().await;
    });
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(receiver.await.is_err());
    assert_eq!(
        lifecycle.receiver().borrow().as_deref(),
        Some("ordinary process wait abandoned")
    );
}

#[tokio::test]
async fn successful_wait_publishes_actual_supplied_witness_once() {
    let lifecycle = Lifecycle::new();
    let (exit, receiver) = oneshot::channel();
    let exited = Arc::new(AtomicBool::new(false));
    let code = Arc::new(Mutex::new(None));
    publish_wait(Ok(23), lifecycle.wait_witness(exit), &exited, &code);
    assert_eq!(receiver.await.unwrap(), 23);
    assert!(exited.load(Ordering::SeqCst));
    assert_eq!(*code.lock().unwrap(), Some(23));
    assert_eq!(*lifecycle.receiver().borrow(), None);
    assert!(lifecycle.receiver().has_changed().is_ok());
}
