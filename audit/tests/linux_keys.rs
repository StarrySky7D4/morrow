#![cfg(target_os = "linux")]
use morrow_audit::keys::Key;
#[cfg(feature = "linux-test-keyring")]
use morrow_audit::keys::KeyError;
#[cfg(feature = "linux-test-keyring")]
use std::os::unix::fs::symlink;
use std::{fs, os::unix::fs::PermissionsExt};
fn private_directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
#[test]
fn missing_bus_child() {
    let Some(path) = std::env::var_os("MORROW_MISSING_BUS_KEY_PATH") else {
        return;
    };
    assert!(Key::create(std::path::Path::new(&path)).is_err());
    assert!(!std::path::Path::new(&path).exists());
}
#[test]
fn missing_or_nonlocal_bus_fails_without_creating_a_key() {
    for address in [None, Some("tcp:host=127.0.0.1,port=1"), Some("autolaunch:")] {
        let directory = private_directory();
        let path = directory.path().join("key");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "missing_bus_child", "--nocapture"])
            .env("MORROW_MISSING_BUS_KEY_PATH", &path)
            .env_remove("DBUS_SESSION_BUS_ADDRESS");
        if let Some(address) = address {
            command.env("DBUS_SESSION_BUS_ADDRESS", address);
        }
        assert!(command.status().unwrap().success());
        assert!(!path.exists());
    }
}
#[cfg(feature = "linux-test-keyring")]
#[test]
fn explicit_synthetic_fixture_roundtrips_without_plaintext_seed_or_overwrite() {
    let _fixture = morrow_audit::keys::test_keyring::activate().unwrap();
    let directory = private_directory();
    let path = directory.path().join("key");
    let key = Key::create(&path).unwrap().trust();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let loaded = Key::load(&path).unwrap().trust();
    assert_eq!(loaded.id, key.id);
    assert_eq!(loaded.key, key.key);
    assert!(matches!(Key::create(&path), Err(KeyError::AlreadyExists)));
    assert_eq!(bytes, fs::read(&path).unwrap());
    let other = Key::create(&directory.path().join("other"))
        .unwrap()
        .trust();
    assert_ne!(key.id, other.id);
    assert_ne!(key.key, other.key);
    let mut damaged = bytes.clone();
    damaged[8] ^= 1;
    fs::write(&path, &damaged).unwrap();
    assert!(Key::load(&path).is_err());
    assert!(matches!(Key::create(&path), Err(KeyError::AlreadyExists)));
    assert_eq!(damaged, fs::read(&path).unwrap());
}
#[cfg(feature = "linux-test-keyring")]
#[test]
fn key_files_reject_symlinks_hardlinks_and_modes() {
    let _fixture = morrow_audit::keys::test_keyring::activate().unwrap();
    let directory = private_directory();
    let path = directory.path().join("key");
    Key::create(&path).unwrap();
    fs::hard_link(&path, directory.path().join("hard")).unwrap();
    assert!(Key::load(&path).is_err());
    fs::remove_file(directory.path().join("hard")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Key::load(&path).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let alias = directory.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(Key::load(&alias).is_err());
}
