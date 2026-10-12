//! Actual StateSlot/Workbench/AD31 route, deliberately WITHOUT an owner/worker.
//! Ordinary disposable SQLite supplies only a genuine local binding for an empty
//! context ledger. No Storage/Session, grant, admission, connection, package,
//! context factory, AgentWorker, OS child or protected native backend is created.
//! Retained exit records below are synthetic field fixtures, not actual exits.
use super::*;
use crate::agent_task_discovery::{Kind, Observation, Request};
use morrow_core::store::{EventBudget, Store};

struct Fixture {
    app: Workbench,
    _binding_runtime: HostRuntime,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let runtime = HostRuntime::new(
            Store::open(&dir.path().join("ordinary-binding.sqlite"), EventBudget::default()).unwrap(),
        ).unwrap();
        let state = StateSlot {
            agent_preparation_debt: None,
            agent_contexts: crate::agent_tasks::context_lifecycle::ContextLifecycleLedger::new(runtime.binding()),
            product_gate: Default::default(),
            owner: None,
            task: None,
            cleanup: None,
            agent_exit: None,
            repair_needed: false,
            lost: false,
            service_submissions: Default::default(),
            file_submissions: Default::default(),
            mutation_submissions: Default::default(),
            mutation_checkpoints: Default::default(),
            submission: None,
        };
        let app = Workbench {
            state,
            channel_tasks: Default::default(),
            http_tasks: Default::default(),
            product_session_wire: Default::default(),
        };
        Self { app, _binding_runtime: runtime, _dir: dir }
    }
    fn retain_other_task(&mut self, key: TaskKey) {
        self.app.state.task = Some(Task {
            key,
            commands: Default::default(),
            worker: None,
            handle: None,
            stopping: true,
            exit: None,
            service: None,
            file: None,
            directory: None,
            mutation: None,
        });
    }
    fn reply(&mut self, id: u8) -> Observation {
        let q = Request { id: [id; 32] };
        let input = q.encode().unwrap();
        let output = crate::protocol::respond(&mut self.app, &input).unwrap();
        Observation::decode_for(q, &input, &output).unwrap()
    }
}

fn key(n: u8) -> TaskKey { TaskKey::from_bytes(&[n; 32]).unwrap() }
fn unavailable() -> Observation { Observation { kind: Kind::Unavailable, task: None } }
fn other() -> Observation { Observation { kind: Kind::OtherTask, task: None } }
fn exit_status() -> crate::agent_tasks::AgentExitStatus {
    crate::agent_tasks::AgentExitStatus {
        execution: Err(crate::agent_tasks::AgentError::Unavailable),
        disconnect: Err(crate::agent_tasks::AgentError::Disconnect),
        maintenance: Err(crate::agent_tasks::AgentError::Maintenance),
    }
}

#[test]
fn actual_empty_slot_without_original_owner_never_reports_none() {
    let mut f = Fixture::new();
    assert_eq!(f.app.observe_agent_task(), unavailable());
    assert_eq!(f.reply(1), unavailable());
    assert!(f.app.state.owner.is_none() && f.app.state.task.is_none());
    assert!(!f.app.state.product_gate.closed() && !f.app.state.lost);
}

#[test]
fn actual_other_task_observation_is_repeatable_and_preserves_slot_fields() {
    let mut f = Fixture::new();
    f.retain_other_task(key(7));
    f.app.state.repair_needed = true;
    f.app.state.submission = Some([9; 32]);
    let ledger = f.app.state.agent_contexts.clone();
    for id in 1..=3 {
        assert_eq!(f.app.observe_agent_task(), other());
        assert_eq!(f.reply(id), other());
        let slot = &f.app.state;
        let task = slot.task.as_ref().unwrap();
        assert_eq!(task.key, key(7));
        assert!(task.worker.is_none() && task.handle.is_none() && task.exit.is_none());
        assert!(task.stopping && slot.repair_needed && slot.owner.is_none());
        assert_eq!(slot.submission, Some([9; 32]));
        assert!(std::sync::Arc::ptr_eq(&slot.agent_contexts, &ledger));
    }
}

#[test]
fn actual_gate_close_hides_slot_and_rejects_direct_ad31_route() {
    let mut f = Fixture::new();
    f.retain_other_task(key(7));
    assert_eq!(f.reply(1), other());
    f.app.product_gate().revoke(); // Empty fixture gate has no worker callbacks.
    assert_eq!(f.app.observe_agent_task(), unavailable());
    let q = Request { id: [2; 32] }.encode().unwrap();
    assert!(crate::protocol::respond(&mut f.app, &q).is_err());
    assert_eq!(f.app.state.task.as_ref().unwrap().key, key(7));
    assert!(f.app.state.owner.is_none() && !f.app.state.lost);
}

#[test]
fn actual_lost_owner_transition_hides_other_task_without_reclaim() {
    let mut f = Fixture::new();
    f.retain_other_task(key(7));
    assert_eq!(f.reply(1), other());
    f.app.state.channel_owner_lost();
    assert_eq!(f.app.observe_agent_task(), unavailable());
    assert_eq!(f.reply(2), unavailable());
    assert!(f.app.state.lost && f.app.state.owner.is_none());
    assert!(!f.app.state.product_gate.closed());
    assert_eq!(f.app.state.task.as_ref().unwrap().key, key(7));
}

#[test]
fn actual_synthetic_same_key_exit_without_owner_never_reports_retained_success() {
    let mut f = Fixture::new();
    f.retain_other_task(key(7));
    f.app.state.agent_exit = Some((key(7), exit_status()));
    assert_eq!(f.app.observe_agent_task(), unavailable());
    assert_eq!(f.reply(1), unavailable());
    assert_eq!(f.app.state.agent_exit.as_ref().unwrap().0, key(7));
    assert!(f.app.state.task.as_ref().unwrap().worker.is_none());
    assert!(f.app.state.owner.is_none());
}

#[test]
fn actual_stale_exit_cannot_promote_other_task_and_zero_task_key_is_hidden() {
    let mut f = Fixture::new();
    f.retain_other_task(key(7));
    f.app.state.agent_exit = Some((key(8), exit_status()));
    assert_eq!(f.app.observe_agent_task(), other());
    assert_eq!(f.reply(1), other());
    f.app.state.task.as_mut().unwrap().key = key(0);
    assert_eq!(f.app.observe_agent_task(), unavailable());
    assert_eq!(f.reply(2), unavailable());
    assert_eq!(f.app.state.agent_exit.as_ref().unwrap().0, key(8));
}
