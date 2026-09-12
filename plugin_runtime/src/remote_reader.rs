//! Trusted reader process ownership. A reply, EOF or timeout is never proof of page release.
use crate::shared_objects::{Error, Mapping, Result, SharedObjects};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    shared_transfer::{MAX_FRAME_BYTES, Offer, Reply},
};
use std::{
    io::{self, Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
};
static NEXT_TRANSFER: AtomicU64 = AtomicU64::new(1);
struct Held {
    child: Child,
    mapping: Mapping,
}
/// Owns the exact process object and its mapping pin. The reader is trusted not to forward handles.
pub struct RemoteReader {
    held: Option<Held>,
    transfer: u64,
    reply: Option<Receiver<io::Result<Reply>>>,
    keep_open: Option<Sender<()>>,
    delivered: bool,
    exited: Option<ExitStatus>,
}
fn protocol() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid shared reader reply")
}
fn frame(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err(protocol());
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}
impl RemoteReader {
    pub(crate) fn spawn(
        command: &mut Command,
        mapping: Mapping,
        authorize: impl FnOnce(&Mapping) -> Result<()>,
    ) -> Result<Self> {
        if mapping.bytes().len() > morrow_core::shared_transfer::MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        let transfer = NEXT_TRANSFER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()?;
        // Install the ownership guard before duplication or any fallible handshake work.
        let mut reader = Self {
            held: Some(Held { child, mapping }),
            transfer,
            reply: None,
            keep_open: None,
            delivered: false,
            exited: None,
        };
        let held = reader.held.as_mut().expect("new reader");
        authorize(&held.mapping)?;
        let remote_handle = held.mapping.duplicate_readonly(&held.child)?;
        let offer = Offer {
            transfer,
            remote_handle,
            descriptor: held.mapping.descriptor().clone(),
        }
        .encode()?;
        let mut stdin = held.child.stdin.take().ok_or(Error::Denied)?;
        let mut stdout = held.child.stdout.take().ok_or(Error::Denied)?;
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let (keep_tx, keep_rx) = mpsc::channel();
        reader.reply = Some(reply_rx);
        reader.keep_open = Some(keep_tx);
        let write_tx = reply_tx.clone();
        thread::Builder::new()
            .name("morrow-reader-write".into())
            .spawn(move || {
                let result = (|| -> io::Result<()> {
                    stdin.write_all(&(offer.len() as u32).to_le_bytes())?;
                    stdin.write_all(&offer)?;
                    stdin.flush()
                })();
                if let Err(error) = result {
                    let _ = write_tx.try_send(Err(error));
                    return;
                }
                // Independent of reply reading: closing input also works before any reply.
                let _ = keep_rx.recv();
                drop(stdin);
            })?;
        thread::Builder::new()
            .name("morrow-reader-read".into())
            .spawn(move || {
                let result = frame(&mut stdout)
                    .and_then(|bytes| Reply::decode(&bytes).map_err(|_| protocol()));
                let _ = reply_tx.try_send(result);
            })?;
        Ok(reader)
    }
    /// Returns a validated raw snapshot once. This is not a content commit or release acknowledgement.
    pub fn receive(
        &mut self,
        objects: &mut SharedObjects,
        host: &HostRuntime,
        consumer: &Connection,
        mut clock: impl FnMut() -> u64,
    ) -> Result<Option<Vec<u8>>> {
        if self.delivered {
            return Err(Error::Denied);
        }
        let held = self.held.as_ref().ok_or(Error::Retired)?;
        objects.validate_mapping(host, consumer, &held.mapping, clock())?;
        let reply = match self.reply.as_ref().ok_or(Error::Denied)?.try_recv() {
            Ok(reply) => reply?,
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => return Err(Error::Denied),
        };
        if reply.transfer != self.transfer
            || &reply.descriptor != held.mapping.descriptor()
            || !reply.write_rejected
        {
            return Err(Error::Denied);
        }
        objects.validate_mapping(host, consumer, &held.mapping, clock())?;
        self.delivered = true;
        Ok(Some(reply.payload))
    }
    /// Close the trusted protocol normally; the pin remains until poll_exit confirms exit.
    pub fn close_input(&mut self) {
        self.keep_open.take();
    }
    /// Safety stop requests termination. Kill failure is not permission to release pages.
    pub fn stop(&mut self) -> io::Result<()> {
        self.close_input();
        match self.held.as_mut() {
            Some(held) => held.child.kill(),
            None => Ok(()),
        }
    }
    /// Observation failures retain the exact Child and Mapping. No PID reopening or remote handle close.
    pub fn poll_exit(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.exited.is_some() {
            return Ok(self.exited);
        }
        let Some(held) = self.held.as_mut() else {
            return Ok(None);
        };
        match held.child.try_wait()? {
            Some(status) => {
                self.exited = Some(status);
                self.held.take();
                self.keep_open.take();
                Ok(Some(status))
            }
            None => Ok(None),
        }
    }
}
impl Drop for RemoteReader {
    fn drop(&mut self) {
        self.keep_open.take();
        let Some(mut held) = self.held.take() else {
            return;
        };
        let _ = held.child.kill();
        if matches!(held.child.try_wait(), Ok(Some(_))) {
            return;
        }
        // Keep a separate owner while starting a reaper: failed spawn drops its closure.
        let retained = Arc::new(Mutex::new(Some(held)));
        let worker = Arc::clone(&retained);
        let started = thread::Builder::new()
            .name("morrow-reader-reap".into())
            .spawn(move || {
                let mut guard = worker.lock().unwrap_or_else(|e| e.into_inner());
                let confirmed = guard.as_mut().is_some_and(|h| h.child.wait().is_ok());
                if confirmed {
                    guard.take();
                }
                drop(guard);
                if !confirmed {
                    std::mem::forget(worker);
                } // Unknown outcome: retain pages, never recycle by assumption.
            });
        if started.is_err() {
            std::mem::forget(retained);
        } // No reaper exists; conservative retention until host exit.
    }
}
