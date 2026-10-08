//! Original persistent Catalog/Registry/Manager/SQLite authority; providers here are logical.
//! Rust Wasm tests require explicit sealed artifacts and never substitute WAT for those guests.
use morrow_agent_process_control_v1::{
    self as process, Capabilities as PC, OutputPage, ReadQuery,
    host::{Budget, EffectOutcome, Host as ProcessHost, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request,
    authority::{Admission, Capabilities as SC, SessionExecHost},
    hash,
    safe_exec::ToolIdentity,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Declaration, ManagedPreparedPackage,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    plugin_package::{Package, catalog::Catalog, registry::Registry},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Cancellation, Limits, manager::Manager};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const ID: &str = "managed-codex";
fn sc() -> SC {
    SC {
        session_read: true,
        session_write: true,
        propose: true,
        ..SC::default()
    }
}
fn pc() -> PC {
    PC {
        read: true,
        events: true,
        write: true,
        terminate: true,
        ..PC::default()
    }
}
fn limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        memory_bytes: 16 * 1024 * 1024,
        host_calls: 16,
    }
}
fn module() -> Vec<u8> {
    wat::parse_str(r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_agent_session_process_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 6)
      (func (export "morrow_run") (result i32) (local $n i32)
        (local.set $n (call $read (i32.const 0) (i32.const 131072)))
        (local.set $n (call $call (i32.const 0) (local.get $n) (i32.const 131072) (i32.const 131072)))
        (drop (call $done (i32.const 131072) (local.get $n))) i32.const 0))"#).unwrap()
}
fn sealed(env: &str, sha: &str) -> Vec<u8> {
    let path = std::env::var(env).unwrap_or_else(|_| panic!("required sealed artifact {env}"));
    assert!(std::path::Path::new(&path).is_absolute());
    let bytes = std::fs::read(path).unwrap();
    let actual = hash(&bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    assert_eq!(actual, sha, "sealed Wasm input changed");
    bytes
}
fn wrapper(base: &Package, domain: &str) -> AgentProcessPackage {
    AgentProcessPackage::build(
        Package::decode(base.archive()).unwrap(),
        Declaration {
            session: sc(),
            process: pc(),
            sessions: vec!["session".into(), "session-child".into()],
            execution_domain: domain.into(),
        },
    )
    .unwrap()
}
struct Fixture {
    manager: Manager,
    runtime: HostRuntime,
    host: SessionExecHost,
    base: Package,
    processes: ProcessHost,
    temp: tempfile::TempDir,
}
impl Fixture {
    fn new(wasm: &[u8], budget: Limits) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base =
            Package::build(Package::manifest_for_task(ID, "1.0.0", wasm, vec![]), wasm).unwrap();
        let catalog = Catalog::open(&temp.path().join("catalog")).unwrap();
        catalog.install(&base).unwrap();
        let registry = Registry::open(&temp.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, budget);
        manager.select(&base, manager.revision()).unwrap();
        manager
            .set_enabled(ID, base.digest(), true, manager.revision())
            .unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        Self {
            manager,
            runtime,
            host,
            base,
            processes: ProcessHost::default(),
            temp,
        }
    }
    fn connect(&mut self) -> ManagedPreparedPackage {
        let package = wrapper(&self.base, "domain");
        let sha = package.review_sha256();
        let revision = self.manager.revision();
        ManagedPreparedPackage::connect(
            package,
            &mut self.manager,
            &mut self.runtime,
            &self.host,
            revision,
            sha,
            sc(),
            pc(),
            vec!["session".into(), "session-child".into()],
            1000,
            0,
        )
        .unwrap()
    }
    fn run(
        &mut self,
        bridge: &ManagedPreparedPackage,
        raw: &[u8],
    ) -> Result<morrow_plugin_runtime::TaskRun, Error> {
        bridge.run(
            &self.manager,
            &mut self.runtime,
            &self.host,
            &mut self.processes,
            raw,
            || 6,
        )
    }
    fn native_started(&mut self) -> (Arc<Connection>, Admission, ToolIdentity) {
        let artifact = self.temp.path().join("logical-artifact.bin");
        std::fs::write(&artifact, b"logical").unwrap();
        let proposer = self.runtime.connect().unwrap();
        let admission = self
            .host
            .admit(
                &self.runtime,
                &proposer,
                sc(),
                sc(),
                vec!["session".into()],
                "domain".into(),
                1000,
                0,
            )
            .unwrap();
        for request in [
            Request::new(
                "native-create",
                Action::Create {
                    session_id: "session".into(),
                    parent: None,
                    parent_tail: 0,
                },
            )
            .unwrap(),
            Request::new(
                "native-propose",
                Action::Propose {
                    session_id: "session".into(),
                    intent: Intent {
                        operation_id: "operation".into(),
                        program: artifact.to_str().unwrap().into(),
                        argv: vec![],
                        cwd: self.temp.path().to_str().unwrap().into(),
                        env: vec![],
                        input: vec![],
                        execution_domain: "domain".into(),
                        max_runtime_ms: 100,
                        artifact_sha256: hash(b"logical"),
                    },
                },
            )
            .unwrap(),
        ] {
            let reply = self
                .host
                .dispatch(
                    &mut self.runtime,
                    &proposer,
                    &admission,
                    request.raw(),
                    || 1,
                )
                .unwrap();
            assert!(!matches!(
                Reply::decode_for(&request, &reply).unwrap().outcome,
                Outcome::Rejected(_)
            ));
        }
        let executor = Arc::new(self.runtime.connect().unwrap());
        let rights = SC {
            session_read: true,
            execute: true,
            ..SC::default()
        };
        let execution = self
            .host
            .admit(
                &self.runtime,
                &executor,
                rights,
                rights,
                vec!["session".into()],
                "domain".into(),
                1000,
                1,
            )
            .unwrap();
        let review = self
            .host
            .review_tool(&self.runtime, "operation", 2)
            .unwrap();
        let permit = self
            .host
            .approve(
                &mut self.runtime,
                &executor,
                &execution,
                "operation",
                review.proposal_sha256,
                review.intent_sha256,
                2,
            )
            .unwrap();
        let request = Request::new(
            "native-claim",
            Action::Claim {
                operation_id: "operation".into(),
                permit,
            },
        )
        .unwrap();
        let reply = self
            .host
            .dispatch(
                &mut self.runtime,
                &executor,
                &execution,
                request.raw(),
                || 3,
            )
            .unwrap();
        let Outcome::Claimed { claim, .. } = Reply::decode_for(&request, &reply).unwrap().outcome
        else {
            panic!()
        };
        assert_eq!(
            self.host.execute_claimed(
                &mut self.runtime,
                &executor,
                &execution,
                "operation",
                claim,
                || 4,
                |_| Err(Error::CommitUnknown)
            ),
            Err(Error::CommitUnknown)
        );
        let identity = self
            .host
            .inspect_tool_record(&self.runtime, "operation")
            .unwrap()
            .identity;
        (executor, execution, identity)
    }
}
fn request() -> Vec<u8> {
    Request::new("inspect", Action::List)
        .unwrap()
        .raw()
        .to_vec()
}

#[test]
fn original_connection_is_retained_and_close_blocks_further_dispatch() {
    let mut f = Fixture::new(&module(), limits());
    let bridge = f.connect();
    let connection = bridge.instance().shared_connection();
    assert!(Arc::ptr_eq(&connection, &bridge.shared_connection()));
    assert_eq!(connection.package_digest(), Some(f.base.digest()));
    let run = f.run(&bridge, &request()).unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    assert!(matches!(
        Reply::decode_for(
            &Request::decode(&request()).unwrap(),
            &run.completion.unwrap()
        )
        .unwrap()
        .outcome,
        Outcome::Sessions(_)
    ));
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
    assert!(!bridge.instance().is_active());
    assert!(f.runtime.connection_phase(&connection).is_err());
    assert!(f.run(&bridge, &request()).is_err());
}
#[test]
fn disable_remove_and_upgrade_revoke_the_original_managed_control() {
    for kind in 0..3 {
        let mut f = Fixture::new(&module(), limits());
        let bridge = f.connect();
        match kind {
            0 => f
                .manager
                .set_enabled(ID, f.base.digest(), false, f.manager.revision())
                .unwrap(),
            1 => f.manager.remove(ID, f.manager.revision()).unwrap(),
            _ => {
                let wasm = module();
                let upgraded = Package::build(
                    Package::manifest_for_task(ID, "2.0.0", &wasm, vec![]),
                    &wasm,
                )
                .unwrap();
                f.manager.install_package(upgraded.archive()).unwrap();
                f.manager.select(&upgraded, f.manager.revision()).unwrap();
            }
        }
        assert!(!bridge.instance().is_active());
        assert!(f.run(&bridge, &request()).is_err());
        bridge
            .close(&mut f.runtime, &f.host, &mut f.processes)
            .unwrap();
    }
}
#[test]
fn stale_revision_foreign_manager_and_runtime_are_denied() {
    let mut f = Fixture::new(&module(), limits());
    let bridge = f.connect();
    let mut other = Fixture::new(&module(), limits());
    assert!(
        bridge
            .run(
                &other.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                &request(),
                || 6
            )
            .is_err()
    );
    assert!(
        bridge
            .run(
                &f.manager,
                &mut other.runtime,
                &f.host,
                &mut f.processes,
                &request(),
                || 6
            )
            .is_err()
    );
    let p = wrapper(&f.base, "domain");
    let sha = p.review_sha256();
    assert!(
        ManagedPreparedPackage::connect(
            p,
            &mut f.manager,
            &mut f.runtime,
            &f.host,
            0,
            sha,
            sc(),
            pc(),
            vec!["session".into()],
            1000,
            0
        )
        .is_err()
    );
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
}
#[test]
fn base_selection_never_approves_a_different_wrapper_or_excess_permissions() {
    let mut f = Fixture::new(&module(), limits());
    let first_sha = wrapper(&f.base, "domain").review_sha256();
    for kind in 0..5 {
        let p = wrapper(&f.base, if kind == 0 { "other-domain" } else { "domain" });
        let sha = if kind == 0 {
            first_sha
        } else if kind == 1 {
            f.base.digest()
        } else {
            p.review_sha256()
        };
        let session = if kind == 2 {
            SC {
                execute: true,
                ..sc()
            }
        } else {
            sc()
        };
        let scope = if kind == 3 {
            vec!["undeclared".into()]
        } else {
            vec!["session".into()]
        };
        let process = if kind == 4 {
            PC {
                resize_pty: true,
                ..pc()
            }
        } else {
            pc()
        };
        let revision = f.manager.revision();
        assert!(
            ManagedPreparedPackage::connect(
                p,
                &mut f.manager,
                &mut f.runtime,
                &f.host,
                revision,
                sha,
                session,
                process,
                scope,
                1000,
                0
            )
            .is_err()
        );
    }
    let bridge = f.connect();
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
}
#[test]
fn stop_and_drop_cannot_leave_a_live_original_connection() {
    let mut f = Fixture::new(&module(), limits());
    let bridge = f.connect();
    let conn = bridge.instance().shared_connection();
    bridge.request_stop();
    assert!(f.run(&bridge, &request()).is_err());
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
    let bridge = f.connect();
    let original = bridge.instance().shared_connection();
    drop(bridge);
    assert_ne!(
        f.runtime.connection_phase(&original),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    assert!(f.runtime.connection_phase(&conn).is_err());
    let _ = f.runtime.disconnect(&original);
}
#[test]
fn manager_drop_rejects_the_same_surviving_bridge() {
    let mut f = Fixture::new(&module(), limits());
    let bridge = f.connect();
    let other = Fixture::new(&module(), limits());
    let old = std::mem::replace(&mut f.manager, other.manager);
    drop(old);
    assert!(!bridge.instance().is_active());
    assert!(f.run(&bridge, &request()).is_err());
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
}
#[test]
fn rust_wasm_uses_original_manager_limits_and_sealed_session_continuation() {
    let wasm = sealed(
        "MORROW_CODEX_COMBINED_GUEST",
        "b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e",
    );
    let mut f = Fixture::new(&wasm, limits());
    let bridge = f.connect();
    let mut config = bridge.generation().to_le_bytes().to_vec();
    config.extend_from_slice(&[7; 16]);
    config.extend_from_slice(b"session");
    let task = Invocation::new_transform(
        "managed-session",
        Transform {
            handler: "codex.session.continue".into(),
            input_type: "codex.session.config.v1".into(),
            output_type: "codex.session.receipt.v1".into(),
            input: config,
        },
    )
    .unwrap();
    let run = f.run(&bridge, task.bytes()).unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 7);
    let out = task.verify_output(&run.completion.unwrap()).unwrap();
    let mut expected = 1u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&hash(b"sealed-state\0\xff"));
    expected.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(out.bytes, expected);
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
    let mut constrained = Fixture::new(
        &wasm,
        Limits {
            host_calls: 1,
            ..limits()
        },
    );
    let bridge = constrained.connect();
    assert_eq!(bridge.limits().host_calls, 1);
    assert!(bridge.limits().fuel <= limits().fuel);
    let run = constrained.run(&bridge, task.bytes()).unwrap();
    assert!(run.report.outcome.is_err());
    assert!(run.completion.is_none());
    bridge
        .close(
            &mut constrained.runtime,
            &constrained.host,
            &mut constrained.processes,
        )
        .unwrap();
}

struct LogicalProvider {
    writes: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    cancel: Option<Cancellation>,
    clock_reads: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
}
struct LateProvider {
    inner: LogicalProvider,
    cancel: Cancellation,
}
impl ProcessProvider for LateProvider {
    fn capabilities(&self) -> PC {
        self.cancel.cancel();
        pc()
    }
    fn read(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        self.inner.read(q)
    }
    fn events(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        self.inner.events(q)
    }
    fn write(&mut self, bytes: &[u8]) -> EffectOutcome {
        self.inner.write(bytes)
    }
    fn close_input(&mut self) -> EffectOutcome {
        self.inner.close_input()
    }
    fn interrupt(&mut self) -> EffectOutcome {
        self.inner.interrupt()
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.inner.terminate()
    }
    fn resize(&mut self, rows: u16, cols: u16) -> EffectOutcome {
        self.inner.resize(rows, cols)
    }
}
impl ProcessProvider for LogicalProvider {
    fn capabilities(&self) -> PC {
        pc()
    }
    fn read(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        Ok(OutputPage {
            events: vec![],
            next_seq: q.after_seq,
            floor_seq: 1,
            gap: false,
            exited: false,
            exit_code: None,
            closed: false,
            failure: None,
        })
    }
    fn events(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        self.read(q)
    }
    fn write(&mut self, _: &[u8]) -> EffectOutcome {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if let Some((samples, seen)) = &self.clock_reads {
            seen.store(samples.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        EffectOutcome::Accepted
    }
    fn close_input(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn interrupt(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.stops.fetch_add(1, Ordering::SeqCst);
        EffectOutcome::Accepted
    }
    fn resize(&mut self, _: u16, _: u16) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
}
#[test]
fn rust_wasm_control_effect_followed_by_managed_stop_stays_unknown_without_replay() {
    let wasm = sealed(
        "MORROW_CODEX_PROCESS_GUEST",
        "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36",
    );
    let mut f = Fixture::new(&wasm, limits());
    let mut bridge = f.connect();
    let (executor, admission, identity) = f.native_started();
    let writes = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let handle = bridge
        .register_process(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut f.processes,
            identity,
            executor,
            &admission,
            pc(),
            Budget::default(),
            Box::new(LogicalProvider {
                writes: writes.clone(),
                stops: stops.clone(),
                cancel: Some(bridge.instance().cancellation()),
                clock_reads: None,
            }),
            || 5,
        )
        .unwrap();
    let input = process::Request::new(
        "managed-input",
        handle.nonce,
        handle.generation,
        process::Action::Write(b"once".to_vec()),
    )
    .unwrap();
    let task = Invocation::new_transform(
        "managed-control",
        Transform {
            handler: "codex.process.control".into(),
            input_type: "codex.process.request.v1".into(),
            output_type: "codex.process.reply.v1".into(),
            input: input.encode().unwrap(),
        },
    )
    .unwrap();
    let run = f.run(&bridge, task.bytes()).unwrap();
    assert!(run.report.outcome.is_err());
    assert!(run.completion.is_none());
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    let actual = process::Request::new(
        "managed-input-2",
        handle.nonce,
        handle.generation,
        input.action,
    )
    .unwrap();
    f.processes.veto_delivery(&actual).unwrap(); // Existing Unknown receipt, no provider re-entry.
    assert!(f.run(&bridge, task.bytes()).is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert_eq!(bridge.owned_handles(&f.runtime).unwrap(), vec![handle]);
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[test]
fn last_pre_effect_clock_stop_never_calls_the_provider() {
    let mut before_effect = 0;
    for revoke in [false, true] {
        let mut f = Fixture::new(&module(), limits());
        let mut bridge = f.connect();
        let (executor, admission, identity) = f.native_started();
        let samples = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let stops = Arc::new(AtomicUsize::new(0));
        let handle = bridge
            .register_process(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                identity,
                executor,
                &admission,
                pc(),
                Budget::default(),
                Box::new(LogicalProvider {
                    writes: writes.clone(),
                    stops,
                    cancel: None,
                    clock_reads: Some((samples.clone(), seen.clone())),
                }),
                || 5,
            )
            .unwrap();
        let request = process::Request::new(
            "pre-effect",
            handle.nonce,
            handle.generation,
            process::Action::Write(b"once".to_vec()),
        )
        .unwrap();
        let run = bridge
            .run(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                &request.encode().unwrap(),
                || {
                    let count = samples.fetch_add(1, Ordering::SeqCst) + 1;
                    if revoke && count == before_effect {
                        bridge.instance().request_stop();
                    }
                    6
                },
            )
            .unwrap();
        if revoke {
            assert!(run.report.outcome.is_err());
            assert!(run.completion.is_none());
            assert_eq!(
                writes.load(Ordering::SeqCst),
                0,
                "last live_tool clock must not reopen managed authority"
            );
        } else {
            assert_eq!(run.report.outcome, Ok(0));
            assert_eq!(
                process::Reply::decode_for(&request, &run.completion.unwrap())
                    .unwrap()
                    .body,
                process::ReplyBody::Accepted
            );
            assert_eq!(writes.load(Ordering::SeqCst), 1);
            before_effect = seen.load(Ordering::SeqCst);
            assert!(before_effect > 1);
        }
        bridge
            .close(&mut f.runtime, &f.host, &mut f.processes)
            .unwrap();
    }
}

#[test]
fn revoked_original_executor_cannot_be_replaced_by_a_live_managed_reader() {
    let mut f = Fixture::new(&module(), limits());
    let mut bridge = f.connect();
    let (executor, admission, identity) = f.native_started();
    f.host.revoke(&admission).unwrap();
    let writes = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    assert!(
        bridge
            .register_process(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                identity,
                executor,
                &admission,
                pc(),
                Budget::default(),
                Box::new(LogicalProvider {
                    writes: writes.clone(),
                    stops: stops.clone(),
                    cancel: None,
                    clock_reads: None
                }),
                || 5
            )
            .is_err()
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
}

#[test]
fn cancellation_at_r2_last_precommit_clock_cannot_create_or_append() {
    for append in [false, true] {
        let mut f = Fixture::new(&module(), limits());
        let bridge = f.connect();
        if append {
            for request in [
                Request::new(
                    "setup-create",
                    Action::Create {
                        session_id: "session".into(),
                        parent: None,
                        parent_tail: 0,
                    },
                )
                .unwrap(),
                Request::new(
                    "setup-writer",
                    Action::OpenWriter {
                        session_id: "session".into(),
                        expected_epoch: 0,
                    },
                )
                .unwrap(),
            ] {
                let run = f.run(&bridge, request.raw()).unwrap();
                assert_eq!(run.report.outcome, Ok(0));
                assert!(matches!(
                    Reply::decode_for(&request, &run.completion.unwrap())
                        .unwrap()
                        .outcome,
                    Outcome::Session(_)
                ));
            }
        }
        let domain = hash(b"morrow/agent-session-exec-v1/session/state/2");
        let before = f
            .runtime
            .store_local()
            .load_agent_ledger_local(&domain, "session")
            .unwrap()
            .map(|r| (r.revision(), r.payload().to_vec()));
        let action = if append {
            Action::Append {
                session_id: "session".into(),
                epoch: 1,
                expected_tail: 0,
                events: vec![morrow_agent_session_exec_v1_r2::Event {
                    event_id: "never-committed".into(),
                    body: b"cancelled".to_vec(),
                }],
            }
        } else {
            Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            }
        };
        let request = Request::new("cancelled-before-commit", action).unwrap();
        let token = bridge.instance().cancellation();
        let mut samples = 0;
        let run = bridge
            .run(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                request.raw(),
                || {
                    samples += 1;
                    if samples == 2 {
                        token.cancel();
                    }
                    6
                },
            )
            .unwrap();
        assert_eq!(
            samples, 2,
            "last prepublish clock reached, then original Core grant rejected"
        );
        assert!(run.report.outcome.is_err());
        assert!(run.completion.is_none());
        let after = f
            .runtime
            .store_local()
            .load_agent_ledger_local(&domain, "session")
            .unwrap()
            .map(|r| (r.revision(), r.payload().to_vec()));
        assert_eq!(
            after, before,
            "cancellation must not merely suppress a committed reply"
        );
        bridge
            .close(&mut f.runtime, &f.host, &mut f.processes)
            .unwrap();
    }
}

#[test]
fn foreign_r2_issuer_close_still_requests_original_provider_cleanup() {
    let mut f = Fixture::new(&module(), limits());
    let mut bridge = f.connect();
    let (executor, admission, identity) = f.native_started();
    let writes = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let handle = bridge
        .register_process(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut f.processes,
            identity,
            executor,
            &admission,
            pc(),
            Budget::default(),
            Box::new(LogicalProvider {
                writes: writes.clone(),
                stops: stops.clone(),
                cancel: None,
                clock_reads: None,
            }),
            || 5,
        )
        .unwrap();
    let connection = bridge.instance().shared_connection();
    let other = Fixture::new(&module(), limits());
    assert!(
        bridge
            .close(&mut f.runtime, &other.host, &mut f.processes)
            .is_err()
    );
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert!(f.runtime.connection_phase(&connection).is_err());
    assert_eq!(bridge.owned_handles(&f.runtime).unwrap(), vec![handle]);
}

#[test]
fn late_registration_denial_retains_the_original_handle_for_close_cleanup() {
    let mut f = Fixture::new(&module(), limits());
    let mut bridge = f.connect();
    let (executor, admission, identity) = f.native_started();
    let writes = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let provider = LateProvider {
        inner: LogicalProvider {
            writes: writes.clone(),
            stops: stops.clone(),
            cancel: None,
            clock_reads: None,
        },
        cancel: bridge.instance().cancellation(),
    };
    assert!(
        bridge
            .register_process(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut f.processes,
                identity,
                executor,
                &admission,
                pc(),
                Budget::default(),
                Box::new(provider),
                || 5
            )
            .is_err()
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(bridge.owned_handles(&f.runtime).unwrap().len(), 1);
    bridge
        .close(&mut f.runtime, &f.host, &mut f.processes)
        .unwrap();
    assert_eq!(
        stops.load(Ordering::SeqCst),
        2,
        "close still owns the denied registration cleanup"
    );
}

#[test]
fn switching_process_hosts_cannot_register_run_or_claim_successful_cleanup() {
    let mut f = Fixture::new(&module(), limits());
    let mut bridge = f.connect();
    let (executor, admission, identity) = f.native_started();
    let writes = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let make_provider = || {
        Box::new(LogicalProvider {
            writes: writes.clone(),
            stops: stops.clone(),
            cancel: None,
            clock_reads: None,
        })
    };
    let handle = bridge
        .register_process(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut f.processes,
            identity.clone(),
            executor.clone(),
            &admission,
            pc(),
            Budget::default(),
            make_provider(),
            || 5,
        )
        .unwrap();
    let mut other = ProcessHost::default();
    assert!(
        bridge
            .register_process(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut other,
                identity,
                executor,
                &admission,
                pc(),
                Budget::default(),
                make_provider(),
                || 6
            )
            .is_err()
    );
    assert!(
        bridge
            .run(
                &f.manager,
                &mut f.runtime,
                &f.host,
                &mut other,
                &request(),
                || 6
            )
            .is_err()
    );
    let connection = bridge.shared_connection();
    assert_eq!(
        bridge.close(&mut f.runtime, &f.host, &mut other),
        Err(Error::Denied)
    );
    assert!(f.runtime.connection_phase(&connection).is_err());
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
    assert_eq!(bridge.owned_handles(&f.runtime).unwrap(), vec![handle]);
    assert!(matches!(
        f.processes.trusted_cleanup(handle).unwrap(),
        EffectOutcome::Accepted
    ));
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}
