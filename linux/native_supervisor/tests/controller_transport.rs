#![cfg(target_os = "linux")]
use morrow_linux_supervisor_foundation::{
    ControllerLimits, OwnedProcess, SealedExecutable, digest, fixture_channel_pair,
};
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
            "morrow-controller-foundation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let artifact = root.join("peer");
        let bytes = fs::read(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture")).unwrap();
        fs::write(&artifact, &bytes).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        Self {
            root,
            artifact,
            hash: digest(&bytes),
        }
    }
    fn sealed(&self) -> SealedExecutable {
        SealedExecutable::read(&self.artifact, self.hash).unwrap()
    }
    fn pair(&self, mode: &str) -> (OwnedProcess, OwnedProcess) {
        self.pair_with(mode, "normal")
    }
    fn pair_with(&self, mode: &str, supervisor_mode: &str) -> (OwnedProcess, OwnedProcess) {
        let (controller, supervisor) = fixture_channel_pair(ControllerLimits {
            heartbeat_timeout: Duration::from_millis(200),
            frame_timeout: Duration::from_millis(200),
        })
        .unwrap();
        let controller = controller
            .spawn(
                self.sealed(),
                &self.root,
                &[OsStr::new("controller-peer"), OsStr::new(mode)],
            )
            .unwrap();
        let original = controller.original_controller().unwrap();
        assert_eq!(original.diagnostic_pid(), controller.pid());
        assert!(!original.poll_exited().unwrap());
        let supervisor = supervisor
            .bind_original(original)
            .unwrap()
            .spawn(
                self.sealed(),
                &self.root,
                &[
                    OsStr::new("supervisor-peer"),
                    self.artifact.as_os_str(),
                    OsStr::new(supervisor_mode),
                ],
            )
            .unwrap();
        (controller, supervisor)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn output(owner: &OwnedProcess) -> String {
    String::from_utf8(owner.output().stdout.clone()).unwrap()
}
fn assert_tracked_only(supervisor: &OwnedProcess) {
    let text = output(supervisor);
    assert!(text.contains("data-admitted=1"), "{text}");
    assert!(
        text.contains(
            "direct-child-reaped=true;stdout-eof=true;stderr-eof=true;control-io-retired=true"
        ),
        "{text}"
    );
    assert!(
        text.contains("tree-empty=unproved;product-owner=false"),
        "{text}"
    );
}
#[test]
fn inherited_controller_channel_tracks_close_without_product_release() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) = fixture.pair("normal");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
    assert_tracked_only(&supervisor);
    assert!(output(&controller).contains("controller-tracked-status"));
}
#[test]
fn original_controller_death_revokes_despite_descendant_retained_pipe() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) = fixture.pair("retain-and-exit");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert_tracked_only(&supervisor);
    assert!(
        output(&supervisor).contains("OriginalProcessExited"),
        "{}",
        output(&supervisor)
    );
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
}
#[test]
fn heartbeat_stall_revokes_without_controller_process_exit() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) = fixture.pair("stall");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert_tracked_only(&supervisor);
    assert!(
        output(&supervisor).contains("HeartbeatExpired"),
        "{}",
        output(&supervisor)
    );
    assert!(
        !controller
            .original_controller()
            .unwrap()
            .poll_exited()
            .unwrap()
    );
    controller.terminate().unwrap();
    assert!(controller.wait_bounded(Duration::from_secs(2)).unwrap());
}
#[test]
fn eof_revokes_even_while_original_controller_remains_live() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) = fixture.pair("eof-alive");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert_tracked_only(&supervisor);
    assert!(
        output(&supervisor).contains("TransportEof"),
        "{}",
        output(&supervisor)
    );
    assert!(
        !controller
            .original_controller()
            .unwrap()
            .poll_exited()
            .unwrap()
    );
    assert!(controller.wait_bounded(Duration::from_secs(2)).unwrap());
}
#[test]
fn controller_reference_cannot_be_created_after_reap() {
    let fixture = Fixture::new();
    let mut child =
        OwnedProcess::spawn(fixture.sealed(), &fixture.root, &[OsStr::new("echo")]).unwrap();
    assert!(child.wait_bounded(Duration::from_secs(2)).unwrap());
    assert!(child.original_controller().is_err());
}

#[test]
fn controller_loss_during_artifact_preparation_creates_no_dependent_child() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) =
        fixture.pair_with("exit-in-preparation", "delayed-preparation");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
    let text = output(&supervisor);
    assert!(text.contains("OriginalProcessExited"), "{text}");
    assert!(
        text.contains("no-dependent-child;data-admitted=0"),
        "{text}"
    );
}
#[test]
fn controller_loss_after_data_during_preclone_preparation_is_not_created() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) =
        fixture.pair_with("exit-in-launch-preparation", "delayed-launch");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
    let text = output(&supervisor);
    assert!(text.contains("OriginalProcessExited"), "{text}");
    assert!(
        text.contains("no-dependent-child;data-admitted=0;child-created=0"),
        "{text}"
    );
}
#[test]
fn claimed_role_descriptors_do_not_escape_to_the_dependent_child() {
    let fixture = Fixture::new();
    let (mut controller, mut supervisor) = fixture.pair_with("normal", "guest-fd-check");
    assert!(supervisor.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(controller.wait_bounded(Duration::from_secs(3)).unwrap());
    assert!(output(&supervisor).contains("guest-extra-open-fds=0"));
    assert_tracked_only(&supervisor);
}
#[test]
fn closed_stdio_cannot_clobber_typed_role_artifact_status_or_pipe_mapping() {
    let fixture = Fixture::new();
    let receipt = fixture.root.join("closed-controller.receipt");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture"))
        .arg("controller-harness-closed-stdio")
        .arg(&fixture.artifact)
        .arg(&fixture.root)
        .arg(&receipt)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        fs::read(receipt).unwrap(),
        b"typed-controller-closed-stdio-ok\n"
    );
}

#[test]
fn unlinked_ordinary_bootstrap_is_rejected_on_failed_seal_query_in_isolated_process() {
    let fixture = Fixture::new();
    let path = fixture.root.join("ordinary-bootstrap");
    let mut bytes = [0u8; 128];
    bytes[..8].copy_from_slice(b"MCBOOT01");
    bytes[8] = 1;
    bytes[9] = 1;
    bytes[16..20].copy_from_slice(&200u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&200u32.to_le_bytes());
    bytes[24..28].copy_from_slice(&(128u32 * 1024).to_le_bytes());
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_morrow-linux-supervisor-fixture"))
        .arg("bootstrap-ordinary-reject")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"ordinary-unlinked-bootstrap-rejected-before-read\n"
    );
}
