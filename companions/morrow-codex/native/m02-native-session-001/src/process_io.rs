//! Process-scoped stdio workers. No parsing or authority lives here.
//!
//! A blocked OS pipe read/write never blocks the controlling main thread. The
//! protocol driver must enforce its deadline and return from main on failure.
//! Workers are not joined at exit: process termination closes their OS handles.
//! Dropping this value alone is NOT evidence that a blocked worker has finished.
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::thread;
use std::time::Duration;

static STDIO_CLAIMED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub enum ReadEvent {
    Chunk(Vec<u8>),
    Eof,
    Failed(io::ErrorKind),
}

/// Each direction has one bounded queue. At most one write may be outstanding.
pub struct ProcessIo {
    incoming: Receiver<ReadEvent>,
    outgoing: SyncSender<Vec<u8>>,
    completed: Receiver<Result<(), io::ErrorKind>>,
    maximum_write: usize,
    pending_write: bool,
    write_failed: bool,
}

impl ProcessIo {
    /// Claim protocol stdin/stdout once. A reconnect must not create another set
    /// of workers or imply renewal of a host session or authorization.
    pub fn claim_stdio(maximum_read_chunk: usize, maximum_write: usize) -> io::Result<Self> {
        if maximum_read_chunk == 0 || maximum_write == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "zero I/O bound",
            ));
        }
        if STDIO_CLAIMED.swap(true, Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "stdio already claimed",
            ));
        }
        Self::spawn(io::stdin(), io::stdout(), maximum_read_chunk, maximum_write)
    }

    fn spawn(
        mut reader: impl Read + Send + 'static,
        mut writer: impl Write + Send + 'static,
        maximum_read_chunk: usize,
        maximum_write: usize,
    ) -> io::Result<Self> {
        let (read_tx, incoming) = mpsc::sync_channel(1);
        let (outgoing, write_rx) = mpsc::sync_channel::<Vec<u8>>(1);
        let (done_tx, completed) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("session-pipe-read".into())
            .spawn(move || {
                loop {
                    let mut chunk = vec![0; maximum_read_chunk];
                    let event = match reader.read(&mut chunk) {
                        Ok(0) => ReadEvent::Eof,
                        Ok(count) => {
                            chunk.truncate(count);
                            ReadEvent::Chunk(chunk)
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => ReadEvent::Failed(error.kind()),
                    };
                    let terminal = !matches!(event, ReadEvent::Chunk(_));
                    if read_tx.send(event).is_err() || terminal {
                        return;
                    }
                }
            })?;
        thread::Builder::new()
            .name("session-pipe-write".into())
            .spawn(move || {
                while let Ok(bytes) = write_rx.recv() {
                    let result = writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .map_err(|e| e.kind());
                    let failed = result.is_err();
                    if done_tx.send(result).is_err() || failed {
                        return;
                    }
                }
            })?;
        Ok(Self {
            incoming,
            outgoing,
            completed,
            maximum_write,
            pending_write: false,
            write_failed: false,
        })
    }

    /// Does not block or replay. A write error has an unknown sent-byte extent.
    pub fn try_write(&mut self, bytes: Vec<u8>) -> Result<(), &'static str> {
        if self.write_failed {
            return Err("writer failed; replay forbidden");
        }
        if bytes.is_empty() || bytes.len() > self.maximum_write {
            return Err("invalid write byte bound");
        }
        if self.pending_write {
            return Err("one write already pending");
        }
        self.outgoing
            .try_send(bytes)
            .map_err(|_| "writer unavailable")?;
        self.pending_write = true;
        Ok(())
    }

    pub fn poll_write(&mut self) -> Result<Option<Result<(), io::ErrorKind>>, &'static str> {
        if !self.pending_write {
            return Ok(None);
        }
        match self.completed.try_recv() {
            Ok(result) => {
                self.pending_write = false;
                self.write_failed = result.is_err();
                Ok(Some(result))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.write_failed = true;
                Err("writer terminated without completion")
            }
        }
    }

    /// The protocol driver polls input before write completion so Stop/EOF can
    /// be processed while output is blocked. It supplies the remaining deadline.
    pub fn read_event(&self, maximum_wait: Duration) -> Result<ReadEvent, RecvTimeoutError> {
        self.incoming.recv_timeout(maximum_wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn fragmented_input_is_bounded_and_reports_eof() {
        let pipes = ProcessIo::spawn(Cursor::new(b"abcde"), io::sink(), 2, 4).unwrap();
        let mut chunks = vec![];
        loop {
            match pipes.read_event(Duration::from_secs(1)).unwrap() {
                ReadEvent::Chunk(bytes) => {
                    assert!(bytes.len() <= 2);
                    chunks.extend(bytes);
                }
                ReadEvent::Eof => break,
                ReadEvent::Failed(error) => panic!("{error:?}"),
            }
        }
        assert_eq!(chunks, b"abcde");
    }

    #[test]
    fn failed_write_is_reported_once_without_retry() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut pipes = ProcessIo::spawn(io::empty(), Broken, 2, 4).unwrap();
        assert!(pipes.try_write(vec![0; 5]).is_err());
        pipes.try_write(vec![1]).unwrap();
        assert!(pipes.try_write(vec![2]).is_err());
        let deadline = crate::Deadline::start(Duration::from_secs(1));
        loop {
            if let Some(result) = pipes.poll_write().unwrap() {
                assert_eq!(result, Err(io::ErrorKind::BrokenPipe));
                break;
            }
            assert!(deadline.remaining().is_some());
            thread::yield_now();
        }
        assert!(pipes.try_write(vec![3]).is_err());
    }

    #[test]
    fn blocked_output_does_not_block_control_input_observation() {
        struct BlockedWriter {
            entered: SyncSender<()>,
            release: Receiver<()>,
        }
        impl Write for BlockedWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let writer = BlockedWriter {
            entered: entered_tx,
            release: release_rx,
        };
        let mut pipes = ProcessIo::spawn(Cursor::new(b"control"), writer, 8, 8).unwrap();
        pipes.try_write(vec![1]).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(pipes.poll_write().unwrap(), None);
        let observed = pipes.read_event(Duration::from_secs(1));
        // Always release our test worker before an assertion can panic.
        release_tx.send(()).unwrap();
        assert_eq!(observed.unwrap(), ReadEvent::Chunk(b"control".to_vec()));
    }
}
