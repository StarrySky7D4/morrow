//! Real three-language guest -> managed worker -> TCP -> durable receipt.
use super::*;

struct SdkGuest {
    language: &'static str,
    wasm: Vec<u8>,
    package: Option<Package>,
}
fn guests() -> Vec<SdkGuest> {
    let keys = [
        ("Rust", "MORROW_RUST_IO_WASM", "MORROW_SDK_IO_PACKAGE_RUST"),
        ("C", "MORROW_SDK_IO_GUEST_C", "MORROW_SDK_IO_PACKAGE_C"),
        (
            "C++",
            "MORROW_SDK_IO_GUEST_CPP",
            "MORROW_SDK_IO_PACKAGE_CPP",
        ),
    ];
    let packaged = keys
        .iter()
        .any(|(_, _, key)| std::env::var_os(key).is_some());
    keys.into_iter()
        .map(|(language, wasm_key, package_key)| {
            if packaged {
                let path = std::env::var_os(package_key)
                    .unwrap_or_else(|| panic!("all three packages required: {package_key}"));
                let package =
                    morrow_core::plugin_package::catalog::read_file(std::path::Path::new(&path))
                        .expect("original generated IO package");
                // Match the workbench HTTP input profile, not capability inference.
                assert!(
                    package
                        .io_declaration()
                        .unwrap()
                        .handlers
                        .iter()
                        .any(|h| h == "morrow.http.forward.v1"),
                    "generated HTTP package must be eligible for the workbench HTTP entry"
                );
                SdkGuest {
                    language,
                    wasm: package.module().to_vec(),
                    package: Some(package),
                }
            } else {
                let path = std::env::var_os(wasm_key).unwrap_or_else(|| {
                    panic!("run tool/verify_plugin_io_network.ps1; missing {wasm_key}")
                });
                SdkGuest {
                    language,
                    wasm: std::fs::read(path).expect("compiled IO guest"),
                    package: None,
                }
            }
        })
        .collect()
}
fn running(
    wasm: &[u8],
    package: Option<&Package>,
    config: EndpointApproval,
    credentials: bool,
) -> Running {
    Running::build_with_package(config, credentials, false, None, None, Some(wasm), package)
        .unwrap()
}

#[tokio::test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_network.ps1"]
async fn seven_methods_preserve_binary_bytes_headers_and_durable_receipts() {
    const METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
    for SdkGuest {
        language,
        wasm,
        package,
    } in guests()
    {
        for method in METHODS {
            let body: &[u8] = if method == "HEAD" {
                b""
            } else {
                b"\0\xffreply"
            };
            let mut server = Server::new(Some(raw_response("200 OK", body)), Duration::ZERO).await;
            let mut config = approval(&server.origin);
            config.methods = METHODS.iter().map(|m| (*m).into()).collect();
            let mut run = running(&wasm, package.as_ref(), config, false);
            let operation = format!("sdk-{method}");
            let mut input = submission(&run.endpoint, &operation);
            input.method = method.into();
            let (request, mut job) = run.submit(&input);
            let report = consume(&mut job).await;
            assert_eq!(report.task.execution.outcome, Ok(0), "{language} {method}");
            assert!(!report.unknown);
            let outcome = report.http_response.as_ref().unwrap();
            assert_eq!(outcome.status, Status::Completed);
            assert_eq!(outcome.http_status, 200);
            assert_eq!(outcome.body, body);
            assert_eq!(
                outcome
                    .headers
                    .iter()
                    .filter(|h| h.name == "x-repeat")
                    .map(|h| h.value.as_slice())
                    .collect::<Vec<_>>(),
                vec![b"one".as_slice(), b"two".as_slice()]
            );
            let wire = server.request().await;
            assert!(wire.starts_with(format!("{method} /api?q=one&q=two HTTP/1.1\r\n").as_bytes()));
            let body_start = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            assert_eq!(&wire[body_start..], input.body.as_slice());
            let headers = String::from_utf8_lossy(&wire[..body_start]).to_ascii_lowercase();
            assert!(
                headers.contains("x-request: one\r\n") && headers.contains("x-request: two\r\n")
            );
            assert_eq!(server.calls.load(Ordering::SeqCst), 1);
            assert_observed(&run.finish().await, &request, &operation, &report);
        }
    }
}

#[tokio::test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_network.ps1"]
async fn credential_injection_and_http_error_are_observed_without_secret_in_request_material() {
    for SdkGuest {
        language,
        wasm,
        package,
    } in guests()
    {
        let mut server = Server::new(
            Some(raw_response("429 Too Many Requests", b"limited")),
            Duration::ZERO,
        )
        .await;
        let mut config = approval(&server.origin);
        config.credential = Some(
            Credential::header(
                b"sdk-credential".to_vec(),
                "authorization",
                &format!("Bearer {TOKEN}"),
            )
            .unwrap(),
        );
        let mut run = running(&wasm, package.as_ref(), config, true);
        let mut input = submission(&run.endpoint, "sdk-credential");
        input.credential = b"sdk-credential".to_vec();
        let (request, mut job) = run.submit(&input);
        let report = consume(&mut job).await;
        assert_eq!(report.task.execution.outcome, Ok(0), "{language}");
        let outcome = report.http_response.as_ref().unwrap();
        assert_eq!(outcome.status, Status::Completed);
        assert_eq!(outcome.http_status, 429);
        assert_eq!(outcome.body, b"limited");
        let wire = server.request().await;
        assert!(
            String::from_utf8_lossy(&wire).contains(&format!("authorization: Bearer {TOKEN}\r\n"))
        );
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        assert!(
            !request
                .bytes()
                .windows(TOKEN.len())
                .any(|w| w == TOKEN.as_bytes())
        );
        let host = run.finish().await;
        assert_observed(&host, &request, "sdk-credential", &report);
        let stored = host
            .store_local()
            .io_material(ID, "sdk-credential", Kind::Request)
            .unwrap()
            .unwrap();
        assert!(
            !stored
                .payload()
                .windows(TOKEN.len())
                .any(|w| w == TOKEN.as_bytes())
        );
    }
}

#[tokio::test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_network.ps1"]
async fn unapproved_endpoint_method_and_credential_never_send_or_create_intent() {
    for SdkGuest {
        language,
        wasm,
        package,
    } in guests()
    {
        for mismatch in 0..3 {
            let server =
                Server::new(Some(raw_response("200 OK", b"unwanted")), Duration::ZERO).await;
            let mut run = running(&wasm, package.as_ref(), approval(&server.origin), true);
            let mut input = submission(&run.endpoint, "sdk-deny");
            match mismatch {
                0 => input.endpoint = b"wrong-endpoint".to_vec(),
                1 => input.method = "DELETE".into(),
                2 => input.credential = b"wrong-credential".to_vec(),
                _ => unreachable!(),
            }
            let (_, mut job) = run.submit(&input);
            let report = consume(&mut job).await;
            assert!(
                report.task.execution.outcome.is_err(),
                "{language} {mismatch}"
            );
            assert!(!report.unknown);
            assert!(report.http_response.is_none());
            assert_eq!(server.calls.load(Ordering::SeqCst), 0);
            assert!(
                run.finish()
                    .await
                    .store_local()
                    .lookup_io_intent(ID, "sdk-deny")
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_network.ps1"]
async fn dropped_response_is_unknown_not_retried_and_survives_store_reopen() {
    for SdkGuest {
        language,
        wasm,
        package,
    } in guests()
    {
        let mut server = Server::new(None, Duration::ZERO).await;
        let mut run = running(&wasm, package.as_ref(), approval(&server.origin), false);
        let input = submission(&run.endpoint, "sdk-unknown");
        let (_, mut first) = run.submit(&input);
        server.request().await;
        let report = consume(&mut first).await;
        assert!(
            report.unknown && report.task.execution.outcome.is_err(),
            "{language}"
        );
        assert!(report.http_response.is_none());
        let (_, mut second) = run.submit(&input);
        let again = consume(&mut second).await;
        assert!(again.unknown && again.task.execution.outcome.is_err());
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        let host = run.finish().await;
        assert_unknown(&host, "sdk-unknown");
        drop(host);
        let reopened = HostRuntime::new(
            Store::open_existing(&run._dir.path().join("db"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        assert_unknown(&reopened, "sdk-unknown");
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    }
}
