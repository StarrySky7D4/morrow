//! Independently built original standalone IO archives through the production file carrier.
//! Three methods exercise original archives; malformed and HTTP-codec methods exercise modules only.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{Request, Response, Status},
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits, Runner, file_io::FileBroker, manager::Manager,
};
use sha2::Digest;
use std::collections::BTreeSet;

fn guests() -> Vec<(&'static str, Vec<u8>, Package)> {
    [("Rust", "RUST"), ("C", "C"), ("C++", "CPP")]
        .into_iter()
        .map(|(language, suffix)| {
            let prefix = format!("MORROW_STANDALONE_IO_{suffix}");
            let input = |name| std::env::var(format!("{prefix}_{name}")).unwrap();
            let archive = std::fs::read(input("PACKAGE")).unwrap();
            let wasm = std::fs::read(input("WASM")).unwrap();
            for (name, bytes) in [("PACKAGE_SHA256", &archive), ("WASM_SHA256", &wasm)] {
                let sha = sha2::Sha256::digest(bytes);
                assert_eq!(format!("{sha:x}"), input(name), "independent {language} {name}");
            }
            let package = Package::decode(&archive).expect("original standalone archive");
            assert_eq!(package.archive(), archive);
            assert_eq!(package.module(), wasm);
            assert_eq!(package.manifest().package_id, input("ID"));
            assert_eq!(package.manifest().package_version, "0.1.0");
            assert!(package.capabilities().is_empty());
            let budget = package.manifest().budget.as_ref().unwrap();
            assert_eq!(budget.fuel, 20_000_000);
            assert_eq!(budget.memory_bytes, 16 * 1024 * 1024);
            assert_eq!(budget.host_calls, 16);
            let declaration = package.io_declaration().unwrap();
            assert_eq!(declaration.handlers, ["io.request"]);
            assert_eq!(declaration.requested_capabilities, [IoCapability::FileRead.number()]);
            eprintln!("standalone_original language={language} id={} archive_sha256={} wasm_sha256={} archive_unchanged=true", package.manifest().package_id, input("PACKAGE_SHA256"), input("WASM_SHA256"));
            (language, wasm, package)
        })
        .collect()
}

fn declared_limits(package: &Package) -> Limits {
    let budget = package.manifest().budget.as_ref().unwrap();
    Limits {
        fuel: budget.fuel,
        memory_bytes: usize::try_from(budget.memory_bytes).unwrap(),
        host_calls: budget.host_calls,
    }
}

#[test]
#[ignore = "requires three independently pinned original standalone IO archives"]
fn compiled_sdk_guest_reads_and_finishes_host_selected_file() {
    for (_language, _wasm, package) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, declared_limits(&package));
        manager.select(&package, manager.revision()).unwrap();
        let approval = BTreeSet::from([IoCapability::FileRead]);
        manager
            .approve_io(
                &package.manifest().package_id,
                package.digest(),
                approval.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
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
        // The original compiled SDK module receives no OS path or handle.
        let original = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let path = dir.path().join("host-selected.bin");
        std::fs::write(&path, &original).unwrap();
        let selected = broker
            .grant_open_file(
                &manager,
                &host,
                &instance,
                &binding,
                std::fs::File::open(&path).unwrap(),
                original.len() as u64,
                || 2,
            )
            .unwrap();
        assert_eq!(
            selected.sha256,
            <[u8; 32]>::from(sha2::Sha256::digest(&original))
        );
        std::fs::remove_file(&path).unwrap();
        let mut retained = Vec::new();
        for (id, offset) in [(1, 0), (2, 65_536)] {
            let read = Request::encode_read(id, &selected.reference, offset, 0).unwrap();
            let result = broker.run(
                &manager,
                &host,
                &instance,
                &binding,
                "io.request",
                &read,
                || 3,
            );
            assert_eq!(result.execution.outcome, Ok(0));
            assert_eq!(result.execution.host_calls, 1);
            let response = result.response.expect("host verified guest-delivered read");
            assert_eq!(response.status, Status::Completed);
            assert_eq!(response.offset, offset);
            assert_eq!(response.eof, offset > 0);
            retained.extend(response.payload);
        }
        assert_eq!(retained, original);
        assert_eq!(broker.usage(), (1, original.len() as u64));
        let finish = Request::encode_finish(3, &selected.reference).unwrap();
        let result = broker.run(
            &manager,
            &host,
            &instance,
            &binding,
            "io.request",
            &finish,
            || 4,
        );
        assert_eq!(result.execution.outcome, Ok(0));
        assert_eq!(result.execution.host_calls, 1);
        assert_eq!(result.response.unwrap().status, Status::Completed);
        assert_eq!(broker.usage(), (0, 0));
        assert_eq!(binding.usage().resources, 0);
    }
}

#[test]
#[ignore = "requires three independently pinned original standalone IO archives"]
fn package_declaration_without_manager_approval_cannot_bind() {
    for (_language, _wasm, package) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, declared_limits(&package));
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
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
#[ignore = "requires three independently pinned original standalone IO archives"]
fn sdk_guest_rejects_bad_input_and_uncorrelated_broker_response() {
    for (_language, wasm, package) in guests() {
        let runner = Runner::new_io_task(&wasm, declared_limits(&package)).unwrap();
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
#[ignore = "requires three independently pinned original standalone IO archives"]
fn cross_instance_file_denial_is_validated_and_completed_by_sdk_guest() {
    for (_language, wasm, package) in guests() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, declared_limits(&package));
        manager.select(&package, manager.revision()).unwrap();
        let approval = BTreeSet::from([IoCapability::FileRead]);
        manager
            .approve_io(
                &package.manifest().package_id,
                package.digest(),
                approval.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(
                &package.manifest().package_id,
                package.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let owner = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
        let other = manager
            .connect(&package.manifest().package_id, &mut host)
            .unwrap();
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
        let direct = Runner::new_io_task(&wasm, declared_limits(&package))
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
            "io.request",
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
            "io.request",
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
            "io.request",
            &request,
            || 4,
        );
        assert_eq!(owner_read.execution.outcome, Ok(0));
        assert_eq!(owner_read.response.unwrap().payload, b"private");
    }
}

#[test]
#[ignore = "requires three independently pinned original standalone IO archives"]
fn sdk_guest_completes_core_encoded_http_status_and_rejects_wrong_http_original() {
    use morrow_core::io::{Header, HttpOutcome, HttpSubmission};
    for (_language, wasm, package) in guests() {
        let runner = Runner::new_io_task(&wasm, declared_limits(&package)).unwrap();
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
