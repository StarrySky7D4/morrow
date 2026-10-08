#![cfg(all(windows, feature = "qualification-harness"))]
//! Ordinary TempDir/current-test-exe qualification only: no ProtectedSession,
//! DPAPI, user database, production Environment, or OS sandbox qualification.
//! The child entry is a helper and must be excluded from meaningful pass counts.
//! This suite does not run or rebuild the sealed proposal/process Wasm guests.

use codex_exec_server::{
    ExecBackend, ExecBackendFuture, ExecParams, ExecProcess, ExecProcessEventReceiver,
    ExecProcessFuture, ProcessId, ProcessSignal, ReadResponse, WriteResponse,
};
use codex_utils_path_uri::PathUri;
use morrow_agent_process_control_v1::{
    EventKind, OutputStream, ReadQuery,
    host::{EffectOutcome, ProcessProvider},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request, Result, ToolPhase,
    authority::{Admission, Capabilities, SessionExecHost},
    hash,
    safe_exec::ToolObservation,
};
use morrow_codex_session_exec_windows_v1::{
    BorrowedNativeResources, BorrowedWindowsExecutionPort, ExecServerRuntimeOptions,
    HttpClientFactory, MAX_NATIVE_EXECUTIONS, OutboundProxyPolicy, ProvisionedWindowsBackend,
    ReviewedBorrowedInvocation, SandboxType, WindowsProcessProvider,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    store::{EventBudget, Store},
};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const SESSION: &str = "borrowed-synthetic-session";
const EXPIRES: u64 = 100_000;

#[test]
fn borrowed_owner_child_entry() {
    let Ok(mode) = std::env::var("MORROW_BORROWED_CHILD") else {
        eprintln!("child helper skipped: no synthetic child invocation");
        return;
    };
    assert_eq!(std::env::var("MORROW_FIXED").unwrap(), "approved");
    println!("borrowed-actual-stdout");
    eprintln!("borrowed-actual-stderr");
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    if mode == "read-to-end" {
        let mut input = Vec::new();
        std::io::stdin().lock().read_to_end(&mut input).unwrap();
        let digest: String = hash(&input)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        println!("borrowed-input-eof:{}:{digest}", input.len());
        std::io::stdout().flush().unwrap();
        return;
    }
    if mode == "pty-observe" {
        // Python's safe stdlib queries this child's real Win32 console. No
        // parent PTY cached size or process control acknowledgement is sampled.
        let python = std::env::var_os("MORROW_TEST_PYTHON")
            .expect("ordinary ConPTY observation requires explicit MORROW_TEST_PYTHON");
        let script = "import os,sys\nprint('borrowed-pty-ready',flush=True)\nfor line in sys.stdin:\n token=line.strip()\n if not token: continue\n if token=='exit': break\n if token not in ('size-1','size-2'): raise RuntimeError('unexpected query')\n size=os.get_terminal_size(sys.stdout.fileno())\n print('borrowed-pty-'+token+':'+str(size.lines)+':'+str(size.columns),flush=True)\n";
        let status = std::process::Command::new(python)
            .args(["-I", "-u", "-c", script])
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "independent OS console query failed: {status}"
        );
        return;
    }
    if mode == "interactive" {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).unwrap();
        println!("borrowed-actual-input:{}", line.trim_end());
        std::io::stdout().flush().unwrap();
    }
    if mode != "oneshot" {
        std::thread::sleep(Duration::from_secs(15));
    }
}

/// Fault timing and call counts wrap the real upstream backend. Every success
/// returns its actual StartedExecProcess and original event receiver unchanged.
struct CountedBackend {
    actual: Arc<dyn ExecBackend>,
    calls: Arc<AtomicUsize>,
    launches: Arc<AtomicUsize>,
    delay: Duration,
    legacy_controls: bool,
    pending_capabilities: bool,
}
impl ExecBackend for CountedBackend {
    fn start(&self, params: ExecParams) -> ExecBackendFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            self.launches.fetch_add(1, Ordering::SeqCst);
            let mut started = self.actual.start(params).await?;
            if self.pending_capabilities {
                started.process = Arc::new(PendingCapabilitiesAdapter(LegacyProcessAdapter(
                    started.process,
                )));
            } else if self.legacy_controls {
                started.process = Arc::new(LegacyProcessAdapter(started.process));
            }
            Ok(started)
        })
    }
}

/// Delegates genuine old process IO/lifecycle but deliberately does not opt in
/// to the new checked controls. Their trait defaults must stay unsupported.
struct LegacyProcessAdapter(Arc<dyn ExecProcess>);
impl ExecProcess for LegacyProcessAdapter {
    fn process_id(&self) -> &ProcessId {
        self.0.process_id()
    }
    fn subscribe_wake(&self) -> tokio::sync::watch::Receiver<u64> {
        self.0.subscribe_wake()
    }
    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        self.0.subscribe_events()
    }
    fn read(
        &self,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
        wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        self.0.read(after_seq, max_bytes, wait_ms)
    }
    fn write(&self, chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        self.0.write(chunk)
    }
    fn signal(&self, signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        self.0.signal(signal)
    }
    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        self.0.terminate()
    }
}

/// A readonly discovery can hang while the actual child's old IO and lifetime
/// remain healthy. The native provider must bound that query before returning.
struct PendingCapabilitiesAdapter(LegacyProcessAdapter);
impl ExecProcess for PendingCapabilitiesAdapter {
    fn process_id(&self) -> &ProcessId {
        self.0.process_id()
    }
    fn subscribe_wake(&self) -> tokio::sync::watch::Receiver<u64> {
        self.0.subscribe_wake()
    }
    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        self.0.subscribe_events()
    }
    fn read(
        &self,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
        wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        self.0.read(after_seq, max_bytes, wait_ms)
    }
    fn write(&self, chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        self.0.write(chunk)
    }
    fn signal(&self, signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        self.0.signal(signal)
    }
    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        self.0.terminate()
    }
    fn checked_control_capabilities(
        &self,
    ) -> ExecProcessFuture<'_, codex_exec_server::ProcessControlCapabilities> {
        Box::pin(std::future::pending())
    }
}

/// Complete ordinary owner; no Arc<Mutex<NativeR2State>> or replacement Core.
/// Moving this owner must retain the same Store, host and original connections.
struct SyntheticOwner {
    runtime: HostRuntime,
    host: Arc<SessionExecHost>,
    proposer_connection: Connection,
    executor_connection: Arc<Connection>,
    sentinel: Arc<()>,
}
impl SyntheticOwner {
    fn new(path: &std::path::Path) -> Self {
        let mut runtime =
            HostRuntime::new(Store::open(path, EventBudget::default()).unwrap()).unwrap();
        let proposer_connection = runtime.connect().unwrap();
        let executor_connection = Arc::new(runtime.connect().unwrap());
        let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
        Self {
            runtime,
            host,
            proposer_connection,
            executor_connection,
            sentinel: Arc::new(()),
        }
    }
    fn dispatch(
        &mut self,
        admission: &Admission,
        action: Action,
        executor: bool,
        nonce: &str,
    ) -> Result<Outcome> {
        let request =
            Request::new_for_generation(nonce, self.host.generation(&self.runtime)?, action)?;
        let connection = if executor {
            self.executor_connection.as_ref()
        } else {
            &self.proposer_connection
        };
        let raw = self.host.dispatch(
            &mut self.runtime,
            connection,
            admission,
            request.raw(),
            || 1,
        )?;
        Reply::decode_for(&request, &raw).map(|reply| reply.outcome)
    }
    fn observe(&self, operation: &str) -> ToolObservation {
        self.host
            .inspect_tool_record(&self.runtime, operation)
            .unwrap()
    }
    fn claim(&mut self, intent: &Intent, create_session: bool) -> (Admission, [u8; 32]) {
        let pc = Capabilities {
            session_read: true,
            session_write: true,
            propose: true,
            execute: false,
            retire: false,
        };
        let ec = Capabilities {
            session_read: true,
            execute: true,
            ..Capabilities::default()
        };
        let proposer = self
            .host
            .admit(
                &self.runtime,
                &self.proposer_connection,
                pc,
                pc,
                vec![SESSION.into()],
                intent.execution_domain.clone(),
                EXPIRES,
                1,
            )
            .unwrap();
        let executor = self
            .host
            .admit(
                &self.runtime,
                &self.executor_connection,
                ec,
                ec,
                vec![SESSION.into()],
                intent.execution_domain.clone(),
                EXPIRES,
                1,
            )
            .unwrap();
        if create_session {
            assert!(matches!(
                self.dispatch(
                    &proposer,
                    Action::Create {
                        session_id: SESSION.into(),
                        parent: None,
                        parent_tail: 0
                    },
                    false,
                    "borrowed-create"
                )
                .unwrap(),
                Outcome::Session(_)
            ));
        }
        let operation = &intent.operation_id;
        assert!(matches!(
            self.dispatch(
                &proposer,
                Action::Propose {
                    session_id: SESSION.into(),
                    intent: intent.clone()
                },
                false,
                &format!("propose-{operation}")
            )
            .unwrap(),
            Outcome::Tool(_)
        ));
        let review = self.host.review_tool(&self.runtime, operation, 1).unwrap();
        let permit = self
            .host
            .approve(
                &mut self.runtime,
                &self.executor_connection,
                &executor,
                operation,
                review.proposal_sha256,
                review.intent_sha256,
                1,
            )
            .unwrap();
        let claim = match self
            .dispatch(
                &executor,
                Action::Claim {
                    operation_id: operation.clone(),
                    permit,
                },
                true,
                &format!("claim-{operation}"),
            )
            .unwrap()
        {
            Outcome::Claimed { claim, .. } => claim,
            _ => panic!("original Claim did not return a claim"),
        };
        assert!(!self.observe(operation).invocation_started);
        (executor, claim)
    }
}

fn params(temp: &tempfile::TempDir, operation: &str, mode: &str) -> ExecParams {
    let exe = temp.path().join("borrowed-synthetic-test.exe");
    if !exe.exists() {
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    }
    ExecParams {
        process_id: ProcessId::new(operation),
        metadata: None,
        argv: vec![
            exe.to_str().unwrap().into(),
            "--exact".into(),
            "borrowed_owner_child_entry".into(),
            "--nocapture".into(),
            "--quiet".into(),
        ],
        cwd: PathUri::from_host_native_path(temp.path()).unwrap(),
        env_policy: None,
        shell_snapshot: None,
        env: HashMap::from([
            ("MORROW_BORROWED_CHILD".into(), mode.into()),
            ("MORROW_FIXED".into(), "approved".into()),
        ]),
        tty: false,
        pipe_stdin: matches!(mode, "interactive" | "read-to-end"),
        arg0: None,
        sandbox: None,
        enforce_managed_network: false,
        managed_network: None,
        network_proxy: None,
    }
}

#[derive(Default)]
struct Observed {
    after: u64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exited: bool,
    closed: bool,
    saw_exited: bool,
    saw_closed: bool,
    exit_code: Option<i32>,
    allow_pty: bool,
}
impl Observed {
    fn read(&mut self, provider: &mut impl ProcessProvider) {
        let page = provider
            .events(ReadQuery {
                after_seq: self.after,
                max_bytes: 32768,
                max_events: 16,
                wait_ms: 100,
            })
            .unwrap();
        assert!(!page.gap);
        assert!(page.failure.is_none(), "{:?}", page.failure);
        for event in page.events {
            assert_eq!(
                event.seq,
                self.after + 1,
                "missing or duplicated actual sequence"
            );
            self.after = event.seq;
            match event.kind {
                EventKind::Output { stream, chunk } => match stream {
                    OutputStream::Stdout => self.stdout.extend(chunk),
                    OutputStream::Stderr => self.stderr.extend(chunk),
                    OutputStream::Pty => {
                        assert!(self.allow_pty, "pipe fixture produced PTY output");
                        self.stdout.extend(chunk);
                    }
                },
                EventKind::Exited { exit_code, .. } => {
                    self.saw_exited = true;
                    self.exit_code = Some(exit_code);
                }
                EventKind::Closed => self.saw_closed = true,
            }
        }
        assert_eq!(page.next_seq, self.after);
        self.exited = page.exited;
        self.closed = page.closed;
        if page.exited {
            assert_eq!(page.exit_code, self.exit_code);
        }
    }
    fn collect(&mut self, provider: &mut impl ProcessProvider, until: Option<&str>) {
        let start = Instant::now();
        loop {
            assert!(
                start.elapsed() < Duration::from_secs(12),
                "actual exit and EOF not both observed"
            );
            self.read(provider);
            if until.is_some_and(|text| String::from_utf8_lossy(&self.stdout).contains(text))
                || self.exited && self.closed && self.saw_exited && self.saw_closed
            {
                return;
            }
        }
    }
}

fn scheduler() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

struct Fixture {
    owner: SyntheticOwner,
    port: BorrowedWindowsExecutionPort,
    resources: Arc<BorrowedNativeResources>,
    provisioned: Arc<ProvisionedWindowsBackend>,
    calls: Arc<AtomicUsize>,
    launches: Arc<AtomicUsize>,
    scheduler: Arc<tokio::runtime::Runtime>,
    temp: tempfile::TempDir,
}
impl Fixture {
    fn new(delay: Duration) -> Self {
        Self::with_legacy_controls(delay, false)
    }
    fn with_legacy_controls(delay: Duration, legacy_controls: bool) -> Self {
        Self::with_discovery(delay, legacy_controls, false)
    }
    fn with_discovery(delay: Duration, legacy_controls: bool, pending_capabilities: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let initial = params(&temp, "provisioning", "oneshot");
        let helper = std::path::PathBuf::from(&initial.argv[0]);
        let helper_sha = hash(&std::fs::read(&helper).unwrap());
        let scheduler = Arc::new(scheduler());
        let calls = Arc::new(AtomicUsize::new(0));
        let launches = Arc::new(AtomicUsize::new(0));
        let counted_calls = calls.clone();
        let counted_launches = launches.clone();
        let provisioned = ProvisionedWindowsBackend::ordinary_qualification_decorated(
            ExecServerRuntimeOptions::new(helper, None).unwrap(),
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
            helper_sha,
            scheduler.clone(),
            move |actual| {
                Arc::new(CountedBackend {
                    actual,
                    calls: counted_calls,
                    launches: counted_launches,
                    delay,
                    legacy_controls,
                    pending_capabilities,
                })
            },
        )
        .unwrap();
        assert!(!provisioned.is_production());
        let owner = SyntheticOwner::new(&temp.path().join("borrowed-owner.sqlite"));
        let resources = BorrowedNativeResources::for_owner(&owner.runtime).unwrap();
        let port = BorrowedWindowsExecutionPort::new(
            &owner.runtime,
            owner.host.clone(),
            resources.clone(),
            provisioned.clone(),
        )
        .unwrap();
        assert_eq!(port.owner_binding(), owner.runtime.binding());
        assert_eq!(resources.owner_binding(), owner.runtime.binding());
        assert!(!port.is_production());
        Self {
            owner,
            port,
            resources,
            provisioned,
            calls,
            launches,
            scheduler,
            temp,
        }
    }
    fn review(&self, operation: &str, mode: &str, deadline: u64) -> ReviewedBorrowedInvocation {
        let params = params(&self.temp, operation, mode);
        let sha = hash(&std::fs::read(&params.argv[0]).unwrap());
        self.port
            .review_fixed(operation.into(), params, sha, deadline)
            .unwrap()
    }
    fn claim(
        &mut self,
        reviewed: &ReviewedBorrowedInvocation,
        create: bool,
    ) -> (Admission, [u8; 32]) {
        self.owner.claim(reviewed.intent(), create)
    }
    fn start(
        &mut self,
        admission: &Admission,
        reviewed: ReviewedBorrowedInvocation,
        claim: [u8; 32],
    ) -> Result<WindowsProcessProvider> {
        self.port.start_claimed(
            &mut self.owner.runtime,
            self.owner.executor_connection.clone(),
            admission,
            reviewed,
            claim,
            || 1,
            || true,
        )
    }
    fn wait_for_facts(&self) {
        let until = Instant::now() + Duration::from_secs(8);
        while !self
            .resources
            .pending_facts()
            .unwrap()
            .iter()
            .any(|entry| entry.facts.is_some())
        {
            assert!(
                Instant::now() < until,
                "real completion facts were not retained"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn drain(&mut self) {
        self.resources
            .drain_completed(&mut self.owner.runtime, &self.owner.host)
            .unwrap();
    }
    fn clean(&mut self) {
        let until = Instant::now() + Duration::from_secs(12);
        while !self.resources.is_clean().unwrap() {
            self.drain();
            self.resources.reap_cleanup().unwrap();
            assert!(
                Instant::now() < until,
                "native ownership still charged; owner must not return"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(self.resources.cleanup_status().unwrap().is_empty());
        assert!(self.resources.pending_facts().unwrap().is_empty());
    }
}

#[test]
fn borrowed_same_core_owner_moves_and_real_exit_eof_facts_return() {
    let mut f = Fixture::new(Duration::ZERO);
    let binding = f.owner.runtime.binding();
    let sentinel = f.owner.sentinel.clone();
    let connection = f.owner.executor_connection.clone();
    let reviewed = f.review("move-owner", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    let f = std::thread::spawn(move || {
        assert_eq!(f.owner.runtime.binding(), binding);
        let mut provider = f.start(&admission, reviewed, claim).unwrap();
        let observation = f.owner.observe("move-owner");
        assert!(observation.invocation_started);
        assert_eq!(observation.phase, ToolPhase::DispatchUnknown);
        assert_eq!(provider.observed_identity(), &observation.identity);
        let mut observed = Observed::default();
        observed.collect(&mut provider, None);
        assert!(String::from_utf8_lossy(&observed.stdout).contains("borrowed-actual-stdout"));
        assert!(String::from_utf8_lossy(&observed.stderr).contains("borrowed-actual-stderr"));
        assert_eq!(observed.exit_code, Some(0));
        assert!(observed.exited && observed.closed && observed.saw_closed);
        f.wait_for_facts();
        assert!(
            f.owner.observe("move-owner").latest.is_none(),
            "async monitor must not access Core"
        );
        f.drain();
        let facts = f.owner.observe("move-owner").latest.unwrap();
        assert_eq!(facts.exit_code, Some(0));
        assert!(facts.output_closed);
        assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
        assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
        assert!(
            !f.resources.is_clean().unwrap(),
            "live provider still occupies the original quota"
        );
        drop(provider);
        f.clean();
        f
    })
    .join()
    .expect("ordinary owner thread must actually join");
    assert_eq!(f.owner.runtime.binding(), binding);
    assert!(Arc::ptr_eq(&f.owner.sentinel, &sentinel));
    assert!(Arc::ptr_eq(&f.owner.executor_connection, &connection));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.launches.load(Ordering::SeqCst), 1);
    // This proves the generic ordinary owner move, not WorkbenchState/Manager,
    // ProtectedSession maintenance or the new AgentWorker's production join.
}

#[test]
fn borrowed_real_write_stop_preserves_exit_and_closed_and_cleanup_accounting() {
    let mut f = Fixture::new(Duration::ZERO);
    let reviewed = f.review("write-stop", "interactive", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    assert!(provider.capabilities().write);
    assert!(provider.capabilities().close_input);
    assert!(!provider.capabilities().resize_pty);
    assert_eq!(
        provider.write(b"borrowed-fixed-input\n"),
        EffectOutcome::Accepted
    );
    let mut observed = Observed::default();
    observed.collect(
        &mut provider,
        Some("borrowed-actual-input:borrowed-fixed-input"),
    );
    assert!(!observed.exited && !observed.closed);
    assert!(!f.resources.is_clean().unwrap());
    f.port.stop_handle().request_stop();
    observed.collect(&mut provider, None);
    assert!(observed.saw_exited && observed.saw_closed && observed.exited && observed.closed);
    f.wait_for_facts();
    f.drain();
    assert!(
        !f.resources.is_clean().unwrap(),
        "accepted stop and actual EOF do not release a live provider"
    );
    drop(provider);
    f.clean();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn borrowed_pipe_close_input_drains_bytes_and_child_observes_real_eof() {
    let mut f = Fixture::new(Duration::ZERO);
    let binding = f.owner.runtime.binding();
    let connection = f.owner.executor_connection.clone();
    let reviewed = f.review("pipe-close-eof", "read-to-end", 8000);
    let (admission, claim) = f.claim(&reviewed, true);
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    assert!(provider.capabilities().write && provider.capabilities().close_input);
    assert!(!provider.capabilities().resize_pty);
    let mut observed = Observed::default();
    observed.collect(&mut provider, Some("borrowed-actual-stdout"));
    assert!(!observed.exited && !observed.closed);
    let first = b"fixed-input\0\xff\x01";
    let second = b"second-input\r\nwithout-final-newline";
    let expected = [first.as_slice(), second.as_slice()].concat();
    assert_eq!(provider.write(first), EffectOutcome::Accepted);
    assert_eq!(provider.write(second), EffectOutcome::Accepted);
    assert_eq!(provider.close_input(), EffectOutcome::Accepted);
    assert!(!provider.capabilities().write && !provider.capabilities().close_input);
    assert_eq!(
        provider.write(b"must-not-reach-child"),
        EffectOutcome::Rejected(morrow_agent_process_control_v1::Error::Closed)
    );
    observed.collect(&mut provider, None);
    let expected_hash: String = hash(&expected)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let marker = format!("borrowed-input-eof:{}:{expected_hash}", expected.len());
    assert!(String::from_utf8_lossy(&observed.stdout).contains(&marker));
    assert!(String::from_utf8_lossy(&observed.stderr).contains("borrowed-actual-stderr"));
    assert_eq!(observed.exit_code, Some(0));
    assert!(observed.saw_exited && observed.saw_closed && observed.exited && observed.closed);
    println!("ordinary pipe close actual child receipt: {marker}");
    f.wait_for_facts();
    f.drain();
    let facts = f.owner.observe("pipe-close-eof").latest.unwrap();
    assert_eq!(facts.exit_code, Some(0));
    assert!(facts.output_closed);
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert_eq!(f.owner.runtime.binding(), binding);
    assert!(Arc::ptr_eq(&f.owner.executor_connection, &connection));
    assert!(
        !f.resources.is_clean().unwrap(),
        "live provider remains charged"
    );
    drop(provider);
    f.clean();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.launches.load(Ordering::SeqCst), 1);
}

#[test]
fn borrowed_conpty_resize_twice_is_observed_by_child_os_console_queries() {
    let python = std::env::var_os("MORROW_TEST_PYTHON")
        .expect("set explicit ordinary Python fixture; missing interpreter is not a pass");
    assert!(std::path::Path::new(&python).is_file());
    let mut f = Fixture::new(Duration::ZERO);
    let binding = f.owner.runtime.binding();
    let mut invocation = params(&f.temp, "pty-size-query", "pty-observe");
    invocation.tty = true;
    invocation.pipe_stdin = false;
    invocation.env.insert(
        "MORROW_TEST_PYTHON".into(),
        python
            .into_string()
            .expect("public fixture path must be Unicode"),
    );
    let artifact_sha = hash(&std::fs::read(&invocation.argv[0]).unwrap());
    let reviewed = f
        .port
        .review_fixed("pty-size-query".into(), invocation, artifact_sha, 12000)
        .unwrap();
    let (admission, claim) = f.claim(&reviewed, true);
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    assert!(provider.capabilities().write && provider.capabilities().resize_pty);
    assert!(
        !provider.capabilities().close_input,
        "ConPTY is not pipe stdin"
    );
    let mut observed = Observed {
        allow_pty: true,
        ..Observed::default()
    };
    observed.collect(&mut provider, Some("borrowed-pty-ready"));
    assert!(!observed.exited && !observed.closed);
    for (query, rows, cols) in [("size-1", 25, 91), ("size-2", 33, 117)] {
        assert_eq!(provider.resize(rows, cols), EffectOutcome::Accepted);
        assert_eq!(
            provider.write(format!("{query}\r\n").as_bytes()),
            EffectOutcome::Accepted
        );
        let marker = format!("borrowed-pty-{query}:{rows}:{cols}");
        observed.collect(&mut provider, Some(&marker));
        assert!(String::from_utf8_lossy(&observed.stdout).contains(&marker));
        assert!(!observed.exited && !observed.closed);
        println!("ordinary ConPTY independent child OS receipt: {marker}");
    }
    assert_eq!(provider.write(b"exit\r\n"), EffectOutcome::Accepted);
    observed.collect(&mut provider, None);
    assert_eq!(observed.exit_code, Some(0));
    assert!(observed.saw_exited && observed.saw_closed && observed.exited && observed.closed);
    f.wait_for_facts();
    f.drain();
    let facts = f.owner.observe("pty-size-query").latest.unwrap();
    assert_eq!(facts.exit_code, Some(0));
    assert!(facts.output_closed);
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert_eq!(f.owner.runtime.binding(), binding);
    assert!(!f.resources.is_clean().unwrap());
    drop(provider);
    f.clean();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn borrowed_legacy_process_adapter_does_not_inherit_checked_control_support() {
    let mut f = Fixture::with_legacy_controls(Duration::ZERO, true);
    let reviewed = f.review("legacy-controls", "interactive", 8000);
    let (admission, claim) = f.claim(&reviewed, true);
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    assert!(provider.capabilities().write);
    assert!(!provider.capabilities().close_input && !provider.capabilities().resize_pty);
    assert_eq!(
        provider.close_input(),
        EffectOutcome::Rejected(morrow_agent_process_control_v1::Error::Unsupported)
    );
    assert_eq!(
        provider.resize(25, 91),
        EffectOutcome::Rejected(morrow_agent_process_control_v1::Error::Unsupported)
    );
    // Unsupported controls must not close or replay this genuine backend's IO.
    assert_eq!(
        provider.write(b"legacy-still-open\n"),
        EffectOutcome::Accepted
    );
    let mut observed = Observed::default();
    observed.collect(
        &mut provider,
        Some("borrowed-actual-input:legacy-still-open"),
    );
    assert!(!observed.exited && !observed.closed);
    assert_eq!(provider.terminate(), EffectOutcome::Accepted);
    observed.collect(&mut provider, None);
    assert!(observed.saw_exited && observed.saw_closed && observed.exited && observed.closed);
    f.wait_for_facts();
    f.drain();
    assert!(
        f.owner
            .observe("legacy-controls")
            .latest
            .unwrap()
            .output_closed
    );
    assert!(!f.resources.is_clean().unwrap());
    drop(provider);
    f.clean();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn borrowed_hanging_control_discovery_returns_and_keeps_real_child_cleanup_live() {
    let mut f = Fixture::with_discovery(Duration::ZERO, false, true);
    let binding = f.owner.runtime.binding();
    let connection = f.owner.executor_connection.clone();
    let reviewed = f.review("pending-discovery", "interactive", 10000);
    let (admission, claim) = f.claim(&reviewed, true);
    let started = Instant::now();
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "readonly discovery must not delay start/observer until the process deadline"
    );
    assert!(provider.capabilities().write);
    assert!(!provider.capabilities().close_input && !provider.capabilities().resize_pty);
    assert_eq!(
        provider.write(b"pending-discovery-real-io\n"),
        EffectOutcome::Accepted
    );
    let mut observed = Observed::default();
    observed.collect(
        &mut provider,
        Some("borrowed-actual-input:pending-discovery-real-io"),
    );
    assert!(!observed.exited && !observed.closed);
    assert_eq!(provider.terminate(), EffectOutcome::Accepted);
    observed.collect(&mut provider, None);
    assert!(observed.saw_exited && observed.saw_closed && observed.exited && observed.closed);
    f.wait_for_facts();
    f.drain();
    let facts = f.owner.observe("pending-discovery").latest.unwrap();
    assert!(facts.output_closed);
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert_eq!(f.owner.runtime.binding(), binding);
    assert!(Arc::ptr_eq(&f.owner.executor_connection, &connection));
    assert!(!f.resources.is_clean().unwrap());
    drop(provider);
    f.clean();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.launches.load(Ordering::SeqCst), 1);
}

#[test]
fn borrowed_revoked_claim_and_closed_gate_have_zero_backend_calls() {
    let mut f = Fixture::new(Duration::ZERO);
    let reviewed = f.review("revoked-claim", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    f.owner.host.revoke(&admission).unwrap();
    assert!(matches!(
        f.start(&admission, reviewed, claim),
        Err(Error::Denied)
    ));
    assert!(!f.owner.observe("revoked-claim").invocation_started);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    let reviewed = f.review("closed-gate", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, false);
    assert!(matches!(
        f.port.start_claimed(
            &mut f.owner.runtime,
            f.owner.executor_connection.clone(),
            &admission,
            reviewed,
            claim,
            || 1,
            || false
        ),
        Err(Error::Denied)
    ));
    assert!(!f.owner.observe("closed-gate").invocation_started);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.launches.load(Ordering::SeqCst), 0);
    f.clean();
}

#[test]
fn borrowed_gate_loss_after_actual_start_returns_unknown_without_replay() {
    let mut f = Fixture::new(Duration::ZERO);
    let reviewed = f.review("lost-delivery", "sleep", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    let calls = f.calls.clone();
    let result = f.port.start_claimed(
        &mut f.owner.runtime,
        f.owner.executor_connection.clone(),
        &admission,
        reviewed,
        claim,
        || 1,
        || calls.load(Ordering::SeqCst) == 0,
    );
    assert!(matches!(result, Err(Error::CommitUnknown)));
    assert!(f.owner.observe("lost-delivery").invocation_started);
    assert_eq!(f.launches.load(Ordering::SeqCst), 1);
    let again = f.review("lost-delivery", "sleep", 5000);
    assert!(matches!(
        f.start(&admission, again, claim),
        Err(Error::Denied)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    // Guest delivery is gone; trusted original owner still observes cleanup.
    f.clean();
}

#[test]
fn borrowed_real_facts_survive_cas_conflict_and_executor_revocation() {
    let mut f = Fixture::new(Duration::ZERO);
    let reviewed = f.review("facts-conflict", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    let mut provider = f.start(&admission, reviewed, claim).unwrap();
    let identity = provider.observed_identity().clone();
    let before = f.owner.observe("facts-conflict");
    let mut observed = Observed::default();
    observed.collect(&mut provider, None);
    f.wait_for_facts();
    assert!(matches!(
        f.resources.drain_completed_at(
            &mut f.owner.runtime,
            &f.owner.host,
            &identity,
            before.record_revision - 1
        ),
        Err(Error::Conflict)
    ));
    assert!(f.owner.observe("facts-conflict").latest.is_none());
    assert_eq!(f.resources.pending_facts().unwrap().len(), 1);
    assert!(!f.resources.is_clean().unwrap());
    f.owner.host.revoke(&admission).unwrap();
    assert!(
        f.owner
            .host
            .validate_started_tool(
                &f.owner.runtime,
                &f.owner.executor_connection,
                &admission,
                &identity,
                || 1
            )
            .is_err()
    );
    assert!(
        f.resources
            .drain_completed_at(
                &mut f.owner.runtime,
                &f.owner.host,
                &identity,
                before.record_revision
            )
            .unwrap()
    );
    assert!(f.resources.pending_facts().unwrap().is_empty());
    let facts = f.owner.observe("facts-conflict").latest.unwrap();
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert!(facts.output_closed);
    assert_eq!(
        f.owner.observe("facts-conflict").phase,
        ToolPhase::DispatchUnknown
    );
    drop(provider);
    f.clean();
}

#[test]
fn borrowed_ports_share_sixteen_actual_process_reservations() {
    let mut f = Fixture::new(Duration::ZERO);
    let same_owner_resources = BorrowedNativeResources::for_owner(&f.owner.runtime).unwrap();
    assert!(Arc::ptr_eq(&same_owner_resources, &f.resources));
    let other = BorrowedWindowsExecutionPort::new(
        &f.owner.runtime,
        f.owner.host.clone(),
        same_owner_resources,
        f.provisioned.clone(),
    )
    .unwrap();
    let mut providers = Vec::new();
    for n in 0..MAX_NATIVE_EXECUTIONS {
        let operation = format!("shared-{n}");
        let reviewed = f.review(&operation, "oneshot", 5000);
        let (admission, claim) = f.claim(&reviewed, n == 0);
        let port = if n % 2 == 0 { &f.port } else { &other };
        let mut provider = port
            .start_claimed(
                &mut f.owner.runtime,
                f.owner.executor_connection.clone(),
                &admission,
                reviewed,
                claim,
                || 1,
                || true,
            )
            .unwrap();
        let mut observed = Observed::default();
        observed.collect(&mut provider, None);
        assert!(observed.exited && observed.closed);
        providers.push(provider);
    }
    assert_eq!(
        f.resources.cleanup_status().unwrap().len(),
        MAX_NATIVE_EXECUTIONS
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), MAX_NATIVE_EXECUTIONS);
    let reviewed = f.review("shared-overflow", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, false);
    assert!(matches!(
        other.start_claimed(
            &mut f.owner.runtime,
            f.owner.executor_connection.clone(),
            &admission,
            reviewed,
            claim,
            || 1,
            || true
        ),
        Err(Error::Limit)
    ));
    assert!(!f.owner.observe("shared-overflow").invocation_started);
    assert_eq!(f.calls.load(Ordering::SeqCst), MAX_NATIVE_EXECUTIONS);
    drop(providers);
    f.clean();
}

#[test]
fn borrowed_late_actual_start_is_unknown_charged_and_never_replayed() {
    let mut f = Fixture::new(Duration::from_millis(700));
    let reviewed = f.review("late-start", "oneshot", 100);
    let (admission, claim) = f.claim(&reviewed, true);
    assert!(matches!(
        f.start(&admission, reviewed, claim),
        Err(Error::CommitUnknown)
    ));
    assert!(f.owner.observe("late-start").invocation_started);
    let status = f.resources.cleanup_status().unwrap();
    assert_eq!(status.len(), 1);
    assert!(status[0].unknown);
    assert!(!status[0].exited_and_closed);
    assert!(!f.resources.is_clean().unwrap());
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    let again = f.review("late-start", "oneshot", 100);
    assert!(matches!(
        f.start(&admission, again, claim),
        Err(Error::Denied)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        f.drain();
        f.resources.reap_cleanup().unwrap();
        if f.resources.is_clean().unwrap() {
            let facts = f.owner.observe("late-start").latest.unwrap();
            assert!(facts.output_closed && facts.exit_code.is_some());
            eprintln!("late start: actual exit/EOF and complete prefix facts acknowledged; clean");
            break;
        }
        let status = f.resources.cleanup_status().unwrap();
        let pending = f.resources.pending_facts().unwrap();
        if status.len() == 1
            && status[0].exited_and_closed
            && pending.len() == 1
            && pending[0].unknown
            && pending[0].facts.is_none()
        {
            assert!(status[0].unknown);
            assert!(!status[0].provider_active);
            assert!(!status[0].start_pending);
            assert_eq!(pending[0].identity, f.owner.observe("late-start").identity);
            assert!(f.owner.observe("late-start").latest.is_none());
            assert!(!f.resources.is_clean().unwrap());
            // Actual exit/EOF permits no manufactured output digest. This
            // branch proves retained Unknown safety, not successful reclamation.
            eprintln!(
                "late start: actual terminal observed, incomplete prefix retained Unknown; NOT_RECLAIMED"
            );
            break;
        }
        assert!(
            Instant::now() < until,
            "late actual process never supplied exit and EOF evidence"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        f.launches.load(Ordering::SeqCst),
        1,
        "late future must delegate to actual backend exactly once"
    );
}

#[test]
fn borrowed_foreign_runtime_or_replaced_r2_issuer_cannot_use_owner_resources() {
    let mut f = Fixture::new(Duration::ZERO);
    let other_temp = tempfile::tempdir().unwrap();
    // Independent negative-test owner, never a second Store for f's authority.
    let other = SyntheticOwner::new(&other_temp.path().join("foreign-owner.sqlite"));
    assert!(matches!(
        BorrowedWindowsExecutionPort::new(
            &other.runtime,
            other.host.clone(),
            f.resources.clone(),
            f.provisioned.clone()
        ),
        Err(Error::Denied)
    ));
    // Original Core permits trusted issuer replacement, which invalidates the
    // prior lease. It must not silently rebind these existing native resources.
    let replacement = Arc::new(SessionExecHost::new(&mut f.owner.runtime).unwrap());
    assert!(f.owner.host.generation(&f.owner.runtime).is_err());
    let same = BorrowedNativeResources::for_owner(&f.owner.runtime).unwrap();
    assert!(Arc::ptr_eq(&same, &f.resources));
    assert!(matches!(
        BorrowedWindowsExecutionPort::new(
            &f.owner.runtime,
            replacement,
            same,
            f.provisioned.clone()
        ),
        Err(Error::Denied)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert!(f.resources.is_clean().unwrap());
}

#[test]
fn borrowed_stop_before_start_is_sticky_and_has_no_native_effect() {
    let mut f = Fixture::new(Duration::ZERO);
    let reviewed = f.review("cancel-before-start", "oneshot", 5000);
    let (admission, claim) = f.claim(&reviewed, true);
    let again = f.review("cancel-before-start", "oneshot", 5000);
    f.port.stop_handle().request_stop();
    assert!(matches!(
        f.start(&admission, reviewed, claim),
        Err(Error::Denied)
    ));
    assert!(!f.owner.observe("cancel-before-start").invocation_started);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.launches.load(Ordering::SeqCst), 0);
    assert!(f.resources.is_clean().unwrap());
    assert!(matches!(
        f.start(&admission, again, claim),
        Err(Error::Denied)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn borrowed_wrong_artifact_and_undriven_scheduler_are_rejected() {
    let f = Fixture::new(Duration::ZERO);
    let original = params(&f.temp, "wrong-artifact", "oneshot");
    assert!(matches!(
        f.port
            .review_fixed("wrong-artifact".into(), original.clone(), [77; 32], 5000),
        Err(Error::Denied)
    ));
    let approved = f.review("fixed-artifact", "oneshot", 5000);
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&approved.intent().program)
            .is_err()
    );
    drop(approved);
    let helper = std::path::PathBuf::from(&original.argv[0]);
    let sha = hash(&std::fs::read(&helper).unwrap());
    assert!(matches!(
        ProvisionedWindowsBackend::production(
            ExecServerRuntimeOptions::new(helper.clone(), None).unwrap(),
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
            sha,
            f.scheduler.clone(),
            SandboxType::None
        ),
        Err(Error::Denied)
    ));
    let current_thread = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    assert!(matches!(
        ProvisionedWindowsBackend::ordinary_qualification_decorated(
            ExecServerRuntimeOptions::new(helper.clone(), None).unwrap(),
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
            sha,
            current_thread,
            |actual| actual
        ),
        Err(Error::Invalid)
    ));
    let single_worker = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
    );
    assert!(matches!(
        ProvisionedWindowsBackend::ordinary_qualification_decorated(
            ExecServerRuntimeOptions::new(helper, None).unwrap(),
            HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
            sha,
            single_worker,
            |actual| actual
        ),
        Err(Error::Invalid)
    ));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert!(f.resources.is_clean().unwrap());
}

/// Observer only: retains the exact actual process object returned by the same
/// backend. It changes no process, event receiver, authority or receipt.
struct GenerationRecordingBackend {
    actual: Arc<dyn ExecBackend>,
    originals: Arc<std::sync::Mutex<Vec<Arc<dyn ExecProcess>>>>,
}
impl ExecBackend for GenerationRecordingBackend {
    fn start(&self, params: ExecParams) -> ExecBackendFuture<'_> {
        Box::pin(async move {
            let started = self.actual.start(params).await?;
            self.originals.lock().unwrap().push(started.process.clone());
            Ok(started)
        })
    }
}

fn generation_fixture() -> (Fixture, Arc<std::sync::Mutex<Vec<Arc<dyn ExecProcess>>>>) {
    // Same existing ordinary fixture constructor, with an observation-only
    // decorator: exactly one original Core/Store, Environment and scheduler.
    let temp = tempfile::tempdir().unwrap();
    let initial = params(&temp, "generation-provisioning", "oneshot");
    let helper = std::path::PathBuf::from(&initial.argv[0]);
    let helper_sha = hash(&std::fs::read(&helper).unwrap());
    let scheduler = Arc::new(scheduler());
    let calls = Arc::new(AtomicUsize::new(0));
    let launches = Arc::new(AtomicUsize::new(0));
    let counted_calls = calls.clone();
    let counted_launches = launches.clone();
    let originals = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = originals.clone();
    let provisioned = ProvisionedWindowsBackend::ordinary_qualification_decorated(
        ExecServerRuntimeOptions::new(helper, None).unwrap(),
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        helper_sha,
        scheduler.clone(),
        move |actual| {
            Arc::new(GenerationRecordingBackend {
                actual: Arc::new(CountedBackend {
                    actual,
                    calls: counted_calls,
                    launches: counted_launches,
                    delay: Duration::ZERO,
                    legacy_controls: false,
                    pending_capabilities: false,
                }),
                originals: recorded,
            })
        },
    )
    .unwrap();
    assert!(!provisioned.is_production());
    let owner = SyntheticOwner::new(&temp.path().join("generation-owner.sqlite"));
    let resources = BorrowedNativeResources::for_owner(&owner.runtime).unwrap();
    let port = BorrowedWindowsExecutionPort::new(
        &owner.runtime,
        owner.host.clone(),
        resources.clone(),
        provisioned.clone(),
    )
    .unwrap();
    assert_eq!(port.owner_binding(), owner.runtime.binding());
    assert_eq!(resources.owner_binding(), owner.runtime.binding());
    (
        Fixture {
            owner,
            port,
            resources,
            provisioned,
            calls,
            launches,
            scheduler,
            temp,
        },
        originals,
    )
}

#[test]
fn borrowed_old_provider_drop_after_real_retention_cannot_target_reused_process_id() {
    let (mut f, originals) = generation_fixture();
    let binding = f.owner.runtime.binding();
    let connection = f.owner.executor_connection.clone();
    let sentinel = f.owner.sentinel.clone();
    let generation = f.owner.host.generation(&f.owner.runtime).unwrap();
    let reused_id = ProcessId::new("borrowed-generation-reused-id");

    // Each operation is independently reviewed/approved/claimed on the same
    // original owner. The shared backend process ID is part of each fixed input.
    let mut a_params = params(&f.temp, "generation-original-a", "oneshot");
    a_params.process_id = reused_id.clone();
    let a_sha = hash(&std::fs::read(&a_params.argv[0]).unwrap());
    let a_review = f
        .port
        .review_fixed("generation-original-a".into(), a_params, a_sha, 5000)
        .unwrap();
    let (a_admission, a_claim) = f.claim(&a_review, true);
    let mut a_provider = f.start(&a_admission, a_review, a_claim).unwrap();
    let a_original = originals.lock().unwrap()[0].clone();
    assert_eq!(a_original.process_id(), &reused_id);
    let a_wake = a_original.subscribe_wake();
    let a_identity = a_provider.observed_identity().clone();
    let mut a_observed = Observed::default();
    a_observed.collect(&mut a_provider, None);
    assert!(a_observed.exited && a_observed.closed);
    assert!(a_observed.saw_exited && a_observed.saw_closed);
    assert_eq!(a_observed.exit_code, Some(0));
    let a_terminal = f
        .scheduler
        .block_on(a_original.read(None, None, Some(0)))
        .unwrap();
    assert!(a_terminal.exited && a_terminal.closed);
    assert_eq!(a_terminal.exit_code, Some(0));
    f.wait_for_facts();
    f.drain();
    let a_facts = f.owner.observe("generation-original-a").latest.unwrap();
    assert!(a_facts.output_closed);
    assert_eq!(a_facts.exit_code, Some(0));
    assert_eq!(a_facts.stdout_sha256, hash(&a_observed.stdout));
    assert_eq!(a_facts.stderr_sha256, hash(&a_observed.stderr));
    assert!(!f.resources.is_clean().unwrap()); // A provider is still live.

    // Exec-server is a normal dependency, so its real production retention is
    // 30 seconds, not the 25ms internal cfg(test) fixture shortcut. No map entry
    // is removed or terminal flag injected by this test.
    let retention_wait = Instant::now();
    std::thread::sleep(Duration::from_secs(31));
    assert!(retention_wait.elapsed() >= Duration::from_secs(30));
    let retired_read = f.scheduler.block_on(a_original.read(None, None, Some(0)));
    assert!(
        retired_read.is_err(),
        "A map generation must actually retire"
    );
    eprintln!(
        "generation-A-retired-after-ms={}",
        retention_wait.elapsed().as_millis()
    );

    let mut b_params = params(&f.temp, "generation-replacement-b", "read-to-end");
    b_params.process_id = reused_id.clone();
    let b_sha = hash(&std::fs::read(&b_params.argv[0]).unwrap());
    let b_review = f
        .port
        .review_fixed("generation-replacement-b".into(), b_params, b_sha, 20000)
        .unwrap();
    let (b_admission, b_claim) = f.claim(&b_review, false);
    let mut b_provider = f.start(&b_admission, b_review, b_claim).unwrap();
    assert_eq!(originals.lock().unwrap().len(), 2);
    let b_original = originals.lock().unwrap()[1].clone();
    assert_eq!(b_original.process_id(), &reused_id);
    assert!(!Arc::ptr_eq(&a_original, &b_original));
    assert!(!a_wake.same_channel(&b_original.subscribe_wake()));
    let b_identity = b_provider.observed_identity().clone();
    assert_ne!(a_identity.operation_id, b_identity.operation_id);
    assert_ne!(a_identity.proposal_sha256, b_identity.proposal_sha256);
    assert_eq!(
        a_identity.original_issuer_nonce,
        b_identity.original_issuer_nonce
    );
    assert_eq!(a_identity.generation, b_identity.generation);
    let mut b_observed = Observed::default();
    b_observed.collect(&mut b_provider, Some("borrowed-actual-stdout"));
    assert!(!b_observed.exited && !b_observed.closed);

    // This invokes the original Native Drop, including its original JobGuard,
    // per-slot cleanup marker and five-second read-only cleanup window. It does
    // not manufacture a ProcessHost binding; its finish wrapper is not covered.
    drop(a_provider);
    let cleanup_window = Instant::now();
    while cleanup_window.elapsed() < Duration::from_secs(6) {
        b_observed.read(&mut b_provider);
        assert!(
            !b_observed.exited && !b_observed.closed,
            "A Drop affected real B"
        );
        let live_b = f
            .scheduler
            .block_on(b_original.read(None, Some(1), Some(0)))
            .unwrap();
        assert!(!live_b.exited && !live_b.closed);
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!(
        "generation-B-survived-A-cleanup-ms={}",
        cleanup_window.elapsed().as_millis()
    );
    assert!(
        f.scheduler
            .block_on(a_original.read(None, None, Some(0)))
            .is_err()
    );
    assert!(b_provider.capabilities().close_input);
    let input = b"generation-B-still-live-after-A-drop\0\xff\n";
    assert_eq!(b_provider.write(input), EffectOutcome::Accepted);
    assert_eq!(b_provider.close_input(), EffectOutcome::Accepted);
    b_observed.collect(&mut b_provider, None);
    assert!(b_observed.exited && b_observed.closed);
    assert!(b_observed.saw_exited && b_observed.saw_closed);
    assert_eq!(b_observed.exit_code, Some(0));
    let digest: String = hash(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let eof = format!("borrowed-input-eof:{}:{digest}", input.len());
    assert!(String::from_utf8_lossy(&b_observed.stdout).contains(&eof));
    let facts_deadline = Instant::now() + Duration::from_secs(8);
    while !f
        .resources
        .pending_facts()
        .unwrap()
        .iter()
        .any(|entry| entry.identity == b_identity && entry.facts.is_some())
    {
        assert!(
            Instant::now() < facts_deadline,
            "B actual facts were not retained"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    f.drain();
    let b_facts = f.owner.observe("generation-replacement-b").latest.unwrap();
    assert!(b_facts.output_closed);
    assert_eq!(b_facts.exit_code, Some(0));
    assert_eq!(b_facts.stdout_sha256, hash(&b_observed.stdout));
    assert_eq!(b_facts.stderr_sha256, hash(&b_observed.stderr));
    assert_eq!(
        f.owner.observe("generation-original-a").latest.unwrap(),
        a_facts
    );
    drop(b_provider);
    f.clean();
    assert_eq!(f.owner.runtime.binding(), binding);
    assert_eq!(
        f.owner.host.generation(&f.owner.runtime).unwrap(),
        generation
    );
    assert!(Arc::ptr_eq(&f.owner.executor_connection, &connection));
    assert!(Arc::ptr_eq(&f.owner.sentinel, &sentinel));
    assert_eq!(f.calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.launches.load(Ordering::SeqCst), 2);
    assert!(f.resources.pending_facts().unwrap().is_empty());
    assert!(f.resources.cleanup_status().unwrap().is_empty());
    assert!(f.resources.is_clean().unwrap());
    // Observation-only references retire on this synchronous owner while its
    // existing scheduler anchor is retained. This is not Runtime task-join proof.
    originals.lock().unwrap().clear();
    drop(a_original);
    drop(b_original);
    eprintln!("generation-real-A-B-original-owner-facts-clean");
}
