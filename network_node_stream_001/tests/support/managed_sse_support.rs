//! Disposable loopback HTTP server. Every accepted connection and server thread is joined.
use std::{
    io::{Read, Write}, net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const WAIT: Duration = Duration::from_secs(5);
pub const HEAD: &str = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/event-stream; charset=utf-8\r\n\r\n";
#[derive(Clone, Default)]
pub struct Observations {
    pub requests: Arc<Mutex<Vec<Vec<u8>>>>,
    pub eof_written: Arc<AtomicBool>,
    pub release_eof: Arc<AtomicBool>,
}
impl Observations {
    pub fn posts(&self) -> usize { self.requests.lock().unwrap().len() }
    pub fn release(&self) { self.release_eof.store(true, Ordering::SeqCst); }
}
pub struct Server {
    pub origin: String,
    pub observed: Observations,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}
impl Server {
    pub fn new(head: &str, parts: Vec<Vec<u8>>, hold_eof: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let observed = Observations::default();
        observed.release_eof.store(!hold_eof, Ordering::SeqCst);
        let o = observed.clone();
        let s = stop.clone();
        let head = head.as_bytes().to_vec();
        let handle = thread::spawn(move || {
            let until = Instant::now() + WAIT;
            let mut children = Vec::new();
            while !s.load(Ordering::SeqCst) && Instant::now() < until {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        assert!(children.len() < 8, "unexpected repeated POST");
                        let o = o.clone(); let s = s.clone();
                        let head = head.clone(); let parts = parts.clone();
                        children.push(thread::spawn(move || {
                            socket.set_nonblocking(false).unwrap();
                            socket.set_read_timeout(Some(WAIT)).unwrap();
                            socket.set_write_timeout(Some(WAIT)).unwrap();
                            let request = read_request(&mut socket);
                            o.requests.lock().unwrap().push(request);
                            if socket.write_all(&head).is_err() { return; }
                            for part in parts {
                                assert!(!part.is_empty(), "zero-sized HTTP chunk is EOF");
                                if socket.write_all(format!("{:x}\r\n", part.len()).as_bytes()).is_err()
                                    || socket.write_all(&part).is_err()
                                    || socket.write_all(b"\r\n").is_err() { return; }
                            }
                            let until = Instant::now() + WAIT;
                            while !o.release_eof.load(Ordering::SeqCst) && !s.load(Ordering::SeqCst)
                                && Instant::now() < until { thread::sleep(Duration::from_millis(1)); }
                            if o.release_eof.load(Ordering::SeqCst)
                                && socket.write_all(b"0\r\n\r\n").is_ok() {
                                o.eof_written.store(true, Ordering::SeqCst);
                            }
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(1)),
                    Err(e) => panic!("loopback accept: {e}"),
                }
            }
            for child in children { child.join().expect("actual loopback connection join"); }
        });
        Self { origin, observed, stop, handle: Some(handle) }
    }
    pub fn finish(mut self) -> Observations {
        // Let a queued second connection be observed before stopping the listener.
        thread::sleep(Duration::from_millis(20));
        self.stop.store(true, Ordering::SeqCst);
        self.handle.take().unwrap().join().expect("actual server join");
        self.observed.clone()
    }
}
fn read_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new(); let mut block = [0; 1024];
    loop {
        let n = socket.read(&mut block).expect("bounded synthetic POST read");
        assert!(n > 0, "POST ended before its declared body");
        bytes.extend_from_slice(&block[..n]); assert!(bytes.len() <= 16384);
        if let Some(start) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let head = std::str::from_utf8(&bytes[..start]).unwrap();
            assert!(head.starts_with("POST /synthetic-events HTTP/1.1"));
            let length = head.lines().filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap()).unwrap_or(0);
            if bytes.len() >= start + 4 + length {
                assert_eq!(&bytes[start + 4..start + 4 + length], b"{\"synthetic\":true}");
                return bytes;
            }
        }
    }
}
pub fn until(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !predicate() { assert!(Instant::now() < end, "bounded observable condition"); thread::sleep(Duration::from_millis(1)); }
}
// Failure cleanup is not counted as qualification. Successful tests call finish explicitly.
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() { let _ = handle.join(); }
    }
}
