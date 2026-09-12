#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::{Key, KeyError},
    sealer::Sealer,
};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
fn fill(store: &mut Store, n: u32) {
    for i in 0..n {
        store
            .create_local(
                &format!("create-{i}"),
                &CardRecord::new(&format!("card-{i}"), "text", 1, "Synthetic", vec![1]).unwrap(),
            )
            .unwrap();
    }
}
#[test]
fn dpapi_random_keys_reload_and_never_overwrite() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("key");
    let a = Key::create(&p).unwrap().trust();
    let before = std::fs::read(&p).unwrap();
    let b = Key::create(&d.path().join("other")).unwrap().trust();
    assert_ne!(a.id, b.id);
    assert_ne!(a.key, b.key);
    let loaded = Key::load(&p).unwrap().trust();
    assert_eq!(a.id, loaded.id);
    assert_eq!(a.key, loaded.key);
    assert!(matches!(Key::create(&p), Err(KeyError::AlreadyExists)));
    assert_eq!(std::fs::read(&p).unwrap(), before);
}
#[test]
fn missing_malformed_and_tampered_keys_fail_without_replacement() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("key");
    assert!(Key::load(&p).is_err());
    assert!(!p.exists());
    Key::create(&p).unwrap();
    let original = std::fs::read(&p).unwrap();
    for at in [0, 8, original.len() - 1] {
        let mut damaged = original.clone();
        damaged[at] ^= 0x80;
        std::fs::write(&p, &damaged).unwrap();
        assert!(Key::load(&p).is_err());
        assert!(matches!(Key::create(&p), Err(KeyError::AlreadyExists)));
        assert_eq!(std::fs::read(&p).unwrap(), damaged);
    }
    std::fs::write(&p, vec![0; 65537]).unwrap();
    assert!(Key::load(&p).is_err());
}
#[test]
fn bounded_sealer_resumes_after_reopen_and_rejects_wrong_key() {
    let d = tempfile::tempdir().unwrap();
    let key = d.path().join("key");
    let db = d.path().join("db");
    let sealer = Sealer::new(Key::create(&key).unwrap());
    let mut store = Store::open_audited(&db, EventBudget::default(), true, sealer.trust()).unwrap();
    fill(&mut store, 130);
    assert!(sealer.flush(&mut store, 0).is_err());
    let one = sealer.flush(&mut store, 1).unwrap();
    assert_eq!(one.events, 128);
    assert!(one.more_pending);
    let wrong = Sealer::new(Key::create(&d.path().join("other")).unwrap());
    assert!(wrong.flush(&mut store, 1).is_err());
    assert_eq!(store.pending(0, 10).unwrap().len(), 2);
    drop(store);
    drop(sealer);
    let sealer = Sealer::new(Key::load(&key).unwrap());
    let mut store =
        Store::open_audited(&db, EventBudget::default(), false, sealer.trust()).unwrap();
    let two = sealer.flush(&mut store, 1).unwrap();
    assert_eq!(two.events, 2);
    assert!(!two.more_pending);
    assert_eq!(sealer.flush(&mut store, 1).unwrap().events, 0);
    store.integrity_check().unwrap();
}
#[test]
fn cli_missing_key_and_repeated_create_do_not_touch_database() {
    let d = tempfile::tempdir().unwrap();
    let key = d.path().join("key");
    let db = d.path().join("db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    fill(&mut store, 1);
    drop(store);
    let before = std::fs::read(&db).unwrap();
    let exe = env!("CARGO_BIN_EXE_morrow-audit-seal");
    let output = std::process::Command::new(exe)
        .arg(&key)
        .arg(&db)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!key.exists());
    assert_eq!(std::fs::read(&db).unwrap(), before);
    let output = std::process::Command::new(exe)
        .arg("--new-key")
        .arg(&key)
        .output()
        .unwrap();
    assert!(output.status.success());
    let protected = std::fs::read(&key).unwrap();
    let output = std::process::Command::new(exe)
        .arg("--new-key")
        .arg(&key)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&key).unwrap(), protected);
    let output = std::process::Command::new(exe)
        .arg(&key)
        .arg(&db)
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = std::process::Command::new(exe)
        .arg(&key)
        .arg(&db)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("0 segments / 0 events")
    );
}
#[cfg(feature = "fault-injection")]
#[test]
fn key_child() {
    let Ok(root) = std::env::var("MORROW_KEY_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    if std::env::var("MORROW_AUDIT_CRASH_AT").unwrap() == "sealer-after-batch" {
        let sealer = Sealer::new(Key::load(&root.join("key")).unwrap());
        let mut store = Store::open_audited(
            &root.join("db"),
            EventBudget::default(),
            false,
            sealer.trust(),
        )
        .unwrap();
        sealer.flush(&mut store, 2).unwrap();
    } else {
        Key::create(&root.join("key")).unwrap();
    }
    panic!("crash boundary missed");
}
#[cfg(feature = "fault-injection")]
#[test]
fn publication_and_batch_crashes_recover_without_rotating_identity() {
    for point in [
        "key-before-publish",
        "key-after-publish",
        "sealer-after-batch",
    ] {
        let d = tempfile::tempdir().unwrap();
        let key = d.path().join("key");
        let db = d.path().join("db");
        let original = if point == "sealer-after-batch" {
            let key = Key::create(&key).unwrap();
            let trust = key.trust();
            let mut store =
                Store::open_audited(&db, EventBudget::default(), true, trust.clone()).unwrap();
            fill(&mut store, 130);
            Some(trust)
        } else {
            None
        };
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "key_child", "--nocapture"])
            .env("MORROW_KEY_ROOT", d.path())
            .env("MORROW_AUDIT_CRASH_AT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        if point == "key-before-publish" {
            assert!(!key.exists());
            Key::create(&key).unwrap();
        } else {
            assert!(matches!(Key::create(&key), Err(KeyError::AlreadyExists)));
        }
        let loaded = Key::load(&key).unwrap();
        if let Some(original) = original {
            assert_eq!(loaded.trust().key, original.key);
            assert_eq!(loaded.trust().id, original.id);
            let sealer = Sealer::new(loaded);
            let mut store =
                Store::open_audited(&db, EventBudget::default(), false, sealer.trust()).unwrap();
            assert_eq!(store.pending(0, 10).unwrap().len(), 2);
            assert_eq!(sealer.flush(&mut store, 2).unwrap().events, 2);
            store.integrity_check().unwrap();
        }
        println!("PASS protected-key crash {point}");
    }
}

#[test]
fn oversized_pending_group_shrinks_without_skipping_events() {
    let d = tempfile::tempdir().unwrap();
    let sealer = Sealer::new(Key::create(&d.path().join("key")).unwrap());
    let mut store = Store::open_audited(
        &d.path().join("db"),
        EventBudget::default(),
        true,
        sealer.trust(),
    )
    .unwrap();
    let mut seed = 0x12345678u32;
    let body: Vec<u8> = (0..7 * 1024 * 1024)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    for i in 0..5 {
        store
            .create_local(
                &format!("create-{i}"),
                &CardRecord::new(
                    &format!("card-{i}"),
                    "text",
                    1,
                    "Large synthetic",
                    body.clone(),
                )
                .unwrap(),
            )
            .unwrap();
    }
    assert!(matches!(
        store.pending(0, 128),
        Err(morrow_core::Error::Limit)
    ));
    let first = sealer.flush(&mut store, 1).unwrap();
    assert_eq!(first.events, 4);
    assert!(first.more_pending);
    assert_eq!(store.pending(0, 1).unwrap()[0].0, 5);
    let last = sealer.flush(&mut store, 1).unwrap();
    assert_eq!(last.events, 1);
    assert!(!last.more_pending);
    store.integrity_check().unwrap();
}
