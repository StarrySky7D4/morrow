#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{Capability, TransformHandler},
        registry::Registry,
    },
    response::{Failure, Outcome, Response},
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
    task::{FailureCode, Invocation, PluginFailure, Transform},
    transaction::Lookup,
};
use morrow_plugin_runtime::{
    Fault, Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
};
const ID: &str = "test.pool.task";
fn record() -> CardRecord {
    CardRecord::new("card", "text", 1, "secret title", b"body".to_vec()).unwrap()
}
fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn module(input: &Invocation, segments: &[(usize, Vec<u8>)], body: &str) -> Vec<u8> {
    let mut data = format!("(data (i32.const 0) \"{}\")", escaped(input.bytes()));
    for (offset, bytes) in segments {
        data.push_str(&format!(
            "(data (i32.const {offset}) \"{}\")",
            escaped(bytes)
        ));
    }
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_v1" "exchange" (func $exchange (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 6) {data}
      (func $equal (param $a i32) (param $b i32) (param $n i32)
        (local $i i32)
        (block $end (loop $next
          local.get $i local.get $n i32.ge_u br_if $end
          local.get $a local.get $i i32.add i32.load8_u
          local.get $b local.get $i i32.add i32.load8_u
          i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.set $i br $next)))
      (func (export "morrow_run") (result i32) (local $n i32)
        i32.const 262144 i32.const 131072 call $read i32.const {size} i32.ne if unreachable end
        i32.const 0 i32.const 262144 i32.const {size} call $equal
        {body}))"#,
        size = input.bytes().len()
    ))
    .unwrap()
}
fn summary_input() -> Invocation {
    Invocation::new(
        "summary-task",
        &Command::ReadSummary {
            request_id: "query".into(),
            card_id: "card".into(),
        },
    )
    .unwrap()
}
fn summary_package() -> Package {
    let input = summary_input();
    let good = Response {
        request_id: "query".into(),
        outcome: Outcome::Summary(record().summary()),
    }
    .encode()
    .unwrap();
    let denied = Response {
        request_id: "query".into(),
        outcome: Outcome::Rejected(Failure::Denied),
    }
    .encode()
    .unwrap();
    let completion = input.completion(&good).unwrap();
    let rejected = input.completion(&denied).unwrap();
    let body = format!(
        r#"
      i32.const 16384 i32.const {} i32.const 196608 i32.const 65536 call $exchange local.set $n
      local.get $n i32.const {} i32.eq
      if
        i32.const 32768 i32.const 196608 local.get $n call $equal
        i32.const 65536 i32.const {} call $done drop
      else
        local.get $n i32.const {} i32.ne if unreachable end
        i32.const 49152 i32.const 196608 local.get $n call $equal
        i32.const 98304 i32.const {} call $done drop
      end i32.const 0"#,
        input.command_bytes().len(),
        good.len(),
        completion.len(),
        denied.len(),
        rejected.len()
    );
    let wasm = module(
        &input,
        &[
            (16384, input.command_bytes().to_vec()),
            (32768, good),
            (49152, denied),
            (65536, completion),
            (98304, rejected),
        ],
        &body,
    );
    Package::build(
        Package::manifest_for_task(ID, "1.0.0", &wasm, vec![Capability::ReadSummary]),
        &wasm,
    )
    .unwrap()
}
fn transform_input(handler: &str, bytes: Vec<u8>) -> Invocation {
    Invocation::new_transform(
        "transform-task",
        Transform {
            handler: handler.into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            input: bytes,
        },
    )
    .unwrap()
}
fn transform_package(mode: &str) -> Package {
    let input = transform_input("plain.transform", b"hello".to_vec());
    let completion = if mode == "failure" {
        input
            .failure_completion(&PluginFailure {
                code: FailureCode::UnsupportedInput,
                message: "unsupported sample".into(),
            })
            .unwrap()
    } else {
        input.output_completion(b"actual output").unwrap()
    };
    let body = match mode {
        "trap" => "unreachable".into(),
        "protocol" => "i32.const 0".into(),
        "limits" => "(loop $forever br $forever) i32.const 0".into(),
        _ => format!(
            "i32.const 16384 i32.const {} call $done drop i32.const 0",
            completion.len()
        ),
    };
    let wasm = module(&input, &[(16384, completion)], &body);
    let mut manifest = Package::manifest_for_transform(
        ID,
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "plain.transform".into(),
            input_type: "bytes".into(),
            output_type: "answer".into(),
            max_input_bytes: 1024,
            max_output_bytes: 1024,
        }],
    );
    if mode == "limits" {
        manifest.budget.as_mut().unwrap().fuel = 10000;
    }
    Package::build(manifest, &wasm).unwrap()
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    pool: Pool,
}
impl Fixture {
    fn new(package: &Package, grants: &[GrantKind]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
        catalog.install(package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(package, manager.revision()).unwrap();
        manager
            .approve(
                ID,
                package.digest(),
                grants.iter().copied().collect(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
        store.create_local("seed", &record()).unwrap();
        let host = HostRuntime::new(store).unwrap();
        let pool = Pool::new(&host, Default::default()).unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            pool,
        }
    }
    fn start(&mut self) -> Session {
        let revision = self.manager.revision();
        self.pool
            .start(&mut self.manager, &mut self.host, ID, &[], revision)
            .unwrap()
    }
    fn grant(&mut self, session: &Session, kind: GrantKind) {
        self.pool
            .grant_root(&mut self.host, session, kind, "card", 100, 0)
            .unwrap();
    }
}
#[test]
fn ordinary_transform_has_real_bound_output_without_dependency_providers() {
    let package = transform_package("output");
    assert!(package.manifest().dependencies.is_empty());
    let mut f = Fixture::new(&package, &[]);
    let session = f.start();
    let report = f
        .pool
        .run_task(
            &f.manager,
            &mut f.host,
            &session,
            &transform_input("plain.transform", b"hello".to_vec()),
            || panic!("pure task must not pretend to call dispatch clock"),
        )
        .unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 0);
    let output = report.output.unwrap();
    assert_eq!(output.type_id, "answer");
    assert_eq!(output.bytes, b"actual output");
    assert!(report.response.is_none() && report.failure.is_none());
    assert_eq!(f.pool.usage().providers, 0);
    assert!(f.pool.root(&session).is_ok());
}
#[test]
fn actual_read_response_and_scope_revocation_are_per_real_root() {
    let mut f = Fixture::new(&summary_package(), &[GrantKind::ReadSummary]);
    let a = f.start();
    let b = f.start();
    assert_ne!(
        f.pool.root(&a).unwrap().connection().binding(),
        f.pool.root(&b).unwrap().connection().binding()
    );
    f.grant(&a, GrantKind::ReadSummary);
    f.grant(&b, GrantKind::ReadSummary);
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &a, &summary_input(), || 1)
        .unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 1);
    assert!(
        matches!(report.response.unwrap().outcome,Outcome::Summary(s) if s.title=="secret title")
    );
    f.pool
        .revoke_root(&mut f.host, &a, GrantKind::ReadSummary, "card")
        .unwrap();
    let denied = f
        .pool
        .run_task(&f.manager, &mut f.host, &a, &summary_input(), || 2)
        .unwrap();
    assert_eq!(denied.execution.outcome, Ok(0));
    assert!(matches!(
        denied.response.unwrap().outcome,
        Outcome::Rejected(Failure::Denied)
    ));
    assert!(f.pool.root(&a).is_ok());
    let allowed = f
        .pool
        .run_task(&f.manager, &mut f.host, &b, &summary_input(), || 2)
        .unwrap();
    assert!(matches!(
        allowed.response.unwrap().outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn trap_quarantines_only_the_actual_session_not_another_same_package_root() {
    let mut f = Fixture::new(&summary_package(), &[GrantKind::ReadSummary]);
    let a = f.start();
    let b = f.start();
    f.grant(&a, GrantKind::ReadSummary);
    f.grant(&b, GrantKind::ReadSummary);
    let wrong = Invocation::new(
        "unexpected-task",
        &Command::ReadSummary {
            request_id: "query".into(),
            card_id: "card".into(),
        },
    )
    .unwrap();
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &a, &wrong, || 1)
        .unwrap();
    assert_eq!(report.execution.outcome, Err(Fault::Trap));
    assert_eq!(report.execution.host_calls, 0);
    assert!(f.pool.root(&a).is_err());
    assert!(
        f.pool
            .run_task(&f.manager, &mut f.host, &a, &summary_input(), || 1)
            .is_err()
    );
    let healthy = f
        .pool
        .run_task(&f.manager, &mut f.host, &b, &summary_input(), || 1)
        .unwrap();
    assert_eq!(healthy.execution.outcome, Ok(0));
    assert!(matches!(
        healthy.response.unwrap().outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn protocol_and_fuel_faults_stop_roots_without_automatic_retry() {
    for (mode, fault) in [("protocol", Fault::TaskProtocol), ("limits", Fault::Limits)] {
        let mut f = Fixture::new(&transform_package(mode), &[]);
        let session = f.start();
        let report = f
            .pool
            .run_task(
                &f.manager,
                &mut f.host,
                &session,
                &transform_input("plain.transform", b"hello".to_vec()),
                || 1,
            )
            .unwrap();
        assert_eq!(report.execution.outcome, Err(fault), "{mode}");
        assert!(report.output.is_none() && report.response.is_none());
        assert!(f.pool.root(&session).is_err());
        assert!(
            f.pool
                .run_task(
                    &f.manager,
                    &mut f.host,
                    &session,
                    &transform_input("plain.transform", b"hello".to_vec()),
                    || 1
                )
                .is_err()
        );
    }
}
#[test]
fn business_failure_keeps_a_healthy_instance_available() {
    let mut f = Fixture::new(&transform_package("failure"), &[]);
    let session = f.start();
    for _ in 0..2 {
        let report = f
            .pool
            .run_task(
                &f.manager,
                &mut f.host,
                &session,
                &transform_input("plain.transform", b"hello".to_vec()),
                || 1,
            )
            .unwrap();
        assert_eq!(report.execution.outcome, Ok(0));
        assert_eq!(report.failure.unwrap().code, FailureCode::UnsupportedInput);
        assert!(report.output.is_none() && report.response.is_none());
        assert!(f.pool.root(&session).is_ok());
    }
}
#[test]
fn invalid_host_transform_request_does_not_quarantine_the_guest() {
    let mut f = Fixture::new(&transform_package("output"), &[]);
    let session = f.start();
    for input in [
        transform_input("unknown", b"hello".to_vec()),
        transform_input("plain.transform", vec![1; 1025]),
    ] {
        assert!(matches!(
            f.pool
                .run_task(&f.manager, &mut f.host, &session, &input, || 1),
            Err(morrow_plugin_runtime::instance_pool::Error::Core(_))
        ));
        assert!(f.pool.root(&session).is_ok());
    }
    assert_eq!(
        f.pool
            .run_task(
                &f.manager,
                &mut f.host,
                &session,
                &transform_input("plain.transform", b"hello".to_vec()),
                || 1
            )
            .unwrap()
            .execution
            .outcome,
        Ok(0)
    );
}
#[test]
fn foreign_host_manager_and_session_are_rejected_without_revoking_valid_grants() {
    let package = summary_package();
    let mut f = Fixture::new(&package, &[GrantKind::ReadSummary]);
    let a = f.start();
    f.grant(&a, GrantKind::ReadSummary);
    let mut other = Fixture::new(&package, &[GrantKind::ReadSummary]);
    let foreign = other.start();
    assert!(
        f.pool
            .run_task(&other.manager, &mut f.host, &a, &summary_input(), || 1)
            .is_err()
    );
    assert!(
        f.pool
            .run_task(&f.manager, &mut other.host, &a, &summary_input(), || 1)
            .is_err()
    );
    assert!(
        f.pool
            .run_task(&f.manager, &mut f.host, &foreign, &summary_input(), || 1)
            .is_err()
    );
    assert!(
        f.pool
            .revoke_root(&mut other.host, &a, GrantKind::ReadSummary, "card")
            .is_err()
    );
    assert!(
        f.pool
            .revoke_root(&mut f.host, &foreign, GrantKind::ReadSummary, "card")
            .is_err()
    );
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &a, &summary_input(), || 1)
        .unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert!(matches!(
        report.response.unwrap().outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn revocation_during_actual_dispatch_stops_only_that_root_and_returns_no_summary() {
    let mut f = Fixture::new(&summary_package(), &[GrantKind::ReadSummary]);
    let a = f.start();
    let b = f.start();
    f.grant(&a, GrantKind::ReadSummary);
    f.grant(&b, GrantKind::ReadSummary);
    let signal = f
        .host
        .revocation(f.pool.root(&a).unwrap().connection())
        .unwrap();
    let mut ticks = 0;
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &a, &summary_input(), || {
            ticks += 1;
            signal.revoke();
            1
        })
        .unwrap();
    assert!(
        ticks > 0,
        "revocation hook must execute at a real dispatch boundary"
    );
    assert_eq!(report.execution.host_calls, 1);
    assert!(!matches!(
        report.response.as_ref().map(|r| &r.outcome),
        Some(Outcome::Summary(_))
    ));
    assert!(report.output.is_none());
    assert!(f.pool.root(&a).is_err());
    let healthy = f
        .pool
        .run_task(&f.manager, &mut f.host, &b, &summary_input(), || 1)
        .unwrap();
    assert!(matches!(
        healthy.response.unwrap().outcome,
        Outcome::Summary(_)
    ));
}
#[test]
fn trap_after_real_commit_preserves_content_and_never_retries() {
    let command = Command::Rename(RenameRequest {
        operation_id: "rename-once".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "committed before trap".into(),
    });
    let input = Invocation::new("rename-task", &command).unwrap();
    let body = format!(
        "i32.const 16384 i32.const {} i32.const 196608 i32.const 65536 call $exchange drop unreachable",
        input.command_bytes().len()
    );
    let wasm = module(&input, &[(16384, input.command_bytes().to_vec())], &body);
    let package = Package::build(
        Package::manifest_for_task(ID, "1.0.0", &wasm, vec![Capability::RenameCard]),
        &wasm,
    )
    .unwrap();
    let mut f = Fixture::new(&package, &[GrantKind::Rename]);
    let session = f.start();
    f.grant(&session, GrantKind::Rename);
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &session, &input, || 1)
        .unwrap();
    assert_eq!(report.execution.outcome, Err(Fault::Trap));
    assert_eq!(report.execution.host_calls, 1);
    assert!(f.pool.root(&session).is_err());
    assert!(
        matches!(f.host.store_local().lookup_for_card("card","rename-once").unwrap(),Lookup::Committed(r) if r.revision==2)
    );
    assert_eq!(
        f.host
            .store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .title,
        "committed before trap"
    );
    assert!(
        f.pool
            .run_task(&f.manager, &mut f.host, &session, &input, || 1)
            .is_err()
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 2);
}

#[test]
fn failing_an_ordinary_root_preserves_the_same_package_shared_provider() {
    let provider = transform_package("output");
    let mut f = Fixture::new(&provider, &[]);
    let input = transform_input("plain.transform", b"hello".to_vec());
    let completion = input.output_completion(b"dependent output").unwrap();
    let wasm = module(
        &input,
        &[(16384, completion.clone())],
        &format!(
            "i32.const 16384 i32.const {} call $done drop i32.const 0",
            completion.len()
        ),
    );
    let mut dependents = vec![];
    for id in ["test.pool.dependent-a", "test.pool.dependent-b"] {
        let mut manifest = Package::manifest_for_transform(
            id,
            "1.0.0",
            &wasm,
            vec![TransformHandler {
                handler: "plain.transform".into(),
                input_type: "bytes".into(),
                output_type: "answer".into(),
                max_input_bytes: 1024,
                max_output_bytes: 1024,
            }],
        );
        manifest.required_features.push("dependencies-v1".into());
        manifest
            .dependencies
            .push(morrow_core::plugin_package::proto::DependencyRequirement {
                slot: "shared".into(),
                handler: "plain.transform".into(),
                input_type: "bytes".into(),
                output_type: "answer".into(),
                provider_version: "^1.0".into(),
                optional: false,
            });
        let package = Package::build(manifest, &wasm).unwrap();
        Catalog::open(&f._dir.path().join("packages"))
            .unwrap()
            .install(&package)
            .unwrap();
        f.manager.select(&package, f.manager.revision()).unwrap();
        f.manager
            .approve_dependency(
                id,
                package.digest(),
                "shared",
                ID,
                provider.digest(),
                f.manager.revision(),
            )
            .unwrap();
        f.manager
            .set_enabled(id, package.digest(), true, f.manager.revision())
            .unwrap();
        let revision = f.manager.revision();
        dependents.push(
            f.pool
                .start(&mut f.manager, &mut f.host, id, &[], revision)
                .unwrap(),
        );
    }
    let ordinary = f.start();
    assert_eq!(f.pool.usage().providers, 1);
    let provider_binding = f.pool.provider(ID).unwrap().connection().binding();
    assert_ne!(
        provider_binding,
        f.pool.root(&ordinary).unwrap().connection().binding()
    );
    // The contract is valid, but these unexpected bytes trap in the actual ordinary guest.
    let bad = transform_input("plain.transform", b"unexpected".to_vec());
    let report = f
        .pool
        .run_task(&f.manager, &mut f.host, &ordinary, &bad, || 1)
        .unwrap();
    assert_eq!(report.execution.outcome, Err(Fault::Trap));
    assert!(f.pool.root(&ordinary).is_err());
    assert_eq!(f.pool.usage().providers, 1);
    assert_eq!(
        f.pool.provider(ID).unwrap().connection().binding(),
        provider_binding
    );
    assert_eq!(
        f.host
            .connection_phase(f.pool.provider(ID).unwrap().connection()),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    for session in &dependents {
        let report = f
            .pool
            .run_task(&f.manager, &mut f.host, session, &input, || 1)
            .unwrap();
        assert_eq!(report.execution.outcome, Ok(0));
        assert_eq!(report.output.unwrap().bytes, b"dependent output");
    }
}
