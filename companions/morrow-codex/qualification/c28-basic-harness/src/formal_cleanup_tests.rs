//! Scripted caller-policy tests. No Workbench, Core, protected session, native
//! process or repair is fabricated or run here; actual owner tests are separate.
use super::*;
use morrow_workbench_host::{
    agent_tasks::{AgentError, AgentLimits},
    io_tasks::Snapshot,
};
use std::collections::VecDeque;

struct Script {
    identity: u8,
    current: Option<AgentSnapshot>,
    statuses: VecDeque<Result<Option<AgentSnapshot>>>,
    start_result: Option<Result<TaskKey>>,
    after_start: Option<AgentSnapshot>,
    recover_result: Option<Result<AgentSnapshot>>,
    polls: VecDeque<Result<AgentSnapshot>>,
    ack_fail: bool,
    calls: Vec<&'static str>,
}
impl Script {
    fn new(current: Option<AgentSnapshot>) -> Self {
        Self {
            identity: 1,
            current,
            statuses: Default::default(),
            start_result: None,
            after_start: None,
            recover_result: None,
            polls: Default::default(),
            ack_fail: false,
            calls: Vec::new(),
        }
    }
    fn controls(&self) -> Vec<&'static str> {
        self.calls
            .iter()
            .copied()
            .filter(|c| *c != "status")
            .collect()
    }
}
impl Owner for Script {
    type Identity = u8;
    fn identity(&self) -> Result<u8> {
        Ok(self.identity)
    }
    fn status(&mut self) -> Result<Option<AgentSnapshot>> {
        self.calls.push("status");
        self.statuses.pop_front().unwrap_or(Ok(self.current))
    }
    fn start(&mut self, _: AgentStart, _: &mut Option<AgentContext>) -> Result<TaskKey> {
        self.calls.push("start");
        self.current = self.after_start.take();
        self.start_result
            .take()
            .unwrap_or_else(|| Err(anyhow!("start delivery Unknown")))
    }
    fn stop(&mut self, _: TaskKey) -> Result<AgentSnapshot> {
        self.calls.push("stop");
        self.current.ok_or_else(|| anyhow!("task absent"))
    }
    fn recover(&mut self, _: TaskKey) -> Result<AgentSnapshot> {
        self.calls.push("recover");
        let result = self
            .recover_result
            .take()
            .unwrap_or_else(|| self.current.ok_or_else(|| anyhow!("task absent")));
        if let Ok(status) = &result {
            self.current = Some(*status);
        }
        result
    }
    fn poll(&mut self, _: TaskKey) -> Result<AgentSnapshot> {
        self.calls.push("poll");
        self.polls
            .pop_front()
            .unwrap_or_else(|| self.current.ok_or_else(|| anyhow!("task absent")))
    }
    fn acknowledge(&mut self, _: TaskKey) -> Result<()> {
        self.calls.push("ack");
        ensure!(!self.ack_fail, "ACK failed");
        self.current = None;
        Ok(())
    }
}
fn key(value: u8) -> TaskKey {
    TaskKey::from_bytes(&[value; 32]).unwrap()
}
fn exit(failed: bool) -> AgentExitStatus {
    AgentExitStatus {
        execution: Err(AgentError::Unknown),
        disconnect: Ok(()),
        maintenance: if failed {
            Err(AgentError::Maintenance)
        } else {
            Ok(())
        },
    }
}
fn snapshot(k: TaskKey, phase: StoragePhase, failed: bool) -> AgentSnapshot {
    AgentSnapshot {
        task: Snapshot {
            key: Some(k),
            storage: phase,
            delivery: None,
            exit: None,
        },
        progress: None,
        exit: Some(exit(failed)),
    }
}
fn options() -> AgentStart {
    AgentStart {
        package_id: "policy-script-only".into(),
        full_sha256: [1; 32],
        revisions: morrow_agent_catalog_admin_v1::Revisions {
            catalog: 1,
            manager: 1,
        },
        lifetime: Duration::from_secs(60),
        limits: AgentLimits::default(),
    }
}

#[test]
fn normal_cleanup_acknowledges_once_without_recovery() {
    let k = key(1);
    let mut owner = Script::new(Some(snapshot(k, StoragePhase::Reclaimed, false)));
    let mut tracker = Tracker::default();
    let receipt = tracker
        .settle_task(&mut owner, Some(k), false, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.key, k);
    assert!(!receipt.explicit_recovery);
    assert_eq!(owner.controls(), ["ack"]);
    assert_eq!(receipt.historical_exit.execution, Err(AgentError::Unknown));
}
#[test]
fn historical_failure_requires_explicit_recovery_even_when_reclaimed() {
    let k = key(1);
    let mut owner = Script::new(Some(snapshot(k, StoragePhase::Reclaimed, true)));
    let mut tracker = Tracker::default();
    assert!(
        tracker
            .settle_task(&mut owner, Some(k), false, Duration::ZERO)
            .is_err()
    );
    assert!(owner.controls().is_empty());
    assert!(owner.current.is_some());
    let receipt = tracker
        .settle_task(&mut owner, Some(k), true, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(owner.controls(), ["recover", "ack"]);
    assert!(receipt.explicit_recovery);
    assert_eq!(
        receipt.historical_exit.maintenance,
        Err(AgentError::Maintenance)
    );
    assert_eq!(receipt.historical_exit.execution, Err(AgentError::Unknown));
}
#[test]
fn unknown_or_wrong_current_task_has_zero_control_calls() {
    let k = key(1);
    for known in [None, Some(key(2))] {
        let mut owner = Script::new(Some(snapshot(k, StoragePhase::Running, false)));
        assert!(
            Tracker::default()
                .settle_task(&mut owner, known, true, Duration::ZERO)
                .is_err()
        );
        assert!(owner.controls().is_empty());
    }
}
#[test]
fn existing_task_prevents_start_without_adoption() {
    let mut owner = Script::new(Some(snapshot(key(1), StoragePhase::Reclaimed, false)));
    let mut known = None;
    let mut context = None;
    assert!(
        Tracker::default()
            .start_once(&mut owner, options(), &mut context, &mut known)
            .is_err()
    );
    assert!(known.is_none());
    assert!(owner.controls().is_empty());
}
#[test]
fn failed_start_captures_only_new_same_borrow_task_and_does_not_replay() {
    let k = key(1);
    let mut owner = Script::new(None);
    owner.after_start = Some(snapshot(k, StoragePhase::RecoveryRequired, false));
    let mut known = None;
    let mut context = None;
    let mut tracker = Tracker::default();
    assert!(
        tracker
            .start_once(&mut owner, options(), &mut context, &mut known)
            .is_err()
    );
    assert_eq!(known, Some(k));
    assert_eq!(owner.controls(), ["start"]);
    assert!(
        tracker
            .start_once(&mut owner, options(), &mut context, &mut known)
            .is_err()
    );
    assert_eq!(owner.controls(), ["start"]);
}
#[test]
fn post_start_status_failure_stays_unknown_and_never_adopts_later_task() {
    let k = key(1);
    let mut owner = Script::new(None);
    owner.statuses = VecDeque::from([Ok(None), Err(anyhow!("status unavailable"))]);
    owner.after_start = Some(snapshot(k, StoragePhase::Reclaimed, false));
    let mut known = None;
    let mut context = None;
    let mut tracker = Tracker::default();
    assert!(
        tracker
            .start_once(&mut owner, options(), &mut context, &mut known)
            .is_err()
    );
    assert!(known.is_none());
    assert!(
        tracker
            .settle_task(&mut owner, known, true, Duration::ZERO)
            .is_err()
    );
    assert_eq!(owner.controls(), ["start"]);
}
#[test]
fn failed_start_without_new_task_keeps_external_cleanup_available() {
    let mut owner = Script::new(None);
    let mut tracker = Tracker::default();
    let mut known = None;
    let mut context = None;
    assert!(
        tracker
            .start_once(&mut owner, options(), &mut context, &mut known)
            .is_err()
    );
    assert!(known.is_none());
    assert!(
        tracker
            .settle_task(&mut owner, known, true, Duration::ZERO)
            .unwrap()
            .is_none()
    );
    assert_eq!(owner.controls(), ["start"]);
}
#[test]
fn failed_or_busy_recovery_cannot_acknowledge_or_clear_identity() {
    let k = key(1);
    for result in [
        Err(anyhow!("Busy")),
        Ok(snapshot(k, StoragePhase::RecoveryRequired, true)),
    ] {
        let mut owner = Script::new(Some(snapshot(k, StoragePhase::RecoveryRequired, true)));
        owner.recover_result = Some(result);
        let mut tracker = Tracker::default();
        assert!(
            tracker
                .settle_task(&mut owner, Some(k), true, Duration::ZERO)
                .is_err()
        );
        assert_eq!(owner.controls(), ["recover"]);
        assert!(tracker.acknowledged.is_none());
        assert!(owner.current.is_some());
    }
}
#[test]
fn changed_recovery_or_poll_key_cannot_acknowledge() {
    let k = key(1);
    for from_poll in [false, true] {
        let mut owner = Script::new(Some(snapshot(k, StoragePhase::Running, false)));
        owner.recover_result = Some(Ok(snapshot(
            if from_poll { k } else { key(2) },
            if from_poll {
                StoragePhase::Stopping
            } else {
                StoragePhase::Reclaimed
            },
            false,
        )));
        owner
            .polls
            .push_back(Ok(snapshot(key(2), StoragePhase::Reclaimed, false)));
        assert!(
            Tracker::default()
                .settle_task(&mut owner, Some(k), true, Duration::from_secs(1))
                .is_err()
        );
        assert!(!owner.controls().contains(&"ack"));
    }
}
#[test]
fn missing_terminal_exit_cannot_acknowledge() {
    let k = key(1);
    let mut status = snapshot(k, StoragePhase::Reclaimed, false);
    status.exit = None;
    let mut owner = Script::new(Some(status));
    assert!(
        Tracker::default()
            .settle_task(&mut owner, Some(k), true, Duration::ZERO)
            .is_err()
    );
    assert!(owner.controls().is_empty());
}
#[test]
fn failed_ack_preserves_task_and_does_not_create_receipt() {
    let k = key(1);
    let mut owner = Script::new(Some(snapshot(k, StoragePhase::Reclaimed, false)));
    owner.ack_fail = true;
    let mut tracker = Tracker::default();
    assert!(
        tracker
            .settle_task(&mut owner, Some(k), false, Duration::ZERO)
            .is_err()
    );
    assert!(tracker.acknowledged.is_none());
    assert!(owner.current.is_some());
}
#[test]
fn acknowledged_receipt_resumes_same_owner_debt_without_duplicate_ack() {
    let k = key(1);
    let mut owner = Script::new(Some(snapshot(k, StoragePhase::Reclaimed, true)));
    let mut tracker = Tracker::default();
    let first = tracker
        .settle_task(&mut owner, Some(k), true, Duration::ZERO)
        .unwrap()
        .unwrap();
    // Simulated later caller debt failure leaves the private receipt intact.
    let resumed = tracker
        .settle_task(&mut owner, Some(k), false, Duration::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.key, first.key);
    assert_eq!(owner.controls(), ["recover", "ack"]);
    assert_eq!(
        resumed.historical_exit.maintenance,
        Err(AgentError::Maintenance)
    );
}
#[test]
fn post_ack_foreign_owner_or_new_task_cannot_resume() {
    let k = key(1);
    let mut owner = Script::new(Some(snapshot(k, StoragePhase::Reclaimed, false)));
    let mut tracker = Tracker::default();
    tracker
        .settle_task(&mut owner, Some(k), false, Duration::ZERO)
        .unwrap();
    owner.identity = 2;
    assert!(
        tracker
            .settle_task(&mut owner, Some(k), true, Duration::ZERO)
            .is_err()
    );
    owner.identity = 1;
    owner.current = Some(snapshot(key(2), StoragePhase::Reclaimed, false));
    assert!(
        tracker
            .settle_task(&mut owner, Some(k), true, Duration::ZERO)
            .is_err()
    );
    assert_eq!(owner.controls(), ["ack"]);
    assert!(tracker.acknowledged.is_some());
}
#[test]
fn disappearance_without_ack_is_not_retirement_evidence() {
    let mut owner = Script::new(None);
    assert!(
        Tracker::default()
            .settle_task(&mut owner, Some(key(1)), true, Duration::ZERO)
            .is_err()
    );
    assert!(owner.controls().is_empty());
}


#[test]
fn failed_start_absent_status_blocks_second_start_but_keeps_empty_cleanup() {
    let mut owner = Script::new(None);
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert!(known.is_none());
    assert!(tracker.settle_task(&mut owner, known, true, Duration::ZERO).unwrap().is_none());
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert_eq!(owner.controls(), ["start"]);
}

#[test]
fn failed_start_status_error_blocks_second_start_and_later_adoption() {
    let mut owner = Script::new(None);
    owner.statuses = VecDeque::from([Ok(None), Err(anyhow!("synthetic status failure"))]);
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    owner.current = Some(snapshot(key(9), StoragePhase::Reclaimed, false));
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert!(tracker.settle_task(&mut owner, known, true, Duration::ZERO).is_err());
    assert_eq!(owner.controls(), ["start"]);
    assert!(known.is_none());
}

#[test]
fn failed_start_keyless_status_blocks_replay_and_blind_cleanup() {
    let mut owner = Script::new(None);
    let mut keyless = snapshot(key(1), StoragePhase::Running, false);
    keyless.task.key = None;
    owner.after_start = Some(keyless);
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert!(tracker.settle_task(&mut owner, known, true, Duration::ZERO).is_err());
    assert_eq!(owner.controls(), ["start"]);
    assert!(known.is_none());
}

#[test]
fn normal_session_join_ack_allows_following_native_lane_on_same_tracker() {
    let mut owner = Script::new(None);
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    let mut session = snapshot(key(1), StoragePhase::Reclaimed, false);
    session.exit.as_mut().unwrap().execution = Ok(());
    owner.start_result = Some(Ok(key(1)));
    owner.after_start = Some(session);
    assert_eq!(tracker.start_once(&mut owner, options(), &mut context, &mut known).unwrap(), key(1));
    // formal.rs session-join does stop/poll/ACK directly, then clears its key.
    let session_key = known.unwrap();
    owner.stop(session_key).unwrap();
    let joined = owner.poll(session_key).unwrap();
    assert_eq!(joined.task.storage, StoragePhase::Reclaimed);
    let exit = joined.exit.unwrap();
    assert!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
    owner.acknowledge(session_key).unwrap();
    known = None;
    owner.start_result = Some(Ok(key(2)));
    owner.after_start = Some(snapshot(key(2), StoragePhase::Running, false));
    assert_eq!(tracker.start_once(&mut owner, options(), &mut context, &mut known).unwrap(), key(2));
    assert_eq!(known, Some(key(2)));
    assert_eq!(owner.controls(), ["start", "stop", "poll", "ack", "start"]);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert_eq!(owner.controls(), ["start", "stop", "poll", "ack", "start"]);
}

#[test]
fn pre_start_existing_task_rejection_does_not_consume_attempt() {
    let mut owner = Script::new(Some(snapshot(key(2), StoragePhase::Running, false)));
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert!(owner.controls().is_empty());
    owner.current = None;
    owner.start_result = Some(Ok(key(1)));
    owner.after_start = Some(snapshot(key(1), StoragePhase::Running, false));
    assert_eq!(tracker.start_once(&mut owner, options(), &mut context, &mut known).unwrap(), key(1));
    assert_eq!(owner.controls(), ["start"]);
}

#[test]
fn complete_never_clears_failed_start_replay_gate() {
    let mut owner = Script::new(None);
    let mut tracker = Tracker::default();
    let (mut known, mut context) = (None, None);
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    tracker.complete();
    assert!(tracker.start_once(&mut owner, options(), &mut context, &mut known).is_err());
    assert_eq!(owner.controls(), ["start"]);
    assert!(tracker.settle_task(&mut owner, known, true, Duration::ZERO).unwrap().is_none());
}
