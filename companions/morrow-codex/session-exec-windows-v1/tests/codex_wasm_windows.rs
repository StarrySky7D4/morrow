//! Actual Windows processes coupled to separately compiled, hash-pinned Rust Wasm.
//! Ordinary copied synthetic test executables only; no logical provider stands in for OS effects.
#![cfg(windows)]
use codex_exec_server::{Environment, ExecParams, ProcessId};
use codex_utils_path_uri::PathUri;
use morrow_agent_process_control_v1::{
    self as process, Action as ProcessAction, Capabilities as ProcessCaps, EventKind, OutputPage,
    OutputStream, ReadQuery, ReplyBody,
    host::{Budget, Handle, Host as ProcessHost, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request, ToolPhase,
    authority::{Capabilities as SessionCaps, SessionExecHost},
    hash,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Approved, Declaration, PreparedPackage,
};
use morrow_codex_session_exec_windows_v1::{
    NativeExecutionRegistry, NativeR2State, SharedNativeR2State, WindowsExecutionPort,
    fixed_intent, policy_domain,
};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::Package,
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Cancellation, Limits, TaskRun};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const SESSION: &str = "wasm-windows-session";
const OP: &str = "wasm-windows-operation";
const EXPIRES: u64 = 100_000;
const EXECUTOR_EXPIRES: u64 = 1_000;
const PROPOSAL_SHA: &str = "9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243";
const PROCESS_SHA: &str = "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36";

#[test]
fn wasm_coupled_child_entry() {
    let Ok(mode) = std::env::var("MORROW_WASM_COUPLED_CHILD") else {
        return;
    };
    assert_eq!(std::env::var("MORROW_FIXED").unwrap(), "approved");
    println!("coupled-stdout-approved");
    eprintln!("coupled-stderr-approved");
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    if mode == "interactive" {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut line = String::new();
            if stdin.read_line(&mut line).unwrap() == 0 {
                break;
            }
            println!("coupled-input:{}", line.trim_end());
            std::io::stdout().flush().unwrap();
            if line.trim_end() == "exit" {
                break;
            }
        }
    } else if mode == "sleep" {
        std::thread::sleep(Duration::from_secs(10));
    }
}
fn pinned_module(variable: &str, expected: &str) -> Vec<u8> {
    let path = PathBuf::from(
        std::env::var_os(variable).expect("explicit sealed Rust guest path required"),
    );
    assert!(path.is_absolute());
    let bytes = std::fs::read(&path).unwrap();
    let actual: String = hash(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(actual, expected, "sealed guest hash drift: {variable}");
    bytes
}
fn session_caps() -> SessionCaps {
    SessionCaps {
        session_read: true,
        session_write: true,
        propose: true,
        ..Default::default()
    }
}
fn process_caps() -> ProcessCaps {
    ProcessCaps {
        read: true,
        events: true,
        write: true,
        interrupt: true,
        terminate: true,
        ..Default::default()
    }
}
fn prepared(
    id: &str,
    bytes: &[u8],
    domain: &str,
    session: SessionCaps,
    process: ProcessCaps,
) -> PreparedPackage {
    let base = Package::build(
        Package::manifest_for_task(id, "1.0.0", bytes, vec![]),
        bytes,
    )
    .unwrap();
    let package = AgentProcessPackage::build(
        base,
        Declaration {
            session,
            process,
            sessions: vec![SESSION.into()],
            execution_domain: domain.into(),
        },
    )
    .unwrap();
    PreparedPackage::new(
        package,
        Limits {
            fuel: 100_000_000,
            memory_bytes: 16 * 1024 * 1024,
            host_calls: 4,
        },
    )
    .unwrap()
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
struct Fixture {
    state: SharedNativeR2State,
    clock: Arc<AtomicU64>,
    proposal: PreparedPackage,
    proposal_approved: Approved,
    process: PreparedPackage,
    process_approved: Approved,
    processes: ProcessHost,
    params: ExecParams,
    intent: Intent,
    port: WindowsExecutionPort,
    handle: Option<Handle>,
    task_sequence: u64,
    // Release process, artifact and SQLite handles before TempDir cleanup.
    _temp: tempfile::TempDir,
}
impl Fixture {
    fn new(mode: &str, handle: tokio::runtime::Handle) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("actual-wasm-coupled-test.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let params = ExecParams {
            process_id: ProcessId::new(OP),
            metadata: None,
            argv: vec![
                exe.to_str().unwrap().into(),
                "--exact".into(),
                "wasm_coupled_child_entry".into(),
                "--nocapture".into(),
                "--quiet".into(),
            ],
            cwd: PathUri::from_host_native_path(temp.path()).unwrap(),
            env_policy: None,
            shell_snapshot: None,
            env: HashMap::from([
                ("MORROW_WASM_COUPLED_CHILD".into(), mode.into()),
                ("MORROW_FIXED".into(), "approved".into()),
            ]),
            tty: false,
            pipe_stdin: mode == "interactive",
            arg0: None,
            sandbox: None,
            enforce_managed_network: false,
            managed_network: None,
            network_proxy: None,
        };
        let domain = policy_domain(&params).unwrap();
        let intent = fixed_intent(
            OP.into(),
            &params,
            hash(&std::fs::read(&exe).unwrap()),
            10_000,
        )
        .unwrap();
        let mut original = HostRuntime::new(
            Store::open(
                &temp.path().join("actual-wasm.sqlite"),
                EventBudget::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let proposer_connection = original.connect().unwrap();
        let executor_connection = Arc::new(original.connect().unwrap());
        let host = SessionExecHost::new(&mut original).unwrap();
        let pc = session_caps();
        let ec = SessionCaps {
            session_read: true,
            execute: true,
            ..Default::default()
        };
        let proposer = host
            .admit(
                &original,
                &proposer_connection,
                pc,
                pc,
                vec![SESSION.into()],
                domain.clone(),
                EXPIRES,
                1,
            )
            .unwrap();
        let executor = host
            .admit(
                &original,
                &executor_connection,
                ec,
                ec,
                vec![SESSION.into()],
                domain.clone(),
                EXECUTOR_EXPIRES,
                1,
            )
            .unwrap();
        let proposal_caps = SessionCaps {
            session_read: true,
            propose: true,
            ..Default::default()
        };
        let proposal = prepared(
            "actual-rust-proposal",
            &pinned_module("MORROW_PROPOSAL_GUEST", PROPOSAL_SHA),
            &domain,
            proposal_caps,
            ProcessCaps::default(),
        );
        let proposal_approved = proposal
            .approve(
                &mut original,
                &host,
                proposal.package().review_sha256(),
                proposal_caps,
                ProcessCaps::default(),
                vec![SESSION.into()],
                EXPIRES,
                1,
            )
            .unwrap();
        let control_session = SessionCaps {
            session_read: true,
            ..Default::default()
        };
        let process = prepared(
            "actual-rust-process",
            &pinned_module("MORROW_PROCESS_GUEST", PROCESS_SHA),
            &domain,
            control_session,
            process_caps(),
        );
        let process_approved = process
            .approve(
                &mut original,
                &host,
                process.package().review_sha256(),
                control_session,
                process_caps(),
                vec![SESSION.into()],
                EXPIRES,
                1,
            )
            .unwrap();
        let create = Request::new(
            "create-before-actual-wasm",
            Action::Create {
                session_id: SESSION.into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        let raw = host
            .dispatch(
                &mut original,
                &proposer_connection,
                &proposer,
                create.raw(),
                || 1,
            )
            .unwrap();
        assert!(matches!(
            Reply::decode_for(&create, &raw).unwrap().outcome,
            Outcome::Session(_)
        ));
        let clock = Arc::new(AtomicU64::new(1));
        let current = clock.clone();
        let state = Arc::new(Mutex::new(NativeR2State {
            runtime: original,
            host,
            proposer_connection,
            executor_connection,
            proposer,
            executor,
            session_id: SESSION.into(),
            clock: Arc::new(move || current.load(Ordering::SeqCst)),
            native_executions: Arc::new(NativeExecutionRegistry::default()),
        }));
        let backend = {
            // Upstream Environment starts its async actor during construction.
            let _entered = handle.enter();
            Environment::default_for_tests().get_exec_backend()
        };
        let port = WindowsExecutionPort::new(state.clone(), backend, handle).unwrap();
        Self {
            _temp: temp,
            state,
            clock,
            proposal,
            proposal_approved,
            process,
            process_approved,
            processes: ProcessHost::default(),
            params,
            intent,
            port,
            handle: None,
            task_sequence: 0,
        }
    }
    fn propose_claim_start_and_register(&mut self) {
        let input = Request::new_for_generation(
            "host-fixed-proposal-task",
            self.proposal_approved.generation(),
            Action::Propose {
                session_id: SESSION.into(),
                intent: self.intent.clone(),
            },
        )
        .unwrap();
        let nonce: String = input.digest()[..16]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let actual = Request::new_for_generation(
            format!("codex-{nonce}-1"),
            input.generation(),
            input.action().clone(),
        )
        .unwrap();
        let task = Invocation::new_transform(
            "actual-wasm-proposal-task",
            Transform {
                handler: "codex.session.propose".into(),
                input_type: "codex.session.proposal.v1".into(),
                output_type: "codex.session.proposal.receipt.v1".into(),
                input: input.raw().to_vec(),
            },
        )
        .unwrap();
        let state = self.state.clone();
        let clock = self.clock.clone();
        let claim = NativeR2State::with_state(&state, |s| {
            let run = self.proposal.run(&mut s.runtime,&s.host,&self.proposal_approved,&mut self.processes,task.bytes(),||clock.load(Ordering::SeqCst),Cancellation::default())?;
            assert_eq!(run.report.outcome,Ok(0),"actual proposal Wasm: {run:?}");
            assert_eq!(run.report.host_calls,1);
            let output = task.verify_output(run.completion.as_ref().unwrap()).unwrap();
            assert!(matches!(Reply::decode_for(&actual,&output.bytes)?.outcome,Outcome::Tool(info) if info.phase==ToolPhase::Proposed));
            let review = s.host.review_tool(&s.runtime,OP,(s.clock)())?;
            assert_eq!(review.proposal_sha256,actual.digest());
            assert_eq!(review.intent_sha256,self.intent.digest()?);
            let permit = s.host.approve(&mut s.runtime,&s.executor_connection,&s.executor,OP,actual.digest(),review.intent_sha256,(s.clock)())?;
            match s.dispatch(&Request::new("actual-wasm-native-claim",Action::Claim { operation_id:OP.into(),permit })?,true)? {
                Outcome::Claimed { claim,.. } => Ok(claim), _=>Err(Error::Denied),
            }
        }).unwrap();
        // Start outside the owner mutex. No guest or logical callback starts a process.
        let provider = self
            .port
            .start_claimed(
                self.port
                    .approve_fixed(self.intent.clone(), self.params.clone())
                    .unwrap(),
                claim,
            )
            .unwrap();
        let identity = provider.observed_identity().clone();
        println!(
            "actual-coupling: proposal_request={} proposal_sha={:02x?} intent_sha={:02x?} artifact_sha={:02x?} sandbox={:?}",
            actual.id(),
            actual.digest(),
            identity.intent_sha256,
            self.intent.artifact_sha256,
            provider.sandbox_type()
        );
        assert!(provider.capabilities().events);
        assert!(!provider.capabilities().close_input);
        assert!(!provider.capabilities().resize_pty);
        let clock = self.clock.clone();
        let handle = NativeR2State::with_state(&state, |s| {
            let observed = s.host.inspect_tool_record(&s.runtime, OP)?;
            assert!(observed.invocation_started);
            assert_eq!(identity, observed.identity);
            self.process.register_process(
                &mut s.runtime,
                &s.host,
                &mut self.process_approved,
                &mut self.processes,
                identity,
                s.executor_connection.clone(),
                &s.executor,
                process_caps(),
                Budget::default(),
                Box::new(provider),
                || clock.load(Ordering::SeqCst),
            )
        })
        .unwrap();
        self.handle = Some(handle);
        assert!(matches!(
            self.port.start_claimed(
                self.port
                    .approve_fixed(self.intent.clone(), self.params.clone())
                    .unwrap(),
                claim
            ),
            Err(Error::Denied)
        ));
    }
    fn run_process(
        &mut self,
        id: &str,
        action: ProcessAction,
    ) -> (Invocation, process::Request, TaskRun) {
        self.task_sequence += 1;
        let handle = self.handle.unwrap();
        let input =
            process::Request::new(id, handle.nonce, handle.generation, action.clone()).unwrap();
        let suffix = if matches!(action, ProcessAction::Discover) {
            1
        } else {
            2
        };
        let actual = process::Request::new(
            format!("{id}-{suffix}"),
            handle.nonce,
            handle.generation,
            action,
        )
        .unwrap();
        let task = Invocation::new_transform(
            &format!("actual-process-task-{}", self.task_sequence),
            Transform {
                handler: "codex.process.control".into(),
                input_type: "codex.process.request.v1".into(),
                output_type: "codex.process.reply.v1".into(),
                input: input.encode().unwrap(),
            },
        )
        .unwrap();
        let state = self.state.clone();
        let clock = self.clock.clone();
        let run = NativeR2State::with_runtime(&state, |runtime, host| {
            self.process.run(
                runtime,
                host,
                &self.process_approved,
                &mut self.processes,
                task.bytes(),
                || clock.load(Ordering::SeqCst),
                Cancellation::default(),
            )
        })
        .unwrap();
        (task, actual, run)
    }
    fn guest(&mut self, id: &str, action: ProcessAction) -> ReplyBody {
        let (task, actual, run) = self.run_process(id, action);
        assert_eq!(run.report.outcome, Ok(0), "actual process Wasm: {run:?}");
        assert_eq!(
            run.report.host_calls,
            if matches!(actual.action, ProcessAction::Discover) {
                1
            } else {
                2
            }
        );
        let output = task
            .verify_output(run.completion.as_ref().unwrap())
            .unwrap();
        process::Reply::decode_for(&actual, &output.bytes)
            .unwrap()
            .body
    }
    fn page(&mut self, after: u64, events: bool) -> OutputPage {
        let q = ReadQuery {
            after_seq: after,
            max_bytes: 4096,
            max_events: 16,
            wait_ms: 100,
        };
        let id = format!("observed-page-{}", self.task_sequence + 1);
        match self.guest(
            &id,
            if events {
                ProcessAction::Events(q)
            } else {
                ProcessAction::Read(q)
            },
        ) {
            ReplyBody::Page(page) => page,
            body => panic!("{body:?}"),
        }
    }
    fn trusted_page(&mut self, after: u64) -> OutputPage {
        self.processes
            .trusted_observe(
                self.handle.unwrap(),
                ReadQuery {
                    after_seq: after,
                    max_bytes: 4096,
                    max_events: 16,
                    wait_ms: 100,
                },
            )
            .unwrap()
    }
    fn cleanup(&mut self) {
        self.processes
            .trusted_cleanup(self.handle.unwrap())
            .unwrap();
    }
    fn finish(&mut self) {
        self.processes.finish(self.handle.unwrap()).unwrap();
        let start = Instant::now();
        loop {
            let statuses =
                NativeR2State::with_state(&self.state, |s| s.native_executions.statuses()).unwrap();
            if statuses.is_empty() {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(6),
                "native owner still has charged process resources: {statuses:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            let _ = self.processes.trusted_cleanup(handle);
        }
    }
}
#[derive(Default)]
struct Observed {
    after: u64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
    exited: bool,
    closed: bool,
    saw_closed: bool,
}
impl Observed {
    fn accept(&mut self, page: OutputPage) {
        assert!(!page.gap);
        assert!(page.failure.is_none(), "{:?}", page.failure);
        for event in page.events {
            assert_eq!(event.seq, self.after + 1);
            self.after = event.seq;
            match event.kind {
                EventKind::Output { stream, chunk } => {
                    if stream == OutputStream::Stderr {
                        self.stderr.extend(chunk)
                    } else {
                        self.stdout.extend(chunk)
                    }
                }
                EventKind::Exited { exit_code, .. } => self.exit_code = Some(exit_code),
                EventKind::Closed => self.saw_closed = true,
            }
        }
        assert_eq!(page.next_seq, self.after);
        self.exited = page.exited;
        self.exit_code = page.exit_code;
        self.closed = page.closed;
    }
    fn collect(&mut self, f: &mut Fixture, trusted: bool, until: Option<&str>) {
        let start = Instant::now();
        loop {
            self.accept(if trusted {
                f.trusted_page(self.after)
            } else {
                f.page(self.after, true)
            });
            if until.is_some_and(|s| String::from_utf8_lossy(&self.stdout).contains(s))
                || self.exited && self.closed && self.saw_closed
            {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "real stdout/exit/EOF observation timed out"
            );
        }
    }
}

#[test]
fn actual_rust_wasm_proposal_claim_windows_process_and_wasm_controls() {
    let rt = runtime();
    let mut f = Fixture::new("interactive", rt.handle().clone());
    f.propose_claim_start_and_register();
    let discovered = f.guest("capabilities", ProcessAction::Discover);
    assert!(
        matches!(discovered,ReplyBody::Capabilities(c) if c.read&&c.events&&c.write&&c.terminate&&!c.close_input&&!c.resize_pty)
    );
    let mut observed = Observed::default();
    observed.collect(&mut f, false, Some("coupled-stdout-approved"));
    assert_eq!(
        f.guest(
            "actual-write",
            ProcessAction::Write(b"approved-input\n".to_vec())
        ),
        ReplyBody::Accepted
    );
    observed.collect(&mut f, false, Some("coupled-input:approved-input"));
    assert!(!observed.exited);
    let read = f.page(observed.after, false);
    observed.accept(read);
    assert_eq!(
        f.guest("actual-terminate", ProcessAction::Terminate),
        ReplyBody::Accepted
    );
    observed.collect(&mut f, false, None);
    assert!(observed.exited && observed.closed);
    assert!(observed.exit_code.is_some());
    assert!(String::from_utf8_lossy(&observed.stderr).contains("coupled-stderr-approved"));
    let start = Instant::now();
    loop {
        let retained =
            NativeR2State::with_state(&f.state, |s| s.host.inspect_tool_record(&s.runtime, OP))
                .unwrap();
        if let Some(facts) = retained.latest {
            assert_eq!(facts.exit_code, observed.exit_code);
            assert!(facts.output_closed);
            assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
            assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
            assert_eq!(facts.stdout_bytes, observed.stdout.len() as u64);
            assert_eq!(facts.stderr_bytes, observed.stderr.len() as u64);
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "actual terminal facts were not retained"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    f.finish();
    println!(
        "actual-coupling: verified stdout={} stderr={} exit={:?} EOF={} native resources released",
        observed.stdout.len(),
        observed.stderr.len(),
        observed.exit_code,
        observed.closed
    );
}
#[test]
fn expired_or_revoked_original_executor_blocks_actual_wasm_controls() {
    let rt = runtime();
    for expired in [true, false] {
        let mut f = Fixture::new("sleep", rt.handle().clone());
        f.propose_claim_start_and_register();
        let mut observed = Observed::default();
        observed.collect(&mut f, false, Some("coupled-stdout-approved"));
        if expired {
            // Both guest package admissions are still live; only the original
            // executor expires. Package-local scope cannot revive its authority.
            f.clock.store(EXECUTOR_EXPIRES, Ordering::SeqCst);
        } else {
            NativeR2State::with_state(&f.state, |s| s.host.revoke(&s.executor)).unwrap();
        }
        let (_, _, run) = f.run_process("forbidden-control", ProcessAction::Terminate);
        assert!(run.report.outcome.is_err());
        assert!(run.completion.is_none());
        assert_eq!(run.report.host_calls, 1);
        let page = f.trusted_page(observed.after);
        assert!(
            !page.exited,
            "unauthorized guest terminated the actual process"
        );
        observed.accept(page);
        f.cleanup();
        observed.collect(&mut f, true, None);
        assert!(observed.exited && observed.closed);
        f.finish();
        println!(
            "actual-coupling: original executor {} prevented guest control; owner observed real exit and EOF",
            if expired { "expiry" } else { "revocation" }
        );
    }
}
#[test]
fn actual_input_effect_unknown_delivery_is_not_replayed_by_rust_wasm() {
    let rt = runtime();
    let mut f = Fixture::new("interactive", rt.handle().clone());
    f.propose_claim_start_and_register();
    let mut observed = Observed::default();
    observed.collect(&mut f, false, Some("coupled-stdout-approved"));
    let action = ProcessAction::Write(b"once-marker\n".to_vec());
    assert_eq!(
        f.guest("lost-delivery", action.clone()),
        ReplyBody::Accepted
    );
    observed.collect(&mut f, false, Some("coupled-input:once-marker"));
    let handle = f.handle.unwrap();
    let actual = process::Request::new(
        "lost-delivery-2",
        handle.nonce,
        handle.generation,
        action.clone(),
    )
    .unwrap();
    // The provider really accepted input, then the trusted owner records lost delivery.
    // This injection changes the receipt; it performs no replacement process effect.
    f.processes.veto_delivery(&actual).unwrap();
    assert_eq!(
        f.guest("lost-delivery", action.clone()),
        ReplyBody::Rejected(process::Error::Unknown)
    );
    assert_eq!(
        f.guest("new-attempt", action),
        ReplyBody::Rejected(process::Error::Unknown)
    );
    assert_eq!(
        f.guest("unknown-terminate", ProcessAction::Terminate),
        ReplyBody::Rejected(process::Error::Unknown)
    );
    f.cleanup();
    observed.collect(&mut f, true, None);
    assert!(observed.exited && observed.closed);
    assert_eq!(
        String::from_utf8_lossy(&observed.stdout)
            .matches("coupled-input:once-marker")
            .count(),
        1
    );
    f.finish();
    println!(
        "actual-coupling: once-marker effects=1 after Unknown replay and new-control attempts; owner observed real exit and EOF"
    );
}
