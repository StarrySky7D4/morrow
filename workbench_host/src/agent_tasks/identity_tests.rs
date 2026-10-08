//! Ordinary real Core identities and original context debts only.
//! No public protected Workbench, DPAPI, native child, or agent-worker join qualification.
use super::*;
use crate::agent_tasks::preparation::{AgentPreparation, AgentPreparationDebt};
use morrow_agent_session_exec_v1_r2::authority::Capabilities;
use morrow_core::{
    lifecycle::InstancePhase,
    store::{EventBudget, Store},
};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn runtime(temp: &tempfile::TempDir) -> HostRuntime {
    HostRuntime::new(
        Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap()
}

#[test]
fn original_binding_survives_runtime_move_and_rejects_foreign_repair() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let owner = runtime(&a);
    let foreign = runtime(&b);
    let binding = owner.binding();
    let copied = binding;
    let ledger = ContextLifecycleLedger::new(binding);
    assert_ne!(copied, foreign.binding());
    let returned = std::thread::spawn(move || owner).join().unwrap();
    assert_eq!(ledger.owner_binding(), returned.binding());
    assert_eq!(ledger.owner_binding(), copied);
    // Copying an opaque identity never grants a foreign owner repair authority.
    assert!(ledger.take_abandoned(foreign.binding()).is_err());
    assert!(ledger.take_abandoned(returned.binding()).unwrap().is_none());
}

#[test]
fn binding_remains_readable_when_ledger_lock_is_poisoned() {
    let temp = tempfile::tempdir().unwrap();
    let owner = runtime(&temp);
    let ledger = ContextLifecycleLedger::new(owner.binding());
    let binding = ledger.owner_binding();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _locked = ledger.entries.lock().unwrap();
        panic!("synthetic ledger poison");
    }));
    assert!(result.is_err());
    assert!(ledger.ensure_capacity().is_err());
    assert!(ledger.needs_repair());
    assert_eq!(ledger.owner_binding(), binding);
    assert_eq!(ledger.owner_binding(), owner.binding());
}

#[test]
fn binding_does_not_retire_actual_abandoned_grant_or_hide_context_debt() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = runtime(&temp);
    let host = Arc::new(SessionExecHost::new(&mut owner).unwrap());
    let ledger = ContextLifecycleLedger::new(owner.binding());
    let binding = ledger.owner_binding();
    let context = {
        let mut preparation = AgentPreparation::with_catalog(
            &mut owner,
            crate::platform::Instant::now(),
            None,
            None,
            vec![],
            host.clone(),
            ledger.clone(),
        );
        let connection = preparation.connect().unwrap();
        let caps = Capabilities {
            session_read: true,
            ..Default::default()
        };
        let expires = preparation.expires_after(60_000).unwrap();
        preparation
            .admit(
                &host,
                &connection,
                caps,
                caps,
                vec!["identity-session".into()],
                "identity-domain".into(),
                expires,
            )
            .unwrap();
        let token = preparation.context(Default::default(), None).unwrap();
        preparation.release_context(token).unwrap()
    };
    let connection = context.executor_connection.clone();
    drop(context);
    assert_eq!(ledger.owner_binding(), binding);
    assert!(ledger.outstanding() && ledger.needs_repair());
    assert_eq!(
        owner.connection_phase(&connection).unwrap(),
        InstancePhase::Ready
    );
    let context = ledger.take_abandoned(owner.binding()).unwrap().unwrap();
    AgentPreparationDebt::for_context(context)
        .cleanup(&mut owner)
        .unwrap();
    assert!(owner.connection_phase(&connection).is_err());
    assert!(!ledger.outstanding());
    assert_eq!(ledger.owner_binding(), binding);
}
