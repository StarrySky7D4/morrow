use std::io;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::watch;

#[derive(Clone, Debug)]
enum Completion {
    Pending,
    Closed,
    Failed(io::ErrorKind, String),
}

/// One sticky request and the actual writer's terminal acknowledgement.
#[derive(Clone)]
pub(crate) struct PipeInputClose {
    request: watch::Sender<bool>,
    completion: watch::Receiver<Completion>,
}

pub(crate) struct PipeWriterControl {
    request: watch::Receiver<bool>,
    completion: CompletionGuard,
}

struct CompletionGuard {
    sender: watch::Sender<Completion>,
    finished: bool,
}

impl PipeInputClose {
    pub(crate) fn channel() -> (Self, PipeWriterControl) {
        let (request, request_rx) = watch::channel(false);
        let (completion_tx, completion) = watch::channel(Completion::Pending);
        (
            Self {
                request,
                completion,
            },
            PipeWriterControl {
                request: request_rx,
                completion: CompletionGuard {
                    sender: completion_tx,
                    finished: false,
                },
            },
        )
    }

    pub(crate) fn request(&self) {
        self.request.send_if_modified(|requested| {
            if *requested {
                false
            } else {
                *requested = true;
                true
            }
        });
    }

    pub(crate) fn requested(&self) -> bool {
        *self.request.borrow()
    }

    pub(crate) async fn wait_closed(&self) -> io::Result<()> {
        let mut completion = self.completion.clone();
        loop {
            let observed = completion.borrow_and_update().clone();
            match observed {
                Completion::Closed => return Ok(()),
                Completion::Failed(kind, detail) => return Err(io::Error::new(kind, detail)),
                Completion::Pending => {}
            }
            completion.changed().await.map_err(|_| {
                io::Error::other("stdin writer lost its completion acknowledgement")
            })?;
        }
    }
}

impl CompletionGuard {
    fn finish(mut self, result: io::Result<()>) {
        let result = match result {
            Ok(()) => Completion::Closed,
            Err(error) => Completion::Failed(error.kind(), error.to_string()),
        };
        self.sender.send_replace(result);
        self.finished = true;
    }
}

impl Drop for CompletionGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.sender.send_replace(Completion::Failed(
                io::ErrorKind::Other,
                "stdin writer ended before a confirmed close".to_string(),
            ));
        }
    }
}

/// Drain already acquired permits as well as queued input before closing the
/// actual writer. Cloned senders do not prevent a receiver-driven close.
/// Cancellation of a waiter never cancels this writer or resets the request.
pub(crate) async fn run_pipe_writer<W>(
    mut writer: W,
    mut receiver: mpsc::Receiver<Vec<u8>>,
    mut control: PipeWriterControl,
) where
    W: AsyncWrite + Unpin,
{
    let mut closing = false;
    let result = async {
        loop {
            if !closing && *control.request.borrow() {
                receiver.close();
                closing = true;
            }
            let bytes = if closing {
                receiver.recv().await
            } else {
                tokio::select! {
                    biased;
                    _ = control.request.changed() => {
                        receiver.close();
                        closing = true;
                        continue;
                    }
                    bytes = receiver.recv() => bytes,
                }
            };
            let Some(bytes) = bytes else {
                break;
            };
            writer.write_all(&bytes).await?;
            writer.flush().await?;
        }
        writer.flush().await?;
        writer.shutdown().await
    }
    .await;

    // No acknowledged close precedes the destruction of the actual ChildStdin.
    // On failure, also close the receiver so no new reservations can succeed.
    drop(receiver);
    drop(writer);
    control.completion.finish(result);
}

#[cfg(test)]
#[path = "checked_control_tests.rs"]
mod tests;
