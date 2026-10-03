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
            "morrow-orphan-guardian-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let data = fs::read(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture")).unwrap();
        let artifact = root.join("peer");
        fs::write(&artifact, &data).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            artifact,
            hash: digest(&data),
        }
    }
    fn run(&self, mode: &str) -> String {
        let executable = SealedExecutable::read(&self.artifact, self.hash).unwrap();
        let mut g = OwnedProcess::spawn(
            executable,
            &self.root,
            &[
                OsStr::new("orphan-guardian-peer"),
                self.artifact.as_os_str(),
                OsStr::new(mode),
            ],
        )
        .unwrap();
        let completed = g.wait_bounded(Duration::from_secs(14)).unwrap();
        if !completed {
            g.terminate().unwrap();
            let _ = g.wait_bounded(Duration::from_secs(2));
        }
        let out = String::from_utf8(g.output().stdout.clone()).unwrap();
        let err = String::from_utf8(g.output().stderr.clone()).unwrap();
        assert!(completed, "{mode}: {out}\n{err}");
        assert_eq!(
            g.exit_observation(),
            Some(ExitObservation::Exited(0)),
            "{mode}: {out}\n{err}"
        );
        assert!(g.cleanup_complete() && g.output().stdout_eof && g.output().stderr_eof);
        println!(
            "mode={mode};outer-exact-G-reaped=true;G-stdout-eof=true;G-stderr-eof=true\n{out}"
        );
        out
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn scalar(line: &str, prefix: &str) -> u32 {
    line.split_whitespace()
        .find_map(|v| v.strip_prefix(prefix))
        .unwrap()
        .trim_end_matches(';')
        .parse()
        .unwrap()
}
fn assert_real_adoption(out: &str) {
    let g = out.lines().find(|l| l.starts_with("G pid=")).unwrap();
    let c = out.lines().find(|l| l.starts_with("C2 pid=")).unwrap();
    let s = out.lines().find(|l| l.starts_with("S2 pid=")).unwrap();
    let d = out.lines().find(|l| l.starts_with("D pid=")).unwrap();
    let gp = scalar(g, "pid=");
    let cp = scalar(c, "pid=");
    let sp = scalar(s, "pid=");
    let dp = scalar(d, "pid=");
    assert_eq!(scalar(c, "parent="), gp);
    assert_eq!(scalar(s, "initial-parent="), cp);
    assert_eq!(scalar(s, "original-C="), cp);
    assert_eq!(scalar(s, "G="), gp);
    assert_eq!(scalar(d, "parent="), sp);
    assert_eq!(scalar(d, "pdeathsig="), 9);
    assert_ne!(gp, cp);
    assert_ne!(cp, sp);
    assert_ne!(sp, dp);
    assert_ne!(gp, sp);
    assert_ne!(gp, dp);
    assert_ne!(cp, dp);
    assert!(out.contains("subreaper=1"));
    assert!(out.contains("pre-adoption-wait=ECHILD"));
    assert!(
        out.contains(
            "post-adoption-live-wait=true;S-parent=G;original-C-loss=OriginalProcessExited"
        )
    );
    assert!(out.contains("S-only-pdeathsig=0"));
    assert!(out.contains("OriginalProcessExited;sticky=true;data-admitted=1;late-heartbeat-rejected=1;late-data-denied=1;D-exact-reaped=true;D-stdout-eof=true;D-stderr-eof=true;control-io-retired=true"));
    assert!(out.contains("G-exact-C-reaped=true;C-stdout-eof=true;C-stderr-eof=true;G-exact-adopted-S-reaped=true;S-stdout-eof=true;S-stderr-eof=true;passive-C-writer-holder=G;tree-empty=unproved;product-owner=false"));
}
#[test]
fn original_c_death_adopts_live_supervisor_and_rejects_genuine_late_frames() {
    let f = Fixture::new();
    let out = f.run("positive");
    assert_real_adoption(&out);
}
#[test]
fn injected_subreaper_setup_refusal_creates_no_controller_supervisor_or_dependent() {
    let f = Fixture::new();
    let out = f.run("setup-reject");
    assert!(out.contains("setup-error-injected=true;subreaper-before=0;no-C-no-S-no-D=true;no-deliberate-controller-death=true"));
    assert!(!out.contains("S2 pid="));
}
#[test]
fn malformed_registration_rights_roles_and_replay_refuse_death_handoff() {
    let f = Fixture::new();
    for mode in [
        "registration-missing",
        "registration-extra",
        "registration-truncated",
        "registration-role",
        "registration-replay",
    ] {
        let out = f.run(mode);
        assert!(out.contains(&format!("registration-refused={mode}")));
        assert!(out.contains("C-still-live=true;no-deliberate-controller-death=true"));
        assert!(out.contains(
            "S-live-C-exact-reaped=true;S-stdout-eof=true;S-stderr-eof=true;no-dependent-child"
        ));
        assert!(out.contains("registration-negative=true"));
        assert!(!out.contains("D pid="));
    }
}
#[test]
fn adopted_supervisor_reap_cannot_complete_with_g_retained_stdout_writer() {
    let f = Fixture::new();
    let out = f.run("hold-stdout");
    assert_real_adoption(&out);
    assert!(out.contains("G-owned-hold-stdout-writer=true;S-exact-reaped-before-output-release=true;cleanup-with-one-missing-eof=false;stdout-eof=false;stderr-eof=true"));
}
#[test]
fn adopted_supervisor_reap_cannot_complete_with_g_retained_stderr_writer() {
    let f = Fixture::new();
    let out = f.run("hold-stderr");
    assert_real_adoption(&out);
    assert!(out.contains("G-owned-hold-stderr-writer=true;S-exact-reaped-before-output-release=true;cleanup-with-one-missing-eof=false;stdout-eof=true;stderr-eof=false"));
}

#[test]
fn forked_controller_reclaim_refuses_before_signalling_actual_supervisor() {
    let f = Fixture::new();
    let out = f.run("fork-reclaim");
    assert_real_adoption(&out);
    assert!(out.contains("fork-reclaim-refused-before-signal=true;"));
    assert!(out.contains("fork-witness-exact-reaped=true;real-S-retained-by-C=true"));
}
