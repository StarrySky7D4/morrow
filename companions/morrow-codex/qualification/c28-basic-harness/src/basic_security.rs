//! Fixed guest-local SID query and owned loopback challenge; no authority mint.
#![forbid(unsafe_code)]
use anyhow::{Result, ensure, bail};
use crate::preflight::Artifact;
use codex_windows_sandbox::MatchedRunnerArtifact;
use std::{io::{Read, Write},
    net::{TcpListener, TcpStream, SocketAddr, Shutdown}, path::Path,
    process::{Command, Stdio}, os::windows::process::CommandExt,
    sync::mpsc::{sync_channel, SyncSender}, thread::{self, JoinHandle},
    time::{Duration, Instant}};

pub const SYSTEM_QUERY: &str = r"C:\Windows\System32\whoami.exe";
// This private fixture accepts only the root's freshly observed selected-guest image.
// It is not a portable system-image pin and was not derived from the host image.
pub const SYSTEM_QUERY_SHA256: &str = "574bc2a2995fe2b1f732ccd39f2d99460ace980af29efdf1eb0d3e888be7d6f0";
pub const SYSTEM_QUERY_BYTES: u64 = 94208;
pub const QUERY_ARGS: [&str; 4] = ["/user", "/fo", "csv", "/nh"];
const QUERY_OUTPUT_MAX: u64 = 16384;
#[derive(Clone)]
pub struct SecuritySpec { pub query: Artifact, pub expected_sid: String, pub port: u16 }

pub fn valid_sid(sid: &str) -> bool {
    let pieces: Vec<_> = sid.split('-').collect();
    sid.len() <= 128 && pieces.len() == 8 && pieces[..4] == ["S", "1", "5", "21"]
        && pieces[4..].iter().all(|part| !part.is_empty()
            && part.parse::<u32>().is_ok_and(|value| value.to_string() == *part))
}
/// Only one bounded CSV row, two quoted fields and a canonical local account SID.
/// The display name may use the guest code page; it is never used as authority.
pub fn parse_sid_csv(raw: &[u8]) -> Result<String> {
    ensure!(!raw.is_empty() && raw.len() <= QUERY_OUTPUT_MAX as usize, "SID CSV bound");
    let row = raw.strip_suffix(b"\r\n").or_else(|| raw.strip_suffix(b"\n")).unwrap_or(raw);
    ensure!(!row.iter().any(|b| matches!(b, b'\r' | b'\n' | 0)), "SID CSV multiple rows");
    let mut at = 0;
    let mut fields = Vec::new();
    for field in 0..2 {
        ensure!(row.get(at) == Some(&b'"'), "SID CSV quoted field"); at += 1;
        let mut value = Vec::new();
        loop {
            let byte = *row.get(at).ok_or_else(|| anyhow::anyhow!("SID CSV truncated field"))?;
            at += 1;
            if byte == b'"' {
                if row.get(at) == Some(&b'"') { value.push(b'"'); at += 1; }
                else { break; }
            } else { value.push(byte); }
        }
        ensure!(!value.is_empty(), "SID CSV empty field"); fields.push(value);
        if field == 0 { ensure!(row.get(at) == Some(&b','), "SID CSV separator"); at += 1; }
    }
    ensure!(at == row.len(), "SID CSV trailing fields/data");
    let sid = std::str::from_utf8(&fields[1])?.to_owned();
    ensure!(valid_sid(&sid), "SID CSV noncanonical account SID"); Ok(sid)
}

pub fn pin_query(query: &Artifact) -> Result<MatchedRunnerArtifact> {
    ensure!(query.path.to_str().is_some_and(|s| s.eq_ignore_ascii_case(SYSTEM_QUERY)),
        "only the explicit guest System32 whoami image is allowed");
    ensure!(crate::preflight::hex(&query.sha256) == SYSTEM_QUERY_SHA256,
        "SID query SHA differs from the fresh selected-guest inventory");
    let guard = MatchedRunnerArtifact::acquire(&query.path, query.sha256)?;
    let observed = crate::preflight::observe_artifact(query)?;
    ensure!(observed.bytes == SYSTEM_QUERY_BYTES
        && observed.canonical_path == Path::new(SYSTEM_QUERY).canonicalize()?
        && guard.path().canonicalize()? == observed.canonical_path, "system query path mismatch");
    // This reuses only the safe file/ancestor lock, not runner negotiation or identity.
    Ok(guard)
}

fn capture_query_output(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut raw = Vec::new(); let mut oversized = false; let mut chunk = [0; 4096];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            // Windows anonymous-pipe ERROR_BROKEN_PIPE is genuine writer EOF.
            Err(e) if e.raw_os_error() == Some(109) => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        let room = (QUERY_OUTPUT_MAX as usize).saturating_sub(raw.len());
        raw.extend_from_slice(&chunk[..count.min(room)]);
        oversized |= count > room;
        // Drain after the bound without retaining bytes; never block a child
        // on a full pipe merely to report a validation failure.
    }
    ensure!(!oversized, "SID query output exceeded bound"); Ok(raw)
}
fn prepare_reader<R: Read + Send + 'static>() -> std::io::Result<(SyncSender<R>, JoinHandle<Result<Vec<u8>>>)> {
    let (send, receive) = sync_channel(1);
    let join = thread::Builder::new().name("c28-fixed-sid-output".into()).spawn(move || {
        match receive.recv() {
            Ok(reader) => capture_query_output(reader),
            Err(_) => bail!("SID query reader received no child pipe"),
        }
    })?;
    Ok((send, join))
}
/// Both bounded drain threads are prepared before process creation. Every
/// post-spawn outcome waits the same child and joins both readers before error
/// propagation. There is no thread detach, kill, retry, shell or PATH lookup.
pub fn query_actual_sid(spec: &SecuritySpec, workspace: &Path, nonce: &str) -> Result<String> {
    ensure!(crate::witness::valid_nonce(nonce) && valid_sid(&spec.expected_sid), "SID query binding");
    crate::preflight::no_reparse_ancestors(workspace)?;
    let _guard = pin_query(&spec.query)?;
    let (out_send, out_join) = prepare_reader::<std::process::ChildStdout>()?;
    let (err_send, err_join) = match prepare_reader::<std::process::ChildStderr>() {
        Ok(reader) => reader,
        Err(error) => { drop(out_send); let _ = out_join.join(); return Err(error.into()); }
    };
    let mut command = Command::new(&spec.query.path);
    command.args(QUERY_ARGS).current_dir(workspace).stdin(Stdio::null()).stdout(Stdio::piped())
        .stderr(Stdio::piped()).creation_flags(0x08000000);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            drop(out_send); drop(err_send);
            let _ = out_join.join(); let _ = err_join.join(); return Err(error.into());
        }
    };
    let out_sent = child.stdout.take().is_some_and(|pipe| out_send.send(pipe).is_ok());
    let err_sent = child.stderr.take().is_some_and(|pipe| err_send.send(pipe).is_ok());
    drop(out_send); drop(err_send);
    let status = child.wait();
    // Collect both join results before any fallible validation can return.
    let out = out_join.join(); let err = err_join.join();
    ensure!(out_sent && err_sent, "SID query original pipe delivery failed");
    let status = status?;
    let raw = out.map_err(|_| anyhow::anyhow!("SID stdout reader panicked"))??;
    let stderr = err.map_err(|_| anyhow::anyhow!("SID stderr reader panicked"))??;
    ensure!(status.code() == Some(0), "fixed SID query actual exit was not zero");
    ensure!(stderr.is_empty(), "fixed SID query stderr was nonempty");
    let sid = parse_sid_csv(&raw)?;
    ensure!(sid == spec.expected_sid, "actual inherited child user differs from setup Offline SID");
    Ok(sid)
}

pub fn challenge(nonce: &str, reply: bool) -> Result<Vec<u8>> {
    ensure!(crate::witness::valid_nonce(nonce), "network challenge nonce");
    Ok(format!("C28-LOCAL-{} {nonce}\n", if reply { "REPLY" } else { "REQUEST" }).into_bytes())
}
fn read_exact_message(stream: &mut TcpStream, expected: &[u8]) -> Result<()> {
    let mut raw = Vec::new();
    stream.take(expected.len() as u64 + 1).read_to_end(&mut raw)?;
    ensure!(raw == expected, "local challenge mismatch or extra bytes"); Ok(())
}
/// The same owned ephemeral listener remains held before Claim and after EOF.
/// Baseline reads/writes have finite timeouts; no background or detached task.
pub struct NetworkFixture { listener: TcpListener, pub port: u16 }
impl NetworkFixture {
    pub fn create(nonce: &str) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        ensure!(port != 0, "ephemeral listener port");
        let fixture = Self { listener, port }; fixture.baseline(nonce)?; Ok(fixture)
    }
    pub fn baseline(&self, nonce: &str) -> Result<()> {
        let request = challenge(nonce, false)?; let reply = challenge(nonce, true)?;
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let timeout = Duration::from_secs(1);
        let mut client = TcpStream::connect_timeout(&address, timeout)?;
        client.set_read_timeout(Some(timeout))?; client.set_write_timeout(Some(timeout))?;
        client.write_all(&request)?; client.shutdown(Shutdown::Write)?;
        let until = Instant::now() + timeout;
        let mut server = loop {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    ensure!(peer.ip().is_loopback(), "baseline nonlocal peer"); break stream;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                    std::thread::sleep(Duration::from_millis(1)),
                Err(e) => return Err(e.into()),
            }
        };
        server.set_read_timeout(Some(timeout))?; server.set_write_timeout(Some(timeout))?;
        read_exact_message(&mut server, &request)?;
        server.write_all(&reply)?; server.shutdown(Shutdown::Write)?;
        read_exact_message(&mut client, &reply)?; Ok(())
    }
    pub fn verify_after_child(&self, nonce: &str) -> Result<()> {
        match self.listener.accept() {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
            Ok(_) => bail!("unexpected connection reached the retained owned listener"),
        }
        self.baseline(nonce)
    }
}
/// Only WSAEACCES, not timeout/refusal or a broad ErrorKind, proves this denial.
pub fn is_explicit_network_denial(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(10013)
}
pub fn observe_child_denial(port: u16) -> Result<()> {
    ensure!(port != 0, "network witness port");
    match TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_secs(2)) {
        Err(e) if is_explicit_network_denial(&e) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("network result is not explicit WSAEACCES; Unknown: {e}")),
        Ok(_) => bail!("restricted child reached owned loopback listener"),
    }
}
pub fn security_stdout(nonce: &str, spec: &SecuritySpec) -> Vec<u8> {
    format!("C28-SID {nonce} {} {}\nC28-NET {nonce} 127.0.0.1 {} denied=10013\n",
        spec.expected_sid, crate::preflight::hex(&spec.query.sha256), spec.port).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    const NONCE: &str = "00112233445566778899aabbccddeeff";
    #[test]
    fn sid_csv_requires_one_correlated_canonical_account() {
        assert_eq!(parse_sid_csv(b"\"vm\\user\",\"S-1-5-21-1-2-3-1001\"\r\n").unwrap(), "S-1-5-21-1-2-3-1001");
        assert!(parse_sid_csv(b"\"vm\\user\",\"S-1-5-21-01-2-3-1001\"\n").is_err());
        assert!(parse_sid_csv(b"\"vm\",\"S-1-5-21-1-2-3-1001\"\nsecond\n").is_err());
        assert!(parse_sid_csv(b"\"vm\",\"S-1-5-21-1-2-3-1001\",\"extra\"").is_err());
        assert!(parse_sid_csv(b"\"vm\",\"S-1-5-18\"").is_err());
    }
    #[test]
    fn challenge_bytes_are_fixed_and_nonce_bound() {
        assert_eq!(challenge(NONCE, false).unwrap(), format!("C28-LOCAL-REQUEST {NONCE}\n").as_bytes());
        assert_ne!(challenge(NONCE, false).unwrap(), challenge(NONCE, true).unwrap());
        assert!(challenge("wrong", false).is_err());
    }
    #[test]
    fn timeout_refusal_and_untyped_permission_denied_are_not_network_proof() {
        assert!(is_explicit_network_denial(&std::io::Error::from_raw_os_error(10013)));
        for code in [10060, 10061, 5, 995] {
            assert!(!is_explicit_network_denial(&std::io::Error::from_raw_os_error(code)));
        }
        assert!(!is_explicit_network_denial(&std::io::Error::new(std::io::ErrorKind::PermissionDenied, "synthetic")));
    }
}
