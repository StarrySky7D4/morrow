//! Stdio writes report bytes only after the native overlapped operation completes.

use std::{
    future::Future,
    io,
    os::windows::io::{IntoRawHandle, OwnedHandle},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::AsyncWrite;

use super::{Kind, MAX_IO, Pipe};

/// A retained observation token. True means native writes were reaped and the
/// parent stdin handle was successfully closed; failure or leakage remains false.
#[derive(Clone)]
pub struct JobStdinProof {
    confirmed: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

impl JobStdinProof {
    pub fn closed_and_reaped(&self) -> bool {
        self.confirmed.load(Ordering::Acquire)
    }

    /// True after the owned reclaim attempt has ended. A finished attempt can
    /// remain unconfirmed; callers must also require closed_and_reaped().
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

struct NativeWrite {
    bytes: Vec<u8>,
    completion: Option<(usize, Option<u32>)>,
}

/// Owned synchronous-child stdin's overlapped parent end. Cancelled write futures
/// leave their operation owned here until completion or nonblocking cancellation/reap.
/// Drop retains the real Pipe if reap is unconfirmed. An offset of zero is not
/// evidence that no native write was issued.
pub struct JobStdin {
    pipe: Option<Pipe>,
    pending: Option<NativeWrite>,
    wake: Option<Pin<Box<tokio::time::Sleep>>>,
    closed: bool,
    proof: JobStdinProof,
    retain_on_drop: bool,
}

impl JobStdin {
    pub(crate) fn new(handle: OwnedHandle) -> Self {
        Self {
            pipe: Some(Pipe::from_connected_handle(handle)),
            pending: None,
            wake: None,
            closed: false,
            proof: JobStdinProof {
                confirmed: Arc::new(AtomicBool::new(false)),
                finished: Arc::new(AtomicBool::new(false)),
            },
            retain_on_drop: false,
        }
    }

    pub fn proof(&self) -> JobStdinProof {
        self.proof.clone()
    }

    /// Consume the actual writer into a bounded cancellation/reap owner. This
    /// never flushes or reports an aborted write as successful control delivery.
    /// On expiry/error the original Pipe storage is retained and proof is false.
    pub fn reclaim_bounded(mut self, timeout: Duration) -> JobStdinProof {
        let proof = self.proof();
        // The reaper needs no Tokio timer or runtime. Every native operation's
        // stable storage remains owned by Pipe throughout the thread handoff.
        self.wake = None;
        let started = std::time::Instant::now();
        let _ = std::thread::Builder::new()
            .name("morrow-control-stdin-reaper".into())
            .spawn(move || {
                let mut attempted = false;
                loop {
                    if attempted && started.elapsed() >= timeout {
                        self.retain_on_drop = true;
                        break;
                    }
                    attempted = true;
                    match self.close_confirmed() {
                        Ok(()) => break,
                        Err(error)
                            if error.kind() == io::ErrorKind::WouldBlock
                                && started.elapsed() < timeout =>
                        {
                            std::thread::sleep(
                                Duration::from_millis(1)
                                    .min(timeout.saturating_sub(started.elapsed())),
                            );
                        }
                        Err(_) => {
                            // Do not retry beyond the budget in Drop or allow
                            // Pipe's blocking Drop to free kernel-owned storage.
                            self.retain_on_drop = true;
                            break;
                        }
                    }
                }
                drop(self);
            });
        // If spawning fails, Rust drops the closure and its owned writer here:
        // the ordinary one-shot nonblocking Drop sets finished as well.
        proof
    }

    fn close_confirmed(&mut self) -> io::Result<()> {
        if self.proof.closed_and_reaped() {
            return Ok(());
        }
        let pipe = self
            .pipe
            .as_mut()
            .ok_or_else(|| io::Error::other("stdin handle close was not confirmed"))?;
        pipe.cancel_and_reap_nonblocking()?;
        let handle = self.pipe.take().unwrap().into_owned_handle()?;
        let handle = handle.into_raw_handle();
        // SAFETY: all operations were actually reaped and the unique handle is
        // transferred here for an explicit fallible close. A close error retains
        // its kernel handle rather than fabricating a successful close observation.
        if unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.closed = true;
        self.pending = None;
        self.wake = None;
        self.proof.confirmed.store(true, Ordering::Release);
        Ok(())
    }

    /// Includes an issued write whose completion has not yet been returned to its
    /// poll_write caller, even if a flush separately observed native completion.
    pub fn write_unreported(&self) -> bool {
        self.pending.is_some()
    }

    fn completion_result(&self) -> io::Result<()> {
        if let Some(pending) = &self.pending {
            if let Some((transferred, error)) = pending.completion {
                if let Some(error) = error {
                    return Err(io::Error::from_raw_os_error(error as i32));
                }
                if transferred > pending.bytes.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "native write exceeded issued bytes",
                    ));
                }
                if transferred == 0 {
                    return Err(io::ErrorKind::WriteZero.into());
                }
            }
        }
        Ok(())
    }

    fn poll_native(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let Some(pending) = self.pending.as_ref() else {
            return Poll::Ready(Ok(()));
        };
        if pending.completion.is_some() {
            return Poll::Ready(self.completion_result());
        }
        let Some(pipe) = self.pipe.as_mut() else {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        };
        match pipe.poll(Kind::Write) {
            Ok(Some(completed)) => {
                self.pending.as_mut().unwrap().completion =
                    Some((completed.transferred, completed.error));
                self.wake = None;
                Poll::Ready(self.completion_result())
            }
            Err(error) => Poll::Ready(Err(error)),
            Ok(None) => self.poll_again(cx),
        }
    }

    fn poll_again(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let wake = self
            .wake
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(Duration::from_millis(1))));
        if wake.as_mut().poll(cx).is_ready() {
            self.wake = None;
            cx.waker().wake_by_ref();
        }
        Poll::Pending
    }
}

impl Drop for JobStdin {
    fn drop(&mut self) {
        if self.retain_on_drop || self.close_confirmed().is_err() {
            // Preserve the actual owned handle, OVERLAPPED/event and kernel-referenced
            // buffers. Never invoke Pipe's blocking Drop for unconfirmed cancellation.
            if let Some(pipe) = self.pipe.take() {
                std::mem::forget(pipe);
            }
        }
        self.proof.finished.store(true, Ordering::Release);
    }
}

impl AsyncWrite for JobStdin {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.closed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        if let Some(pending) = &this.pending {
            if !bytes.starts_with(&pending.bytes) {
                // Never attribute a cancelled earlier write to a different frame.
                // Keep the original operation/storage for cancellation and reap.
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "a previous native stdin write is still unreported",
                )));
            }
        } else {
            if bytes.is_empty() {
                return Poll::Ready(Ok(0));
            }
            let owned = bytes[..bytes.len().min(MAX_IO)].to_vec();
            // begin_write owns a second stable buffer; issuance occurs only when
            // the caller polls this method, allowing the controller gate to cover it.
            if let Err(error) = this.pipe.as_mut().unwrap().begin_write(owned.clone()) {
                return Poll::Ready(Err(error));
            }
            this.pending = Some(NativeWrite {
                bytes: owned,
                completion: None,
            });
        }
        match this.poll_native(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                let pending = this.pending.take().unwrap();
                Poll::Ready(Ok(pending.completion.unwrap().0))
            }
            Poll::Ready(Err(error)) => {
                if this
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.completion.is_some())
                {
                    this.pending = None;
                    this.wake = None;
                }
                Poll::Ready(Err(error))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().poll_native(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.poll_native(cx) {
            Poll::Ready(Ok(())) => match this.close_confirmed() {
                Ok(()) => Poll::Ready(Ok(())),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => this.poll_again(cx),
                Err(error) => Poll::Ready(Err(error)),
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn real_pipe_pending_write_reaps_before_reporting_and_shutdown() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server, client) = crate::job_process::stdio_pipe(true).expect("create real stdio pipe");
            let mut writer = JobStdin::new(server);
            let proof = writer.proof();
            assert!(!proof.closed_and_reaped());
            let payload = vec![0x7du8; MAX_IO];
            // No reader drains the 4096-byte pipe yet, so this real 32772-byte
            // overlapped write cannot be reported as fully accepted by the OS.
            let first = std::future::poll_fn(|cx| {
                Poll::Ready(Pin::new(&mut writer).poll_write(cx, &payload))
            }).await;
            assert!(first.is_pending(), "full pipe write must retain its native operation");
            assert!(writer.write_unreported());
            let flush = std::future::poll_fn(|cx| {
                Poll::Ready(Pin::new(&mut writer).poll_flush(cx))
            }).await;
            assert!(flush.is_pending(), "flush cannot confirm an incomplete native write");
            // Simulate cancellation of that write future followed by a different
            // control frame. It must not inherit the first frame's completion.
            let different = std::future::poll_fn(|cx| {
                Poll::Ready(Pin::new(&mut writer).poll_write(cx, b"different frame"))
            }).await;
            assert!(matches!(different, Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::InvalidInput));
            assert!(writer.write_unreported());
            let reader = std::thread::spawn(move || {
                let mut file = std::fs::File::from(client);
                let mut received = vec![0u8; MAX_IO];
                file.read_exact(&mut received).expect("drain real child stdio end");
                let eof = file.read(&mut [0u8; 1]).expect("observe EOF after native shutdown");
                (received, eof)
            });
            let written = tokio::time::timeout(Duration::from_secs(2), writer.write(&payload))
                .await.expect("native completion deadline").expect("native write completed");
            assert_eq!(written, payload.len());
            assert!(!writer.write_unreported());
            writer.flush().await.expect("flush actual completed write");
            writer.shutdown().await.expect("close only after native write reap");
            assert!(proof.closed_and_reaped());
            let (received, eof) = reader.join().expect("join child stdio reader");
            assert_eq!(received, payload);
            assert_eq!(eof, 0);
        });
    }

    #[test]
    fn real_pipe_closed_peer_is_an_error_instead_of_accepted_bytes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server, client) =
                crate::job_process::stdio_pipe(true).expect("create real stdio pipe");
            let mut writer = JobStdin::new(server);
            let proof = writer.proof();
            assert!(!proof.closed_and_reaped());
            drop(client);
            let result = tokio::time::timeout(Duration::from_secs(2), writer.write(b"closed peer"))
                .await
                .expect("closed peer completion deadline");
            assert!(
                result.is_err(),
                "failed native write must never report accepted bytes"
            );
            assert!(!writer.write_unreported());
            drop(writer);
            assert!(proof.closed_and_reaped());
        });
    }

    #[test]
    fn real_pending_write_drop_is_bounded_and_proof_matches_handle_state() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server, client) =
                crate::job_process::stdio_pipe(true).expect("create real stdio pipe");
            let original = crate::raw(&server);
            let mut writer = JobStdin::new(server);
            let proof = writer.proof();
            let payload = vec![0x63u8; MAX_IO];
            let first = std::future::poll_fn(|cx| {
                Poll::Ready(Pin::new(&mut writer).poll_write(cx, &payload))
            })
            .await;
            assert!(
                first.is_pending(),
                "real undrained pipe write must be pending"
            );
            assert!(!proof.closed_and_reaped());
            let started = std::time::Instant::now();
            drop(writer);
            assert!(proof.finished(), "ordinary Drop ends its reclaim attempt");
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "stdin Drop must never wait for native completion"
            );
            let mut flags = 0;
            // SAFETY: this API validates either a closed handle value or the actual
            // still-owned leaked Pipe handle; flags is writable DWORD storage.
            let open = unsafe {
                windows_sys::Win32::Foundation::GetHandleInformation(original, &mut flags)
            } != 0;
            if proof.closed_and_reaped() {
                assert!(
                    !open,
                    "confirmed proof requires the parent handle to be closed"
                );
                let mut file = std::fs::File::from(client);
                let mut received = Vec::new();
                file.read_to_end(&mut received)
                    .expect("closed server reaches EOF");
                assert!(payload.starts_with(&received));
            } else {
                assert!(
                    open,
                    "unconfirmed cancellation must retain the actual Pipe handle"
                );
                // Do not wait for EOF: retaining this server is the conservative
                // outcome until process termination, with proof deliberately false.
                drop(client);
            }
        });
    }

    #[test]
    fn real_pending_write_bounded_reclaim_cancels_reaps_and_closes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server, client) = crate::job_process::stdio_pipe(true).unwrap();
            let original = crate::raw(&server);
            let mut writer = JobStdin::new(server);
            let proof = writer.proof();
            assert!(!proof.finished());
            let payload = vec![0x2au8; MAX_IO];
            let first = std::future::poll_fn(|cx| {
                Poll::Ready(Pin::new(&mut writer).poll_write(cx, &payload))
            })
            .await;
            assert!(first.is_pending(), "real unread pipe write must be pending");
            assert!(writer.write_unreported());
            let started = std::time::Instant::now();
            let handed = writer.reclaim_bounded(Duration::from_secs(2));
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "handoff cannot wait for native IO"
            );
            tokio::time::timeout(Duration::from_secs(3), async {
                while !proof.finished() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("reaper finishes within its budget plus scheduling allowance");
            assert!(handed.finished());
            assert!(
                proof.closed_and_reaped(),
                "actual cancelled write must be reaped and server closed"
            );
            let mut flags = 0;
            // SAFETY: GetHandleInformation validates a former handle value and
            // writes DWORD flags without dereferencing a user-owned object.
            assert_eq!(
                unsafe {
                    windows_sys::Win32::Foundation::GetHandleInformation(original, &mut flags)
                },
                0,
                "confirmed reclamation closes the actual parent handle"
            );
            let mut file = std::fs::File::from(client);
            let mut received = Vec::new();
            file.read_to_end(&mut received)
                .expect("reaped parent reaches child EOF");
            assert!(payload.starts_with(&received));
            // Partial bytes or cancellation are resource proof, never an ACK.
        });
    }

    #[test]
    fn real_pending_write_zero_budget_finishes_with_truthful_handle_proof() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server, client) = crate::job_process::stdio_pipe(true).unwrap();
            let original = crate::raw(&server);
            let mut writer = JobStdin::new(server);
            let payload = vec![0x36u8; MAX_IO];
            assert!(
                std::future::poll_fn(|cx| {
                    Poll::Ready(Pin::new(&mut writer).poll_write(cx, &payload))
                })
                .await
                .is_pending()
            );
            let proof = writer.reclaim_bounded(Duration::ZERO);
            tokio::time::timeout(Duration::from_secs(2), async {
                while !proof.finished() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("zero budget reaper must stop");
            let mut flags = 0;
            // SAFETY: validates the actual retained or already closed handle.
            let open = unsafe {
                windows_sys::Win32::Foundation::GetHandleInformation(original, &mut flags)
            } != 0;
            assert_eq!(proof.closed_and_reaped(), !open);
            drop(client); // Never require EOF when actual Pipe remains retained.
        });
    }
}
