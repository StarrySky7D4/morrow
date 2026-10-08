//! Ordinary Windows output/wait witnesses. A failed task never manufactures EOF.
use std::io;
use std::io::Read;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub(crate) struct Lifecycle {
    failure: watch::Sender<Option<String>>,
}

impl Lifecycle {
    pub(crate) fn new() -> Self {
        let (failure, _) = watch::channel(None);
        Self { failure }
    }

    pub(crate) fn receiver(&self) -> watch::Receiver<Option<String>> {
        self.failure.subscribe()
    }

    fn fail(&self, reason: &'static str) {
        self.failure.send_if_modified(|current| {
            if current.is_some() {
                false
            } else {
                *current = Some(reason.into());
                true
            }
        });
    }

    pub(crate) fn wait_witness(&self, exit: oneshot::Sender<i32>) -> Witness<oneshot::Sender<i32>> {
        Witness::new(self.clone(), exit, "ordinary process wait abandoned")
    }
}

/// The anchor is released only after a successful witness or after first-error publication.
/// Construct before spawning: cancellation before first poll must also publish failure.
pub(crate) struct Witness<T> {
    lifecycle: Lifecycle,
    anchor: Option<T>,
    abandoned: &'static str,
    completed: bool,
}

impl<T> Witness<T> {
    fn new(lifecycle: Lifecycle, anchor: T, abandoned: &'static str) -> Self {
        Self {
            lifecycle,
            anchor: Some(anchor),
            abandoned,
            completed: false,
        }
    }

    fn complete(mut self) -> T {
        self.completed = true;
        self.anchor.take().expect("witness owns its anchor")
    }
}

impl<T> Drop for Witness<T> {
    fn drop(&mut self) {
        if !self.completed {
            // Drop runs before Rust drops the anchor field, fencing channel closure.
            self.lifecycle.fail(self.abandoned);
        }
    }
}

// ReadFile's anonymous-pipe writer-close result is a real EOF. Cancellation (995),
// generic BrokenPipe without an OS witness, and writer errors are not output EOF.
fn read_eof(error: &io::Error) -> bool {
    error.raw_os_error() == Some(winapi::shared::winerror::ERROR_BROKEN_PIPE as i32)
}

async fn read_pipe<R: AsyncRead + Unpin>(
    mut reader: R,
    output: mpsc::Sender<Vec<u8>>,
) -> io::Result<()> {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => return Ok(()),
            Ok(n) => output
                .send(buf[..n].to_vec())
                .await
                .map_err(|_| io::Error::other("output receiver abandoned"))?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if read_eof(&error) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn read_pty<R: Read>(mut reader: R, output: mpsc::Sender<Vec<u8>>) -> io::Result<()> {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => output
                .blocking_send(buf[..n].to_vec())
                .map_err(|_| io::Error::other("output receiver abandoned"))?,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) if read_eof(&error) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

struct AbortReaderOnDrop(tokio::task::AbortHandle);

impl Drop for AbortReaderOnDrop {
    fn drop(&mut self) {
        // Dropping JoinHandle detaches. Keep the original async-reader cancellation
        // ownership instead. Started spawn_blocking readers still require real EOF.
        self.0.abort();
    }
}

async fn observe_reader(task: JoinHandle<io::Result<()>>, witness: Witness<mpsc::Sender<Vec<u8>>>) {
    let _abort_reader = AbortReaderOnDrop(task.abort_handle());
    match task.await {
        Ok(Ok(())) => {
            drop(witness.complete());
        }
        Ok(Err(_)) => witness.lifecycle.fail("ordinary output read failed"),
        Err(_) => witness.lifecycle.fail("ordinary output task failed"),
    }
    // On error the witness latches before releasing the last possible sender.
}

pub(crate) fn spawn_pipe_reader<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
    output: mpsc::Sender<Vec<u8>>,
    lifecycle: &Lifecycle,
) -> JoinHandle<()> {
    let witness = Witness::new(
        lifecycle.clone(),
        output.clone(),
        "ordinary output observer abandoned",
    );
    tokio::spawn(async move {
        let task = tokio::spawn(read_pipe(reader, output));
        observe_reader(task, witness).await;
    })
}

pub(crate) fn spawn_pty_reader<R: Read + Send + 'static>(
    reader: R,
    output: mpsc::Sender<Vec<u8>>,
    lifecycle: &Lifecycle,
) -> JoinHandle<()> {
    let witness = Witness::new(
        lifecycle.clone(),
        output.clone(),
        "ordinary output observer abandoned",
    );
    tokio::spawn(async move {
        let task = tokio::task::spawn_blocking(move || read_pty(reader, output));
        observe_reader(task, witness).await;
    })
}

pub(crate) fn publish_wait(
    result: io::Result<i32>,
    witness: Witness<oneshot::Sender<i32>>,
    exited: &Arc<AtomicBool>,
    exit_code: &Arc<Mutex<Option<i32>>>,
) {
    let code = match result {
        Ok(code) => code,
        Err(_) => {
            witness.lifecycle.fail("ordinary process wait failed");
            return;
        }
    };
    let Ok(mut stored) = exit_code.lock() else {
        witness
            .lifecycle
            .fail("ordinary process exit storage unavailable");
        return;
    };
    *stored = Some(code);
    exited.store(true, Ordering::SeqCst);
    let _ = witness.complete().send(code);
}

#[cfg(test)]
#[path = "windows_lifecycle_tests.rs"]
mod tests;
