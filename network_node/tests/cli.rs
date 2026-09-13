#![cfg(feature = "plugin-adapter")]
use std::{path::Path, process::Command};
#[test]
fn invalid_token_is_rejected_before_creating_any_service_directory() {
    let temp = tempfile::tempdir().unwrap();
    let token = temp.path().join("token");
    std::fs::write(&token, b"too short").unwrap();
    let data = temp.path().join("must-not-exist");
    let package = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/compat/guest-v1-rc1/rust-transform.mplugin");
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-api-node"))
        .arg("plugin")
        .arg(package)
        .arg(&data)
        .arg("bytes.reverse")
        .arg(&token)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "network: Invalid"
    );
    assert!(!data.exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("too short"));
}

#[test]
fn invalid_tls_identity_is_rejected_before_creating_service_data() {
    let temp = tempfile::tempdir().unwrap();
    let token = temp.path().join("token");
    std::fs::write(&token, b"synthetic-node-secret-0123456789abcdef").unwrap();
    let cert = temp.path().join("cert.pem");
    let key = temp.path().join("key.pem");
    std::fs::write(&cert, b"not a certificate").unwrap();
    std::fs::write(&key, b"synthetic-invalid-private-key").unwrap();
    let data = temp.path().join("must-not-exist");
    let package = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/compat/guest-v1-rc1/rust-transform.mplugin");
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-api-node"))
        .arg("plugin-tls")
        .arg(package)
        .arg(&data)
        .arg("bytes.reverse")
        .arg(&token)
        .arg(&cert)
        .arg(&key)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "network: Invalid"
    );
    assert!(!data.exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-key"));
}
