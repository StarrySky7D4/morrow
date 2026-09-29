//! Test peer only. Never a host authority, approved runtime or production client.
use morrow_native_session_wire::{self as wire, Frame, Kind};
use std::io::{Read, Write};
use std::time::Duration;

fn receive() -> Result<Frame, u8> {
    let mut prefix = [0; 4];
    std::io::stdin().read_exact(&mut prefix).map_err(|_| 24u8)?;
    let length = wire::payload_length(&prefix).map_err(|_| 16u8)?;
    let mut bytes = prefix.to_vec();
    bytes.resize(length + 4, 0);
    std::io::stdin()
        .read_exact(&mut bytes[4..])
        .map_err(|_| 24u8)?;
    Frame::decode(&bytes).map_err(|_| 16)
}
fn send(frame: &Frame) -> Result<(), u8> {
    std::io::stdout()
        .write_all(&frame.encode())
        .and_then(|()| std::io::stdout().flush())
        .map_err(|_| 24)
}
fn expect_denial() -> Result<(), u8> {
    let frame = receive()?;
    if matches!(frame.kind, Kind::Denied | Kind::Stop) && (16..=25).contains(&frame.code) {
        Ok(())
    } else {
        Err(90)
    }
}
fn exercise(scenario: &str) -> Result<(), u8> {
    let initial = receive()?;
    if initial.kind != Kind::Challenge
        || initial.schema != wire::schema_digest()
        || initial.pid != std::process::id()
    {
        return Err(17);
    }
    let mut hello = initial.request(Kind::Hello, 1);
    match scenario {
        "wrong-pid" => hello.pid = hello.pid.wrapping_add(1),
        "modified-epoch" => hello.epoch = hello.epoch.wrapping_add(1),
        "wrong-artifact" => hello.artifact[0] ^= 1,
        "no-hello" => {
            std::thread::sleep(Duration::from_secs(8));
            return Err(91);
        }
        "partial-frame" => {
            std::io::stdout()
                .write_all(&hello.encode()[..12])
                .and_then(|()| std::io::stdout().flush())
                .map_err(|_| 24u8)?;
            std::thread::sleep(Duration::from_secs(8));
            return Err(91);
        }
        "flood-invalid" => {
            let _ = std::io::stdout().write_all(&[255; 8192]);
            std::thread::sleep(Duration::from_secs(8));
            return Err(91);
        }
        "unsupported-kind" | "ignore-stop" | "hold-output" => {}
        _ => return Err(2),
    }
    send(&hello)?;
    if matches!(scenario, "wrong-pid" | "modified-epoch" | "wrong-artifact") {
        return expect_denial();
    }
    let welcome = receive()?;
    if welcome.kind != Kind::Welcome || welcome.sequence != 1 {
        return Err(90);
    }
    if scenario == "unsupported-kind" {
        send(&initial.request(Kind::State, 2))?;
        return expect_denial();
    }
    send(&initial.request(Kind::Query, 2))?;
    let state = receive()?;
    if state.kind != Kind::State || state.sequence != 2 {
        return Err(90);
    }
    if scenario == "ignore-stop" {
        std::thread::sleep(Duration::from_secs(8));
        return Err(91);
    }
    send(&initial.request(Kind::Close, 3))?;
    let stop = receive()?;
    if stop.kind != Kind::Stop || stop.sequence != 0 || stop.code != 25 {
        return Err(90);
    }
    let mut command = std::process::Command::new(std::env::current_exe().map_err(|_| 92u8)?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW, explicit for this descendant.
    }
    let child = command
        .arg("--hold-output-child")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|_| 92u8)?;
    eprintln!("test-only-output-holder-pid={}", child.id());
    // Intentional test-only descendant. It exits itself after 1.5 seconds.
    Ok(())
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--hold-output-child"] {
        std::thread::sleep(Duration::from_millis(1500));
        return std::process::ExitCode::SUCCESS;
    }
    if args == ["--help"] {
        println!(
            "Test peer only. Host arguments: --morrow-native-session-v2 --scenario wrong-pid|modified-epoch|wrong-artifact|unsupported-kind|no-hello|partial-frame|flood-invalid|ignore-stop|hold-output"
        );
        return std::process::ExitCode::SUCCESS;
    }
    if args.len() != 3 || args[0] != "--morrow-native-session-v2" || args[1] != "--scenario" {
        return std::process::ExitCode::from(2);
    }
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(97);
    });
    match exercise(&args[2]) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(code) => std::process::ExitCode::from(code),
    }
}
