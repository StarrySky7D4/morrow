#![cfg(target_os = "linux")]
use morrow_linux_supervisor_foundation::{ExitObservation, OwnedProcess, SealedExecutable, digest};
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    hash: [u8; 32],
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "morrow-current-controller-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let bytes = fs::read(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture")).unwrap();
        let artifact = root.join("peer");
        fs::write(&artifact, &bytes).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            artifact,
            hash: digest(&bytes),
        }
    }
    fn run(&self, mode: &str) -> (u32, String) {
        let sealed = SealedExecutable::read(&self.artifact, self.hash).unwrap();
        let mut controller = OwnedProcess::spawn(
            sealed,
            &self.root,
            &[
                OsStr::new("current-controller-peer"),
                self.artifact.as_os_str(),
                OsStr::new(mode),
            ],
        )
        .unwrap();
        let c = controller.pid();
        let original = controller.original_controller().unwrap();
        assert_eq!(original.diagnostic_pid(), c);
        assert!(!original.poll_exited().unwrap());
        let complete = controller.wait_bounded(Duration::from_secs(8)).unwrap();
        if !complete {
            controller.terminate().unwrap();
            assert!(controller.wait_bounded(Duration::from_secs(2)).unwrap());
        }
        let stdout = String::from_utf8(controller.output().stdout.clone()).unwrap();
        let stderr = String::from_utf8(controller.output().stderr.clone()).unwrap();
        assert!(complete, "{stdout}\n{stderr}");
        assert_eq!(
            controller.exit_observation(),
            Some(ExitObservation::Exited(0)),
            "{stdout}\n{stderr}"
        );
        assert!(controller.cleanup_complete());
        assert!(original.poll_exited().unwrap());
        assert!(controller.output().stdout_eof && controller.output().stderr_eof);
        println!(
            "mode={mode};G pid={};C exact-pidfd-reaped=true;C stdout-eof=true;C stderr-eof=true\n{stdout}",
            std::process::id()
        );
        (c, stdout)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn number(line: &str, key: &str) -> u32 {
    line.split_whitespace()
        .find_map(|item| item.strip_prefix(key))
        .unwrap()
        .parse()
        .unwrap()
}
fn assert_chain(c: u32, text: &str) {
    let cline = text.lines().find(|l| l.starts_with("C pid=")).unwrap();
    let sline = text.lines().find(|l| l.starts_with("S pid=")).unwrap();
    let dline = text.lines().find(|l| l.starts_with("D pid=")).unwrap();
    let s = number(sline, "pid=");
    let d = number(dline, "pid=");
    assert_eq!(number(cline, "pid="), c);
    assert_eq!(number(cline, "parent="), std::process::id());
    assert_eq!(number(cline, "S-owned-pid="), s);
    assert_eq!(number(sline, "parent="), c);
    assert_eq!(number(sline, "original-C="), c);
    assert_eq!(number(sline, "pdeathsig="), libc::SIGKILL as u32);
    assert_eq!(number(cline, "pdeathsig="), libc::SIGKILL as u32);
    assert_eq!(number(dline, "parent="), s);
    assert_eq!(number(dline, "pdeathsig="), libc::SIGKILL as u32);
    assert_ne!(c, s);
    assert_ne!(s, d);
    assert_ne!(c, d);
    assert!(text.contains("data-admitted=1;"), "{text}");
    assert!(
        text.contains(
            "direct-child-reaped=true;stdout-eof=true;stderr-eof=true;control-io-retired=true"
        ),
        "{text}"
    );
    assert!(
        text.contains("C-live=true;S-exact-pidfd-reaped=true;S-stdout-eof=true;S-stderr-eof=true"),
        "{text}"
    );
    assert!(
        text.contains("tree-empty=unproved;product-owner=false"),
        "{text}"
    );
}
#[test]
fn current_parent_authenticated_heartbeat_data_reply_and_close_reap_each_owned_child() {
    let f = Fixture::new();
    let (c, text) = f.run("normal");
    assert_chain(c, &text);
    assert!(text.contains("loss=None;"), "{text}");
}
#[test]
fn live_current_controller_heartbeat_expiry_is_sticky_and_reclaims_dependent() {
    let f = Fixture::new();
    let (c, text) = f.run("expiry");
    assert_chain(c, &text);
    assert!(
        text.contains("loss=Some(HeartbeatExpired);sticky-first-loss=true"),
        "{text}"
    );
}
#[test]
fn live_current_controller_transport_eof_retires_io_and_reclaims_dependent() {
    let f = Fixture::new();
    let (c, text) = f.run("eof");
    assert_chain(c, &text);
    assert!(
        text.contains("loss=Some(TransportEof);sticky-first-loss=true"),
        "{text}"
    );
}
#[test]
fn authenticated_data_before_decoded_heartbeat_cannot_create_or_replay_dependent() {
    let f = Fixture::new();
    let (c, text) = f.run("pre-heartbeat");
    assert_chain(c, &text);
    assert!(text.contains("pre-heartbeat-denied=1;"), "{text}");
    assert!(
        text.contains("pre-rejected=true;no-replay-confirmed=true"),
        "{text}"
    );
}
#[test]
fn inherited_self_capture_is_refused_in_fork_before_supervisor_creation() {
    let f = Fixture::new();
    let (_, text) = f.run("fork-reuse");
    assert!(
        text.contains("fork-reuse-refused-before-supervisor-launch=true;"),
        "{text}"
    );
    assert!(
        text.contains("witness-exact-pidfd-reaped=true;no-dependent-child"),
        "{text}"
    );
    assert!(!text.contains("S pid="));
    assert!(!text.contains("D pid="));
}
