#[path = "checked_runner_controls.rs"]
mod checked_runner_controls;
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use checked_runner_controls::start_checked_runner;
pub(crate) use checked_runner_controls::start_checked_runner_with_io;

use crate::ipc_framed::EmptyPayload;
use crate::ipc_framed::FramedMessage;
use crate::ipc_framed::IPC_PROTOCOL_VERSION;
use crate::ipc_framed::Message;
use crate::ipc_framed::OutputStream;
use crate::ipc_framed::ResizePayload;
use crate::ipc_framed::StdinPayload;
use crate::ipc_framed::decode_bytes;
use crate::ipc_framed::encode_bytes;
use anyhow::Result;
use codex_utils_pty::ProcessDriver;
use codex_utils_pty::SpawnedProcess;
use codex_utils_pty::TerminalSize;
use codex_utils_pty::WindowsTtyInputNormalizer;
use codex_utils_pty::spawn_from_driver;
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub(crate) fn finish_driver_spawn(driver: ProcessDriver, stdin_open: bool) -> SpawnedProcess {
    let spawned = spawn_from_driver(driver);
    if !stdin_open {
        spawned.session.close_stdin();
    }
    spawned
}

/// Legacy v6 outbound admission is shared by retained controls, but only the
/// original writer owns the OS pipe. Terminal retirement revokes every clone.
#[derive(Clone)]
pub(crate) struct RunnerOutbound {
    inner: Arc<RunnerOutboundInner>,
}

struct RunnerOutboundInner {
    sender: Mutex<Option<std::sync::mpsc::Sender<FramedMessage>>>,
    retired: AtomicBool,
    retirement: watch::Sender<bool>,
}

impl RunnerOutbound {
    pub(crate) fn send(&self, message: FramedMessage)
        -> std::result::Result<(), std::sync::mpsc::SendError<FramedMessage>> {
        let admission = self.inner.sender.lock().unwrap_or_else(|error| error.into_inner());
        match admission.as_ref() {
            Some(sender) => sender.send(message),
            None => Err(std::sync::mpsc::SendError(message)),
        }
    }

    fn retire(&self) {
        let sender = {
            let mut admission = self.inner.sender.lock().unwrap_or_else(|error| error.into_inner());
            self.inner.retired.store(true, Ordering::SeqCst);
            admission.take()
        };
        // Wake both an idle native recv and an idle async stdin recv. Neither
        // depends on retained session/control sender clones being dropped.
        drop(sender);
        self.inner.retirement.send_replace(true);
    }

    fn retired(&self) -> bool { self.inner.retired.load(Ordering::SeqCst) }
}

/// Same original native writer through terminal completion and reader unwind.
/// No admission mutex is held across write/flush or the actual JoinHandle::join.
pub(crate) struct RunnerPipeWriterOwner {
    outbound: RunnerOutbound,
    original: Option<std::thread::JoinHandle<io::Result<()>>>,
}

impl RunnerPipeWriterOwner {
    fn retire_and_join(&mut self) -> io::Result<()> {
        self.outbound.retire();
        if let Some(original) = self.original.as_ref() {
            let thread = original.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
            // Revoke in-flight IO as well as queued messages. Cancellation targets
            // this original thread, never a process or preserved descendant. Repeat
            // across the check/write race until native thread teardown is observed.
            loop {
                let wait = unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(thread, 10) };
                if wait == windows_sys::Win32::Foundation::WAIT_OBJECT_0 { break; }
                unsafe { windows_sys::Win32::System::IO::CancelSynchronousIo(thread); }
                if wait != windows_sys::Win32::Foundation::WAIT_TIMEOUT {
                    // Failed observation cannot authorize native completion or
                    // dropping the original. Keep this one owner charged.
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
        if let Some(original) = self.original.take() {
            original.join().map_err(|_| io::Error::other("legacy pipe writer panicked"))??;
        }
        Ok(())
    }
}

impl Drop for RunnerPipeWriterOwner {
    fn drop(&mut self) { let _ = self.retire_and_join(); }
}

pub(crate) fn start_runner_pipe_writer(mut pipe_write: File) -> (RunnerOutbound, RunnerPipeWriterOwner) {
    let (sender, receiver) = std::sync::mpsc::channel::<FramedMessage>();
    let (retirement, _) = watch::channel(false);
    let outbound = RunnerOutbound { inner: Arc::new(RunnerOutboundInner {
        sender: Mutex::new(Some(sender)), retired: AtomicBool::new(false), retirement,
    }) };
    let writer_outbound = outbound.clone();
    let original = std::thread::spawn(move || {
        let result: io::Result<()> = (|| {
            while let Ok(message) = receiver.recv() {
                // A terminal reader revokes queued stdin/control frames too. In
                // particular, never send a queued Terminate against preserved descendants.
                if writer_outbound.retired() { break; }
                if let Err(error) = crate::ipc_framed::write_frame(&mut pipe_write, &message) {
                    if writer_outbound.retired() { break; }
                    return Err(io::Error::other(error.to_string()));
                }
            }
            Ok(())
        })();
        drop(pipe_write); // The real host input endpoint closes before native join completes.
        writer_outbound.retire();
        result
    });
    let owner = RunnerPipeWriterOwner { outbound: outbound.clone(), original: Some(original) };
    (outbound, owner)
}

pub(crate) fn start_runner_stdin_writer(
    mut writer_rx: mpsc::Receiver<Vec<u8>>,
    outbound_tx: RunnerOutbound,
    normalize_newlines: bool,
    stdin_open: bool,
) -> tokio::task::JoinHandle<()> {
    let mut retirement = outbound_tx.inner.retirement.subscribe();
    tokio::spawn(async move {
        let mut windows_input = WindowsTtyInputNormalizer::default();
        loop {
            if *retirement.borrow() { break; }
            let bytes = tokio::select! {
                biased;
                _ = retirement.changed() => continue,
                bytes = writer_rx.recv() => match bytes { Some(bytes) => bytes, None => break },
            };
            let bytes = if normalize_newlines { windows_input.normalize(&bytes) } else { bytes };
            if outbound_tx.send(FramedMessage {
                version: IPC_PROTOCOL_VERSION,
                message: Message::Stdin { payload: StdinPayload { data_b64: encode_bytes(&bytes) } },
            }).is_err() { break; }
        }
        // CloseStdin remains a child-stdin request, not a transport EOF substitute.
        // A terminal retirement sends no further wire frame and closes writer_rx.
        if stdin_open && !outbound_tx.retired() {
            let _ = outbound_tx.send(FramedMessage { version: IPC_PROTOCOL_VERSION,
                message: Message::CloseStdin { payload: EmptyPayload::default() } });
        }
    })
}

pub(crate) fn start_runner_stdout_reader(
    mut pipe_read: File,
    mut input_owner: RunnerPipeWriterOwner,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let exit = loop {
            let msg = match crate::ipc_framed::read_frame(&mut pipe_read) {
                Ok(Some(v)) => v,
                Ok(None) => {
                    send_runner_error(
                        "runner pipe closed before exit",
                        &stdout_tx,
                        stderr_tx.as_ref(),
                    );
                    break None;
                }
                Err(err) => {
                    send_runner_error(
                        &format!("runner read failed: {err}"),
                        &stdout_tx,
                        stderr_tx.as_ref(),
                    );
                    break None;
                }
            };

            if msg.version != IPC_PROTOCOL_VERSION {
                send_runner_error(
                    "runner output protocol mismatch",
                    &stdout_tx,
                    stderr_tx.as_ref(),
                );
                break None;
            }
            match msg.message {
                Message::Hello { .. }
                | Message::HelloAck { .. }
                | Message::ControlRequest { .. }
                | Message::ControlResult { .. } => {
                    send_runner_error(
                        "unexpected checked-control result on unchecked driver",
                        &stdout_tx,
                        stderr_tx.as_ref(),
                    );
                    break None;
                }
                Message::Output { payload } => {
                    if let Ok(data) = decode_bytes(&payload.data_b64) {
                        match payload.stream {
                            OutputStream::Stdout => {
                                let _ = stdout_tx.send(data);
                            }
                            OutputStream::Stderr => {
                                if let Some(stderr_tx) = stderr_tx.as_ref() {
                                    let _ = stderr_tx.send(data);
                                } else {
                                    let _ = stdout_tx.send(data);
                                }
                            }
                        }
                    }
                }
                Message::Exit { payload } => {
                    break Some((payload.exit_code, payload.timed_out));
                }
                Message::Error { payload } => {
                    send_runner_error(&payload.message, &stdout_tx, stderr_tx.as_ref());
                    break None;
                }
                Message::SpawnReady { .. }
                | Message::Stdin { .. }
                | Message::CloseStdin { .. }
                | Message::Resize { .. }
                | Message::SpawnRequest { .. }
                | Message::Terminate { .. } => {}
            }
        };
        // Real terminal (and error/EOF) observation retires this original endpoint
        // before publishing completion. The guest's actual input loop then sees EOF,
        // joins normally and can return its complete 32-bit native status.
        if let Err(error) = input_owner.retire_and_join() {
            send_runner_error(&format!("runner input retirement failed: {error}"),
                &stdout_tx, stderr_tx.as_ref());
        }
        crate::elevated::runner_metrics::record_command(exit);
        let _ = exit_tx.send(exit.map_or(-1, |(code, _)| code));
    })
}

/// Observe the actual originals without making completion depend on session Drop.
/// The native join is owned by the blocking observer even if the outer async
/// observer is cancelled; no live native JoinHandle is discarded by cancellation.
pub(crate) fn observe_runner_workers(
    stdin_task: tokio::task::JoinHandle<()>,
    reader: std::thread::JoinHandle<()>,
) -> tokio::task::JoinHandle<()> {
    let reader_join = tokio::task::spawn_blocking(move || reader.join());
    tokio::spawn(async move {
        let _ = stdin_task.await;
        let _ = reader_join.await;
    })
}

pub(crate) fn make_runner_resizer(
    outbound_tx: RunnerOutbound,
) -> Box<dyn FnMut(TerminalSize) -> Result<()> + Send> {
    Box::new(move |size: TerminalSize| {
        outbound_tx
            .send(FramedMessage {
                version: IPC_PROTOCOL_VERSION,
                message: Message::Resize {
                    payload: ResizePayload {
                        rows: size.rows,
                        cols: size.cols,
                    },
                },
            })
            .map_err(|_| anyhow::anyhow!("runner resize pipe closed"))
    })
}

fn send_runner_error(
    message: &str,
    stdout_tx: &broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<&broadcast::Sender<Vec<u8>>>,
) {
    let formatted = format!("runner error: {message}\n").into_bytes();
    if let Some(stderr_tx) = stderr_tx {
        let _ = stderr_tx.send(formatted);
    } else {
        let _ = stdout_tx.send(formatted);
    }
}

#[cfg(test)]
#[path = "windows_common_tests.rs"]
mod tests;
