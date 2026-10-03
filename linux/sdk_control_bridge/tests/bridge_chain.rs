#![cfg(target_os = "linux")]
use morrow_linux_supervisor_foundation::{
    ControllerLimits, ExitObservation, SealedExecutable, digest, fixture_channel_pair,
};
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};
struct Fixture {
    dir: tempfile::TempDir,
    artifact: PathBuf,
    hash: [u8; 32],
    repo: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let bytes = fs::read(env!("CARGO_BIN_EXE_morrow-linux-sdk-control-fixture")).unwrap();
        let artifact = dir.path().join("sdk-bridge-peer");
        fs::write(&artifact, &bytes).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            dir,
            artifact,
            hash: digest(&bytes),
            repo: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        }
    }
    fn sealed(&self) -> SealedExecutable {
        SealedExecutable::read(&self.artifact, self.hash).unwrap()
    }
    fn run(&self, language: &str, mode: &str) -> String {
        let root = self.dir.path().join(format!("worker-{language}-{mode}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let (c, s) = fixture_channel_pair(ControllerLimits {
            heartbeat_timeout: Duration::from_millis(200),
            frame_timeout: Duration::from_millis(200),
        })
        .unwrap();
        let mut c = c
            .spawn(
                self.sealed(),
                self.dir.path(),
                &[OsStr::new("controller"), OsStr::new(mode)],
            )
            .unwrap();
        let original = c.original_controller().unwrap();
        let mut s = s
            .bind_original(original)
            .unwrap()
            .spawn(
                self.sealed(),
                self.dir.path(),
                &[
                    OsStr::new("supervisor"),
                    self.repo.as_os_str(),
                    root.as_os_str(),
                    OsStr::new(language),
                ],
            )
            .unwrap();
        // Retained typed original-object witnesses, never numeric PID probes.
        let c_live = c.original_controller().unwrap();
        let s_live = s.original_controller().unwrap();
        let started = Instant::now();
        let mut collected = false;
        while started.elapsed() < Duration::from_secs(9) {
            c.observe().unwrap();
            s.observe().unwrap();
            let text = String::from_utf8(c.output().stdout.clone()).unwrap();
            let marker = if mode == "held-worker" {
                "actual-worker-held=true;native-driver-still-decoding-heartbeats=true"
            } else {
                "actual-SDK-client=true;original-task-commit=true;original-transform-output=true"
            };
            let ready_to_collect = if mode == "malformed-call" {
                String::from_utf8(s.output().stdout.clone())
                    .unwrap()
                    .contains("error-original-worker-joined=true;primary-error-retained=true")
            } else if mode == "sdk-close" {
                text.contains("actual-SDK-Close-Query=true;closed-channel-Receive-Send-refused=true;original-task-after-close-commit=true;original-transform-after-close-output=true") && String::from_utf8(s.output().stdout.clone()).unwrap().contains("actual-sdk-channel-close-broker-Joined=true;native-watch-live=true;product-gate-open=true;actual-worker-live=true;native-control-io-retired=false;producer-outcome=Unknown;last-acked=1;accepted-sequence=1")
            } else {
                text.contains(marker)
            };
            if !collected && ready_to_collect {
                if mode == "sdk-close" {
                    assert!(
                        !c_live.poll_exited().unwrap() && !s_live.poll_exited().unwrap(),
                        "C/S original pidfds must still be live at channel join"
                    );
                    assert!(c.exit_observation().is_none() && s.exit_observation().is_none());
                    assert!(
                        !c.output().stdout_eof
                            && !c.output().stderr_eof
                            && !s.output().stdout_eof
                            && !s.output().stderr_eof
                    );
                    println!(
                        "G-at-channel-Joined:C-original-live=true;S-original-live=true;C-S-not-reaped=true;four-OS-output-EOF=false"
                    );
                }
                if mode == "malformed-call" {
                    assert!(
                        c.exit_observation().is_none(),
                        "original C must remain genuinely alive through authenticated-error cleanup"
                    );
                }
                c.terminate().unwrap();
                collected = true;
            }
            if c.cleanup_complete() && s.cleanup_complete() {
                break;
            }
            if c.exit_observation().is_some() && !collected {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if !c.cleanup_complete() {
            c.terminate().unwrap();
            let _ = c.wait_bounded(Duration::from_secs(2));
        }
        if !s.cleanup_complete() {
            s.terminate().unwrap();
            let _ = s.wait_bounded(Duration::from_secs(2));
        }
        let co = String::from_utf8(c.output().stdout.clone()).unwrap();
        let ce = String::from_utf8(c.output().stderr.clone()).unwrap();
        let so = String::from_utf8(s.output().stdout.clone()).unwrap();
        let se = String::from_utf8(s.output().stderr.clone()).unwrap();
        println!(
            "language={language};mode={mode};G pid={};C pid={};S pid={}\n{co}\n{so}\n{ce}\n{se}",
            std::process::id(),
            c.pid(),
            s.pid()
        );
        assert!(collected, "{co}\n{so}\n{ce}\n{se}");
        assert_eq!(
            c.exit_observation(),
            Some(ExitObservation::Signaled(libc::SIGKILL)),
            "{ce}"
        );
        assert_eq!(
            s.exit_observation(),
            Some(ExitObservation::Exited(if mode == "malformed-call" {
                101
            } else {
                0
            })),
            "{so}\n{se}"
        );
        assert!(c.cleanup_complete() && s.cleanup_complete());
        assert!(
            c.output().stdout_eof
                && c.output().stderr_eof
                && s.output().stdout_eof
                && s.output().stderr_eof
        );
        if mode == "malformed-call" {
            assert!(so.contains("native-error-gate-closed=true;native-control-io-retired=true;error-original-worker-joined=true;primary-error-retained=true;failed-call-is-not-success=true"), "{so}\n{se}");
            assert!(so.contains("original-after-loss-host-calls=0;foreign-original-guest-task-live=true;late-registration-stopped=true;no-automatic-replay=true;actual-broker-join=Joined;broker-resource-reclaimed=true"), "{so}\n{se}");
            assert!(
                se.contains("SDK-FIXTURE-FAILED: empty SDK fixture call"),
                "{se}"
            );
            return so;
        }
        for marker in [
            "native-original-loss=Some(OriginalProcessExited);gate-closed=true;stop-original-without-SQL-worker-lock=true",
            "original-managed-cancelled=true;original-after-loss-host-calls=0;foreign-original-guest-task-live=true;late-registration-stopped=true;no-automatic-replay=true",
            "actual-broker-join=Joined;broker-resource-reclaimed=true;ACK-is-not-OS-cleanup=true",
            "actual-original-worker-joined=true;native-control-io-retired=true;tree-empty=unproved;Released=false",
        ] {
            assert!(so.contains(marker), "{so}\n{se}");
        }
        if mode == "sdk-close" {
            assert!(so.contains("actual-sdk-channel-close-broker-Joined=true;native-watch-live=true;product-gate-open=true;actual-worker-live=true;native-control-io-retired=false;producer-outcome=Unknown;last-acked=1;accepted-sequence=1"), "{so}\n{se}");
            assert!(
                so.contains("actual-source-terminal=Producer.wait_sent phase=")
                    && so.contains(
                        "producer-outcome=Unknown;Accepted-does-not-prove-observation=true"
                    ),
                "{so}\n{se}"
            );
            assert!(co.contains("actual-SDK-Close-Query=true;closed-channel-Receive-Send-refused=true;original-task-after-close-commit=true;original-transform-after-close-output=true;original-grant-budget-unchanged=true"), "{co}\n{ce}");
        }
        assert!(co.contains("wrong-ACK-denied=true;cursor-ACK=true;send-Accepted-not-success=true;live-producer-not-reclaimed=true;no-auto-replay=true"));
        so
    }
}
#[test]
fn original_rust_guest_and_actual_sdk_client_broker_revoke_only_original_control() {
    let f = Fixture::new();
    f.run("rust", "normal");
}
#[test]
fn original_loss_closes_gate_while_real_worker_is_held_without_driver_lock_wait() {
    let f = Fixture::new();
    f.run("rust", "held-worker");
}
#[test]
fn original_c_guest_and_actual_sdk_client_broker_revoke_only_original_control() {
    let f = Fixture::new();
    f.run("c", "normal");
}
#[test]
fn original_cpp_guest_and_actual_sdk_client_broker_revoke_only_original_control() {
    let f = Fixture::new();
    f.run("cpp", "normal");
}

#[test]
fn authenticated_malformed_call_preserves_primary_error_and_reaps_real_worker() {
    let f = Fixture::new();
    f.run("rust", "malformed-call");
}

#[test]
fn actual_sdk_close_joins_only_channel_while_original_control_and_guests_continue() {
    let f = Fixture::new();
    f.run("rust", "sdk-close");
}
