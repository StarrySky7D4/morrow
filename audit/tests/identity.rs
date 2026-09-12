#![cfg(target_os = "windows")]
use morrow_audit::{
    identity::LeaseError,
    keys::Key,
    sealer::Sealer,
    session::{OpenMode, Session, SessionError, key_path},
    snapshot,
};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
use std::{
    io::{Read, Write},
    path::Path,
};
fn open(db: &Path) -> Result<Session, SessionError> {
    Session::open(db, EventBudget::default(), OpenMode::Initialize)
}
fn copies(root: &Path) {
    let db = root.join("workbench.db");
    let mut owner = open(&db).unwrap();
    owner
        .runtime()
        .store_local_mut()
        .create_local(
            "op",
            &CardRecord::new("card", "text", 1, "original", vec![1]).unwrap(),
        )
        .unwrap();
    owner.backup_snapshot(&root.join("archive")).unwrap();
    snapshot::restore(&root.join("archive"), &root.join("copy")).unwrap();
}
#[test]
fn same_identity_copy_is_blocked_before_writes_but_read_only_verification_remains_available() {
    let d = tempfile::tempdir().unwrap();
    copies(d.path());
    let db = d.path().join("workbench.db");
    let owner = open(&db).unwrap();
    let other = d.path().join("copy/workbench.db");
    let before = std::fs::read(&other).unwrap();
    assert!(matches!(open(&other), Err(SessionError::IdentityBusy)));
    assert!(matches!(
        Sealer::new(Key::load(&key_path(&other).unwrap()).unwrap()),
        Err(LeaseError::Busy)
    ));
    let readonly = Store::open_read_only_audited(&other, owner.trust()).unwrap();
    assert!(readonly.card("card").unwrap().is_some());
    drop(readonly);
    assert_eq!(std::fs::read(&other).unwrap(), before);
    let independent = d.path().join("independent.db");
    let unrelated = open(&independent).unwrap();
    assert_ne!(unrelated.trust().id, owner.trust().id);
    drop(owner);
    let reopened = open(&other).unwrap();
    assert!(reopened.store().card("card").unwrap().is_some());
}
#[test]
fn standalone_legacy_sealer_and_bound_cli_obey_the_same_identity_lease() {
    let d = tempfile::tempdir().unwrap();
    copies(d.path());
    let owner = open(&d.path().join("workbench.db")).unwrap();
    let other = d.path().join("copy/workbench.db");
    let before = std::fs::read(&other).unwrap();
    for arguments in [
        vec![
            key_path(&other).unwrap().into_os_string(),
            other.clone().into_os_string(),
        ],
        vec!["--open-bound".into(), other.clone().into_os_string()],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_morrow-audit-seal"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(std::fs::read(&other).unwrap(), before);
    }
    drop(owner);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_morrow-audit-seal"))
        .args([key_path(&other).unwrap().as_os_str(), other.as_os_str()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn identity_child() {
    let Ok(root) = std::env::var("MORROW_IDENTITY_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    if std::env::var("MORROW_IDENTITY_TEST_MODE").as_deref() == Ok("probe") {
        assert!(matches!(
            open(&root.join("copy/workbench.db")),
            Err(SessionError::IdentityBusy)
        ));
        return;
    }
    let _owner = open(&root.join("workbench.db")).unwrap();
    std::fs::write(root.join("ready"), b"ready").unwrap();
    std::io::stdin().read_exact(&mut [0; 1]).unwrap();
}
#[test]
fn environment_overrides_cannot_split_identity_ownership() {
    let d = tempfile::tempdir().unwrap();
    copies(d.path());
    let _owner = open(&d.path().join("workbench.db")).unwrap();
    let alternate = d.path().join("alternate-profile");
    std::fs::create_dir_all(alternate.join("AppData/Local")).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "identity_child", "--nocapture"])
        .env("MORROW_IDENTITY_TEST_ROOT", d.path())
        .env("MORROW_IDENTITY_TEST_MODE", "probe")
        .env("USERPROFILE", &alternate)
        .env("LOCALAPPDATA", alternate.join("AppData/Local"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn normal_exit_and_process_kill_release_live_ownership_without_deleting_guard_files() {
    for abrupt in [false, true] {
        let d = tempfile::tempdir().unwrap();
        copies(d.path());
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "identity_child", "--nocapture"])
            .env("MORROW_IDENTITY_TEST_ROOT", d.path())
            .env("MORROW_IDENTITY_TEST_MODE", "owner")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !d.path().join("ready").exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                panic!("owner exited before readiness");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !d.path().join("ready").exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owner readiness timeout");
        }
        let other = d.path().join("copy/workbench.db");
        let blocked = matches!(open(&other), Err(SessionError::IdentityBusy));
        if abrupt {
            child.kill().unwrap();
        } else {
            child.stdin.take().unwrap().write_all(b"x").unwrap();
        }
        let status = child.wait().unwrap();
        assert!(blocked);
        if !abrupt {
            assert!(status.success());
        }
        let recovered = open(&other).unwrap();
        assert!(recovered.store().card("card").unwrap().is_some());
    }
}

#[test]
fn sealer_ownership_can_move_between_threads_and_releases_on_the_new_thread() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("key");
    let sealer = Sealer::new(Key::create(&path).unwrap()).unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let owned = sealer;
        ready_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        drop(owned);
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    let blocked = matches!(
        Sealer::new(Key::load(&path).unwrap()),
        Err(LeaseError::Busy)
    );
    release_tx.send(()).unwrap();
    worker.join().unwrap();
    assert!(blocked);
    assert!(Sealer::new(Key::load(&path).unwrap()).is_ok());
}
