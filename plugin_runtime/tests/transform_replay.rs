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
    runtime::Command,
    store::Store,
    task::{FailureCode, Invocation, PluginFailure, Transform},
    task_evidence::{self, Evidence, proto::StableFault},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
    replay,
};
use std::{collections::BTreeSet, fs};
const ID: &str = "test.replay";
fn input() -> Invocation {
    Invocation::new_transform(
        "fixed-task",
        Transform {
            handler: "binary.convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![0, 255, 65, 0, 128],
        },
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        fuel: 100_000,
        memory_bytes: 4 * 65536,
        host_calls: 4,
    }
}
fn data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
#[derive(Clone, Copy)]
enum Mode {
    Success,
    Business,
    Trap,
    AfterCompletionTrap,
    Missing,
    Fuel,
    Exchange,
}
fn package(mode: Mode) -> Package {
    let task = input();
    let done = if matches!(mode, Mode::Business) {
        task.failure_completion(&PluginFailure {
            code: FailureCode::UnsupportedInput,
            message: "unsupported binary sample".into(),
        })
        .unwrap()
    } else {
        task.output_completion(&[128, 0, 65, 255, 0]).unwrap()
    };
    let action = match mode {
        Mode::Trap => "unreachable".into(),
        Mode::Fuel => "(loop $forever br $forever) unreachable".into(),
        Mode::Missing => "i32.const 0".into(),
        _ => format!(
            "{} i32.const 160000 i32.const {} call $done drop {} i32.const 0",
            if matches!(mode, Mode::Exchange) {
                "i32.const 0 i32.const 1 i32.const 196608 i32.const 65536 call $exchange drop"
            } else {
                ""
            },
            done.len(),
            if matches!(mode, Mode::AfterCompletionTrap) {
                "unreachable"
            } else {
                ""
            }
        ),
    };
    let wasm = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
      (import "morrow_v1" "exchange" (func $exchange(param i32 i32 i32 i32)(result i32)))
      (memory(export "memory") 4)
      (data(i32.const 140000) "{}") (data(i32.const 160000) "{}")
      (func(export "morrow_run")(result i32)(local $i i32)
        i32.const 0 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
        (loop $compare
          local.get $i i32.load8_u
          i32.const 140000 local.get $i i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.tee $i i32.const {} i32.lt_u br_if $compare)
        {action}))"#,
        data(task.bytes()),
        data(&done),
        task.bytes().len(),
        task.bytes().len()
    ))
    .unwrap();
    let mut m = Package::manifest_for_transform(
        ID,
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "binary.convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    m.requested_capabilities = vec![Capability::EditContent as i32];
    Package::build(m, &wasm).unwrap()
}
struct Fixture {
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    manager: Manager,
    host: HostRuntime,
    pool: Pool,
    session: Session,
}
impl Fixture {
    fn new(mode: Mode) -> Self {
        Self::with_package(package(mode), limits())
    }
    fn with_package(package: Package, budget: Limits) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
        let path = catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("registry"), catalog).unwrap(),
            budget,
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve(
                ID,
                package.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "test.card", 1, "original", b"original".to_vec()).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut pool = Pool::new(&host, Default::default()).unwrap();
        let revision = manager.revision();
        let session = pool
            .start(&mut manager, &mut host, ID, &[], revision)
            .unwrap();
        Self {
            _dir: dir,
            path,
            manager,
            host,
            pool,
            session,
        }
    }
    fn capture(
        &mut self,
    ) -> morrow_plugin_runtime::instance_pool::Result<
        morrow_plugin_runtime::replay::CapturedTransform,
    > {
        self.pool
            .record_transform(&self.manager, &mut self.host, &self.session, &input())
    }
    fn unchanged(&self) {
        let card = self.host.store_local().card("card").unwrap().unwrap();
        assert_eq!(card.summary().revision, 1);
        assert_eq!(card.body(), b"original");
    }
}
fn roundtrip(evidence: &Evidence) -> Evidence {
    task_evidence::decode(evidence.container(), evidence.digest()).unwrap()
}
#[test]
fn capture_observes_full_invocation_and_replay_repeats_actual_binary_output() {
    let mut f = Fixture::new(Mode::Success);
    let captured = f.capture().unwrap();
    assert_eq!(captured.report().execution.outcome, Ok(0));
    assert_eq!(
        captured.report().output.as_ref().unwrap().bytes,
        [128, 0, 65, 255, 0]
    );
    assert_eq!(captured.evidence().data().invocation, input().bytes());
    assert_eq!(
        captured.evidence().data().package_archive,
        fs::read(&f.path).unwrap()
    );
    let evidence = roundtrip(captured.evidence());
    for _ in 0..3 {
        let replayed = replay::replay(&evidence, limits()).unwrap();
        assert!(replayed.matches);
        assert_eq!(
            replayed.report.output.as_ref().unwrap().bytes,
            [128, 0, 65, 255, 0]
        );
    }
    f.unchanged();
}
#[test]
fn changing_original_invocation_yields_actual_guest_trap_not_constant_success() {
    let mut f = Fixture::new(Mode::Success);
    let original = input();
    let mut changed = original.transform().unwrap().clone();
    changed.input[1] = 17;
    let task = Invocation::new_transform(original.task_id(), changed).unwrap();
    let captured = f
        .pool
        .record_transform(&f.manager, &mut f.host, &f.session, &task)
        .unwrap();
    assert_eq!(captured.report().execution.outcome, Err(Fault::Trap));
    assert!(
        replay::replay(captured.evidence(), limits())
            .unwrap()
            .matches
    );
    assert!(f.pool.root(&f.session).is_err());
    f.unchanged();
}
#[test]
fn business_failure_is_preserved_without_quarantining_healthy_root() {
    let mut f = Fixture::new(Mode::Business);
    let captured = f.capture().unwrap();
    let failure = captured.report().failure.as_ref().unwrap();
    assert_eq!(failure.code, FailureCode::UnsupportedInput);
    assert_eq!(failure.message, "unsupported binary sample");
    assert!(captured.report().output.is_none());
    let result = replay::replay(captured.evidence(), limits()).unwrap();
    assert!(result.matches);
    assert_eq!(result.report.failure.as_ref(), Some(failure));
    assert!(f.pool.root(&f.session).is_ok());
    f.unchanged();
}
#[test]
fn actual_trap_and_post_completion_trap_discard_output_and_stop_root() {
    for mode in [Mode::Trap, Mode::AfterCompletionTrap] {
        let mut f = Fixture::new(mode);
        let captured = f.capture().unwrap();
        assert_eq!(captured.report().execution.outcome, Err(Fault::Trap));
        assert!(captured.report().output.is_none());
        assert!(captured.evidence().data().completion.is_empty());
        assert!(
            replay::replay(captured.evidence(), limits())
                .unwrap()
                .matches
        );
        assert!(f.pool.root(&f.session).is_err());
        assert!(f.capture().is_err());
        f.unchanged();
    }
}
#[test]
fn missing_completion_is_replayed_as_task_protocol_fault() {
    let mut f = Fixture::new(Mode::Missing);
    let captured = f.capture().unwrap();
    assert_eq!(
        captured.report().execution.outcome,
        Err(Fault::TaskProtocol)
    );
    assert!(
        replay::replay(captured.evidence(), limits())
            .unwrap()
            .matches
    );
    assert!(f.pool.root(&f.session).is_err());
    f.unchanged();
}
#[test]
fn actual_fuel_exhaustion_uses_recorded_budget_and_remaining_fuel() {
    let mut f = Fixture::with_package(
        package(Mode::Fuel),
        Limits {
            fuel: 20_000,
            ..limits()
        },
    );
    let captured = f.capture().unwrap();
    assert_eq!(captured.report().execution.outcome, Err(Fault::Limits));
    assert_eq!(
        captured.evidence().data().budget.as_ref().unwrap().fuel,
        20_000
    );
    let result = replay::replay(captured.evidence(), limits()).unwrap();
    assert!(result.matches);
    assert_eq!(
        result.report.execution.fuel_remaining,
        captured.report().execution.fuel_remaining
    );
    assert!(f.pool.root(&f.session).is_err());
    f.unchanged();
}
#[test]
fn ignored_ordinary_exchange_cannot_complete_a_successful_pure_observation() {
    let mut f = Fixture::new(Mode::Exchange);
    f.pool
        .grant_root(
            &mut f.host,
            &f.session,
            GrantKind::EditContent,
            "card",
            100,
            0,
        )
        .unwrap();
    let captured = f.capture().unwrap();
    assert_eq!(
        captured.report().execution.outcome,
        Err(Fault::TaskProtocol)
    );
    assert_eq!(captured.report().execution.host_calls, 1);
    assert!(captured.report().output.is_none());
    assert!(captured.report().response.is_none());
    assert!(
        replay::replay(captured.evidence(), limits())
            .unwrap()
            .matches
    );
    f.unchanged();
}
#[test]
fn caller_constructed_observation_has_integrity_but_does_not_match_real_execution() {
    let mut f = Fixture::new(Mode::Success);
    let captured = f.capture().unwrap();
    for field in 0..4 {
        let mut edited = captured.evidence().data().clone();
        match field {
            0 => edited.completion = input().output_completion(b"fabricated").unwrap(),
            1 => edited.fuel_remaining -= 1,
            2 => edited.observed_host_calls = 1,
            _ => edited.exit_code = Some(1),
        }
        let forged = task_evidence::encode(edited).unwrap();
        assert!(task_evidence::decode(forged.container(), captured.evidence().digest()).is_err());
        let new_integrity_pin = roundtrip(&forged);
        assert!(
            !replay::replay(&new_integrity_pin, limits())
                .unwrap()
                .matches
        );
    }
}
#[test]
fn replay_rejects_unsupported_backend_external_outcomes_and_insufficient_policy() {
    let mut f = Fixture::new(Mode::Success);
    let captured = f.capture().unwrap();
    let mut edited = captured.evidence().data().clone();
    edited.backend = "another-engine/v1".into();
    assert!(matches!(
        replay::replay(&task_evidence::encode(edited).unwrap(), limits()),
        Err(replay::Error::UnsupportedBackend)
    ));
    for fault in [
        StableFault::Cancelled,
        StableFault::Deadline,
        StableFault::InactiveConnection,
        StableFault::PackageBinding,
        StableFault::InvalidModule,
        StableFault::UnsupportedAbi,
    ] {
        let mut edited = captured.evidence().data().clone();
        edited.fault = fault as i32;
        edited.exit_code = None;
        edited.completion.clear();
        assert!(matches!(
            replay::replay(&task_evidence::encode(edited).unwrap(), limits()),
            Err(replay::Error::UnsupportedOutcome)
        ));
    }
    for policy in [
        Limits {
            fuel: 99_999,
            ..limits()
        },
        Limits {
            memory_bytes: 3 * 65536,
            ..limits()
        },
        Limits {
            host_calls: 3,
            ..limits()
        },
        Limits {
            fuel: 0,
            ..limits()
        },
    ] {
        assert!(replay::replay(captured.evidence(), policy).is_err());
    }
    assert!(
        replay::replay(captured.evidence(), Limits::default())
            .unwrap()
            .matches
    );
}
#[test]
fn archive_tamper_missing_bytes_and_wrong_external_pin_fail_before_execution() {
    let mut f = Fixture::new(Mode::Success);
    let captured = f.capture().unwrap();
    assert!(task_evidence::decode(captured.evidence().container(), [0; 32]).is_err());
    let mut container = captured.evidence().container().to_vec();
    let last = container.len() - 1;
    container[last] ^= 1;
    assert!(task_evidence::decode(&container, captured.evidence().digest()).is_err());
    for archive in [vec![], b"not a package".to_vec()] {
        let mut edited = captured.evidence().data().clone();
        edited.package_archive = archive;
        assert!(task_evidence::encode(edited).is_err());
    }
}
#[test]
fn captured_archive_replays_after_catalog_loss_and_cannot_reactivate_stopped_session() {
    let mut f = Fixture::new(Mode::Success);
    let captured = f.capture().unwrap();
    fs::remove_file(&f.path).unwrap();
    f.pool.maintain(&f.manager, &mut f.host).unwrap();
    assert!(f.pool.root(&f.session).is_err());
    assert!(
        replay::replay(captured.evidence(), limits())
            .unwrap()
            .matches
    );
    assert!(f.capture().is_err());
    assert!(f.pool.root(&f.session).is_err());
    f.unchanged();
    let (_, evidence) = captured.into_parts();
    drop(f); // The replay API receives neither the original runtime nor its database directory.
    assert!(
        replay::replay(&roundtrip(&evidence), limits())
            .unwrap()
            .matches
    );
}
#[test]
fn content_commands_and_dependency_enabled_modules_are_rejected_without_executing() {
    let mut f = Fixture::new(Mode::Success);
    let command = Invocation::new(
        "command",
        &Command::ReadSummary {
            request_id: "read".into(),
            card_id: "card".into(),
        },
    )
    .unwrap();
    assert!(
        f.pool
            .record_transform(&f.manager, &mut f.host, &f.session, &command)
            .is_err()
    );
    assert!(f.pool.root(&f.session).is_ok());
    f.unchanged();
    let base = package(Mode::Success);
    let mut manifest = base.manifest().clone();
    manifest
        .required_features
        .extend(["dependencies-v1".into(), "dependency-calls-v1".into()]);
    manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    let dependency = Package::build(manifest, base.module()).unwrap();
    let mut f = Fixture::with_package(dependency, limits());
    assert!(f.capture().is_err());
    assert!(f.pool.root(&f.session).is_ok());
    f.unchanged();
}

#[test]
fn foreign_or_revoked_session_cannot_issue_an_execution_capture() {
    let mut original = Fixture::new(Mode::Success);
    let foreign = Fixture::new(Mode::Success);
    assert!(
        original
            .pool
            .record_transform(
                &original.manager,
                &mut original.host,
                &foreign.session,
                &input()
            )
            .is_err()
    );
    assert!(
        original
            .pool
            .record_transform(
                &foreign.manager,
                &mut original.host,
                &original.session,
                &input()
            )
            .is_err()
    );
    assert!(original.pool.root(&original.session).is_ok());
    assert!(original.capture().is_ok());
    let revocation = original
        .host
        .revocation(original.pool.root(&original.session).unwrap().connection())
        .unwrap();
    revocation.revoke();
    assert!(original.capture().is_err());
    assert!(original.pool.root(&original.session).is_err());
    original.unchanged();
    foreign.unchanged();
}
