//! Adversarial local test fixture. Not the production plugin client.
use morrow_native_session_wire::{Frame, Kind};
use std::{
    io::{Read, Write},
    time::Duration,
};
fn read() -> std::io::Result<Frame> {
    let mut p = [0; 4];
    std::io::stdin().read_exact(&mut p)?;
    let n = morrow_native_session_wire::payload_length(&p).map_err(std::io::Error::other)?;
    let mut b = p.to_vec();
    b.resize(n + 4, 0);
    std::io::stdin().read_exact(&mut b[4..])?;
    Frame::decode(&b).map_err(std::io::Error::other)
}
fn send(f: &Frame) {
    std::io::stdout().write_all(&f.encode()).unwrap();
    std::io::stdout().flush().unwrap();
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("hold-pipes-child") {
        std::thread::sleep(Duration::from_millis(1200));
        return;
    }
    if args.get(1).map(String::as_str) != Some("--morrow-native-session-v2") {
        std::process::exit(2);
    }
    let mode = args.get(2).map(String::as_str).unwrap_or("normal");
    if mode == "silent" {
        std::thread::sleep(Duration::from_secs(4));
        return;
    }
    let initial = read().unwrap();
    assert_eq!(initial.pid, std::process::id());
    assert_eq!(initial.schema, morrow_native_session_wire::schema_digest());
    if mode == "oversize" {
        std::io::stdout().write_all(&4096u32.to_le_bytes()).unwrap();
        std::io::stdout().flush().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        return;
    }
    if mode == "partial" {
        std::io::stdout().write_all(&[48, 1]).unwrap();
        std::io::stdout().flush().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        return;
    }
    let mut hello = initial.request(Kind::Hello, 1);
    if mode == "wrong-epoch" {
        hello.epoch += 1;
    }
    if mode == "stale-epoch" { hello.epoch=hello.epoch.saturating_sub(1); }
    if mode == "malformed" {
        let mut bytes=hello.encode();bytes[8..16].fill(255);
        std::io::stdout().write_all(&bytes).unwrap();std::io::stdout().flush().unwrap();let _=read();return;
    }
    if mode == "wrong-hash" {
        hello.artifact[0] ^= 1;
    }
    if mode == "nonzero-code" {
        hello.code = 1;
    }
    send(&hello);
    let welcome = read().unwrap();
    if welcome.kind != Kind::Welcome {
        return;
    }
    if mode == "duplicate-hello" {
        send(&hello);
        let _ = read();
        return;
    }
    if mode == "ignore-stop" {
        std::thread::sleep(Duration::from_secs(4));
        return;
    }
    if mode == "hold-output" {
        let mut c = std::process::Command::new(std::env::current_exe().unwrap());
        c.arg("hold-pipes-child")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000);
        }
        let _child = c.spawn().unwrap();
        return;
    }
    if mode == "disconnect" {
        return;
    }
    let mut seq = 2;
    if mode == "no-read" {
        for seq in 2..1000 {
            send(&initial.request(Kind::Query, seq));
        }
        std::thread::sleep(Duration::from_secs(4));
        return;
    }
    loop {
        send(&initial.request(Kind::Query, seq));
        let reply = match read() {
            Ok(v) => v,
            Err(_) => return,
        };
        if reply.kind == Kind::Stop {
            return;
        }
        assert!(matches!(reply.kind, Kind::State | Kind::Denied));
        seq += 1;
        if mode == "normal" {
            send(&initial.request(Kind::Close, seq));
            let _ = read();
            return;
        }
        if mode != "flood" {
            std::thread::sleep(Duration::from_millis(80));
        }
    }
}
