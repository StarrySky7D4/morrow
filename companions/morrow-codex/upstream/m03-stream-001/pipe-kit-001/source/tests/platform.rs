#![cfg(windows)]
#![forbid(unsafe_code)]
use morrow_native_pipe_win::{Completed, Kind, Pipe};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn locator(label: &str) -> String {
    format!(
        r"\\.\pipe\morrow-m03-{}-{}-{}",
        std::process::id(),
        label,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
fn complete(pipe: &mut Pipe, kind: Kind) -> Completed {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(c) = pipe.poll(kind).unwrap() {
            return c;
        }
        assert!(Instant::now() < deadline, "I/O still pending {kind:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn line(reader: &mut impl BufRead, marker: &str) -> String {
    loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "EOF waiting {marker}"
        );
        if line.contains(marker) {
            return line;
        }
    }
}
#[test]
fn helper() {
    let Ok(name) = std::env::var("MORROW_PIPE_TEST_LOCATOR") else {
        return;
    };
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(81);
    });
    let mut pipe = Pipe::open_client(&name).unwrap();
    assert!(!pipe.inheritable().unwrap());
    println!("PEER_CONNECTED");
    std::io::stdout().flush().unwrap();
    let mut stdin = BufReader::new(std::io::stdin());
    loop {
        let mut command = String::new();
        if stdin.read_line(&mut command).unwrap() == 0 {
            break;
        }
        match command.trim() {
            "CONTROL" => {
                println!("CONTROL_ACK");
                std::io::stdout().flush().unwrap();
            }
            "READ" => {
                pipe.begin_read(32).unwrap();
                println!("READ_ISSUED");
                std::io::stdout().flush().unwrap();
                let c = complete(&mut pipe, Kind::Read);
                println!("READ_DONE {} {}", c.error.unwrap_or(0), c.transferred);
                std::io::stdout().flush().unwrap();
            }
            "ECHO" => {
                pipe.begin_read(32).unwrap();
                let c = complete(&mut pipe, Kind::Read);
                assert!(c.error.is_none());
                assert_eq!(c.bytes, b"hello");
                pipe.begin_write(b"reply".to_vec()).unwrap();
                assert!(complete(&mut pipe, Kind::Write).error.is_none());
                println!("ECHO_DONE");
                std::io::stdout().flush().unwrap();
            }
            "QUIT" => break,
            _ => panic!("unexpected helper command"),
        }
    }
    let done = pipe.cancel_and_reap().unwrap();
    assert!(done.is_empty());
}
fn peer(name: &str) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "helper", "--nocapture"])
        .env_clear()
        .env("MORROW_PIPE_TEST_LOCATOR", name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000);
    for k in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(v) = std::env::var_os(k) {
            command.env(k, v);
        }
    }
    command.spawn().unwrap()
}
#[test]
fn connect_pending_cancel_is_actually_reaped_and_first_instance_is_exclusive() {
    let name = locator("connect");
    let mut server = Pipe::create_private(&name).unwrap();
    assert!(!server.inheritable().unwrap());
    let actual_dacl = server.security_sddl().unwrap();
    assert_eq!(
        actual_dacl,
        morrow_native_pipe_win::expected_private_sddl().unwrap()
    );
    assert_eq!(actual_dacl.matches("(A;").count(), 2);
    assert!(actual_dacl.starts_with("D:P"));
    println!(
        "actual_dacl_matches_current_sid_and_system=true protected=true allow_aces=2 inherited_handle=false"
    );
    assert!(Pipe::create_private(&name).is_err());
    let info = server.buffer_info().unwrap();
    assert_eq!(info.max_instances, 1);
    println!(
        "buffers inbound={} outbound={}",
        info.inbound, info.outbound
    );
    let started = server.begin_connect().unwrap();
    assert!(started.pending);
    assert!(server.poll(Kind::Connect).unwrap().is_none());
    let result = server.cancel_and_reap().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].kind, Kind::Connect);
    assert_eq!(result[0].error, Some(995));
    assert!(!server.has_operation(Kind::Connect));
    assert!(server.begin_connect().is_err());
    println!("connect_id={} actual_cancel_completion=995", started.id);
}
#[test]
fn actual_child_pid_duplex_bytes_and_client_disconnect_are_observed() {
    let name = locator("duplex");
    let mut server = Pipe::create_private(&name).unwrap();
    server.begin_connect().unwrap();
    let mut child = peer(&name);
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    line(&mut output, "PEER_CONNECTED");
    assert!(complete(&mut server, Kind::Connect).error.is_none());
    assert_eq!(server.client_pid().unwrap(), child.id());
    input.write_all(b"ECHO\n").unwrap();
    server.begin_write(b"hello".to_vec()).unwrap();
    assert_eq!(complete(&mut server, Kind::Write).transferred, 5);
    server.begin_read(32).unwrap();
    let c = complete(&mut server, Kind::Read);
    assert!(c.error.is_none());
    assert_eq!(c.bytes, b"reply");
    line(&mut output, "ECHO_DONE");
    server.begin_read(32).unwrap();
    assert!(server.poll(Kind::Read).unwrap().is_none());
    input.write_all(b"QUIT\n").unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    let c = complete(&mut server, Kind::Read);
    assert_eq!(c.error, Some(109));
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut output, &mut rest).unwrap();
    assert!(!server.has_operation(Kind::Read));
    assert!(server.cancel_and_reap().unwrap().is_empty());
    println!(
        "child_pid={} client_disconnect=109 actual_child_exit=true stdout_eof=true",
        child.id()
    );
}
#[test]
fn stopped_reader_causes_sustained_os_write_pending_while_control_and_cancellation_work() {
    let name = locator("pending");
    let mut server = Pipe::create_private(&name).unwrap();
    server.begin_connect().unwrap();
    let mut child = peer(&name);
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    // Helper has opened the pipe but has issued no ReadFile at this barrier.
    line(&mut output, "PEER_CONNECTED");
    assert!(complete(&mut server, Kind::Connect).error.is_none());
    assert_eq!(server.client_pid().unwrap(), child.id());
    let info = server.buffer_info().unwrap();
    let write = server.begin_write(vec![7; 8192]).unwrap();
    assert!(
        write.pending,
        "fixed8KiB did not reach OS pending; unqualified"
    );
    assert!(server.poll(Kind::Write).unwrap().is_none());
    std::thread::sleep(Duration::from_millis(25));
    assert!(server.poll(Kind::Write).unwrap().is_none());
    input.write_all(b"CONTROL\n").unwrap();
    line(&mut output, "CONTROL_ACK");
    std::thread::sleep(Duration::from_millis(25));
    assert!(server.poll(Kind::Write).unwrap().is_none());
    let read = server.begin_read(32).unwrap();
    assert!(read.pending);
    assert!(server.poll(Kind::Read).unwrap().is_none());
    let done = server.cancel_and_reap().unwrap();
    assert_eq!(done.len(), 2);
    assert!(done.iter().all(|c| c.error == Some(995)));
    assert!(!server.has_operation(Kind::Read) && !server.has_operation(Kind::Write));
    assert!(server.begin_write(vec![1]).is_err());
    input.write_all(b"QUIT\n").unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut output, &mut rest).unwrap();
    println!(
        "child_pid={} requested_write=8192 buffers={info:?} write_id={} read_id={} stable_incomplete_samples=3 spacing_ms=25 control_ack=true read_write_cancel_completion=995",
        child.id(),
        write.id,
        read.id
    );
}
#[test]
fn server_disconnect_completes_client_read_without_false_success() {
    let name = locator("disconnect");
    let mut server = Pipe::create_private(&name).unwrap();
    server.begin_connect().unwrap();
    let mut child = peer(&name);
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    line(&mut output, "PEER_CONNECTED");
    complete(&mut server, Kind::Connect);
    input.write_all(b"READ\n").unwrap();
    line(&mut output, "READ_ISSUED");
    drop(server);
    let result = line(&mut output, "READ_DONE");
    assert!(result.contains("READ_DONE 109 0"), "{result}");
    input.write_all(b"QUIT\n").unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    println!("server_endpoint_closed client_read_error=109");
}
