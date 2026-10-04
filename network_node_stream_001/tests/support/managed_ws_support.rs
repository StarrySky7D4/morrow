//! Synthetic loopback peers only. Successful paths explicitly join every socket thread.
//! Normal traffic uses the same mature backend API as a server; raw peers only inject faults.
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{self, Message};
pub const WAIT: Duration = Duration::from_secs(8);
pub const TEXT: &str = "W15 雪 🙂";
pub const BINARY: &[u8] = &[0, 255, 128, 10, 13, 0, 37];
pub struct Seen {
    pub handshakes: AtomicUsize,
    pub messages: Mutex<Vec<(u8, Vec<u8>)>>,
    pub pongs: AtomicUsize,
    pub closed: AtomicBool,
    pub release: AtomicBool,
    pub burst_written: AtomicBool,
    pub opening_started: AtomicBool,
}
impl Seen {
    fn new() -> Self {
        Self { handshakes: AtomicUsize::new(0), messages: Mutex::new(Vec::new()),
            pongs: AtomicUsize::new(0), closed: AtomicBool::new(false), release: AtomicBool::new(false), burst_written: AtomicBool::new(false), opening_started: AtomicBool::new(false) }
    }
    pub fn count(&self) -> usize { self.messages.lock().unwrap().len() }
    pub fn record(&self, opcode: u8, payload: &[u8]) {
        assert!(payload.len() <= 65536);
        let mut messages = self.messages.lock().unwrap();
        assert!(messages.len() < 16);
        messages.push((opcode, payload.to_vec()));
    }
}
#[derive(Clone, Copy)]
pub enum Scenario {
    EchoDuplex,
    StallOpening,
    StalledSink,
    DelayedText,
    AckHeld,
    InitialText,
    Burst,
    Fragmented,
    InvalidUtf8,
    MaskedServer,
    InvalidControl,
    UnknownOpcode,
    AbruptEof,
    WrongAccept,
    UnrequestedProtocol,
    MissingUpgrade,
}
pub struct Peer {
    pub http_origin: String,
    pub ws_origin: String,
    pub seen: Arc<Seen>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Peer {
    pub fn new(scenario: Scenario) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Seen::new());
        let stop = Arc::new(AtomicBool::new(false));
        let o = seen.clone(); let s = stop.clone();
        let thread = thread::spawn(move || {
            let end = Instant::now() + WAIT;
            let mut children = Vec::new();
            while !s.load(Ordering::SeqCst) && Instant::now() < end {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        assert!(children.len() < 4, "unexpected reconnect/repeated handshake");
                        socket.set_nonblocking(false).unwrap();
                        socket.set_read_timeout(Some(WAIT)).unwrap();
                        socket.set_write_timeout(Some(WAIT)).unwrap();
                        let o = o.clone(); let s = s.clone();
                        children.push(thread::spawn(move || serve(socket, scenario, &o, &s)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock =>
                        thread::sleep(Duration::from_millis(1)),
                    Err(e) => panic!("synthetic listener failed: {e}"),
                }
            }
            for child in children { child.join().expect("actual synthetic socket thread join"); }
        });
        Self { http_origin: format!("http://{address}"), ws_origin: format!("ws://{address}"),
            seen, stop, thread: Some(thread) }
    }
    pub fn finish(mut self) -> Arc<Seen> {
        self.stop.store(true, Ordering::SeqCst);
        self.seen.release.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().expect("actual peer listener join");
        self.seen.clone()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst); self.seen.release.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}
pub fn until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < end, "bounded observable condition");
        thread::sleep(Duration::from_millis(1));
    }
}
fn serve(mut socket: TcpStream, scenario: Scenario, seen: &Seen, stop: &AtomicBool) {
    if matches!(scenario, Scenario::EchoDuplex | Scenario::AckHeld | Scenario::InitialText | Scenario::Burst | Scenario::DelayedText | Scenario::StalledSink) {
        let mut ws = match tungstenite::accept(socket) { Ok(ws) => ws, Err(_) => return };
        seen.handshakes.fetch_add(1, Ordering::SeqCst);
        if matches!(scenario, Scenario::StalledSink) {
            // Complete a real RFC6455 handshake, then deliberately stop reading.
            // No socket-buffer or OS settings are changed to manufacture capacity.
            let end = Instant::now() + WAIT;
            while !stop.load(Ordering::SeqCst) && Instant::now() < end {
                thread::sleep(Duration::from_millis(1));
            }
            return;
        }
        if matches!(scenario, Scenario::DelayedText) {
            let end = Instant::now() + WAIT;
            while !stop.load(Ordering::SeqCst) && !seen.release.load(Ordering::SeqCst) && Instant::now() < end {
                thread::sleep(Duration::from_millis(1));
            }
            if stop.load(Ordering::SeqCst) { return; }
        }
        if matches!(scenario, Scenario::AckHeld | Scenario::InitialText | Scenario::DelayedText) {
            if ws.send(Message::Text(TEXT.into())).is_err() { return; }
            if matches!(scenario, Scenario::AckHeld)
                && ws.send(Message::Ping(b"ack-held".to_vec().into())).is_err() { return; }
        }
        if matches!(scenario, Scenario::Burst) {
            for _ in 0..8 { if ws.send(Message::Binary(b"burst".to_vec().into())).is_err() { return; } }
            seen.burst_written.store(true, Ordering::SeqCst);
        }
        let end = Instant::now() + WAIT;
        while !stop.load(Ordering::SeqCst) && Instant::now() < end {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    seen.record(1, text.as_bytes());
                    if matches!(scenario, Scenario::EchoDuplex)
                        && ws.send(Message::Text(text)).is_err() { return; }
                }
                Ok(Message::Binary(bytes)) => {
                    seen.record(2, &bytes);
                    if matches!(scenario, Scenario::EchoDuplex)
                        && ws.send(Message::Binary(bytes)).is_err() { return; }
                }
                Ok(Message::Pong(bytes)) => {
                    assert_eq!(bytes.as_ref(), b"ack-held"); seen.pongs.fetch_add(1, Ordering::SeqCst);
                }
                Ok(Message::Ping(_)) => { if ws.flush().is_err() { return; } }
                Ok(Message::Close(_)) => {
                    let _ = ws.flush(); seen.closed.store(true, Ordering::SeqCst); return;
                }
                Ok(Message::Frame(_)) => panic!("backend exposed raw frame"),
                Err(_) => return,
            }
        }
        return;
    }
    let mut head = Vec::new(); let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        match socket.read(&mut byte) { Ok(1) => head.push(byte[0]), _ => return }
        assert!(head.len() <= 16384);
    }
    let head = std::str::from_utf8(&head).expect("synthetic request header UTF-8");
    assert!(head.starts_with("GET /synthetic-ws HTTP/1.1\r\n"));
    if matches!(scenario, Scenario::StallOpening) {
        seen.opening_started.store(true, Ordering::SeqCst);
        if matches!(socket.read(&mut byte), Ok(0)) { seen.closed.store(true, Ordering::SeqCst); }
        return;
    }
    let key = head.lines().filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("sec-websocket-key")).unwrap().1.trim();
    let accept = if matches!(scenario, Scenario::WrongAccept) { "wrong".to_owned() }
        else { tungstenite::handshake::derive_accept_key(key.as_bytes()) };
    let extras = if matches!(scenario, Scenario::UnrequestedProtocol) {
        "Sec-WebSocket-Protocol: not-approved\r\n"
    } else { "" };
    let upgrade = if matches!(scenario, Scenario::MissingUpgrade) { "" }
        else { "Upgrade: websocket\r\nConnection: Upgrade\r\n" };
    let response = format!("HTTP/1.1 101 Switching Protocols\r\n{upgrade}Sec-WebSocket-Accept: {accept}\r\n{extras}\r\n");
    if socket.write_all(response.as_bytes()).is_err() { return; }
    seen.handshakes.fetch_add(1, Ordering::SeqCst);
    match scenario {
        Scenario::Fragmented => {
            let bytes = TEXT.as_bytes();
            // Split *inside* the first non-ASCII character, with an interleaved control.
            let split = 5;
            if write_frame(&mut socket, 0x01, &bytes[..split]).is_err() { return; }
            if write_frame(&mut socket, 0x89, b"fragment-ping").is_err() { return; }
            if write_frame(&mut socket, 0x80, &bytes[split..]).is_err() { return; }
            let _ = write_frame(&mut socket, 0x88, &1000u16.to_be_bytes());
        }
        Scenario::InvalidUtf8 => { let _ = write_frame(&mut socket, 0x81, &[0xff]); }
        Scenario::MaskedServer => { let _ = socket.write_all(&[0x82, 0x81, 1, 2, 3, 4, 0x60]); }
        Scenario::InvalidControl => { let _ = write_frame(&mut socket, 0x09, b"fragmented-control"); }
        Scenario::UnknownOpcode => { let _ = write_frame(&mut socket, 0x83, b"bad"); }
        Scenario::AbruptEof | Scenario::WrongAccept | Scenario::UnrequestedProtocol | Scenario::MissingUpgrade => return,
        _ => unreachable!(),
    }
    // Keep the wire alive so EOF does not fabricate the protocol failure.
    let end = Instant::now() + WAIT;
    while !stop.load(Ordering::SeqCst) && !seen.release.load(Ordering::SeqCst) && Instant::now() < end {
        thread::sleep(Duration::from_millis(1));
    }
}
fn write_frame(socket: &mut TcpStream, first: u8, payload: &[u8]) -> std::io::Result<()> {
    assert!(payload.len() <= 125, "raw fault peer supports only small exact synthetic frames");
    socket.write_all(&[first, payload.len() as u8])?; socket.write_all(payload)
}
