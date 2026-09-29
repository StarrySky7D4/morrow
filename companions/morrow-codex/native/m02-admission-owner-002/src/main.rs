//! Bounded evidence peer, not an approval authority or production client.
#![deny(unsafe_code)]
use morrow_native_session_wire::{self as wire, Frame, Kind};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}, time::Duration};

type Result<T> = std::result::Result<T, u8>;
const CAPTURE_FILES: [&str; 7] = ["01-challenge.frame", "02-hello.frame", "03-welcome.frame", "04-query.frame", "05-state.frame", "06-close.frame", "07-stop.frame"];
const OBSERVE_MS: u64 = 150;

struct Options { mode: String, evidence: PathBuf, capture: Option<PathBuf> }
fn options(args: &[std::ffi::OsString]) -> Result<Options> {
    if args.first().and_then(|a| a.to_str()) != Some("--morrow-native-session-v2") { return Err(2); }
    let mut mode = None; let mut evidence = None; let mut capture = None;
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 { return Err(2); }
        match pair[0].to_str() {
            Some("--scenario") if mode.is_none() => mode = Some(pair[1].to_str().ok_or(2u8)?.to_owned()),
            Some("--evidence-dir") if evidence.is_none() => evidence = Some(PathBuf::from(&pair[1])),
            Some("--capture-dir") if capture.is_none() => capture = Some(PathBuf::from(&pair[1])),
            _ => return Err(2),
        }
    }
    let mode = mode.ok_or(2u8)?;
    if !matches!(mode.as_str(), "capture" | "replay-hello" | "replay-query" | "hold-output") || matches!(mode.as_str(), "capture" | "hold-output") == capture.is_some() { return Err(2); }
    Ok(Options { mode, evidence: evidence.ok_or(2u8)?, capture })
}

// The trusted harness creates an ACL-restricted directory before spawning us.
// This check prevents accidental redirection; it is not a hostile same-user sandbox.
fn directory(path: &Path, empty: bool) -> Result<PathBuf> {
    if !path.is_absolute() { return Err(92); }
    for ancestor in path.ancestors() {
        let meta = fs::symlink_metadata(ancestor).map_err(|_| 92u8)?;
        if !meta.is_dir() || meta.file_type().is_symlink() { return Err(92); }
        #[cfg(windows)] {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 { return Err(92); }
        }
    }
    if empty && fs::read_dir(path).map_err(|_| 92u8)?.next().is_some() { return Err(92); }
    fs::canonicalize(path).map_err(|_| 92)
}
fn save(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    if bytes.len() > 4096 { return Err(92); }
    let mut options = OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(windows)] { use std::os::windows::fs::OpenOptionsExt; options.share_mode(0); }
    let mut file = options.open(dir.join(name)).map_err(|_| 92u8)?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|_| 92)
}
fn load(dir: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path).map_err(|_| 92u8)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit { return Err(92); }
    #[cfg(windows)] {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 { return Err(92); }
    }
    let mut bytes = Vec::new();
    File::open(path).map_err(|_| 92u8)?.take(limit + 1).read_to_end(&mut bytes).map_err(|_| 92u8)?;
    if bytes.len() as u64 > limit { return Err(92); }
    Ok(bytes)
}
fn read_frame(input: &mut impl Read) -> Result<Vec<u8>> {
    let mut prefix = [0; 4]; input.read_exact(&mut prefix).map_err(|_| 24u8)?;
    let length = wire::payload_length(&prefix).map_err(|_| 16u8)?;
    let mut bytes = prefix.to_vec(); bytes.resize(length + 4, 0);
    input.read_exact(&mut bytes[4..]).map_err(|_| 24u8)?;
    Ok(bytes)
}
fn receive(dir: &Path, name: &str) -> Result<Frame> {
    let bytes = read_frame(&mut std::io::stdin().lock())?;
    save(dir, name, &bytes)?;
    Frame::decode(&bytes).map_err(|_| 16)
}
fn send_bytes(output: &mut impl Write, bytes: &[u8]) -> Result<()> {
    // Deliberately no decode/encode round trip here: historical bytes are exact.
    output.write_all(bytes).and_then(|()| output.flush()).map_err(|_| 24)
}
fn send(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    save(dir, name, bytes)?;
    send_bytes(&mut std::io::stdout().lock(), bytes)
}
fn binding(frame: &Frame, initial: &Frame) -> bool {
    frame.session == initial.session && frame.epoch == initial.epoch && frame.pid == initial.pid
        && frame.nonce == initial.nonce && frame.schema == initial.schema && frame.artifact == initial.artifact
        && frame.config == initial.config && frame.capabilities == initial.capabilities
}
fn response(frame: &Frame, previous: &Frame, kind: Kind, seq: u64, code: u32) -> Result<()> {
    if !binding(frame, previous) || frame.kind != kind || frame.sequence != seq || frame.code != code
        || frame.generation < previous.generation || frame.generation > 2
        || frame.remaining_ms > previous.remaining_ms || frame.budget > previous.budget { return Err(90); }
    if matches!(kind, Kind::Welcome | Kind::State) && frame.generation != 1 { return Err(90); }
    Ok(())
}
fn challenge(frame: &Frame) -> Result<()> {
    if frame.kind != Kind::Challenge || frame.sequence != 0 || frame.code != 0 || frame.generation != 1
        || frame.session == 0 || frame.epoch == 0 || frame.pid == 0 || frame.schema != wire::schema_digest()
        || frame.remaining_ms == 0 || frame.budget == 0 || frame.capabilities != 1
        || frame.nonce == [0;32] || frame.config == [0;32] { return Err(17); }
    Ok(())
}
fn summary(files: &[Vec<u8>]) -> Vec<u8> {
    let mut text = String::from("test-peer-complete-v1;not-process-exit-proof\n");
    for (name, bytes) in CAPTURE_FILES.iter().zip(files) {
        text.push_str(&format!("{} {} {}\n", name, bytes.len(), wire::hex(&wire::digest(bytes))));
    }
    text.into_bytes()
}
struct History { hello: Vec<u8>, query: Vec<u8>, initial: Frame }
fn historical(dir: &Path) -> Result<History> {
    let bytes: Vec<_> = CAPTURE_FILES.iter().map(|name| load(dir, name, wire::MAX_FRAME as u64)).collect::<Result<_>>()?;
    if load(dir, "complete.txt", 4096)? != summary(&bytes) { return Err(93); }
    let frames: Vec<_> = bytes.iter().map(|b| Frame::decode(b).map_err(|_| 16u8)).collect::<Result<_>>()?;
    let initial = &frames[0]; challenge(initial)?;
    if frames[1] != initial.request(Kind::Hello, 1) || frames[3] != initial.request(Kind::Query, 2)
        || frames[5] != initial.request(Kind::Close, 3) { return Err(93); }
    response(&frames[2], initial, Kind::Welcome, 1, 2)?;
    response(&frames[4], &frames[2], Kind::State, 2, 2)?;
    response(&frames[6], &frames[4], Kind::Stop, 0, 25)?;
    Ok(History { hello: bytes[1].clone(), query: bytes[3].clone(), initial: initial.clone() })
}
fn exercise(opts: &Options, dir: &Path) -> Result<()> {
    let history = opts.capture.as_ref().map(|p| directory(p, false).and_then(|d| historical(&d))).transpose()?;
    let initial = receive(dir, "01-challenge.frame")?; challenge(&initial)?;
    if initial.pid != std::process::id() { return Err(17); }
    let exe = std::env::current_exe().map_err(|_| 92u8)?;
    let own = load(exe.parent().ok_or(92u8)?, exe.file_name().and_then(|n| n.to_str()).ok_or(92u8)?, 64 * 1024 * 1024)?;
    if wire::digest(&own) != initial.artifact { return Err(17); }
    // Independent observation opportunity; host clocks keep running unchanged.
    std::thread::sleep(Duration::from_millis(OBSERVE_MS));
    if let Some(old) = &history {
        if old.initial.session == initial.session || old.initial.nonce == initial.nonce { return Err(93); }
    }
    let hello = if opts.mode == "replay-hello" { history.as_ref().ok_or(93u8)?.hello.clone() } else { initial.request(Kind::Hello, 1).encode() };
    send(dir, "02-hello.frame", &hello)?;
    let first = receive(dir, "03-response.frame")?;
    if opts.mode == "replay-hello" { return rejected(dir, &first, &initial, 1, &opts.mode); }
    response(&first, &initial, Kind::Welcome, 1, 2)?;
    // Keep a stable capture filename in addition to the observed generic response.
    save(dir, "03-welcome.frame", &load(dir, "03-response.frame", wire::MAX_FRAME as u64)?)?;
    let query = if opts.mode == "replay-query" { history.as_ref().ok_or(93u8)?.query.clone() } else { initial.request(Kind::Query, 2).encode() };
    send(dir, "04-query.frame", &query)?;
    let second = receive(dir, "05-response.frame")?;
    if opts.mode == "replay-query" { return rejected(dir, &second, &first, 2, &opts.mode); }
    response(&second, &first, Kind::State, 2, 2)?;
    save(dir, "05-state.frame", &load(dir, "05-response.frame", wire::MAX_FRAME as u64)?)?;
    send(dir, "06-close.frame", &initial.request(Kind::Close, 3).encode())?;
    let stop = receive(dir, "07-stop.frame")?;
    response(&stop, &second, Kind::Stop, 0, 25)?;
    if opts.mode == "hold-output" {
        let mut command = std::process::Command::new(std::env::current_exe().map_err(|_| 92u8)?);
        #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
        let child = command.arg("--hold-output-child")
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit()).spawn().map_err(|_| 92u8)?;
        // PID originates from this owned Child handle, not a process enumeration.
        // The joint observer must independently check image, creation time and exit.
        save(dir, "holder.json", format!("{{\"source\":\"peer_owned_child_handle\",\"root_pid\":{},\"holder_pid\":{},\"holder_lifetime_ms\":2000,\"create_no_window\":true}}\n", std::process::id(), child.id()).as_bytes())?;
        return save(dir, "result.json", b"{\"mode\":\"hold-output\",\"stop_observed\":true,\"process_exit_proven\":false}\n");
    }
    let bytes: Vec<_> = CAPTURE_FILES.iter().map(|name| load(dir, name, wire::MAX_FRAME as u64)).collect::<Result<_>>()?;
    save(dir, "complete.txt", &summary(&bytes))?;
    save(dir, "result.json", format!("{{\"mode\":\"capture\",\"pid\":{},\"accepted_welcome\":true,\"state_observed\":true,\"stop_observed\":true,\"process_exit_proven\":false,\"observation_delay_ms\":{}}}\n", std::process::id(), OBSERVE_MS).as_bytes())
}
fn rejected(dir: &Path, frame: &Frame, previous: &Frame, seq: u64, mode: &str) -> Result<()> {
    let expected_seq = if frame.kind == Kind::Stop { 0 } else { seq };
    if !matches!(frame.kind, Kind::Stop | Kind::Denied) { return Err(90); }
    response(frame, previous, frame.kind, expected_seq, 17)?;
    save(dir, "result.json", format!("{{\"mode\":\"{}\",\"pid\":{},\"identity_rejection_observed\":true,\"code\":17,\"process_exit_proven\":false,\"observation_delay_ms\":{}}}\n", mode, std::process::id(), OBSERVE_MS).as_bytes())
}
fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["--hold-output-child"] {
        std::thread::sleep(Duration::from_millis(2000));
        return std::process::ExitCode::SUCCESS;
    }
    if args == ["--help"] {
        println!("Test peer only: --morrow-native-session-v2 --scenario capture|replay-hello|replay-query|hold-output --evidence-dir ABS [--capture-dir ABS]. Fixed observation delay150ms; lifetime watchdog10s. No approval API.");
        return std::process::ExitCode::SUCCESS;
    }
    let opts = match options(&args) { Ok(v) => v, Err(e) => return e.into() };
    std::thread::spawn(|| { std::thread::sleep(Duration::from_secs(10)); std::process::exit(97); });
    let dir = match directory(&opts.evidence, true) { Ok(d) => d, Err(e) => return e.into() };
    let result = exercise(&opts, &dir);
    let code = result.err().unwrap_or(0);
    if code != 0 { let _ = save(&dir, "failure.txt", format!("test-peer-failed code={code}\n").as_bytes()); }
    eprintln!("test-replay-peer mode={} pid={} exit={code}", opts.mode, std::process::id());
    code.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_writer_preserves_exact_bytes_and_propagates_failure() {
        let raw = include_bytes!("../../m02-native-session-001/vendor/capnp-kit-001/vectors/hello.frame");
        let mut out = Vec::new(); send_bytes(&mut out, raw).unwrap(); assert_eq!(out, raw);
        struct Failed; impl Write for Failed { fn write(&mut self, _: &[u8]) -> std::io::Result<usize> { Err(std::io::ErrorKind::BrokenPipe.into()) } fn flush(&mut self) -> std::io::Result<()> { Ok(()) } }
        assert_eq!(send_bytes(&mut Failed, raw), Err(24));
    }
    #[test]
    fn parser_disallows_approval_duplicate_and_missing_history() {
        for args in [vec!["--approved", "true"], vec!["--morrow-native-session-v2", "--scenario", "replay-hello", "--evidence-dir", "C:/x"], vec!["--morrow-native-session-v2", "--scenario", "capture", "--evidence-dir", "C:/x", "--capture-dir", "C:/old"]] {
            assert!(options(&args.into_iter().map(Into::into).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn response_validation_rejects_identity_swap_and_renewal() {
        let initial = Frame::decode(include_bytes!("../../m02-native-session-001/vendor/capnp-kit-001/vectors/challenge.frame")).unwrap();
        let mut welcome = initial.request(Kind::Welcome, 1); welcome.code = 2;
        assert!(response(&welcome, &initial, Kind::Welcome, 1, 2).is_ok());
        welcome.epoch += 1; assert!(response(&welcome, &initial, Kind::Welcome, 1, 2).is_err());
        welcome.epoch = initial.epoch; welcome.remaining_ms += 1;
        assert!(response(&welcome, &initial, Kind::Welcome, 1, 2).is_err());
    }
}
