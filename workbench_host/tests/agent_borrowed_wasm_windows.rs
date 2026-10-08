//! Sealed Rust proposal before the worker, then sealed process controls on a
//! real generic AgentWorker borrowing the complete same ordinary owner.
//! Synthetic TempDir/current-test-exe and sandbox None only; this does not
//! qualify ProtectedSession, DPAPI, production Workbench or Windows isolation.
//! The child helper must be skipped and excluded from meaningful pass counts.
#![cfg(windows)]

use codex_utils_path_uri::PathUri;
use morrow_agent_process_control_v1::{
    self as process, Action as ProcessAction, Capabilities as ProcessCaps, EventKind, OutputPage,
    OutputStream, ReadQuery, ReplyBody,
    host::{Budget, Handle, Host as ProcessHost},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request, ToolPhase,
    authority::{Capabilities as SessionCaps, SessionExecHost},
    hash,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Declaration,
    catalog::{Approval, Catalog, CatalogManagedPackage, Revisions},
};
use morrow_codex_session_exec_windows_v1::{
    BorrowedNativeResources, BorrowedWindowsExecutionPort, ExecParams, ExecServerRuntimeOptions,
    HttpClientFactory, OutboundProxyPolicy, ProcessId, ProvisionedWindowsBackend,
    ReviewedBorrowedInvocation,
};
use morrow_core::{
    dispatch::{Connection, HostBinding, HostRuntime},
    plugin_package::{Package, catalog::Catalog as BaseCatalog, registry::Registry},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits,
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
};
use morrow_workbench_host::{
    agent_tasks::{
        AgentCommand, AgentCommandHandle, AgentContext, AgentError, AgentExit, AgentLimits,
        AgentPhase, AgentReply, AgentSchedulerPhase, AgentWorker,
    },
    product_gate::ProductGate,
};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::ThreadId,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

type Maintenance = Arc<Mutex<Vec<(&'static str, ThreadId)>>>;

const SESSION: &str = "borrowed-coupled-session";
const OP: &str = "borrowed-coupled-operation";
const EXPIRES: u64 = 60_000;
const PROPOSAL_SHA: &str = "9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243";
const PROCESS_SHA: &str = "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36";

#[test]
fn borrowed_coupled_child_entry() {
    if std::env::var("MORROW_BORROWED_COUPLED_CHILD").as_deref() != Ok("interactive") {
        eprintln!("helper skipped: explicit synthetic child invocation absent");
        return;
    }
    assert_eq!(std::env::var("MORROW_FIXED").unwrap(), "approved");
    println!("borrowed-coupled-stdout-approved");
    eprintln!("borrowed-coupled-stderr-approved");
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    loop {
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).unwrap() == 0 {
            break;
        }
        // Ordinary test-local marker proves the child consumed the actual write.
        std::fs::write("borrowed-coupled-write.marker", line.as_bytes()).unwrap();
        println!("borrowed-coupled-input:{}", line.trim_end());
        std::io::stdout().flush().unwrap();
    }
}

#[test]
fn actual_native_process_without_attached_port_rejects_worker_and_recovers_same_owner() {
    let mut prepared = prepare_with_port(false);
    let port = prepared.trusted_port.take().unwrap();
    let Prepared {
        mut owner,
        mut process,
        mut context,
        reviewed,
        retry,
        claim,
        intent,
        scheduler_witness,
        resources,
        host,
        executor,
        guest_connection,
        ..
    } = prepared;
    assert!(context.port().is_none());
    let binding = owner.runtime.binding();
    let sentinel = owner.sentinel.as_ref() as *const u64 as usize;
    let manager_revision = owner.manager.revision();
    let catalog_revision = owner._catalog.revision();
    let process_host_identity = context.processes.identity();

    owner.assert_proposal_live();
    // The caller really starts and owns a genuine upstream process before
    // requesting a worker that has no port. No test hook invents charged state.
    let provider = port
        .start_claimed(
            &mut owner.runtime,
            executor.clone(),
            &context.executor_admission,
            *reviewed,
            claim,
            || 1,
            || true,
        )
        .unwrap();
    let identity = provider.observed_identity().clone();
    let handle = process
        .register_process(
            &owner.manager,
            &mut owner.runtime,
            &host,
            &mut context.processes,
            identity.clone(),
            executor.clone(),
            &context.executor_admission,
            process_caps(),
            Budget::default(),
            Box::new(provider),
            || 1,
        )
        .unwrap();
    let started = host.inspect_tool_record(&owner.runtime, OP).unwrap();
    assert!(started.invocation_started);
    assert_eq!(started.identity, identity);
    assert!(!resources.is_clean().unwrap());
    assert!(matches!(
        port.start_claimed(
            &mut owner.runtime,
            executor.clone(),
            &context.executor_admission,
            *retry,
            claim,
            || 1,
            || true
        ),
        Err(Error::Denied)
    ));
    assert_eq!(
        host.inspect_tool_record(&owner.runtime, OP)
            .unwrap()
            .record_revision,
        started.record_revision
    );

    let mut observed = Observed::default();
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        observed.accept(
            context
                .processes
                .trusted_observe(
                    handle,
                    ReadQuery {
                        after_seq: observed.after,
                        max_bytes: 32768,
                        max_events: 16,
                        wait_ms: 1000,
                    },
                )
                .unwrap(),
        );
        if String::from_utf8_lossy(&observed.stdout).contains("borrowed-coupled-stdout-approved") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "genuine child did not produce output before rejected spawn"
        );
    }
    assert!(!observed.exited);
    assert!(owner.maintenance.lock().unwrap().is_empty());
    let mut failed = match AgentWorker::spawn(
        owner,
        process,
        context,
        ProductGate::default(),
        AgentLimits::default(),
        EXPIRES,
        Arc::new(|| 1),
    ) {
        Err(failed) => *failed,
        Ok(worker) => {
            worker.stop();
            panic!("charged native process without attached port unexpectedly spawned worker");
        }
    };
    assert_eq!(failed.error, AgentError::Unknown);
    assert_eq!(binding, failed.owner.runtime.binding());
    assert_eq!(
        sentinel,
        failed.owner.sentinel.as_ref() as *const u64 as usize
    );
    assert_eq!(manager_revision, failed.owner.manager.revision());
    assert_eq!(catalog_revision, failed.owner._catalog.revision());
    assert!(
        failed.owner.maintenance.lock().unwrap().is_empty(),
        "preflight entered owner/guest IO"
    );
    assert!(Arc::ptr_eq(&host, &failed.cleanup.context.host));
    assert!(Arc::ptr_eq(&resources, &failed.cleanup.context.resources));
    assert!(Arc::ptr_eq(
        &executor,
        &failed.cleanup.context.executor_connection
    ));
    assert!(Arc::ptr_eq(
        &guest_connection,
        &failed.cleanup.package.shared_connection()
    ));
    assert!(process_host_identity.matches(&failed.cleanup.context.processes));
    assert!(failed.cleanup.context.port().is_none());
    assert!(!resources.is_clean().unwrap());
    assert_eq!(
        host.inspect_tool_record(&failed.owner.runtime, OP)
            .unwrap()
            .identity,
        identity
    );

    // Only the original trusted port is asked to stop; Unknown termination is
    // never reissued. Real reads remain available after the package is stopped.
    port.stop_handle().request_stop();
    loop {
        observed.accept(
            failed
                .cleanup
                .context
                .processes
                .trusted_observe(
                    handle,
                    ReadQuery {
                        after_seq: observed.after,
                        max_bytes: 32768,
                        max_events: 16,
                        wait_ms: 1000,
                    },
                )
                .unwrap(),
        );
        if observed.exited && observed.closed && observed.saw_exited && observed.saw_closed {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real trusted exit/EOF cleanup timed out"
        );
    }
    failed.cleanup.context.processes.finish(handle).unwrap();
    loop {
        let report = resources
            .drain_completed(&mut failed.owner.runtime, &host)
            .unwrap();
        assert!(
            report.errors.is_empty(),
            "original facts CAS returned an error"
        );
        resources.reap_cleanup().unwrap();
        if resources.is_clean().unwrap() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual native resource/facts remained charged"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let tool = host.inspect_tool_record(&failed.owner.runtime, OP).unwrap();
    assert!(tool.invocation_started);
    assert_eq!(tool.identity, identity);
    assert_eq!(tool.identity.intent_sha256, intent.digest().unwrap());
    let facts = tool
        .latest
        .expect("original completion facts missing after trusted drain");
    assert!(facts.output_closed);
    assert_eq!(facts.exit_code, observed.exit_code);
    assert_eq!(facts.stdout_bytes, observed.stdout.len() as u64);
    assert_eq!(facts.stderr_bytes, observed.stderr.len() as u64);
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert_eq!(
        String::from_utf8_lossy(&observed.stdout)
            .matches("borrowed-coupled-stdout-approved")
            .count(),
        1
    );
    failed.owner.assert_proposal_live();
    failed.cleanup.repair(&mut failed.owner).unwrap();
    assert!(failed.owner.proposal.is_none());
    assert!(
        failed
            .owner
            .runtime
            .connection_phase(&failed.owner.proposal_connection)
            .is_err()
    );
    assert_eq!(binding, failed.owner.runtime.binding());
    assert_eq!(
        sentinel,
        failed.owner.sentinel.as_ref() as *const u64 as usize
    );
    assert!(failed.owner.runtime.connection_phase(&executor).is_err());
    assert!(
        failed
            .owner
            .runtime
            .connection_phase(&guest_connection)
            .is_err()
    );
    let maintenance = failed.owner.maintenance.lock().unwrap();
    assert_eq!(maintenance.len(), 1);
    assert_eq!(maintenance[0], ("finish", std::thread::current().id()));
    drop(maintenance);
    drop(port);
    // The rejected worker never owned a scheduler token. The trusted caller
    // must still synchronously shut down its original actual scheduler.
    let mut witness = scheduler_witness;
    let scheduler = loop {
        match Arc::try_unwrap(witness) {
            Ok(scheduler) => break scheduler,
            Err(still_owned) => {
                witness = still_owned;
                assert!(
                    Instant::now() < deadline,
                    "original scheduler references remain after real cleanup"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    };
    assert!(tokio::runtime::Handle::try_current().is_err());
    drop(scheduler);
    // Keep the original Core and TempDir alive until all original scheduler
    // work is joined and artifact/process handles have been released.
    drop(failed);
    println!(
        "port-none-evidence: actual_child=true preflight_unknown=true original_owner_returned=true worker_imports=0 original_exit_EOF_CAS_finish=true caller_scheduler_shutdown=true"
    );
}

fn pinned_module(variable: &str, expected: &str) -> Vec<u8> {
    let path =
        PathBuf::from(std::env::var_os(variable).expect("explicit sealed guest path required"));
    assert!(path.is_absolute());
    let bytes = std::fs::read(path).unwrap();
    let actual: String = hash(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(actual, expected, "sealed guest changed: {variable}");
    bytes
}

fn process_caps() -> ProcessCaps {
    ProcessCaps {
        read: true,
        events: true,
        write: true,
        terminate: true,
        ..Default::default()
    }
}

fn limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        memory_bytes: 16 * 1024 * 1024,
        host_calls: 4,
    }
}

fn package(
    id: &str,
    bytes: &[u8],
    domain: &str,
    session: SessionCaps,
    process: ProcessCaps,
) -> (Package, AgentProcessPackage) {
    let base = Package::build(
        Package::manifest_for_task(id, "1.0.0", bytes, vec![]),
        bytes,
    )
    .unwrap();
    let full = AgentProcessPackage::build(
        Package::decode(base.archive()).unwrap(),
        Declaration {
            session,
            process,
            sessions: vec![SESSION.into()],
            execution_domain: domain.into(),
        },
    )
    .unwrap();
    (base, full)
}

/// This owner has one original Core and no ProtectedSession/key lease.
/// The Catalog remains alive in the whole owner moved into the actual worker.
struct OrdinaryOwner {
    runtime: HostRuntime,
    manager: Manager,
    _catalog: Catalog,
    _temp: tempfile::TempDir,
    // Original proposal grant stays live through start, controls and facts drain.
    proposal: Option<CatalogManagedPackage>,
    proposal_connection: Arc<Connection>,
    proposal_host: Arc<SessionExecHost>,
    proposal_processes: ProcessHost,
    sentinel: Box<u64>,
    maintenance: Maintenance,
}
impl OrdinaryOwner {
    fn assert_proposal_live(&self) {
        let proposal = self
            .proposal
            .as_ref()
            .expect("original proposal already closed");
        assert!(Arc::ptr_eq(
            &self.proposal_connection,
            &proposal.shared_connection()
        ));
        assert_eq!(
            self.runtime.connection_phase(&self.proposal_connection),
            Ok(morrow_core::lifecycle::InstancePhase::Ready)
        );
    }
}
impl HostOwner for OrdinaryOwner {
    fn runtime(&self) -> &HostRuntime {
        &self.runtime
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.runtime
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.assert_proposal_live();
        self.maintenance
            .lock()
            .unwrap()
            .push(("prepare", std::thread::current().id()));
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        // An explicit cleanup retry may follow successful final maintenance
        // while external scheduler references still prevent owner return.
        if self.proposal.is_none() {
            assert!(
                self.runtime
                    .connection_phase(&self.proposal_connection)
                    .is_err()
            );
            return Ok(());
        }
        // Native Exit/EOF/facts cleanup is finished before this original owner
        // maintenance hook. Close the exact original proposal on its same Host.
        let tool = self
            .proposal_host
            .inspect_tool_record(&self.runtime, OP)
            .map_err(|_| JobError::Unavailable)?;
        assert!(tool.invocation_started);
        let facts = tool
            .latest
            .expect("owner facts must precede proposal close");
        assert!(facts.output_closed && facts.exit_code.is_some());
        self.assert_proposal_live();
        self.proposal
            .as_ref()
            .unwrap()
            .close(
                &mut self.runtime,
                &self.proposal_host,
                &mut self.proposal_processes,
            )
            .map_err(|_| JobError::Unavailable)?;
        self.proposal.take();
        assert!(
            self.runtime
                .connection_phase(&self.proposal_connection)
                .is_err()
        );
        self.maintenance
            .lock()
            .unwrap()
            .push(("finish", std::thread::current().id()));
        Ok(())
    }
}
impl ManagedHostOwner for OrdinaryOwner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
    fn with_managed_runtime<T>(
        &mut self,
        action: impl FnOnce(&Manager, &mut HostRuntime) -> T,
    ) -> Option<T> {
        Some(action(&self.manager, &mut self.runtime))
    }
}

struct Prepared {
    owner: OrdinaryOwner,
    process: CatalogManagedPackage,
    context: AgentContext,
    reviewed: Box<ReviewedBorrowedInvocation>,
    retry: Box<ReviewedBorrowedInvocation>,
    claim: [u8; 32],
    intent: Intent,
    scheduler_witness: Arc<tokio::runtime::Runtime>,
    resources: Arc<BorrowedNativeResources>,
    host: Arc<SessionExecHost>,
    executor: Arc<Connection>,
    guest_connection: Arc<Connection>,
    write_marker: PathBuf,
    trusted_port: Option<BorrowedWindowsExecutionPort>,
}

fn prepare() -> Prepared {
    prepare_with_port(true)
}

fn prepare_with_port(attached: bool) -> Prepared {
    // Fail on artifact pin drift before opening even the synthetic store.
    let proposal_bytes = pinned_module("MORROW_PROPOSAL_GUEST", PROPOSAL_SHA);
    let process_bytes = pinned_module("MORROW_PROCESS_GUEST", PROCESS_SHA);
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join("borrowed-coupled-test.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    let artifact_sha = hash(&std::fs::read(&exe).unwrap());
    let scheduler = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );
    let backend = ProvisionedWindowsBackend::ordinary_qualification(
        ExecServerRuntimeOptions::new(exe.clone(), None).unwrap(),
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        artifact_sha,
        scheduler.clone(),
    )
    .unwrap();
    assert!(!backend.is_production());
    let mut runtime = HostRuntime::new(
        Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
    let resources = BorrowedNativeResources::for_owner(&runtime).unwrap();
    assert!(Arc::ptr_eq(
        &resources,
        &BorrowedNativeResources::for_owner(&runtime).unwrap()
    ));
    let port = BorrowedWindowsExecutionPort::new(
        &runtime,
        host.clone(),
        resources.clone(),
        backend.clone(),
    )
    .unwrap();
    let params = ExecParams {
        process_id: ProcessId::new(OP),
        metadata: None,
        argv: vec![
            exe.to_str().unwrap().into(),
            "--exact".into(),
            "borrowed_coupled_child_entry".into(),
            "--nocapture".into(),
            "--quiet".into(),
        ],
        cwd: PathUri::from_host_native_path(temp.path()).unwrap(),
        env_policy: None,
        shell_snapshot: None,
        env: HashMap::from([
            ("MORROW_BORROWED_COUPLED_CHILD".into(), "interactive".into()),
            ("MORROW_FIXED".into(), "approved".into()),
        ]),
        tty: false,
        pipe_stdin: true,
        arg0: None,
        sandbox: None,
        enforce_managed_network: false,
        managed_network: None,
        network_proxy: None,
    };
    let reviewed = port
        .review_fixed(OP.into(), params.clone(), artifact_sha, 20_000)
        .unwrap();
    let retry = port
        .review_fixed(OP.into(), params, artifact_sha, 20_000)
        .unwrap();
    let intent = reviewed.intent().clone();
    let domain = intent.execution_domain.clone();
    let proposer_connection = runtime.connect().unwrap();
    let executor = Arc::new(runtime.connect().unwrap());
    let pc = SessionCaps {
        session_read: true,
        session_write: true,
        ..Default::default()
    };
    let ec = SessionCaps {
        session_read: true,
        execute: true,
        ..Default::default()
    };
    let proposer = host
        .admit(
            &runtime,
            &proposer_connection,
            pc,
            pc,
            vec![SESSION.into()],
            domain.clone(),
            EXPIRES,
            1,
        )
        .unwrap();
    let executor_admission = host
        .admit(
            &runtime,
            &executor,
            ec,
            ec,
            vec![SESSION.into()],
            domain.clone(),
            EXPIRES,
            1,
        )
        .unwrap();
    let create = Request::new_for_generation(
        "borrowed-coupled-create",
        host.generation(&runtime).unwrap(),
        Action::Create {
            session_id: SESSION.into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(
            &mut runtime,
            &proposer_connection,
            &proposer,
            create.raw(),
            || 1,
        )
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&create, &bytes).unwrap().outcome,
        Outcome::Session(_)
    ));

    let proposal_caps = SessionCaps {
        session_read: true,
        propose: true,
        ..Default::default()
    };
    let control_caps = SessionCaps {
        session_read: true,
        ..Default::default()
    };
    let (proposal_base, proposal_full) = package(
        "borrowed-sealed-proposal",
        &proposal_bytes,
        &domain,
        proposal_caps,
        ProcessCaps::default(),
    );
    let (process_base, process_full) = package(
        "borrowed-sealed-process",
        &process_bytes,
        &domain,
        control_caps,
        process_caps(),
    );
    let registry = Registry::open(
        &temp.path().join("registry"),
        BaseCatalog::open(&temp.path().join("base-packages")).unwrap(),
    )
    .unwrap();
    let mut manager = Manager::new(registry, limits());
    for base in [&proposal_base, &process_base] {
        manager.install_package(base.archive()).unwrap();
        manager.select(base, manager.revision()).unwrap();
        manager
            .set_enabled(
                &base.manifest().package_id,
                base.digest(),
                true,
                manager.revision(),
            )
            .unwrap();
    }
    let wrapper_root = temp.path().join("wrapper-catalog");
    let mut catalog = Catalog::open(&wrapper_root, true).unwrap();
    for (base, full, session, caps) in [
        (
            &proposal_base,
            &proposal_full,
            proposal_caps,
            ProcessCaps::default(),
        ),
        (&process_base, &process_full, control_caps, process_caps()),
    ] {
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        let review = catalog
            .install(
                full.archive(),
                full.review_sha256(),
                &mut manager,
                revisions,
            )
            .unwrap();
        assert_eq!(review.base_sha256, base.digest());
        assert_eq!(review.wrapper_sha256, full.review_sha256());
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        catalog
            .select(full.review_sha256(), &mut manager, revisions)
            .unwrap();
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        catalog
            .approve(
                &base.manifest().package_id,
                full.review_sha256(),
                Approval {
                    session,
                    process: caps,
                    sessions: vec![SESSION.into()],
                    execution_domain: domain.clone(),
                },
                &mut manager,
                revisions,
            )
            .unwrap();
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        catalog
            .set_enabled(
                &base.manifest().package_id,
                full.review_sha256(),
                true,
                &mut manager,
                revisions,
            )
            .unwrap();
    }
    let saved_revision = catalog.revision();
    drop(catalog);
    let mut catalog = Catalog::open(&wrapper_root, false).unwrap();
    assert_eq!(catalog.revision(), saved_revision);
    let page = catalog.page(None, 16, saved_revision).unwrap();
    assert_eq!(page.entries.len(), 2);
    assert!(
        page.entries
            .iter()
            .all(|e| e.selected && e.enabled && e.approval.is_some())
    );
    let revisions = Revisions {
        catalog: catalog.revision(),
        manager: manager.revision(),
    };
    let proposal = catalog
        .connect(
            &proposal_base.manifest().package_id,
            proposal_full.review_sha256(),
            &mut manager,
            &mut runtime,
            &host,
            revisions,
            EXPIRES,
            1,
        )
        .unwrap();
    let revisions = Revisions {
        catalog: catalog.revision(),
        manager: manager.revision(),
    };
    let process = catalog
        .connect(
            &process_base.manifest().package_id,
            process_full.review_sha256(),
            &mut manager,
            &mut runtime,
            &host,
            revisions,
            EXPIRES,
            1,
        )
        .unwrap();
    let guest_connection = process.shared_connection();
    assert!(Arc::ptr_eq(&guest_connection, &process.shared_connection()));
    assert_eq!(
        guest_connection.package_digest(),
        Some(process_base.digest())
    );

    // The original sealed proposal guest runs before the OS worker. It does
    // not choose a backend, create an executor grant, or start the executable.
    let input = Request::new_for_generation(
        "borrowed-fixed-proposal",
        proposal.generation(),
        Action::Propose {
            session_id: SESSION.into(),
            intent: intent.clone(),
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
        "borrowed-sealed-proposal-task",
        Transform {
            handler: "codex.session.propose".into(),
            input_type: "codex.session.proposal.v1".into(),
            output_type: "codex.session.proposal.receipt.v1".into(),
            input: input.raw().to_vec(),
        },
    )
    .unwrap();
    // Each guest instance keeps the exact ProcessHost it first used. The
    // proposal has no child handles; its Host moves with the original owner
    // for final explicit close, while the control Host moves in AgentContext.
    let mut proposal_processes = ProcessHost::default();
    let proposal_connection = proposal.shared_connection();
    let run = proposal
        .run(
            &manager,
            &mut runtime,
            &host,
            &mut proposal_processes,
            task.bytes(),
            || 1,
        )
        .unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    let output = task
        .verify_output(run.completion.as_ref().unwrap())
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&actual, &output.bytes).unwrap().outcome,
        Outcome::Tool(info) if info.phase == ToolPhase::Proposed)
    );
    let tool_review = host.review_tool(&runtime, OP, 1).unwrap();
    assert_eq!(tool_review.proposal_sha256, actual.digest());
    assert_eq!(tool_review.intent_sha256, intent.digest().unwrap());
    let permit = host
        .approve(
            &mut runtime,
            &executor,
            &executor_admission,
            OP,
            actual.digest(),
            tool_review.intent_sha256,
            1,
        )
        .unwrap();
    let claim_request = Request::new_for_generation(
        "borrowed-trusted-claim",
        host.generation(&runtime).unwrap(),
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    let raw = host
        .dispatch(
            &mut runtime,
            &executor,
            &executor_admission,
            claim_request.raw(),
            || 1,
        )
        .unwrap();
    let claim = match Reply::decode_for(&claim_request, &raw).unwrap().outcome {
        Outcome::Claimed { claim, .. } => claim,
        other => panic!("original Claim did not return claim: {other:?}"),
    };
    assert!(
        !host
            .inspect_tool_record(&runtime, OP)
            .unwrap()
            .invocation_started
    );
    assert!(resources.cleanup_status().unwrap().is_empty());
    // Claim does not release the original proposing admission: frozen R2
    // live_bound_inputs checks this nonce again at Execute and every control.
    assert!(Arc::ptr_eq(
        &proposal_connection,
        &proposal.shared_connection()
    ));
    assert_eq!(
        runtime.connection_phase(&proposal_connection),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    runtime.disconnect(&proposer_connection).unwrap();
    let processes = ProcessHost::default();

    let write_marker = temp.path().join("borrowed-coupled-write.marker");
    let (context_port, trusted_port) = if attached {
        (Some(port), None)
    } else {
        (None, Some(port))
    };
    let context = AgentContext::new(
        host.clone(),
        processes,
        resources.clone(),
        context_port,
        executor.clone(),
        executor_admission,
    )
    .unwrap_or_else(|_| panic!("ordinary actual agent context rejected"));
    drop(backend); // No caller backend Arc may hide scheduler retirement.
    Prepared {
        owner: OrdinaryOwner {
            runtime,
            manager,
            _catalog: catalog,
            _temp: temp,
            proposal: Some(proposal),
            proposal_connection,
            proposal_host: host.clone(),
            proposal_processes,
            sentinel: Box::new(0xc16),
            maintenance: Arc::new(Mutex::new(Vec::new())),
        },
        process,
        context,
        reviewed: Box::new(reviewed),
        retry: Box::new(retry),
        claim,
        intent,
        scheduler_witness: scheduler,
        resources,
        host,
        executor,
        guest_connection,
        write_marker,
        trusted_port,
    }
}

fn reply(handle: &mut AgentCommandHandle) -> AgentReply {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(reply) = handle.try_read().unwrap() {
            return reply;
        }
        assert!(Instant::now() < deadline, "actual worker reply timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn guest(
    worker: &AgentWorker<OrdinaryOwner>,
    handle: Handle,
    sequence: &mut u64,
    action: ProcessAction,
) -> ReplyBody {
    *sequence += 1;
    let id = format!("borrowed-worker-request-{sequence}");
    let input =
        process::Request::new(&id, handle.nonce, handle.generation, action.clone()).unwrap();
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
        &format!("borrowed-worker-task-{sequence}"),
        Transform {
            handler: "codex.process.control".into(),
            input_type: "codex.process.request.v1".into(),
            output_type: "codex.process.reply.v1".into(),
            input: input.encode().unwrap(),
        },
    )
    .unwrap();
    let mut command = worker
        .submit(AgentCommand::RunFrame(Zeroizing::new(
            task.bytes().to_vec(),
        )))
        .unwrap();
    let reply = reply(&mut command);
    let run = match &reply {
        AgentReply::Frame(run) => run,
        other => panic!("expected sealed process guest result, got {other:?}"),
    };
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, suffix);
    let output = task
        .verify_output(run.completion.as_ref().unwrap())
        .unwrap();
    process::Reply::decode_for(&actual, &output.bytes)
        .unwrap()
        .body
}

#[derive(Default)]
struct Observed {
    after: u64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
    exited: bool,
    closed: bool,
    saw_exited: bool,
    saw_closed: bool,
}
impl Observed {
    fn accept(&mut self, page: OutputPage) {
        assert!(!page.gap);
        assert!(page.failure.is_none(), "{:?}", page.failure);
        for event in page.events {
            assert_eq!(event.seq, self.after + 1, "real sequence gap/duplicate");
            self.after = event.seq;
            match event.kind {
                EventKind::Output {
                    stream: OutputStream::Stdout,
                    chunk,
                } => self.stdout.extend(chunk),
                EventKind::Output {
                    stream: OutputStream::Stderr,
                    chunk,
                } => self.stderr.extend(chunk),
                EventKind::Output {
                    stream: OutputStream::Pty,
                    ..
                } => panic!("unexpected PTY"),
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
    fn collect(
        &mut self,
        worker: &AgentWorker<OrdinaryOwner>,
        handle: Handle,
        sequence: &mut u64,
        until: Option<&str>,
    ) {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let body = guest(
                worker,
                handle,
                sequence,
                ProcessAction::Events(ReadQuery {
                    after_seq: self.after,
                    max_bytes: 32768,
                    max_events: 16,
                    wait_ms: 1000,
                }),
            );
            match body {
                ReplyBody::Page(page) => self.accept(page),
                other => panic!("{other:?}"),
            }
            if until.is_some_and(|s| String::from_utf8_lossy(&self.stdout).contains(s))
                || self.exited && self.closed && self.saw_exited && self.saw_closed
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "actual output/exit/EOF timed out"
            );
        }
    }
}

struct Running {
    worker: AgentWorker<OrdinaryOwner>,
    handle: Handle,
    retry: Option<Box<ReviewedBorrowedInvocation>>,
    claim: [u8; 32],
    intent: Intent,
    witness: Option<Arc<tokio::runtime::Runtime>>,
    resources: Arc<BorrowedNativeResources>,
    host: Arc<SessionExecHost>,
    executor: Arc<Connection>,
    guest_connection: Arc<Connection>,
    binding: HostBinding,
    sentinel: usize,
    maintenance: Maintenance,
    sequence: u64,
    write_marker: PathBuf,
}
fn spawn(prepared: Prepared) -> Running {
    let Prepared {
        owner,
        process,
        context,
        reviewed,
        retry,
        claim,
        intent,
        scheduler_witness,
        resources,
        host,
        executor,
        guest_connection,
        write_marker,
        trusted_port,
    } = prepared;
    assert!(trusted_port.is_none());
    owner.assert_proposal_live();
    let sentinel = owner.sentinel.as_ref() as *const u64 as usize;
    let binding = owner.runtime.binding();
    let maintenance = owner.maintenance.clone();
    let worker = AgentWorker::spawn(
        owner,
        process,
        context,
        ProductGate::default(),
        AgentLimits::default(),
        EXPIRES,
        Arc::new(|| 1),
    )
    .unwrap_or_else(|_| panic!("ordinary actual worker spawn failed"));
    let mut start = worker
        .submit(AgentCommand::StartClaimed {
            reviewed,
            claim,
            capabilities: process_caps(),
            budget: Budget::default(),
        })
        .unwrap();
    let handle = match reply(&mut start) {
        AgentReply::Started(handle) => handle,
        other => panic!("real StartClaimed failed: {other:?}"),
    };
    assert!(start.has_started());
    Running {
        worker,
        handle,
        retry: Some(retry),
        claim,
        intent,
        witness: Some(scheduler_witness),
        resources,
        host,
        executor,
        guest_connection,
        binding,
        sentinel,
        maintenance,
        sequence: 0,
        write_marker,
    }
}

fn stop_without_replay(r: &mut Running) {
    r.worker.stop();
    let commands = r.worker.progress().accepted_commands;
    // The same already-consumed native claim is explicitly attempted once
    // after stop. The worker rejects it without enqueueing or backend restart.
    assert!(matches!(
        r.worker.submit(AgentCommand::StartClaimed {
            reviewed: r.retry.take().unwrap(),
            claim: r.claim,
            capabilities: process_caps(),
            budget: Budget::default(),
        }),
        Err(AgentError::Unknown)
    ));
    assert_eq!(r.worker.progress().accepted_commands, commands);
}

fn assert_returned(r: &Running, exit: &AgentExit<OrdinaryOwner>) {
    assert!(exit.owner.proposal.is_none());
    assert!(
        exit.owner
            .runtime
            .connection_phase(&exit.owner.proposal_connection)
            .is_err()
    );
    assert_eq!(r.binding, exit.owner.runtime.binding());
    assert!(exit.owner.runtime.connection_phase(&r.executor).is_err());
    assert!(
        exit.owner
            .runtime
            .connection_phase(&r.guest_connection)
            .is_err()
    );
    assert!(r.resources.is_clean().unwrap());
    assert!(r.resources.pending_facts().unwrap().is_empty());
    let tool = r.host.inspect_tool_record(&exit.owner.runtime, OP).unwrap();
    assert!(tool.invocation_started);
    assert_eq!(tool.identity.intent_sha256, r.intent.digest().unwrap());
    assert_eq!(tool.identity.session_id, SESSION);
    let facts = tool
        .latest
        .as_ref()
        .expect("real completion facts were not owner-CAS committed");
    assert!(facts.output_closed);
    assert!(facts.exit_code.is_some());
    assert!(facts.stdout_bytes > 0 && facts.stderr_bytes > 0);
    println!(
        "borrowed-worker-evidence: same_binding=true original_started=true stdout={} stderr={} exit={:?} EOF={} worker_join=true scheduler_shutdown=true qualification=ordinary-unsandboxed",
        facts.stdout_bytes, facts.stderr_bytes, facts.exit_code, facts.output_closed
    );
}

fn finish(
    worker: &mut AgentWorker<OrdinaryOwner>,
    witness: Arc<tokio::runtime::Runtime>,
    resources: &BorrowedNativeResources,
    sentinel: usize,
    maintenance: &Maintenance,
) -> AgentExit<OrdinaryOwner> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        assert!(
            worker.try_reclaim().unwrap().is_none(),
            "extra scheduler Arc returned owner"
        );
        if worker.progress().scheduler == AgentSchedulerPhase::WaitingForReferences {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "OS worker/cleanup failed to reach scheduler barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(resources.is_clean().unwrap());
    drop(witness);
    let exit = loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            break exit;
        }
        assert!(
            Instant::now() < deadline,
            "original scheduler did not actually retire"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(worker.progress().phase, AgentPhase::Joined);
    assert_eq!(worker.progress().scheduler, AgentSchedulerPhase::Shutdown);
    assert!(exit.execution.is_ok());
    assert!(exit.disconnect.is_ok());
    assert!(exit.maintenance.is_ok());
    assert!(exit.cleanup.is_none());
    assert_eq!(
        sentinel,
        exit.owner.sentinel.as_ref() as *const u64 as usize
    );
    let log = maintenance.lock().unwrap();
    assert!(log.iter().any(|(kind, _)| *kind == "prepare"));
    assert_eq!(log.iter().filter(|(kind, _)| *kind == "finish").count(), 1);
    assert_eq!(log.last().unwrap().0, "finish");
    let worker_thread = log[0].1;
    assert_ne!(worker_thread, std::thread::current().id());
    assert!(log.iter().all(|(_, thread)| *thread == worker_thread));
    exit
}

#[test]
fn sealed_rust_wasm_borrowed_actual_worker_process_and_original_owner_return() {
    let mut r = spawn(prepare());
    assert!(
        matches!(guest(&r.worker, r.handle, &mut r.sequence, ProcessAction::Discover),
        ReplyBody::Capabilities(c) if c.read && c.events && c.write && c.terminate
            && !c.close_input && !c.resize_pty && !c.interrupt)
    );
    let mut observed = Observed::default();
    observed.collect(
        &r.worker,
        r.handle,
        &mut r.sequence,
        Some("borrowed-coupled-stdout-approved"),
    );
    assert_eq!(
        guest(
            &r.worker,
            r.handle,
            &mut r.sequence,
            ProcessAction::Write(b"approved-input\n".to_vec())
        ),
        ReplyBody::Accepted
    );
    observed.collect(
        &r.worker,
        r.handle,
        &mut r.sequence,
        Some("borrowed-coupled-input:approved-input"),
    );
    assert!(!observed.exited);
    assert_eq!(
        guest(
            &r.worker,
            r.handle,
            &mut r.sequence,
            ProcessAction::Terminate
        ),
        ReplyBody::Accepted
    );
    observed.collect(&r.worker, r.handle, &mut r.sequence, None);
    assert!(observed.exited && observed.closed && observed.saw_exited && observed.saw_closed);
    assert!(String::from_utf8_lossy(&observed.stderr).contains("borrowed-coupled-stderr-approved"));
    stop_without_replay(&mut r);
    let exit = finish(
        &mut r.worker,
        r.witness.take().unwrap(),
        &r.resources,
        r.sentinel,
        &r.maintenance,
    );
    assert_returned(&r, &exit);
    let tool = r.host.inspect_tool_record(&exit.owner.runtime, OP).unwrap();
    let facts = tool.latest.unwrap();
    assert_eq!(facts.exit_code, observed.exit_code);
    assert_eq!(facts.stdout_bytes, observed.stdout.len() as u64);
    assert_eq!(facts.stderr_bytes, observed.stderr.len() as u64);
    assert_eq!(facts.stdout_sha256, hash(&observed.stdout));
    assert_eq!(facts.stderr_sha256, hash(&observed.stderr));
    assert_eq!(
        String::from_utf8_lossy(&observed.stdout)
            .matches("borrowed-coupled-stdout-approved")
            .count(),
        1
    );
}

#[test]
fn sealed_process_real_write_lost_delivery_keeps_unknown_and_trusted_cleanup() {
    let mut r = spawn(prepare());
    let mut observed = Observed::default();
    observed.collect(
        &r.worker,
        r.handle,
        &mut r.sequence,
        Some("borrowed-coupled-stdout-approved"),
    );
    // The child marker is ordinary test evidence of its actual stdin write.
    // Cancel only after the sealed guest effect reached the genuine child,
    // before reading its queued receipt. No observer/events are manufactured.
    let input = process::Request::new(
        "lost-delivery",
        r.handle.nonce,
        r.handle.generation,
        ProcessAction::Write(b"unknown-input\n".to_vec()),
    )
    .unwrap();
    let task = Invocation::new_transform(
        "borrowed-worker-lost-task",
        Transform {
            handler: "codex.process.control".into(),
            input_type: "codex.process.request.v1".into(),
            output_type: "codex.process.reply.v1".into(),
            input: input.encode().unwrap(),
        },
    )
    .unwrap();
    let mut command = r
        .worker
        .submit(AgentCommand::RunFrame(Zeroizing::new(
            task.bytes().to_vec(),
        )))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !command.has_started() {
        assert!(
            Instant::now() < deadline,
            "real worker did not start write command"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    loop {
        if std::fs::read(&r.write_marker).ok().as_deref() == Some(b"unknown-input\n".as_slice()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real child did not consume sealed guest write"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    command.cancel();
    loop {
        match command.try_read() {
            Err(AgentError::Unknown) => break,
            Ok(None) => {
                assert!(
                    Instant::now() < deadline,
                    "lost delivery did not remain Unknown"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            other => panic!("started/cancelled receipt unexpectedly delivered: {other:?}"),
        }
    }
    stop_without_replay(&mut r);
    let exit = finish(
        &mut r.worker,
        r.witness.take().unwrap(),
        &r.resources,
        r.sentinel,
        &r.maintenance,
    );
    // Command delivery is Unknown after a real write. Successful trusted
    // teardown still requires owner-CAS facts, real EOF/exit and both joins.
    assert_returned(&r, &exit);
}
