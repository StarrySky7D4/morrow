//! Compiled SDK IO guests through the real package, manager, binding, and file carrier.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{Request, Response, Status},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner, file_io::FileBroker, manager::Manager,
};
use std::{collections::BTreeSet, path::PathBuf};

const ID: &str = "org.example.sdk.rust-io";
const HANDLER: &str = "file.read-selected";

fn guests() -> Vec<(&'static str, Vec<u8>)> {
    let mut guests = Vec::new();
    for (language, key) in [
        ("Rust", "MORROW_RUST_IO_WASM"),
        ("C", "MORROW_SDK_IO_GUEST_C"),
        ("C++", "MORROW_SDK_IO_GUEST_CPP"),
    ] {
        let path = std::env::var_os(key).unwrap_or_else(|| {
            panic!("build all three IO guests with tool/verify_plugin_io_sdk.ps1; {key} is unset")
        });
        let wasm = std::fs::read(PathBuf::from(path))
            .unwrap_or_else(|error| panic!("read {language} IO guest: {error}"));
        guests.push((language, wasm));
    }
    guests
}

fn package(wasm: &[u8]) -> Package {
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", wasm, vec![]);
    let mut declaration = io::declaration(vec![IoCapability::FileRead], vec![HANDLER.into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_resources = 2;
    budget.max_jobs = 2;
    budget.max_bytes = 1_000_000;
    budget.max_job_bytes = 200_000;
    budget.max_duration_ms = 100;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    Package::build(manifest, wasm).unwrap()
}

#[test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_sdk.ps1"]
fn compiled_sdk_guest_reads_and_finishes_host_selected_file() {
    for (_language, wasm) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let package = package(&wasm);
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        let approval = BTreeSet::from([IoCapability::FileRead]);
        manager
            .approve_io(ID, package.digest(), approval.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &approval,
                90,
                1,
            )
            .unwrap();
        let mut broker = FileBroker::new([0x63; 32]);
        let file_ref = broker
            .grant_file(
                &manager,
                &host,
                &instance,
                &binding,
                b"selected bytes".to_vec(),
                2,
            )
            .unwrap();
        let read = Request::encode_read(1, &file_ref, 0, 14).unwrap();
        let result = broker.run(&manager, &host, &instance, &binding, HANDLER, &read, || 3);
        assert_eq!(result.execution.outcome, Ok(0));
        assert_eq!(result.execution.host_calls, 1);
        let response = result.response.expect("host verified guest-delivered read");
        assert_eq!(response.status, Status::Completed);
        assert_eq!(response.payload, b"selected bytes");
        assert_eq!(broker.usage(), (1, 14));
        let finish = Request::encode_finish(2, &file_ref).unwrap();
        let result = broker.run(&manager, &host, &instance, &binding, HANDLER, &finish, || 4);
        assert_eq!(result.execution.outcome, Ok(0));
        assert_eq!(result.execution.host_calls, 1);
        assert_eq!(result.response.unwrap().status, Status::Completed);
        assert_eq!(broker.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
    }
}

#[test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_sdk.ps1"]
fn package_declaration_without_manager_approval_cannot_bind() {
    for (_language, wasm) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let package = package(&wasm);
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let requested = BTreeSet::from([IoCapability::FileRead]);
        assert!(
            manager
                .bind_io(
                    &host,
                    &instance,
                    package.digest(),
                    manager.revision(),
                    &requested,
                    90,
                    1
                )
                .is_err()
        );
    }
}

#[test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_sdk.ps1"]
fn sdk_guest_rejects_bad_input_and_uncorrelated_broker_response() {
    for (_language, wasm) in guests() {
        let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
        let mut calls = 0;
        let bad = runner.run_task_with_io(
            b"not a capnp IO request",
            &mut |_| panic!("core exchange"),
            &mut |_| {
                calls += 1;
                Err(())
            },
            Cancellation::default(),
        );
        assert_ne!(bad.report.outcome, Ok(0));
        assert!(bad.completion.is_none());
        assert_eq!(calls, 0);

        let selected = [0x71; 32];
        let original = Request::encode_read(1, &selected, 0, 2).unwrap();
        let other = Request::encode_read(2, &selected, 0, 2).unwrap();
        let wrong = Response::encode(&other, Status::Completed, b"ok", 0, true).unwrap();
        let mismatch = runner.run_task_with_io(
            original.bytes(),
            &mut |_| panic!("core exchange"),
            &mut |request| {
                calls += 1;
                assert_eq!(request, original.bytes());
                Ok(wrong.clone())
            },
            Cancellation::default(),
        );
        assert_ne!(mismatch.report.outcome, Ok(0));
        assert!(mismatch.completion.is_none());
        assert_eq!(calls, 1);
    }
}

#[test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_sdk.ps1"]
fn cross_instance_file_denial_is_validated_and_completed_by_sdk_guest() {
    for (_language, wasm) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let package = package(&wasm);
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        let approval = BTreeSet::from([IoCapability::FileRead]);
        manager
            .approve_io(ID, package.digest(), approval.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let owner = manager.connect(ID, &mut host).unwrap();
        let other = manager.connect(ID, &mut host).unwrap();
        let owner_binding = manager
            .bind_io(
                &host,
                &owner,
                package.digest(),
                manager.revision(),
                &approval,
                90,
                1,
            )
            .unwrap();
        let other_binding = manager
            .bind_io(
                &host,
                &other,
                package.digest(),
                manager.revision(),
                &approval,
                90,
                1,
            )
            .unwrap();
        let mut broker = FileBroker::new([0x64; 32]);
        let file_ref = broker
            .grant_file(
                &manager,
                &host,
                &owner,
                &owner_binding,
                b"private".to_vec(),
                2,
            )
            .unwrap();
        let request = Request::encode_read(3, &file_ref, 0, 7).unwrap();
        let raw_denied = broker
            .exchange(&manager, &host, &other, &other_binding, &request, 3)
            .unwrap();
        assert_eq!(
            Response::decode(&request, &raw_denied).unwrap().status,
            Status::Denied
        );
        let sdk_request = morrow_plugin_sdk::io::Request::decode(request.bytes()).unwrap();
        assert_eq!(
            sdk_request.decode_reply(&raw_denied).unwrap().status,
            morrow_plugin_sdk::io::Status::Denied,
        );
        let direct = Runner::new_io_task(&wasm, Limits::default())
            .unwrap()
            .run_task_with_io(
                request.bytes(),
                &mut |_| panic!("core exchange"),
                &mut |_| Ok(raw_denied.clone()),
                Cancellation::default(),
            );
        assert_eq!(direct.report.outcome, Ok(0));
        assert_eq!(direct.report.host_calls, 1);
        assert_eq!(direct.completion.as_deref(), Some(raw_denied.as_slice()));
        // The managed carrier additionally suppresses delivery to a foreign owner.
        let denied = broker.run(
            &manager,
            &host,
            &other,
            &other_binding,
            HANDLER,
            &request,
            || 3,
        );
        assert_eq!(denied.execution.outcome, Err(Fault::InactiveConnection));
        assert_eq!(denied.execution.host_calls, 1);
        assert!(denied.response.is_none());
        assert_eq!(other_binding.usage().bytes, 0);
        let missing = Request::encode_read(4, &[0x72; 32], 0, 7).unwrap();
        let absent = broker.run(
            &manager,
            &host,
            &other,
            &other_binding,
            HANDLER,
            &missing,
            || 4,
        );
        assert_eq!(absent.execution.outcome, Ok(0));
        assert_eq!(absent.response.unwrap().status, Status::NotFound);
        let owner_read = broker.run(
            &manager,
            &host,
            &owner,
            &owner_binding,
            HANDLER,
            &request,
            || 4,
        );
        assert_eq!(owner_read.execution.outcome, Ok(0));
        assert_eq!(owner_read.response.unwrap().payload, b"private");
    }
}

#[test]
#[ignore = "requires all three compiled IO guests; run tool/verify_plugin_io_sdk.ps1"]
fn sdk_guest_completes_core_encoded_http_status_and_rejects_wrong_http_original() {
    use morrow_core::io::{Header, HttpOutcome, HttpSubmission};
    for (_language, wasm) in guests() {
        let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
        let submission = HttpSubmission {
            operation_id: b"sdk-http-1".to_vec(),
            deadline_ms: 50,
            endpoint: b"pinned-endpoint".to_vec(),
            method: "GET".into(),
            relative_target: "/item".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        };
        let request = Request::encode_http_submit(5, &submission).unwrap();
        let outcome = HttpOutcome {
            status: Status::Completed,
            http_status: 429,
            headers: vec![Header {
                name: "retry-after".into(),
                value: b"2".to_vec(),
            }],
            body: b"slow down".to_vec(),
        };
        let encoded = Response::encode_http(&request, &outcome).unwrap();
        let run = runner.run_task_with_io(
            request.bytes(),
            &mut |_| panic!("core exchange"),
            &mut |bytes| {
                assert_eq!(bytes, request.bytes());
                Ok(encoded.clone())
            },
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Ok(0));
        assert_eq!(run.report.host_calls, 1);
        assert_eq!(run.completion.as_deref(), Some(encoded.as_slice()));
        let wrong_request = Request::encode_http_submit(6, &submission).unwrap();
        let wrong_encoded = Response::encode_http(&wrong_request, &outcome).unwrap();
        let wrong = runner.run_task_with_io(
            request.bytes(),
            &mut |_| panic!("core exchange"),
            &mut |_| Ok(wrong_encoded.clone()),
            Cancellation::default(),
        );
        assert_ne!(wrong.report.outcome, Ok(0));
        assert!(wrong.completion.is_none());
    }
}
