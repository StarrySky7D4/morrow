//! Ordinary real Core/R2 with synthetic SQLite. No protected owner or OS backend.
use super::*;
use crate::{
    agent_tasks::preparation::{AgentPreparation, AgentPreparationDebt},
    platform::Instant,
};
use morrow_agent_session_exec_v1_r2::authority::Capabilities;
use morrow_agent_session_exec_v1_r2::{Action, Intent, Outcome, Reply, Request, ToolPhase};
use morrow_core::{
    lifecycle::InstancePhase,
    store::{EventBudget, Store},
};

struct Fixture {
    temp: tempfile::TempDir,
    runtime: HostRuntime,
    host: Arc<SessionExecHost>,
    ledger: Arc<ContextLifecycleLedger>,
    start: Instant,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
        let ledger = ContextLifecycleLedger::new(runtime.binding());
        Self {
            temp,
            runtime,
            host,
            ledger,
            start: Instant::now(),
        }
    }
    fn context(&mut self) -> AgentContext {
        let mut preparation = AgentPreparation::with_catalog(
            &mut self.runtime,
            self.start,
            None,
            None,
            vec![],
            self.host.clone(),
            self.ledger.clone(),
        );
        let connection = preparation.connect().unwrap();
        let caps = Capabilities {
            session_read: true,
            session_write: true,
            propose: true,
            execute: true,
            retire: false,
        };
        let expires = preparation.expires_after(60_000).unwrap();
        preparation
            .admit(
                &self.host,
                &connection,
                caps,
                caps,
                vec!["ledger-session".into()],
                "ledger-domain".into(),
                expires,
            )
            .unwrap();
        let token = preparation.context(Default::default(), None).unwrap();
        preparation.release_context(token).unwrap()
    }
    fn dispatch(&mut self, context: &AgentContext, name: &str, action: Action) -> Outcome {
        let request =
            Request::new_for_generation(name, self.host.generation(&self.runtime).unwrap(), action)
                .unwrap();
        let raw = self
            .host
            .dispatch(
                &mut self.runtime,
                &context.executor_connection,
                &context.executor_admission,
                request.raw(),
                || crate::now(self.start),
            )
            .unwrap();
        Reply::decode_for(&request, &raw).unwrap().outcome
    }
    fn cleanup(&mut self, context: AgentContext) {
        AgentPreparationDebt::for_context(context)
            .cleanup(&mut self.runtime)
            .unwrap();
    }
    fn repair_abandoned(&mut self) {
        while let Some(context) = self.ledger.take_abandoned(self.runtime.binding()).unwrap() {
            self.cleanup(context);
        }
    }
}

#[test]
fn external_drop_keeps_original_live_admission_and_actual_context_charged() {
    let mut f = Fixture::new();
    let context = f.context();
    let connection = context.executor_connection.clone();
    let admission = context.executor_admission.clone();
    assert!(f.ledger.outstanding());
    drop(context);
    assert!(f.ledger.outstanding() && f.ledger.needs_repair());
    assert_eq!(
        f.runtime.connection_phase(&connection).unwrap(),
        InstancePhase::Ready
    );
    let mut rescued = f
        .ledger
        .take_abandoned(f.runtime.binding())
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&connection, &rescued.executor_connection));
    // Drop did not revoke: a genuine R2 Create still succeeds with the issued grant.
    rescued.executor_admission = admission;
    assert!(matches!(
        f.dispatch(
            &rescued,
            "create-after-drop",
            Action::Create {
                session_id: "ledger-session".into(),
                parent: None,
                parent_tail: 0
            }
        ),
        Outcome::Session(_)
    ));
    f.cleanup(rescued);
    assert!(!f.ledger.outstanding());
    assert_ne!(
        f.runtime.connection_phase(&connection).ok(),
        Some(InstancePhase::Ready)
    );
}

#[test]
fn attachment_and_native_clean_cannot_retire_a_live_original_grant() {
    let mut f = Fixture::new();
    let mut context = f.context();
    context.attach_lifecycle().unwrap();
    assert!(context.resources.is_clean().unwrap());
    assert!(context.settle_lifecycle(&f.runtime).is_err());
    assert!(f.ledger.outstanding());
    // This tests the shared attachment state, not an OS-thread join witness.
    f.cleanup(context);
    assert!(!f.ledger.outstanding());
}

#[test]
fn foreign_cleanup_failure_retains_exact_debt_without_revoking_original() {
    let mut f = Fixture::new();
    let mut other = Fixture::new();
    let context = f.context();
    let connection = context.executor_connection.clone();
    let mut debt = AgentPreparationDebt::for_context(context);
    assert!(debt.cleanup(&mut other.runtime).is_err());
    assert!(f.ledger.outstanding() && f.ledger.needs_repair());
    assert_eq!(
        f.runtime.connection_phase(&connection).unwrap(),
        InstancePhase::Ready
    );
    // A failed foreign attempt must preserve even the original admission's liveness.
    let request = Request::new_for_generation(
        "create-after-foreign-cleanup",
        f.host.generation(&f.runtime).unwrap(),
        Action::Create {
            session_id: "ledger-session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let original = f
        .ledger
        .entries
        .lock()
        .unwrap()
        .records
        .values()
        .next()
        .unwrap()
        .original
        .clone();
    let raw = f
        .host
        .dispatch(
            &mut f.runtime,
            &original.connection,
            &original.admission,
            request.raw(),
            || crate::now(f.start),
        )
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&request, &raw).unwrap().outcome,
        Outcome::Session(_)
    ));
    debt.cleanup(&mut f.runtime).unwrap();
    assert!(!f.ledger.outstanding());
}

#[test]
fn foreign_start_validation_preserves_caller_option_and_both_owners() {
    let mut f = Fixture::new();
    let other = Fixture::new();
    let prepared = Some(f.context());
    assert!(
        other
            .ledger
            .validate_external(prepared.as_ref().unwrap(), other.runtime.binding())
            .is_err()
    );
    assert!(prepared.is_some());
    assert!(f.ledger.outstanding() && !other.ledger.outstanding());
    f.cleanup(prepared.unwrap());
}

#[test]
fn replaced_public_fields_do_not_redirect_original_drop_cleanup() {
    let mut f = Fixture::new();
    let mut other = Fixture::new();
    let mut context = f.context();
    let foreign = other.context();
    let original_connection = context.executor_connection.clone();
    let foreign_connection = foreign.executor_connection.clone();
    context.host = foreign.host.clone();
    context.executor_connection = foreign_connection.clone();
    context.executor_admission = foreign.executor_admission.clone();
    context.resources = foreign.resources.clone();
    assert!(
        f.ledger
            .validate_external(&context, f.runtime.binding())
            .is_err()
    );
    drop(context);
    f.repair_abandoned();
    assert_ne!(
        f.runtime.connection_phase(&original_connection).ok(),
        Some(InstancePhase::Ready)
    );
    assert_eq!(
        other.runtime.connection_phase(&foreign_connection).unwrap(),
        InstancePhase::Ready
    );
    assert!(other.ledger.outstanding());
    other.cleanup(foreign);
}

#[test]
fn substituted_opaque_admission_is_not_used_for_disposal() {
    let mut f = Fixture::new();
    let mut other = Fixture::new();
    let mut context = f.context();
    let foreign = other.context();
    context.executor_admission = foreign.executor_admission.clone();
    // The saved issued token, not the mutable public admission, drives cleanup.
    f.cleanup(context);
    assert!(!f.ledger.outstanding());
    assert!(matches!(
        other.dispatch(
            &foreign,
            "foreign-still-live",
            Action::Create {
                session_id: "ledger-session".into(),
                parent: None,
                parent_tail: 0
            }
        ),
        Outcome::Session(_)
    ));
    other.cleanup(foreign);
}

#[test]
fn abandoned_contexts_consume_bounded_quota_until_explicit_cleanup() {
    let mut f = Fixture::new();
    for _ in 0..MAX_CONTEXTS {
        drop(f.context());
    }
    assert!(f.ledger.ensure_capacity().is_err());
    assert_eq!(f.ledger.entries.lock().unwrap().records.len(), MAX_CONTEXTS);
    f.repair_abandoned();
    assert!(!f.ledger.outstanding());
    f.ledger.ensure_capacity().unwrap();
    let context = f.context();
    f.cleanup(context);
}

#[test]
fn completed_r2_proposal_remains_real_and_is_not_replayed_by_cleanup() {
    let mut f = Fixture::new();
    let context = f.context();
    assert!(matches!(
        f.dispatch(
            &context,
            "create-proposal-session",
            Action::Create {
                session_id: "ledger-session".into(),
                parent: None,
                parent_tail: 0
            }
        ),
        Outcome::Session(_)
    ));
    let program = f.temp.path().join("never-executed.exe");
    let artifact = b"synthetic-only-no-execution";
    std::fs::write(&program, artifact).unwrap();
    let intent = Intent {
        operation_id: "ledger-proposal".into(),
        program: program.to_str().unwrap().into(),
        argv: vec![],
        cwd: f.temp.path().to_str().unwrap().into(),
        env: vec![],
        input: vec![],
        execution_domain: "ledger-domain".into(),
        max_runtime_ms: 1000,
        artifact_sha256: morrow_agent_session_exec_v1_r2::hash(artifact),
    };
    assert!(
        matches!(f.dispatch(&context, "actual-proposal", Action::Propose { session_id: "ledger-session".into(), intent }), Outcome::Tool(info) if info.phase == ToolPhase::Proposed)
    );
    let before = f
        .host
        .inspect_tool_record(&f.runtime, "ledger-proposal")
        .unwrap();
    drop(context);
    assert!(f.ledger.outstanding());
    f.repair_abandoned();
    let after = f
        .host
        .inspect_tool_record(&f.runtime, "ledger-proposal")
        .unwrap();
    assert_eq!(before, after);
    assert!(!f.ledger.outstanding());
}

#[test]
fn dropping_failed_disposal_debt_reparks_objects_without_claiming_clean() {
    let mut f = Fixture::new();
    let mut other = Fixture::new();
    let context = f.context();
    let connection = context.executor_connection.clone();
    let mut debt = AgentPreparationDebt::for_context(context);
    assert!(debt.cleanup(&mut other.runtime).is_err());
    drop(debt);
    assert!(f.ledger.needs_repair() && f.ledger.outstanding());
    assert_eq!(
        f.runtime.connection_phase(&connection).unwrap(),
        InstancePhase::Ready
    );
    f.repair_abandoned();
    assert!(!f.ledger.outstanding());
}

#[test]
fn revoked_or_retired_core_phase_alone_is_not_an_original_cleanup_witness() {
    let mut f = Fixture::new();
    let mut context = f.context();
    context.attach_lifecycle().unwrap();
    f.runtime
        .revocation(&context.executor_connection)
        .unwrap()
        .revoke();
    assert_eq!(
        f.runtime
            .connection_phase(&context.executor_connection)
            .unwrap(),
        InstancePhase::Revoked
    );
    assert!(context.settle_lifecycle(&f.runtime).is_err());
    assert!(f.ledger.outstanding());
    f.runtime.disconnect(&context.executor_connection).unwrap();
    assert!(
        f.runtime
            .connection_phase(&context.executor_connection)
            .is_err()
    );
    // Even Core's missing/retired connection does not certify admission cleanup.
    assert!(context.settle_lifecycle(&f.runtime).is_err());
    assert!(f.ledger.outstanding());
    // Actual issued-token revoke and successful original disconnect were performed
    // in this test; publish their private witness, as the production cleanup does.
    f.host.revoke(&context.executor_admission).unwrap();
    context
        .lifecycle
        .as_ref()
        .unwrap()
        .authority_retired(&f.runtime)
        .unwrap();
    context.settle_lifecycle(&f.runtime).unwrap();
    assert!(!f.ledger.outstanding());
}

#[test]
fn pre_spawn_rejection_returns_ticket_to_external_without_retiring_admission() {
    let mut f = Fixture::new();
    let mut context = f.context();
    context.attach_lifecycle().unwrap();
    context
        .lifecycle
        .as_ref()
        .unwrap()
        .detach_before_spawn()
        .unwrap();
    f.ledger
        .validate_external(&context, f.runtime.binding())
        .unwrap();
    assert_eq!(
        f.runtime
            .connection_phase(&context.executor_connection)
            .unwrap(),
        InstancePhase::Ready
    );
    assert!(f.ledger.outstanding());
    f.cleanup(context);
}

#[test]
fn actual_thread_join_is_needed_even_after_original_authority_cleanup() {
    let mut f = Fixture::new();
    let mut context = f.context();
    context.attach_lifecycle().unwrap();
    // Ordinary owner-object movement only, no Wasm, native process or protected Store.
    let join = std::thread::spawn(move || {
        context.lifecycle.as_ref().unwrap().worker_started();
        context
    });
    let mut context = join.join().unwrap();
    f.host.revoke(&context.executor_admission).unwrap();
    f.runtime.disconnect(&context.executor_connection).unwrap();
    context
        .lifecycle
        .as_ref()
        .unwrap()
        .authority_retired(&f.runtime)
        .unwrap();
    assert!(context.settle_lifecycle(&f.runtime).is_err());
    assert!(f.ledger.outstanding());
    context
        .lifecycle
        .as_ref()
        .unwrap()
        .worker_joined(&f.runtime)
        .unwrap();
    context.settle_lifecycle(&f.runtime).unwrap();
    assert!(!f.ledger.outstanding());
}

#[test]
fn stopped_business_gate_does_not_replace_original_cleanup_witness() {
    let mut f = Fixture::new();
    let context = f.context();
    let gate = crate::product_gate::ProductGate::default();
    gate.revoke();
    assert!(gate.check().is_err());
    assert!(f.ledger.outstanding());
    // The production-used cleanup component issues no admission/business work;
    // protected Workbench stop/cleanup is separately NOT_RUN in this fixture.
    f.cleanup(context);
    assert!(!f.ledger.outstanding());
}
