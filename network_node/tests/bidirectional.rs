#![cfg(feature = "plugin-adapter")]
use morrow_network_node::{
    HttpRequest, Limits,
    client::{Client, EndpointPolicy},
    plugin::PluginService,
    server::{Handler, Node, Route},
};
use std::{path::Path, sync::Arc};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn external_request_calls_node_then_upstream_then_real_three_language_guests() {
    for language in ["c", "cpp", "rust"] {
        let temp = tempfile::tempdir().unwrap();
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../sdk/compat/guest-v1-rc1/{language}-transform.mplugin"
        ));
        let service =
            PluginService::open(&package, &temp.path().join("isolated.db"), "bytes.reverse")
                .unwrap();
        let token = "synthetic-test-api-token-0123456789abcdef";
        let upstream = Node::bind(
            "127.0.0.1:0".parse().unwrap(),
            token.into(),
            vec![Route::new("POST", "/compute", service.route_handler()).unwrap()],
            Limits::default(),
        )
        .await
        .unwrap();
        let upstream_origin = format!("http://{}", upstream.local_addr());
        let outgoing = Arc::new(
            Client::new(
                EndpointPolicy::new(&upstream_origin, &["POST"], true).unwrap(),
                Limits::default(),
            )
            .unwrap(),
        );
        let handler: Handler = Arc::new(move |request, cancel| {
            let outgoing = outgoing.clone();
            let target = format!("{upstream_origin}/compute");
            Box::pin(async move {
                let mut response = outgoing
                    .send(
                        HttpRequest {
                            method: "POST".into(),
                            target,
                            headers: vec![
                                ("authorization".into(), format!("Bearer {token}")),
                                ("content-type".into(), "application/octet-stream".into()),
                            ],
                            body: request.body,
                        },
                        cancel,
                    )
                    .await?;
                response.headers.retain(|(key, _)| key == "content-type");
                Ok(response)
            })
        });
        let outer_token = "synthetic-outer-node-secret-0123456789abcdef";
        let node = Node::bind(
            "127.0.0.1:0".parse().unwrap(),
            outer_token.into(),
            vec![Route::new("POST", "/api", handler).unwrap()],
            Limits::default(),
        )
        .await
        .unwrap();
        let origin = format!("http://{}", node.local_addr());
        let client = Client::new(
            EndpointPolicy::new(&origin, &["POST"], true).unwrap(),
            Limits::default(),
        )
        .unwrap();
        for input in [
            vec![],
            vec![0, 1, 255, 2],
            "API 网络 世界".as_bytes().to_vec(),
        ] {
            let mut expected = input.clone();
            expected.reverse();
            let response = client
                .send(
                    HttpRequest {
                        method: "POST".into(),
                        target: format!("{origin}/api"),
                        headers: vec![("authorization".into(), format!("Bearer {outer_token}"))],
                        body: input,
                    },
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(response.body, expected, "actual {language} package output");
        }
        let response = client
            .send(
                HttpRequest {
                    method: "POST".into(),
                    target: format!("{origin}/api"),
                    headers: vec![],
                    body: vec![],
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(response.status, 401);
        node.shutdown().await.unwrap();
        upstream.shutdown().await.unwrap();
        service.shutdown().await.unwrap();
    }
}

#[test]
fn service_refuses_existing_database_and_content_capable_package() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing.db");
    std::fs::write(&existing, b"user data").unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/compat/guest-v1-rc1");
    assert!(
        PluginService::open(
            &root.join("rust-transform.mplugin"),
            &existing,
            "bytes.reverse"
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&existing).unwrap(), b"user data");
    let not_created = temp.path().join("not-created.db");
    assert!(
        PluginService::open(
            &root.join("rust-task.mplugin"),
            &not_created,
            "bytes.reverse"
        )
        .is_err()
    );
    assert!(!not_created.exists());
}

#[test]
fn isolated_service_directory_refuses_existing_sidecars_without_touching_them() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("service");
    std::fs::create_dir(&root).unwrap();
    let sidecar = root.join("workbench.db-wal");
    std::fs::write(&sidecar, b"preserved unrelated sidecar").unwrap();
    let package = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/compat/guest-v1-rc1/rust-transform.mplugin");
    assert!(PluginService::open(&package, &root, "bytes.reverse").is_err());
    assert_eq!(
        std::fs::read(&sidecar).unwrap(),
        b"preserved unrelated sidecar"
    );
    assert!(!root.join("workbench.db").exists());
}
