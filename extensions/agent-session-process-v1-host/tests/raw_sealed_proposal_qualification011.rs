//! Original sealed Rust 9552 Wasm on ordinary Core SQLite; no native process or protected storage.
use morrow_agent_process_control_v1::{self as process, Capabilities as PC};
use morrow_agent_session_exec_v1_r2::{
    Action, Error, Intent, Outcome, Reply, Request, ToolPhase,
    authority::{Capabilities as SC, SessionExecHost},
    hash,
};
use morrow_agent_session_process_v1_host::{
    AgentProcessPackage, Declaration, PreparedSessionExecPackage,
    catalog::{Approval, Catalog, CatalogManagedPackage, Revisions},
};
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::InstancePhase,
    plugin_package::{Package, catalog::Catalog as BaseCatalog, registry::Registry},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Limits, TaskRun,
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
};
use std::{path::PathBuf, sync::Arc};

const ID: &str = "org.example.original-9552-proposal";
const SESSION: &str = "sealed-session";
const DOMAIN: &str = "ordinary-sealed-proposal-domain";
const EXPIRES: u64 = 60_000;
const RAW_SHA: &str = "9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243";
const RAW_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../companions/morrow-codex/qualification/c28-basic-fixtures/morrow_codex_proposal_guest_r2.wasm");

fn caps() -> SC {
    SC {
        session_read: true,
        propose: true,
        ..Default::default()
    }
}
fn limits() -> Limits {
    Limits {
        fuel: 100_000_000,
        memory_bytes: 16 * 1024 * 1024,
        host_calls: 16,
    }
}
fn original_guest() -> Vec<u8> {
    let path = std::env::var_os("MORROW_PROPOSAL_GUEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(RAW_PATH));
    let bytes = std::fs::read(&path).expect("original sealed 9552 bytes must be present");
    assert_eq!(bytes.len(), 442_926);
    let digest: String = hash(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, RAW_SHA);
    println!(
        "original_9552_bytes={} sha={} no_rebuild=true",
        bytes.len(),
        digest
    );
    bytes
}
struct Fixture {
    temp: tempfile::TempDir,
    manager: Manager,
    runtime: HostRuntime,
    host: Arc<SessionExecHost>,
    catalog: Catalog,
    full: AgentProcessPackage,
    maintenance_calls: usize,
    fail_maintenance: bool,
}
impl Fixture {
    fn new() -> Self {
        Self::with_original_module(original_guest())
    }
    fn with_original_module(module: Vec<u8>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = Package::build(
            Package::manifest_for_task(ID, "1.0.0", &module, vec![]),
            &module,
        )
        .unwrap();
        assert_eq!(base.module(), module);
        let registry = Registry::open(
            &temp.path().join("registry"),
            BaseCatalog::open(&temp.path().join("base")).unwrap(),
        )
        .unwrap();
        let mut manager = Manager::new(registry, limits());
        manager.install_package(base.archive()).unwrap();
        manager.select(&base, manager.revision()).unwrap();
        manager
            .set_enabled(ID, base.digest(), true, manager.revision())
            .unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
        // Only this trusted synthetic session creator has write authority.
        let creator = runtime.connect().unwrap();
        let create_caps = SC {
            session_read: true,
            session_write: true,
            ..Default::default()
        };
        let creator_admission = host
            .admit(
                &runtime,
                &creator,
                create_caps,
                create_caps,
                vec!["outside-session".into(), SESSION.into()],
                DOMAIN.into(),
                EXPIRES,
                1,
            )
            .unwrap();
        for (n, session) in [SESSION, "outside-session"].iter().enumerate() {
            let request = Request::new_for_generation(
                format!("create-{n}"),
                host.generation(&runtime).unwrap(),
                Action::Create {
                    session_id: (*session).into(),
                    parent: None,
                    parent_tail: 0,
                },
            )
            .unwrap();
            let bytes = host
                .dispatch(
                    &mut runtime,
                    &creator,
                    &creator_admission,
                    request.raw(),
                    || 1,
                )
                .unwrap();
            assert!(matches!(
                Reply::decode_for(&request, &bytes).unwrap().outcome,
                Outcome::Session(_)
            ));
        }
        host.revoke(&creator_admission).unwrap();
        runtime.disconnect(&creator).unwrap();
        let full = AgentProcessPackage::build(
            base,
            Declaration {
                session: caps(),
                process: PC::default(),
                sessions: vec![SESSION.into()],
                execution_domain: DOMAIN.into(),
            },
        )
        .unwrap();
        let mut catalog = Catalog::open(&temp.path().join("wrappers"), true).unwrap();
        let review = Catalog::inspect(full.archive()).unwrap();
        assert_eq!(review.base_sha256, full.base_sha256());
        assert_eq!(review.wrapper_sha256, full.review_sha256());
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        catalog
            .install(
                full.archive(),
                full.review_sha256(),
                &mut manager,
                revisions,
            )
            .unwrap();
        let revisions = Revisions {
            catalog: catalog.revision(),
            manager: manager.revision(),
        };
        assert!(matches!(
            catalog.connect(
                ID,
                full.review_sha256(),
                &mut manager,
                &mut runtime,
                &host,
                revisions,
                EXPIRES,
                1
            ),
            Err(Error::NotFound)
        ));
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
                ID,
                full.review_sha256(),
                Approval {
                    session: caps(),
                    process: PC::default(),
                    sessions: vec![SESSION.into()],
                    execution_domain: DOMAIN.into(),
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
            .set_enabled(ID, full.review_sha256(), true, &mut manager, revisions)
            .unwrap();
        Self {
            temp,
            manager,
            runtime,
            host,
            catalog,
            full,
            maintenance_calls: 0,
            fail_maintenance: false,
        }
    }
    fn bridge(&mut self) -> CatalogManagedPackage {
        let revisions = Revisions {
            catalog: self.catalog.revision(),
            manager: self.manager.revision(),
        };
        self.catalog
            .connect(
                ID,
                self.full.review_sha256(),
                &mut self.manager,
                &mut self.runtime,
                &self.host,
                revisions,
                EXPIRES,
                1,
            )
            .unwrap()
    }
    fn intent(&self, operation: &str) -> Intent {
        let artifact = self.temp.path().join("fixed-artifact.bin");
        std::fs::write(&artifact, b"ordinary inert fixed artifact").unwrap();
        Intent {
            operation_id: operation.into(),
            artifact_sha256: hash(b"ordinary inert fixed artifact"),
            program: artifact.to_str().unwrap().into(),
            argv: vec!["never-started".into()],
            cwd: self.temp.path().to_str().unwrap().into(),
            env: vec![],
            input: b"synthetic".to_vec(),
            execution_domain: DOMAIN.into(),
            max_runtime_ms: 5_000,
        }
    }
    fn proposal(
        &self,
        bridge: &CatalogManagedPackage,
        operation: &str,
        session: &str,
    ) -> (Invocation, Request, Intent) {
        let intent = self.intent(operation);
        let input = Request::new_for_generation(
            format!("fixed-input-{operation}"),
            bridge.generation(),
            Action::Propose {
                session_id: session.into(),
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
        (task(input.raw()), actual, intent)
    }
    fn propose(
        &mut self,
        bridge: &CatalogManagedPackage,
        operation: &str,
    ) -> (Request, Intent, Vec<u8>) {
        let (task, actual, intent) = self.proposal(bridge, operation, SESSION);
        let original_binding = self.runtime.binding();
        let host = self.host.clone();
        let run = bridge
            .run_session_owned(self, &host, task.bytes(), || 1)
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
        let review = self.host.review_tool(&self.runtime, operation, 1).unwrap();
        assert_eq!(review.proposal_sha256, actual.digest());
        assert_eq!(review.intent_sha256, intent.digest().unwrap());
        let observation = bridge
            .validate_tool_observation(
                &self.manager,
                &self.runtime,
                &self.host,
                operation,
                actual.digest(),
                1,
            )
            .unwrap();
        assert_eq!(observation.phase, ToolPhase::Proposed);
        assert!(!observation.invocation_started);
        assert_eq!(self.runtime.binding(), original_binding);
        (actual, intent, output.bytes)
    }
}
impl HostOwner for Fixture {
    fn runtime(&self) -> &HostRuntime {
        &self.runtime
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.runtime
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.maintenance_calls += 1;
        if self.fail_maintenance {
            Err(JobError::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl ManagedHostOwner for Fixture {
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
fn task(input: &[u8]) -> Invocation {
    Invocation::new_transform(
        "original-sealed-proposal-task",
        Transform {
            handler: "codex.session.propose".into(),
            input_type: "codex.session.proposal.v1".into(),
            output_type: "codex.session.proposal.receipt.v1".into(),
            input: input.to_vec(),
        },
    )
    .unwrap()
}
fn denied(run: TaskRun, host_calls: u32) {
    assert_ne!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, host_calls);
    assert!(run.completion.is_none());
}

#[test]
fn original_9552_combined_catalog_proposes_exact_record_on_same_owner() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let connection = bridge.shared_connection();
    assert!(Arc::ptr_eq(&connection, &bridge.shared_connection()));
    assert_eq!(bridge.session_capabilities(), caps());
    assert!(!bridge.session_capabilities().execute);
    assert_eq!(bridge.package().base().module(), original_guest());
    assert_eq!(
        f.runtime.connection_phase(&connection),
        Ok(InstancePhase::Ready)
    );
    let revocation = bridge.revocation(&f.runtime).unwrap();
    f.propose(&bridge, "exact-operation");
    assert_eq!(f.maintenance_calls, 1);
    assert!(bridge.owned_handles(&f.runtime).unwrap().is_empty());
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
    assert_ne!(
        f.runtime.connection_phase(&connection),
        Ok(InstancePhase::Ready)
    );
    assert!(revocation.is_revoked());
}

#[test]
fn original_9552_is_rejected_by_explicit_single_r2_loader() {
    let mut f = Fixture::new();
    assert!(matches!(
        Catalog::inspect_session_exec(f.full.archive()),
        Err(Error::Contract)
    ));
    let package = AgentProcessPackage::decode(f.full.archive()).unwrap();
    assert!(matches!(
        PreparedSessionExecPackage::new(package, limits()),
        Err(Error::Contract)
    ));
    assert!(
        f.manager
            .connect_agent_session_exec(ID, &mut f.runtime, f.manager.revision())
            .is_err()
    );
}

#[test]
fn original_9552_reply_rejects_wrong_nonce_without_changing_record() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (actual, _, output) = f.propose(&bridge, "correlation-operation");
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "correlation-operation")
        .unwrap();
    let wrong = Request::new_for_generation(
        "codex-wrongnonce-1",
        actual.generation(),
        actual.action().clone(),
    )
    .unwrap();
    assert!(Reply::decode_for(&wrong, &output).is_err());
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "correlation-operation")
            .unwrap(),
        before
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn original_9552_scope_and_execute_process_overreach_have_zero_effects() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (outside, _, _) = f.proposal(&bridge, "outside-operation", "outside-session");
    denied(
        bridge
            .run_session(&f.manager, &mut f.runtime, &f.host, outside.bytes(), || 1)
            .unwrap(),
        1,
    );
    assert_eq!(
        f.host.inspect_tool_record(&f.runtime, "outside-operation"),
        Err(Error::NotFound)
    );
    let claim = Request::new_for_generation(
        "unapproved-claim",
        bridge.generation(),
        Action::Claim {
            operation_id: "unapproved-operation".into(),
            permit: [7; 32],
        },
    )
    .unwrap();
    denied(
        bridge
            .run_session(
                &f.manager,
                &mut f.runtime,
                &f.host,
                task(claim.raw()).bytes(),
                || 1,
            )
            .unwrap(),
        0,
    );
    let process = process::Request::new(
        "unapproved-process",
        [9; 32],
        bridge.generation(),
        process::Action::Discover,
    )
    .unwrap();
    denied(
        bridge
            .run_session(
                &f.manager,
                &mut f.runtime,
                &f.host,
                task(&process.encode().unwrap()).bytes(),
                || 1,
            )
            .unwrap(),
        0,
    );
    assert!(bridge.owned_handles(&f.runtime).unwrap().is_empty());
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "unapproved-operation"),
        Err(Error::NotFound)
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn post_claim_observation_is_pure_and_retains_original_cleanup_authority() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (actual, intent, _) = f.propose(&bridge, "claimed-operation");
    let executor = f.runtime.connect().unwrap();
    let ec = SC {
        session_read: true,
        execute: true,
        ..Default::default()
    };
    let admission = f
        .host
        .admit(
            &f.runtime,
            &executor,
            ec,
            ec,
            vec![SESSION.into()],
            DOMAIN.into(),
            EXPIRES,
            1,
        )
        .unwrap();
    let permit = f
        .host
        .approve(
            &mut f.runtime,
            &executor,
            &admission,
            "claimed-operation",
            actual.digest(),
            intent.digest().unwrap(),
            1,
        )
        .unwrap();
    let claim = Request::new_for_generation(
        "trusted-once-claim",
        bridge.generation(),
        Action::Claim {
            operation_id: "claimed-operation".into(),
            permit,
        },
    )
    .unwrap();
    let raw = f
        .host
        .dispatch(&mut f.runtime, &executor, &admission, claim.raw(), || 1)
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&claim, &raw).unwrap().outcome,
        Outcome::Claimed { .. }
    ));
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "claimed-operation")
        .unwrap();
    assert_eq!(before.phase, ToolPhase::DispatchUnknown);
    assert!(!before.invocation_started);
    for _ in 0..3 {
        assert_eq!(
            bridge
                .validate_tool_observation(
                    &f.manager,
                    &f.runtime,
                    &f.host,
                    "claimed-operation",
                    actual.digest(),
                    1
                )
                .unwrap(),
            before
        );
    }
    assert!(
        bridge
            .validate_tool_observation(
                &f.manager,
                &f.runtime,
                &f.host,
                "claimed-operation",
                [1; 32],
                1
            )
            .is_err()
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "claimed-operation")
            .unwrap(),
        before
    );
    assert_eq!(f.maintenance_calls, 1); // Observations did not execute the sealed task again.
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
    f.host.revoke(&admission).unwrap();
    f.runtime.disconnect(&executor).unwrap();
}

#[test]
fn manager_revocation_blocks_original_9552_before_another_proposal() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let connection = bridge.shared_connection();
    let (actual, _, _) = f.propose(&bridge, "revoked-operation");
    f.manager
        .set_enabled(ID, f.full.base_sha256(), false, f.manager.revision())
        .unwrap();
    assert!(bridge.revocation(&f.runtime).unwrap().is_revoked());
    assert!(
        bridge
            .validate_tool_observation(
                &f.manager,
                &f.runtime,
                &f.host,
                "revoked-operation",
                actual.digest(),
                1
            )
            .is_err()
    );
    let (input, _, _) = f.proposal(&bridge, "never-replayed-operation", SESSION);
    assert!(
        bridge
            .run_session(&f.manager, &mut f.runtime, &f.host, input.bytes(), || 1)
            .is_err()
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "never-replayed-operation"),
        Err(Error::NotFound)
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
    assert_ne!(
        f.runtime.connection_phase(&connection),
        Ok(InstancePhase::Ready)
    );
}

#[test]
fn expiry_and_clock_rollback_reject_without_proposal_replay() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (actual, _, _) = f.propose(&bridge, "expiry-operation");
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "expiry-operation")
        .unwrap();
    let (expired, _, _) = f.proposal(&bridge, "expired-never-created", SESSION);
    // A real original-host call samples the expired time; no clock is reset.
    denied(
        bridge
            .run_session(&f.manager, &mut f.runtime, &f.host, expired.bytes(), || {
                EXPIRES
            })
            .unwrap(),
        1,
    );
    assert!(
        bridge
            .validate_tool_observation(
                &f.manager,
                &f.runtime,
                &f.host,
                "expiry-operation",
                actual.digest(),
                EXPIRES
            )
            .is_err()
    );
    assert!(
        bridge
            .validate_tool_observation(
                &f.manager,
                &f.runtime,
                &f.host,
                "expiry-operation",
                actual.digest(),
                1
            )
            .is_err()
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "expiry-operation")
            .unwrap(),
        before
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "expired-never-created"),
        Err(Error::NotFound)
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn original_owner_maintenance_failure_prevents_sealed_proposal_import() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    f.fail_maintenance = true;
    let (input, _, _) = f.proposal(&bridge, "maintenance-denied-operation", SESSION);
    let binding = f.runtime.binding();
    let host = f.host.clone();
    denied(
        bridge
            .run_session_owned(&mut f, &host, input.bytes(), || 1)
            .unwrap(),
        1,
    );
    assert_eq!(f.maintenance_calls, 1);
    assert_eq!(f.runtime.binding(), binding);
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "maintenance-denied-operation"),
        Err(Error::NotFound)
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn catalog_drop_retires_original_9552_lease_without_restoring_admission() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (actual, _, _) = f.propose(&bridge, "catalog-dropped-operation");
    let (input, _, _) = f.proposal(&bridge, "catalog-never-replayed", SESSION);
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "catalog-dropped-operation")
        .unwrap();
    let revocation = bridge.revocation(&f.runtime).unwrap();
    drop(f.catalog);
    assert!(revocation.is_revoked());
    assert!(
        bridge
            .validate_tool_observation(
                &f.manager,
                &f.runtime,
                &f.host,
                "catalog-dropped-operation",
                actual.digest(),
                1
            )
            .is_err()
    );
    assert!(
        bridge
            .run_session(&f.manager, &mut f.runtime, &f.host, input.bytes(), || 1)
            .is_err()
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "catalog-dropped-operation")
            .unwrap(),
        before
    );
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "catalog-never-replayed"),
        Err(Error::NotFound)
    );
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
}

#[test]
fn existing_combined_process_binding_requires_original_cleanup_host() {
    let mut f = Fixture::new();
    let bridge = f.bridge();
    let (input, actual, _) = f.proposal(&bridge, "combined-binding-operation", SESSION);
    let mut original_process_host = process::host::Host::default();
    let run = bridge
        .run(
            &f.manager,
            &mut f.runtime,
            &f.host,
            &mut original_process_host,
            input.bytes(),
            || 1,
        )
        .unwrap();
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    let output = input
        .verify_output(run.completion.as_ref().unwrap())
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&actual, &output.bytes).unwrap().outcome,
        Outcome::Tool(info) if info.phase == ToolPhase::Proposed)
    );
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "combined-binding-operation")
        .unwrap();
    assert!(
        bridge
            .run_session(&f.manager, &mut f.runtime, &f.host, input.bytes(), || 1)
            .is_err()
    );
    // A shortcut cannot discard ownership of the original ProcessHost, even when empty.
    assert!(bridge.close_session(&mut f.runtime, &f.host).is_err());
    assert_eq!(
        f.host
            .inspect_tool_record(&f.runtime, "combined-binding-operation")
            .unwrap(),
        before
    );
    bridge
        .close(&mut f.runtime, &f.host, &mut original_process_host)
        .unwrap();
}

#[test]
fn original_48e2_process_wire_is_denied_by_combined_r2_only_route() {
    const PROCESS_SHA: &str = "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36";
    const PROCESS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../companions/morrow-codex/qualification/c28-basic-fixtures/morrow_codex_process_control_guest_v1.wasm");
    let path = std::env::var_os("MORROW_PROCESS_GUEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(PROCESS_PATH));
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes.len(), 354_031);
    let digest: String = hash(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, PROCESS_SHA);
    let mut f = Fixture::with_original_module(bytes);
    let bridge = f.bridge();
    let request = process::Request::new(
        "r2-route-reject-process",
        [11; 32],
        bridge.generation(),
        process::Action::Discover,
    )
    .unwrap();
    let input = Invocation::new_transform(
        "original-process-wire-task",
        Transform {
            handler: "codex.process.control".into(),
            input_type: "codex.process.request.v1".into(),
            output_type: "codex.process.reply.v1".into(),
            input: request.encode().unwrap(),
        },
    )
    .unwrap();
    let binding = f.runtime.binding();
    let host = f.host.clone();
    denied(
        bridge
            .run_session_owned(&mut f, &host, input.bytes(), || 1)
            .unwrap(),
        1,
    );
    assert_eq!(f.maintenance_calls, 0); // Schema rejection precedes owner maintenance.
    assert!(bridge.owned_handles(&f.runtime).unwrap().is_empty());
    assert_eq!(f.runtime.binding(), binding);
    bridge.close_session(&mut f.runtime, &f.host).unwrap();
    println!(
        "original_48e2_bytes=354031 sha={digest} actual_process_import=1 maintenance=0 handles=0"
    );
}
