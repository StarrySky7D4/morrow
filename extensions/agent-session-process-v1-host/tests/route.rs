//! Original SQLite authority with logical providers. These are not OS execution proofs.
use morrow_agent_process_control_v1::{
    self as process, Capabilities as PC, OutputPage, ReadQuery, ReplyBody,
    host::{Budget, EffectOutcome, Handle, Host as ProcessHost, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request,
    authority::{Admission, Capabilities as SC, SessionExecHost},
    hash,
    safe_exec::ToolIdentity,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Approved, Declaration, PreparedPackage,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Cancellation, Limits};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn session_caps() -> SC {
    SC {
        session_read: true,
        session_write: true,
        propose: true,
        ..SC::default()
    }
}
fn process_caps() -> PC {
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
fn prepared(module: &[u8], sessions: Vec<String>) -> PreparedPackage {
    let base = Package::build(
        Package::manifest_for_task("codex-process-test", "1.0.0", module, vec![]),
        module,
    )
    .unwrap();
    PreparedPackage::new(
        AgentProcessPackage::build(
            base,
            Declaration {
                session: session_caps(),
                process: process_caps(),
                sessions,
                execution_domain: "fixed-domain".into(),
            },
        )
        .unwrap(),
        limits(),
    )
    .unwrap()
}
fn runtime(temp: &tempfile::TempDir) -> HostRuntime {
    HostRuntime::new(
        Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    runtime: HostRuntime,
    host: SessionExecHost,
    prepared: PreparedPackage,
    approved: Approved,
    processes: ProcessHost,
    executor_connection: Arc<Connection>,
    executor: Admission,
    identity: ToolIdentity,
    claim: [u8; 32],
    calls: Arc<AtomicUsize>,
    handle: Option<Handle>,
}
impl Fixture {
    fn new(start: bool, executor_expiry: u64) -> Self {
        Self::with_module(start, executor_expiry, &module())
    }
    fn with_module(start: bool, executor_expiry: u64, wasm: &[u8]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let artifact = temp.path().join("logical-only.bin");
        std::fs::write(&artifact, b"ordinary synthetic artifact; never executed").unwrap();
        let mut runtime = runtime(&temp);
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let prepared = prepared(wasm, vec!["session".into()]);
        let approved = prepared
            .approve(
                &mut runtime,
                &host,
                prepared.package().review_sha256(),
                session_caps(),
                process_caps(),
                vec!["session".into()],
                1000,
                0,
            )
            .unwrap();
        let executor_connection = Arc::new(runtime.connect().unwrap());
        let ec = SC {
            session_read: true,
            execute: true,
            ..SC::default()
        };
        let executor = host
            .admit(
                &runtime,
                &executor_connection,
                ec,
                ec,
                vec!["session".into()],
                "fixed-domain".into(),
                executor_expiry,
                0,
            )
            .unwrap();
        let create = Request::new(
            "create",
            Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        let bytes = host
            .dispatch(
                &mut runtime,
                approved.connection(),
                approved.admission(),
                create.raw(),
                || 1,
            )
            .unwrap();
        assert!(matches!(
            Reply::decode_for(&create, &bytes).unwrap().outcome,
            Outcome::Session(_)
        ));
        let intent = Intent {
            operation_id: "operation".into(),
            program: artifact.to_str().unwrap().into(),
            argv: vec!["fixed".into()],
            cwd: temp.path().to_str().unwrap().into(),
            env: vec![],
            input: vec![],
            execution_domain: "fixed-domain".into(),
            max_runtime_ms: 100,
            artifact_sha256: hash(&std::fs::read(&artifact).unwrap()),
        };
        let propose = Request::new(
            "propose",
            Action::Propose {
                session_id: "session".into(),
                intent,
            },
        )
        .unwrap();
        let bytes = host
            .dispatch(
                &mut runtime,
                approved.connection(),
                approved.admission(),
                propose.raw(),
                || 2,
            )
            .unwrap();
        assert!(matches!(
            Reply::decode_for(&propose, &bytes).unwrap().outcome,
            Outcome::Tool(_)
        ));
        let review = host.review_tool(&runtime, "operation", 3).unwrap();
        let permit = host
            .approve(
                &mut runtime,
                &executor_connection,
                &executor,
                "operation",
                review.proposal_sha256,
                review.intent_sha256,
                3,
            )
            .unwrap();
        let request = Request::new(
            "claim",
            Action::Claim {
                operation_id: "operation".into(),
                permit,
            },
        )
        .unwrap();
        let bytes = host
            .dispatch(
                &mut runtime,
                &executor_connection,
                &executor,
                request.raw(),
                || 4,
            )
            .unwrap();
        let claim = match Reply::decode_for(&request, &bytes).unwrap().outcome {
            Outcome::Claimed { claim, .. } => claim,
            _ => panic!(),
        };
        if start {
            assert_eq!(
                host.execute_claimed(
                    &mut runtime,
                    &executor_connection,
                    &executor,
                    "operation",
                    claim,
                    || 5,
                    |_| Err(Error::CommitUnknown)
                ),
                Err(Error::CommitUnknown)
            );
        }
        let identity = host
            .inspect_tool_record(&runtime, "operation")
            .unwrap()
            .identity;
        Self {
            _temp: temp,
            runtime,
            host,
            prepared,
            approved,
            processes: ProcessHost::default(),
            executor_connection,
            executor,
            identity,
            claim,
            calls: Arc::new(AtomicUsize::new(0)),
            handle: None,
        }
    }
    fn register(
        &mut self,
        connection: Option<Arc<Connection>>,
        admission: Option<Admission>,
    ) -> Result<Handle, Error> {
        let handle = self.prepared.register_process(
            &mut self.runtime,
            &self.host,
            &mut self.approved,
            &mut self.processes,
            self.identity.clone(),
            connection.unwrap_or_else(|| self.executor_connection.clone()),
            admission.as_ref().unwrap_or(&self.executor),
            process_caps(),
            Budget::default(),
            Box::new(LogicalProvider(self.calls.clone())),
            || 6,
        )?;
        self.handle = Some(handle);
        Ok(handle)
    }
    fn call(
        &mut self,
        id: &str,
        action: process::Action,
        mut clock: impl FnMut() -> u64,
    ) -> ReplyBody {
        let handle = self.handle.unwrap();
        let req = process::Request::new(id, handle.nonce, handle.generation, action).unwrap();
        let run = self
            .prepared
            .run(
                &mut self.runtime,
                &self.host,
                &self.approved,
                &mut self.processes,
                &req.encode().unwrap(),
                &mut clock,
                Cancellation::default(),
            )
            .unwrap();
        assert_eq!(run.report.outcome, Ok(0), "{run:?}");
        process::Reply::decode_for(&req, &run.completion.unwrap())
            .unwrap()
            .body
    }
}
struct LogicalProvider(Arc<AtomicUsize>);
impl ProcessProvider for LogicalProvider {
    fn capabilities(&self) -> PC {
        process_caps()
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
        self.0.fetch_add(1, Ordering::SeqCst);
        EffectOutcome::Accepted
    }
    fn close_input(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn interrupt(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.0.fetch_add(1, Ordering::SeqCst);
        EffectOutcome::Accepted
    }
    fn resize(&mut self, _: u16, _: u16) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
}
#[test]
fn claim_history_alone_cannot_register_a_process_or_invoke_a_provider() {
    let mut f = Fixture::new(false, 1000);
    assert!(matches!(f.register(None, None), Err(Error::Denied)));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.host.execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            "operation",
            f.claim,
            || 7,
            |_| Err(Error::CommitUnknown)
        ),
        Err(Error::CommitUnknown)
    );
    assert!(f.register(None, None).is_err()); // registering at old clock 6 must fail closed
}
#[test]
fn original_executor_is_required_and_historical_identity_is_not_a_new_grant() {
    let mut f = Fixture::new(true, 1000);
    assert!(matches!(
        f.register(
            Some(f.approved.shared_connection()),
            Some(f.approved.admission().clone())
        ),
        Err(Error::Denied)
    ));
    let ec = SC {
        session_read: true,
        execute: true,
        ..SC::default()
    };
    let other = Arc::new(f.runtime.connect().unwrap());
    let admission = f
        .host
        .admit(
            &f.runtime,
            &other,
            ec,
            ec,
            vec!["session".into()],
            "fixed-domain".into(),
            1000,
            6,
        )
        .unwrap();
    assert!(f.register(Some(other), Some(admission)).is_err());
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn native_durable_marker_and_live_original_grant_enable_bounded_controls() {
    let mut f = Fixture::new(true, 1000);
    f.register(None, None).unwrap();
    assert_eq!(
        f.call("discover", process::Action::Discover, || 7),
        ReplyBody::Capabilities(process_caps())
    );
    assert_eq!(
        f.call("write", process::Action::Write(vec![1, 2]), || 8),
        ReplyBody::Accepted
    );
    assert_eq!(
        f.call("write", process::Action::Write(vec![1, 2]), || 9),
        ReplyBody::Accepted
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.call(
            "unsupported",
            process::Action::Resize { rows: 24, cols: 80 },
            || 10
        ),
        ReplyBody::Rejected(process::Error::Unsupported)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn revoke_original_executor_proposer_or_tool_blocks_controls() {
    for which in 0..3 {
        let mut f = Fixture::new(true, 1000);
        f.register(None, None).unwrap();
        match which {
            0 => f.host.revoke(&f.executor).unwrap(),
            1 => f.host.revoke(f.approved.admission()).unwrap(),
            _ => f.host.revoke_tool(&mut f.runtime, "operation", 7).unwrap(),
        }
        assert_eq!(
            f.call("blocked", process::Action::Write(vec![1]), || 8),
            ReplyBody::Rejected(process::Error::Denied)
        );
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn tool_expiry_shorter_than_guest_approval_is_enforced() {
    let mut f = Fixture::new(true, 10);
    f.register(None, None).unwrap();
    assert_eq!(
        f.call("expired", process::Action::Write(vec![1]), || 10),
        ReplyBody::Rejected(process::Error::Denied)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn changing_the_original_session_epoch_invalidates_live_process_controls() {
    let mut f = Fixture::new(true, 1000);
    f.register(None, None).unwrap();
    let writer = Request::new(
        "new-writer",
        Action::OpenWriter {
            session_id: "session".into(),
            expected_epoch: 0,
        },
    )
    .unwrap();
    let raw = f
        .host
        .dispatch(
            &mut f.runtime,
            f.approved.connection(),
            f.approved.admission(),
            writer.raw(),
            || 7,
        )
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&writer, &raw).unwrap().outcome,
        Outcome::Session(_)
    ));
    assert_eq!(
        f.call("stale-context", process::Action::Write(vec![1]), || 8),
        ReplyBody::Rejected(process::Error::Denied)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn clock_advancing_during_original_authority_validation_blocks_first_effect() {
    let mut f = Fixture::new(true, 10);
    f.register(None, None).unwrap();
    let mut now = 6;
    assert_eq!(
        f.call(
            "expires-during-review",
            process::Action::Write(vec![1]),
            || {
                now += 1;
                now
            }
        ),
        ReplyBody::Rejected(process::Error::Denied)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn expiry_after_effect_changes_delivery_to_unknown_without_reexecution() {
    let mut f = Fixture::new(true, 1000);
    f.register(None, None).unwrap();
    let calls = f.calls.clone();
    assert_eq!(
        f.call("write", process::Action::Write(vec![1]), || {
            if calls.load(Ordering::SeqCst) == 0 {
                7
            } else {
                1000
            }
        }),
        ReplyBody::Rejected(process::Error::Unknown)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.call("again", process::Action::Write(vec![1]), || 1001),
        ReplyBody::Rejected(process::Error::Denied)
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn reviewed_container_rejects_scope_elevation_and_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime(&temp);
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let p = prepared(&module(), vec!["session".into()]);
    assert!(
        p.approve(
            &mut runtime,
            &host,
            [9; 32],
            session_caps(),
            process_caps(),
            vec!["session".into()],
            1000,
            0
        )
        .is_err()
    );
    assert!(
        p.approve(
            &mut runtime,
            &host,
            p.package().review_sha256(),
            session_caps(),
            process_caps(),
            vec!["other".into()],
            1000,
            0
        )
        .is_err()
    );
    let elevated = PC {
        resize_pty: true,
        ..process_caps()
    };
    assert!(
        p.approve(
            &mut runtime,
            &host,
            p.package().review_sha256(),
            session_caps(),
            elevated,
            vec!["session".into()],
            1000,
            0
        )
        .is_err()
    );
    let mut bytes = p.package().archive().to_vec();
    *bytes.last_mut().unwrap() ^= 1;
    assert!(AgentProcessPackage::decode(&bytes).is_err());
    let mut bytes = p.package().archive().to_vec();
    bytes.push(0);
    assert!(AgentProcessPackage::decode(&bytes).is_err());
}
#[test]
fn compiled_rust_combined_guest_runs_original_session_flow_on_same_sqlite() {
    let path = std::path::PathBuf::from(
        std::env::var_os("MORROW_CODEX_COMBINED_GUEST")
            .expect("qualification requires explicit actual compiled guest"),
    );
    assert!(path.is_absolute());
    let wasm = std::fs::read(path).unwrap();
    assert_eq!(
        hash(&wasm),
        hex_digest("b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e")
    );
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = runtime(&temp);
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let p = prepared(
        &wasm,
        vec!["guest-session".into(), "guest-session-child".into()],
    );
    let approved = p
        .approve(
            &mut runtime,
            &host,
            p.package().review_sha256(),
            session_caps(),
            PC::default(),
            vec!["guest-session".into(), "guest-session-child".into()],
            1000,
            0,
        )
        .unwrap();
    let mut config = approved.generation().to_le_bytes().to_vec();
    config.extend_from_slice(&[7; 16]);
    config.extend_from_slice(b"guest-session");
    let task = Invocation::new_transform(
        "host-bound-task",
        Transform {
            handler: "codex.session.continue".into(),
            input_type: "codex.session.config.v1".into(),
            output_type: "codex.session.receipt.v1".into(),
            input: config,
        },
    )
    .unwrap();
    let run = p
        .run(
            &mut runtime,
            &host,
            &approved,
            &mut ProcessHost::default(),
            task.bytes(),
            || 1,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(run.report.outcome, Ok(0), "{run:?}");
    assert_eq!(run.report.host_calls, 7);
    let output = task.verify_output(&run.completion.unwrap()).unwrap();
    let mut expected = 1u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&hash(b"sealed-state\0\xff"));
    expected.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(output.bytes, expected);
}
fn hex_digest(v: &str) -> [u8; 32] {
    std::array::from_fn(|i| u8::from_str_radix(&v[2 * i..2 * i + 2], 16).unwrap())
}

#[test]
fn started_tool_validation_reads_authority_without_changing_the_record() {
    let mut f = Fixture::new(true, 1000);
    let before = f.host.inspect_tool_record(&f.runtime, "operation").unwrap();
    for _ in 0..3 {
        f.host
            .validate_started_tool(
                &f.runtime,
                &f.executor_connection,
                &f.executor,
                &f.identity,
                || 6,
            )
            .unwrap();
    }
    assert_eq!(
        f.host.inspect_tool_record(&f.runtime, "operation").unwrap(),
        before
    );
    assert_eq!(
        f.host.execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            "operation",
            f.claim,
            || 7,
            |_| panic!("validation must not revive invocation")
        ),
        Err(Error::Denied)
    );
    assert_eq!(
        f.host.inspect_tool_record(&f.runtime, "operation").unwrap(),
        before
    );
}

struct LateProvider {
    calls: Arc<AtomicUsize>,
    late: Arc<AtomicUsize>,
}
impl ProcessProvider for LateProvider {
    fn capabilities(&self) -> PC {
        self.late.store(1, Ordering::SeqCst);
        process_caps()
    }
    fn read(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        LogicalProvider(self.calls.clone()).read(q)
    }
    fn events(&mut self, q: ReadQuery) -> process::Result<OutputPage> {
        self.read(q)
    }
    fn write(&mut self, _: &[u8]) -> EffectOutcome {
        panic!("no guest authority may be delivered")
    }
    fn close_input(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn interrupt(&mut self) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
    fn terminate(&mut self) -> EffectOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        EffectOutcome::Accepted
    }
    fn resize(&mut self, _: u16, _: u16) -> EffectOutcome {
        EffectOutcome::Rejected(process::Error::Unsupported)
    }
}
#[test]
fn denied_late_registration_remains_discoverable_for_trusted_cleanup() {
    let mut f = Fixture::new(true, 1000);
    let late = Arc::new(AtomicUsize::new(0));
    let result = f.prepared.register_process(
        &mut f.runtime,
        &f.host,
        &mut f.approved,
        &mut f.processes,
        f.identity.clone(),
        f.executor_connection.clone(),
        &f.executor,
        process_caps(),
        Budget::default(),
        Box::new(LateProvider {
            calls: f.calls.clone(),
            late: late.clone(),
        }),
        || {
            if late.load(Ordering::SeqCst) == 0 {
                6
            } else {
                1000
            }
        },
    );
    assert!(matches!(result, Err(Error::Denied)));
    let handles = f.prepared.owned_handles(&f.runtime, &f.approved).unwrap();
    assert_eq!(handles.len(), 1);
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.processes.finish(handles[0]),
        Err(process::Error::Conflict)
    );
    let other = tempfile::tempdir().unwrap();
    let other = runtime(&other);
    assert!(f.prepared.owned_handles(&other, &f.approved).is_err());
}

#[test]
fn compiled_rust_process_guest_discovers_and_controls_original_live_binding() {
    let path = std::path::PathBuf::from(
        std::env::var_os("MORROW_CODEX_PROCESS_GUEST")
            .expect("qualification requires explicit actual compiled process guest"),
    );
    let wasm = std::fs::read(path).unwrap();
    assert_eq!(
        hash(&wasm),
        hex_digest("48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36")
    );
    let mut f = Fixture::with_module(true, 1000, &wasm);
    let h = f.register(None, None).unwrap();
    let action = process::Action::Write(b"ordinary synthetic input".to_vec());
    let input =
        process::Request::new("guest-control", h.nonce, h.generation, action.clone()).unwrap();
    let task = Invocation::new_transform(
        "guest-control-task",
        Transform {
            handler: "codex.process.control".into(),
            input_type: "codex.process.request.v1".into(),
            output_type: "codex.process.reply.v1".into(),
            input: input.encode().unwrap(),
        },
    )
    .unwrap();
    let run = f
        .prepared
        .run(
            &mut f.runtime,
            &f.host,
            &f.approved,
            &mut f.processes,
            task.bytes(),
            || 7,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(run.report.outcome, Ok(0), "{run:?}");
    assert_eq!(run.report.host_calls, 2);
    let output = task.verify_output(&run.completion.unwrap()).unwrap();
    let executed = process::Request::new("guest-control-2", h.nonce, h.generation, action).unwrap();
    assert_eq!(
        process::Reply::decode_for(&executed, &output.bytes)
            .unwrap()
            .body,
        ReplyBody::Accepted
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}
