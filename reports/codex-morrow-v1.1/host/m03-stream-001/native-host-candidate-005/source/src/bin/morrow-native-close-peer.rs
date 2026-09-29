//! Synthetic protocol fixture, no Core/backend/HTTP. Never a plugin substitute.
use morrow_native_session_stream::wire as w;
use std::io::{Read, Write};
fn read(r: &mut impl Read) -> Result<w::Frame, String> {
    let mut p = [0; 4];
    r.read_exact(&mut p).map_err(|e| e.to_string())?;
    let n = w::payload_length(&p).map_err(str::to_owned)?;
    let mut b = p.to_vec();
    b.resize(n + 4, 0);
    r.read_exact(&mut b[4..]).map_err(|e| e.to_string())?;
    w::Frame::decode(&b).map_err(str::to_owned)
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 || args[1] != "--morrow-native-http-v3" {
        return Err("fixture args".into());
    }
    let mode = &args[2];
    let path = &args[3];
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let initial = read(&mut input)?;
    if initial.kind != w::Kind::Challenge || initial.child_pid != std::process::id() {
        return Err("challenge identity".into());
    }
    let mut hello = initial.clone();
    hello.kind = w::Kind::Hello;
    hello.sequence = 1;
    output
        .write_all(&hello.encode().map_err(str::to_owned)?)
        .map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    loop {
        let f = read(&mut input)?;
        if f.kind == w::Kind::Welcome {
            if f.code != 0 || f.sequence != 1 {
                return Err("Welcome requires success code0 and Hello sequence1".into());
            }
            break;
        }
        if f.kind == w::Kind::Stop {
            return Err("pre-Welcome Stop".into());
        }
    }
    let mut close = initial.clone();
    close.kind = w::Kind::Close;
    close.sequence = if mode == "bad-sequence" { 3 } else { 2 };
    let mut bytes = close.encode().map_err(str::to_owned)?;
    if mode == "post-close-bytes" {
        bytes.push(0xff);
    }
    output.write_all(&bytes).map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())?;
    let mut frames = vec![];
    let mut ack = false;
    let mut stop = false;
    loop {
        let mut prefix = [0; 4];
        match input.read(&mut prefix[..1]) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
        input
            .read_exact(&mut prefix[1..])
            .map_err(|e| e.to_string())?;
        let n = w::payload_length(&prefix).map_err(str::to_owned)?;
        let mut bytes = prefix.to_vec();
        bytes.resize(n + 4, 0);
        input
            .read_exact(&mut bytes[4..])
            .map_err(|e| e.to_string())?;
        let f = w::Frame::decode(&bytes).map_err(str::to_owned)?;
        if f.kind == w::Kind::State && f.sequence == 2 && f.code == 0 {
            ack = true;
            if let w::Payload::Progress(p) = &f.payload {
                if p.owner_released || p.child_exited {
                    return Err("premature session release".into());
                }
            } else {
                return Err("ACK payload".into());
            }
        }
        if f.kind == w::Kind::Stop {
            stop = true;
        }
        frames.push(w::hex(&bytes));
    }
    let result = serde_json::json!({"mode":mode,"ack_received":ack,"stop_received":stop,"stdin_eof_after_frames":true,"frames":frames,"http_not_invoked":true});
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut f, &result).map_err(|e| e.to_string())?;
    if mode == "normal" && (!ack || stop) {
        return Err("normal Close lacks exclusive complete ACK".into());
    }
    if mode == "bad-sequence" && (ack || !stop) {
        return Err("invalid Close accepted".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2)
    }
}
