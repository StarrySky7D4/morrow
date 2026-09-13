#![cfg(all(feature = "plugin-adapter", windows))]
use morrow_network_node::{
    HttpRequest, Limits,
    client::{Client, EndpointPolicy},
};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn actual_https_executable_authenticates_and_runs_a_frozen_rust_plugin() {
    let temp = tempfile::tempdir().unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_path = temp.path().join("certificate.pem");
    let key_path = temp.path().join("private-key.pem");
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();
    let token = "synthetic-tls-executable-node-token-0123456789abcdef";
    let token_path = temp.path().join("node-token");
    std::fs::write(&token_path, token).unwrap();
    let package = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/compat/guest-v1-rc1/rust-transform.mplugin");
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_morrow-api-node"))
        .arg("plugin-tls")
        .arg(package)
        .arg(temp.path().join("new-service"))
        .arg("bytes.reverse")
        .arg(token_path)
        .arg(cert_path)
        .arg(key_path)
        .arg("127.0.0.1:0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(process.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let address = line
        .strip_prefix("Morrow API node listening on https://")
        .unwrap()
        .strip_suffix("/v1/invoke")
        .unwrap();
    let port = address.rsplit_once(':').unwrap().1;
    let origin = format!("https://localhost:{port}");
    let client = Client::with_root_certificate(
        EndpointPolicy::local_https(&origin, &["POST"]).unwrap(),
        Limits::default(),
        cert.cert.der(),
    )
    .unwrap();
    for (authorized, status) in [(false, 401), (true, 200)] {
        let response = client
            .send(
                HttpRequest {
                    method: "POST".into(),
                    target: format!("{origin}/v1/invoke"),
                    headers: if authorized {
                        vec![("authorization".into(), format!("Bearer {token}"))]
                    } else {
                        vec![]
                    },
                    body: vec![0, 65, 255, 90],
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, status);
        if authorized {
            assert_eq!(response.body, vec![90, 255, 65, 0]);
        }
    }
    assert!(!line.contains(token));
    // Exact child cleanup; graceful-stop behavior has separate live server tests.
    process.kill().await.unwrap();
    process.wait().await.unwrap();
}
