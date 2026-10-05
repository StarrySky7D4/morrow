//! Bounded synthetic typed-SDK peer; never a public server or credential source.
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{self, Message};
pub const TEXT: &str = "C04 雪 🙂";
pub const BINARY: &[u8] = &[0, 255, 128, 10, 13, 0, 37];
pub const CONTROL: &[u8] = b"sdk-control";
pub const WAIT: Duration = Duration::from_secs(8);
pub struct Seen {
    pub handshakes: AtomicUsize,
    pub messages: Mutex<Vec<(u8, Vec<u8>)>>,
    pub auto_pongs: AtomicUsize,
    pub closed: AtomicBool,
}
pub struct Peer {
    pub origin: String,
    pub seen: Arc<Seen>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Peer {
    pub fn new(hold: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Seen {
            handshakes: AtomicUsize::new(0),
            messages: Mutex::new(Vec::new()),
            auto_pongs: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let observed = seen.clone();
        let signal = stop.clone();
        let thread = thread::spawn(move || {
            let end = Instant::now() + WAIT;
            while !signal.load(Ordering::SeqCst) && Instant::now() < end {
                let (socket, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("loopback accept {e}"),
                };
                // Windows accepted sockets inherit the nonblocking listener mode.
                // The synchronous backend handshake must use a bounded blocking socket.
                socket.set_nonblocking(false).unwrap();
                socket.set_read_timeout(Some(WAIT)).unwrap();
                socket.set_write_timeout(Some(WAIT)).unwrap();
                let mut ws = match tungstenite::accept(socket) {
                    Ok(ws) => ws,
                    Err(error) => {
                        eprintln!("C04 synthetic peer handshake first error: {error:?}");
                        continue;
                    }
                };
                assert!(observed.handshakes.fetch_add(1, Ordering::SeqCst) < 2);
                ws.get_mut()
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
                let mut pongs = 0;
                while !signal.load(Ordering::SeqCst) && Instant::now() < end {
                    match ws.read() {
                        Ok(Message::Text(text)) => {
                            record(&observed, 1, text.as_bytes());
                            if !hold && ws.send(Message::Text(text)).is_err() {
                                break;
                            }
                        }
                        Ok(Message::Binary(bytes)) => {
                            record(&observed, 2, &bytes);
                            if !hold && ws.send(Message::Binary(bytes)).is_err() {
                                break;
                            }
                        }
                        Ok(Message::Ping(bytes)) => {
                            record(&observed, 9, &bytes);
                            assert_eq!(bytes.as_ref(), CONTROL);
                            if ws.flush().is_err() {
                                break;
                            }
                        }
                        Ok(Message::Pong(bytes)) => {
                            record(&observed, 10, &bytes);
                            assert_eq!(bytes.as_ref(), CONTROL);
                            pongs += 1;
                            if pongs == 1 {
                                if ws.send(Message::Ping(CONTROL.to_vec().into())).is_err() {
                                    break;
                                }
                            } else {
                                observed.auto_pongs.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        Ok(Message::Close(close)) => {
                            let close = close.expect("guest requests exact close code");
                            assert_eq!(u16::from(close.code), 1000);
                            assert!(close.reason.is_empty());
                            record(&observed, 8, &1000u16.to_be_bytes());
                            let _ = ws.flush();
                            observed.closed.store(true, Ordering::SeqCst);
                            break;
                        }
                        Ok(Message::Frame(_)) => panic!("backend exposed raw frame"),
                        Err(tungstenite::Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) =>
                        {
                            continue;
                        }
                        Err(_) => break,
                    }
                }
            }
        });
        Self {
            origin: format!("ws://{address}"),
            seen,
            stop,
            thread: Some(thread),
        }
    }
    pub fn finish(mut self) -> Arc<Seen> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect("actual loopback peer join");
        self.seen.clone()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
fn record(seen: &Seen, opcode: u8, payload: &[u8]) {
    assert!(payload.len() <= 65536);
    let mut all = seen.messages.lock().unwrap();
    assert!(all.len() < 16);
    all.push((opcode, payload.to_vec()));
}
pub fn until(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !f() {
        assert!(Instant::now() < end, "bounded observed SDK condition");
        thread::sleep(Duration::from_millis(1));
    }
}
