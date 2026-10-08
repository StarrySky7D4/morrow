#![cfg(windows)]
use codex_exec_server::{Environment, ExecParams, ProcessId};
use codex_utils_path_uri::PathUri;
use morrow_agent_process_control_v1::{
    EventKind, ReadQuery,
    host::{EffectOutcome, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Request, ToolPhase,
    authority::{Capabilities, SessionExecHost},
    hash,
};
use morrow_codex_session_exec_windows_v1::{
    NativeR2State, SharedNativeR2State, WindowsExecutionPort, WindowsProcessProvider, fixed_intent,
    policy_domain,
};
use morrow_core::{
    dispatch::HostRuntime,
    store::{EventBudget, Store},
};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const SESSION: &str = "synthetic-session";
const OP: &str = "synthetic-process";
#[test]
fn child_entry() {
    let Ok(mode) = std::env::var("MORROW_CHILD") else {
        return;
    };
    assert_eq!(std::env::var("MORROW_FIXED").unwrap(), "approved");
    println!("actual-stdout-approved");
    eprintln!("actual-stderr-approved");
    std::io::stdout().flush().unwrap();
    if mode == "interactive" {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).unwrap();
        println!("actual-input:{line}");
        std::io::stdout().flush().unwrap();
    }
    if mode != "oneshot" {
        std::thread::sleep(Duration::from_secs(10));
    }
}
struct Fixture {
    _temp: tempfile::TempDir,
    state: SharedNativeR2State,
    clock: Arc<AtomicU64>,
    params: ExecParams,
    intent: Intent,
    port: WindowsExecutionPort,
}
impl Fixture {
    fn new(mode: &str, deadline: u64, handle: tokio::runtime::Handle) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("synthetic-test.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let params = ExecParams {
            process_id: ProcessId::new(OP),
            metadata: None,
            argv: vec![
                exe.to_str().unwrap().into(),
                "--exact".into(),
                "child_entry".into(),
                "--nocapture".into(),
                "--quiet".into(),
            ],
            cwd: PathUri::from_host_native_path(temp.path()).unwrap(),
            env_policy: None,
            shell_snapshot: None,
            env: HashMap::from([
                ("MORROW_CHILD".into(), mode.into()),
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
            deadline,
        )
        .unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(
                &temp.path().join("synthetic.sqlite"),
                EventBudget::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let proposer_connection = runtime.connect().unwrap();
        let executor_connection = Arc::new(runtime.connect().unwrap());
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let pc = Capabilities {
            session_read: true,
            session_write: true,
            propose: true,
            execute: false,
            retire: false,
        };
        let ec = Capabilities {
            session_read: true,
            session_write: false,
            propose: false,
            execute: true,
            retire: false,
        };
        let proposer = host
            .admit(
                &runtime,
                &proposer_connection,
                pc,
                pc,
                vec![SESSION.into()],
                domain.clone(),
                100000,
                1,
            )
            .unwrap();
        let executor = host
            .admit(
                &runtime,
                &executor_connection,
                ec,
                ec,
                vec![SESSION.into()],
                domain,
                100000,
                1,
            )
            .unwrap();
        let clock = Arc::new(AtomicU64::new(1));
        let c = clock.clone();
        let state = Arc::new(Mutex::new(NativeR2State {
            runtime,
            host,
            proposer_connection,
            executor_connection,
            proposer,
            executor,
            session_id: SESSION.into(),
            native_executions: Arc::new(Default::default()),
            clock: Arc::new(move || c.load(Ordering::SeqCst)),
        }));
        NativeR2State::with_state(&state, |s| {
            let request = Request::new(
                "create",
                Action::Create {
                    session_id: SESSION.into(),
                    parent: None,
                    parent_tail: 0,
                },
            )?;
            assert!(matches!(s.dispatch(&request, false)?, Outcome::Session(_)));
            Ok(())
        })
        .unwrap();
        // LocalProcess construction starts its real notification drain on this runtime.
        let backend = {
            let _entered = handle.enter();
            Environment::default_for_tests().get_exec_backend()
        };
        let port = WindowsExecutionPort::new(state.clone(), backend, handle).unwrap();
        Self {
            _temp: temp,
            state,
            clock,
            params,
            intent,
            port,
        }
    }
    fn claim(&self) -> [u8; 32] {
        NativeR2State::with_state(&self.state, |s| {
            let request = Request::new(
                "propose",
                Action::Propose {
                    session_id: SESSION.into(),
                    intent: self.intent.clone(),
                },
            )?;
            assert!(matches!(s.dispatch(&request, false)?, Outcome::Tool(_)));
            let review = s.host.review_tool(&s.runtime, OP, (s.clock)())?;
            let permit = s.host.approve(
                &mut s.runtime,
                &s.executor_connection,
                &s.executor,
                OP,
                review.proposal_sha256,
                review.intent_sha256,
                (s.clock)(),
            )?;
            let request = Request::new(
                "claim",
                Action::Claim {
                    operation_id: OP.into(),
                    permit,
                },
            )?;
            match s.dispatch(&request, true)? {
                Outcome::Claimed { claim, .. } => Ok(claim),
                _ => Err(Error::Denied),
            }
        })
        .unwrap()
    }
    fn start(
        &self,
        claim: [u8; 32],
    ) -> morrow_agent_session_exec_v1_r2::Result<WindowsProcessProvider> {
        self.port.start_claimed(
            self.port
                .approve_fixed(self.intent.clone(), self.params.clone())?,
            claim,
        )
    }
    fn observed(&self) -> morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation {
        NativeR2State::with_state(&self.state, |s| s.host.inspect_tool_record(&s.runtime, OP))
            .unwrap()
    }
}
fn collect(
    provider: &mut WindowsProcessProvider,
    stop_on_text: Option<&str>,
) -> (Vec<u8>, Vec<u8>, bool, bool) {
    let start = Instant::now();
    let mut after = 0;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "real exit/output closure not observed"
        );
        let page = provider
            .events(ReadQuery {
                after_seq: after,
                max_bytes: 32768,
                max_events: 16,
                wait_ms: 100,
            })
            .unwrap();
        assert!(!page.gap);
        assert!(page.failure.is_none(), "{:?}", page.failure);
        for event in page.events {
            assert!(event.seq > after);
            after = event.seq;
            if let EventKind::Output { stream, chunk } = event.kind {
                if stream == morrow_agent_process_control_v1::OutputStream::Stderr {
                    stderr.extend(chunk);
                } else {
                    stdout.extend(chunk);
                }
            }
        }
        if stop_on_text.is_some_and(|text| String::from_utf8_lossy(&stdout).contains(text))
            || page.exited && page.closed
        {
            return (stdout, stderr, page.exited, page.closed);
        }
    }
}
#[test]
fn actual_stdout_stderr_exit_eof_and_durable_identity() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("oneshot", 5000, rt.handle().clone());
    let claim = f.claim();
    let mut p = f.start(claim).unwrap();
    assert!(f.observed().invocation_started);
    assert_eq!(p.observed_identity(), &f.observed().identity);
    let (stdout, stderr, exited, closed) = collect(&mut p, None);
    assert!(exited && closed);
    assert!(String::from_utf8_lossy(&stdout).contains("actual-stdout-approved"));
    assert!(String::from_utf8_lossy(&stderr).contains("actual-stderr-approved"));
    for _ in 0..100 {
        if f.observed().latest.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let facts = f.observed().latest.unwrap();
    assert_eq!(facts.exit_code, Some(0));
    assert!(facts.output_closed);
    assert_eq!(facts.stdout_sha256, hash(&stdout));
    assert_eq!(facts.stderr_sha256, hash(&stderr));
    assert_eq!(f.observed().phase, ToolPhase::DispatchUnknown);
    assert!(matches!(f.start(claim), Err(Error::Denied)));
}
#[test]
fn actual_write_then_terminate_and_typed_unsupported() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("interactive", 5000, rt.handle().clone());
    let mut p = f.start(f.claim()).unwrap();
    assert!(p.capabilities().write);
    assert_eq!(
        p.close_input(),
        EffectOutcome::Rejected(morrow_agent_process_control_v1::Error::Unsupported)
    );
    assert_eq!(
        p.resize(25, 80),
        EffectOutcome::Rejected(morrow_agent_process_control_v1::Error::Unsupported)
    );
    assert_eq!(p.write(b"original-input\n"), EffectOutcome::Accepted);
    let (_, _, exited, _) = collect(&mut p, Some("actual-input:original-input"));
    assert!(!exited);
    assert_eq!(p.terminate(), EffectOutcome::Accepted);
    let (_, _, exited, closed) = collect(&mut p, None);
    assert!(exited && closed);
}
#[test]
fn actual_deadline_and_interrupt_use_original_process() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("sleep", 200, rt.handle().clone());
    let mut p = f.start(f.claim()).unwrap();
    let (_, _, exited, closed) = collect(&mut p, None);
    assert!(exited && closed);
    let f = Fixture::new("sleep", 5000, rt.handle().clone());
    let mut p = f.start(f.claim()).unwrap();
    assert!(p.capabilities().interrupt);
    assert_eq!(p.interrupt(), EffectOutcome::Accepted);
    let (_, _, exited, closed) = collect(&mut p, None);
    assert!(exited && closed);
}
#[test]
fn revoked_claim_never_starts_and_policy_changes_are_rejected() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("oneshot", 5000, rt.handle().clone());
    let claim = f.claim();
    let mut changed = f.params.clone();
    changed.enforce_managed_network = true;
    assert!(matches!(
        f.port.approve_fixed(f.intent.clone(), changed),
        Err(Error::Denied)
    ));
    NativeR2State::with_state(&f.state, |s| s.host.revoke(&s.executor)).unwrap();
    assert!(f.start(claim).is_err());
    assert!(!f.observed().invocation_started);
}
#[test]
fn expired_claim_never_starts() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("oneshot", 5000, rt.handle().clone());
    let claim = f.claim();
    f.clock.store(100000, Ordering::SeqCst);
    assert!(f.start(claim).is_err());
    assert!(!f.observed().invocation_started);
}

#[test]
fn unsupported_runtime_and_changed_artifact_never_invoke_backend() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("oneshot", 5000, rt.handle().clone());
    let current = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        WindowsExecutionPort::new(
            f.state.clone(),
            {
                let _entered = rt.enter();
                Environment::default_for_tests().get_exec_backend()
            },
            current.handle().clone()
        ),
        Err(Error::Invalid)
    ));
    let mut wrong = f.intent.clone();
    wrong.artifact_sha256 = [77; 32];
    assert!(matches!(
        f.port.approve_fixed(wrong, f.params.clone()),
        Err(Error::Denied)
    ));
    let approved = f
        .port
        .approve_fixed(f.intent.clone(), f.params.clone())
        .unwrap();
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&f.intent.program)
            .is_err()
    );
    drop(approved);
}

#[test]
fn upstream_scrubbed_keys_are_rejected_before_native_start() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let f = Fixture::new("oneshot", 5000, rt.handle().clone());
    for key in codex_protocol::shell_environment::NON_INHERITABLE_ENV_VARS
        .iter()
        .copied()
        .chain(std::iter::once(
            codex_exec_server::CODEX_EXEC_SERVER_EXIT_ON_STDIN_CLOSE_ENV_VAR,
        ))
    {
        let mut params = f.params.clone();
        params.env.insert(key.into(), "synthetic-only".into());
        assert!(matches!(
            fixed_intent(OP.into(), &params, f.intent.artifact_sha256, 5000),
            Err(Error::Invalid)
        ));
        let mut params = f.params.clone();
        params
            .env
            .insert(key.to_ascii_lowercase(), "synthetic-only".into());
        assert!(matches!(
            fixed_intent(OP.into(), &params, f.intent.artifact_sha256, 5000),
            Err(Error::Invalid)
        ));
    }
}
