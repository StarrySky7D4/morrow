//! Ordinary real Core Store qualification. No SDK-owned OS sandbox/backend.
use morrow_agent_session_exec_v1::{
    Action, Error, ExecutionFacts, Intent, Outcome, Reply, Request, ToolPhase,
    authority::{Admission, Capabilities, SessionExecHost},
    hash,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    store::{EventBudget, Store},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

const SESSION: &str = "session";
const OP: &str = "fixed-operation";
const EXPIRES: u64 = 1000;
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    runtime: HostRuntime,
    host: SessionExecHost,
    proposer_connection: Connection,
    executor_connection: Connection,
    proposer: Admission,
    executor: Admission,
}
fn proposer_capabilities() -> Capabilities {
    Capabilities {
        session_read: true,
        session_write: true,
        propose: true,
        execute: false,
    }
}
fn executor_capabilities() -> Capabilities {
    Capabilities {
        session_read: true,
        session_write: false,
        propose: false,
        execute: true,
    }
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("single-core.sqlite");
        let runtime =
            HostRuntime::new(Store::open(&path, EventBudget::default()).unwrap()).unwrap();
        Self::from_runtime(temp, path, runtime)
    }
    fn from_runtime(temp: tempfile::TempDir, path: PathBuf, mut runtime: HostRuntime) -> Self {
        let proposer_connection = runtime.connect().unwrap();
        let executor_connection = runtime.connect().unwrap();
        let host = SessionExecHost::new(&mut runtime).unwrap();
        let proposer = host
            .admit(
                &runtime,
                &proposer_connection,
                proposer_capabilities(),
                proposer_capabilities(),
                vec![SESSION.into()],
                "test-domain".into(),
                EXPIRES,
                0,
            )
            .unwrap();
        let executor = host
            .admit(
                &runtime,
                &executor_connection,
                executor_capabilities(),
                executor_capabilities(),
                vec![SESSION.into()],
                "test-domain".into(),
                EXPIRES,
                0,
            )
            .unwrap();
        let mut fixture = Self {
            _temp: temp,
            path,
            runtime,
            host,
            proposer_connection,
            executor_connection,
            proposer,
            executor,
        };
        let request = Request::new(
            "create",
            Action::Create {
                session_id: SESSION.into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap();
        assert!(matches!(
            fixture.dispatch(&request, false, 1),
            Outcome::Session(_)
        ));
        fixture
    }
    fn intent(&self) -> Intent {
        Intent {
            operation_id: OP.into(),
            program: "/usr/bin/printf".into(),
            argv: vec!["fixed-output".into()],
            cwd: self._temp.path().to_str().unwrap().into(),
            env: vec![morrow_agent_session_exec_v1::Environment {
                name: "MORROW_FIXED".into(),
                value: "approved".into(),
            }],
            input: b"immutable-input".to_vec(),
            execution_domain: "test-domain".into(),
            max_runtime_ms: 1000,
            artifact_sha256: hash(&std::fs::read("/usr/bin/printf").unwrap()),
        }
    }
    fn dispatch(&mut self, request: &Request, executor: bool, now: u64) -> Outcome {
        let (connection, admission) = if executor {
            (&self.executor_connection, &self.executor)
        } else {
            (&self.proposer_connection, &self.proposer)
        };
        let bytes = self
            .host
            .dispatch(
                &mut self.runtime,
                connection,
                admission,
                request.raw(),
                || now,
            )
            .unwrap();
        Reply::decode_for(request, &bytes).unwrap().outcome
    }
    fn propose(&mut self) -> Request {
        let request = Request::new(
            "propose",
            Action::Propose {
                session_id: SESSION.into(),
                intent: self.intent(),
            },
        )
        .unwrap();
        assert!(
            matches!(self.dispatch(&request,false,2),Outcome::Tool(info) if info.phase==ToolPhase::Proposed)
        );
        request
    }
    fn approve(&mut self) -> [u8; 32] {
        let review = self.host.review_tool(&self.runtime, OP, 3).unwrap();
        assert_eq!(review.intent, self.intent());
        assert_eq!(review.session_id, SESSION);
        assert_eq!(review.session_epoch, 0);
        self.host
            .approve(
                &mut self.runtime,
                &self.executor_connection,
                &self.executor,
                OP,
                review.proposal_sha256,
                review.intent_sha256,
                3,
            )
            .unwrap()
    }
    fn claim(&mut self, permit: [u8; 32]) -> [u8; 32] {
        let request = Request::new(
            "claim",
            Action::Claim {
                operation_id: OP.into(),
                permit,
            },
        )
        .unwrap();
        match self.dispatch(&request, true, 4) {
            Outcome::Claimed {
                info,
                intent,
                claim,
            } => {
                assert_eq!(info.phase, ToolPhase::DispatchUnknown);
                assert_eq!(intent, self.intent());
                claim
            }
            _ => panic!("first claim must win"),
        }
    }
    fn inspect(&mut self) -> ToolPhase {
        let request = Request::new(
            "inspect",
            Action::Inspect {
                operation_id: OP.into(),
            },
        )
        .unwrap();
        match self.dispatch(&request, false, 10) {
            Outcome::Tool(info) => info.phase,
            _ => panic!("inspect must return history"),
        }
    }
}
fn facts() -> ExecutionFacts {
    ExecutionFacts {
        exit_code: Some(0),
        output_closed: true,
        stdout_sha256: hash(b"fixed-output"),
        stderr_sha256: hash(&[]),
        stdout_bytes: 12,
        stderr_bytes: 0,
    }
}
fn rejected(outcome: Outcome) {
    assert!(
        matches!(outcome, Outcome::Rejected(_)),
        "expected refusal: {outcome:?}"
    );
}

#[test]
fn proposal_and_approval_have_no_effect_and_claim_is_once() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let count = AtomicUsize::new(0);
    let claim = f.claim(permit);
    let request = Request::new(
        "claim-again",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    rejected(f.dispatch(&request, true, 5));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let observed = f
        .host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 6,
            |input| {
                assert_eq!(input.input, b"immutable-input");
                count.fetch_add(1, Ordering::SeqCst);
                Ok(facts())
            },
        )
        .unwrap();
    assert_eq!(observed, facts());
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 7,
                |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(facts())
                }
            )
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
}

#[test]
fn exact_report_retry_is_durable_and_terminal_facts_are_immutable() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 5,
            |_| Ok(facts()),
        )
        .unwrap();
    let request = Request::new(
        "report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: facts(),
        },
    )
    .unwrap();
    let first = f.dispatch(&request, true, 6);
    let duplicate = f.dispatch(&request, true, 7);
    assert_eq!(first, duplicate);
    assert!(matches!(first,Outcome::Tool(info) if info.phase==ToolPhase::Reported));
    let mut altered = facts();
    altered.exit_code = Some(9);
    let changed = Request::new(
        "report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: altered,
        },
    )
    .unwrap();
    rejected(f.dispatch(&changed, true, 8));
    let changed_id = Request::new(
        "other-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: facts(),
        },
    )
    .unwrap();
    rejected(f.dispatch(&changed_id, true, 9));
    assert_eq!(f.inspect(), ToolPhase::Reported);
    let reopened = Store::open_existing(&f.path, EventBudget::default()).unwrap();
    reopened.integrity_check().unwrap();
}

#[test]
fn guest_cannot_report_unobserved_or_fabricated_execution_facts() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let report = Request::new(
        "early-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: facts(),
        },
    )
    .unwrap();
    rejected(f.dispatch(&report, true, 5));
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 6,
            |_| Ok(facts()),
        )
        .unwrap();
    let mut fabricated = facts();
    fabricated.output_closed = false;
    let report = Request::new(
        "false-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: fabricated,
        },
    )
    .unwrap();
    rejected(f.dispatch(&report, true, 7));
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
}

#[test]
fn trusted_approval_requires_the_complete_original_proposal_digest() {
    let mut f = Fixture::new();
    let proposal = f.propose();
    let intent = f.intent().digest().unwrap();
    let other = Request::new("other-request-identity", proposal.action().clone()).unwrap();
    assert!(
        f.host
            .approve(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                other.digest(),
                intent,
                3
            )
            .is_err()
    );
    assert_eq!(f.inspect(), ToolPhase::Proposed);
}

#[test]
fn exit_observed_does_not_imply_output_closed() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let mut open_output = facts();
    open_output.output_closed = false;
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 5,
            |_| Ok(open_output.clone()),
        )
        .unwrap();
    let request = Request::new(
        "separate-exit-eof",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: open_output,
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&request,true,6),Outcome::Tool(info) if info.facts.as_ref().unwrap().exit_code==Some(0) && !info.facts.as_ref().unwrap().output_closed)
    );
}

#[test]
fn expiration_at_the_final_encoded_reply_boundary_withdraws_fixed_input() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let request = Request::new(
        "late-claim",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    let mut clocks = 0;
    let bytes = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            request.raw(),
            || {
                clocks += 1;
                if clocks >= 9 { EXPIRES } else { 4 }
            },
        )
        .unwrap();
    rejected(Reply::decode_for(&request, &bytes).unwrap().outcome);
    assert!(
        !bytes
            .windows(b"immutable-input".len())
            .any(|part| part == b"immutable-input")
    );
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
}

#[test]
fn callback_deadline_expiry_preserves_unknown_observation_without_replay() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let completed = std::cell::Cell::new(false);
    let executions = AtomicUsize::new(0);
    let result = f.host.execute_claimed(
        &mut f.runtime,
        &f.executor_connection,
        &f.executor,
        OP,
        claim,
        || if completed.get() { EXPIRES } else { 5 },
        |_| {
            executions.fetch_add(1, Ordering::SeqCst);
            completed.set(true);
            Ok(facts())
        },
    );
    assert_eq!(result.unwrap_err(), Error::CommitUnknown);
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || EXPIRES + 1,
                |_| {
                    executions.fetch_add(1, Ordering::SeqCst);
                    Ok(facts())
                }
            )
            .is_err()
    );
    assert_eq!(executions.load(Ordering::SeqCst), 1);
    f.runtime.store_local().integrity_check().unwrap();

    // Tick 5 follows the pre-callback session decode; tick 8 follows the
    // observed-facts write. Both must use fresh, lightweight authorization.
    for (expiry_tick, expected_executions) in [(5, 0), (8, 1)] {
        let mut f = Fixture::new();
        f.propose();
        let permit = f.approve();
        let claim = f.claim(permit);
        let mut ticks = 0;
        let count = AtomicUsize::new(0);
        let result = f.host.execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || {
                ticks += 1;
                if ticks >= expiry_tick { EXPIRES } else { 5 }
            },
            |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(facts())
            },
        );
        assert_eq!(result.unwrap_err(), Error::CommitUnknown);
        assert_eq!(count.load(Ordering::SeqCst), expected_executions);
        assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
        assert!(
            f.host
                .execute_claimed(
                    &mut f.runtime,
                    &f.executor_connection,
                    &f.executor,
                    OP,
                    claim,
                    || EXPIRES + 1,
                    |_| {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(facts())
                    },
                )
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), expected_executions);
    }
}

#[test]
fn callback_failure_or_lost_report_never_replays_execution() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let count = AtomicUsize::new(0);
    assert_eq!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 5,
                |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Err(Error::Storage)
                }
            )
            .unwrap_err(),
        Error::CommitUnknown
    );
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 6,
                |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(facts())
                }
            )
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
}

#[test]
fn restart_preserves_unknown_but_never_restores_old_permit_or_claim() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let path = f.path.clone();
    let Fixture {
        _temp,
        runtime: old_runtime,
        host: old_host,
        ..
    } = f;
    drop(old_host);
    drop(old_runtime);
    let mut runtime =
        HostRuntime::new(Store::open_existing(&path, EventBudget::default()).unwrap()).unwrap();
    let connection = runtime.connect().unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let caps = Capabilities {
        session_read: true,
        execute: true,
        ..Capabilities::default()
    };
    let admission = host
        .admit(
            &runtime,
            &connection,
            caps,
            caps,
            vec![SESSION.into()],
            "test-domain".into(),
            EXPIRES,
            0,
        )
        .unwrap();
    let inspect = Request::new(
        "restart-inspect",
        Action::Inspect {
            operation_id: OP.into(),
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, inspect.raw(), || 10)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&inspect,&bytes).unwrap().outcome,Outcome::Tool(info) if info.phase==ToolPhase::DispatchUnknown)
    );
    let old = Request::new(
        "restart-claim",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &admission, old.raw(), || 11)
        .unwrap();
    rejected(Reply::decode_for(&old, &bytes).unwrap().outcome);
    let count = AtomicUsize::new(0);
    assert!(
        host.execute_claimed(
            &mut runtime,
            &connection,
            &admission,
            OP,
            claim,
            || 12,
            |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(facts())
            }
        )
        .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn new_host_can_read_unknown_observation_but_cannot_restore_execution_authority() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 5,
            |_| Ok(facts()),
        )
        .unwrap();
    let path = f.path.clone();
    let Fixture {
        _temp,
        runtime: old_runtime,
        host: old_host,
        ..
    } = f;
    drop(old_host);
    drop(old_runtime);
    let mut runtime =
        HostRuntime::new(Store::open_existing(&path, EventBudget::default()).unwrap()).unwrap();
    let connection = runtime.connect().unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let caps = Capabilities {
        session_read: true,
        ..Capabilities::default()
    };
    let reader = host
        .admit(
            &runtime,
            &connection,
            caps,
            caps,
            vec![SESSION.into()],
            "test-domain".into(),
            EXPIRES,
            0,
        )
        .unwrap();
    let inspect = Request::new(
        "new-host-history",
        Action::Inspect {
            operation_id: OP.into(),
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &reader, inspect.raw(), || 10)
        .unwrap();
    assert!(
        matches!(Reply::decode_for(&inspect,&bytes).unwrap().outcome,Outcome::Tool(info) if info.phase==ToolPhase::DispatchUnknown && info.facts.is_none())
    );
    assert_eq!(
        host.inspect_tool_observation(&runtime, &connection, &reader, OP, 11)
            .unwrap(),
        Some(facts())
    );
    let old = Request::new(
        "new-host-old-permit",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    let bytes = host
        .dispatch(&mut runtime, &connection, &reader, old.raw(), || 12)
        .unwrap();
    rejected(Reply::decode_for(&old, &bytes).unwrap().outcome);
    assert!(
        host.execute_claimed(
            &mut runtime,
            &connection,
            &reader,
            OP,
            claim,
            || 13,
            |_| panic!("historical observation cannot restore callback")
        )
        .is_err()
    );
}

#[test]
fn original_proposer_or_executor_revocation_prevents_claim_and_callback() {
    for revoke_proposer in [false, true] {
        let mut f = Fixture::new();
        f.propose();
        let permit = f.approve();
        f.host
            .revoke(if revoke_proposer {
                &f.proposer
            } else {
                &f.executor
            })
            .unwrap();
        let request = Request::new(
            "revoked-claim",
            Action::Claim {
                operation_id: OP.into(),
                permit,
            },
        )
        .unwrap();
        rejected(f.dispatch(&request, true, 4));
    }
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    f.host.revoke(&f.proposer).unwrap();
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 5,
                |_| panic!("revoked callback")
            )
            .is_err()
    );
}

#[test]
fn revoked_tool_retains_unknown_and_never_invokes_callback() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    f.host.revoke_tool(&mut f.runtime, OP, 4).unwrap();
    let request = Request::new(
        "revoked-tool-claim",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    rejected(f.dispatch(&request, true, 5));
    assert_eq!(f.inspect(), ToolPhase::Revoked);
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    f.host.revoke_tool(&mut f.runtime, OP, 5).unwrap();
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 6,
                |_| panic!("revoked callback")
            )
            .is_err()
    );
    assert_eq!(f.inspect(), ToolPhase::DispatchUnknown);
}

#[test]
fn expired_or_foreign_connection_never_gets_fixed_inputs() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let request = Request::new(
        "foreign-claim",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    let foreign = f.runtime.connect().unwrap();
    let bytes = f
        .host
        .dispatch(&mut f.runtime, &foreign, &f.executor, request.raw(), || 4)
        .unwrap();
    rejected(Reply::decode_for(&request, &bytes).unwrap().outcome);
    rejected(f.dispatch(&request, true, EXPIRES));
}

#[test]
fn immutable_environment_input_and_session_epoch_are_approval_bound() {
    let mut f = Fixture::new();
    let proposal = f.propose();
    let mut intent = f.intent();
    intent.env[0].value = "changed".into();
    intent.input.push(7);
    let changed = Request::new(
        "propose",
        Action::Propose {
            session_id: SESSION.into(),
            intent: intent.clone(),
        },
    )
    .unwrap();
    rejected(f.dispatch(&changed, false, 3));
    assert!(
        f.host
            .approve(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                proposal.digest(),
                intent.digest().unwrap(),
                4
            )
            .is_err()
    );
    let writer = Request::new(
        "writer",
        Action::OpenWriter {
            session_id: SESSION.into(),
            expected_epoch: 0,
        },
    )
    .unwrap();
    assert!(matches!(f.dispatch(&writer, false, 5), Outcome::Session(_)));
    let original = f.intent().digest().unwrap();
    assert!(
        f.host
            .approve(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                proposal.digest(),
                original,
                6
            )
            .is_err()
    );
}

#[test]
fn two_store_handles_cannot_win_the_same_claim_or_callback_twice() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    // A second runtime on the same physical Store has no original live admission.
    let mut other =
        HostRuntime::new(Store::open_existing(&f.path, EventBudget::default()).unwrap()).unwrap();
    assert!(SessionExecHost::new(&mut other).is_err());
    // Historical data remains readable, but another Store cannot install a
    // competing live owner or advance the session while callback is authorized.
    let domain = hash(b"agent-session-exec-v1/tool");
    assert!(
        other
            .store_local()
            .load_agent_ledger_local(&domain, OP)
            .unwrap()
            .is_some()
    );
    let counter = Arc::new(AtomicUsize::new(0));
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 6,
            |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(facts())
            },
        )
        .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn reserved_terminal_capacity_survives_other_writers_filling_the_store() {
    use morrow_core::agent_ledger::Record;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("full-single-core.sqlite");
    let budget = EventBudget {
        max_count: 1024,
        max_bytes: 16 * 1024,
    };
    let runtime = HostRuntime::new(Store::open(&path, budget).unwrap()).unwrap();
    let mut f = Fixture::from_runtime(temp, path, runtime);
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let mut used = 0;
    for (domain, id) in [
        (hash(b"agent-session-exec-v1/tool"), OP),
        (
            hash(b"morrow/agent-session-exec-v1/session/state/1"),
            SESSION,
        ),
    ] {
        used += f
            .runtime
            .store_local()
            .load_agent_ledger_local(&domain, id)
            .unwrap()
            .unwrap()
            .retained_bytes();
    }
    let filler_domain = hash(b"bounded-capacity-filler");
    let mut low = 0usize;
    let mut high = 16 * 1024usize;
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate = Record::new(filler_domain, "filler", 1, vec![0; mid]).unwrap();
        if candidate.retained_bytes() <= 16 * 1024 - used {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let filler = Record::new(filler_domain, "filler", 1, vec![0; low]).unwrap();
    f.runtime
        .store_local_mut()
        .compare_exchange_agent_ledger_local(&filler, 0)
        .unwrap();
    let extra = Record::new(filler_domain, "overflow", 1, vec![0; 1]).unwrap();
    assert!(
        f.runtime
            .store_local_mut()
            .compare_exchange_agent_ledger_local(&extra, 0)
            .is_err()
    );
    f.host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 5,
            |_| Ok(facts()),
        )
        .unwrap();
    let report = Request::new(
        "reserved-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: facts(),
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&report,true,6),Outcome::Tool(info) if info.phase==ToolPhase::Reported)
    );
}

#[cfg(unix)]
#[test]
fn trusted_callback_runs_one_fixed_real_process_and_reports_exit_and_output_facts() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let counter = AtomicUsize::new(0);
    let observed = f
        .host
        .execute_claimed(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            claim,
            || 5,
            |intent| {
                counter.fetch_add(1, Ordering::SeqCst);
                if hash(&std::fs::read(&intent.program).map_err(|_| Error::Storage)?)
                    != intent.artifact_sha256
                {
                    return Err(Error::Denied);
                }
                let output = std::process::Command::new(&intent.program)
                    .args(&intent.argv)
                    .env_clear()
                    .envs(intent.env.iter().map(|v| (&v.name, &v.value)))
                    .current_dir(&intent.cwd)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .map_err(|_| Error::Storage)?;
                Ok(ExecutionFacts {
                    exit_code: output.status.code(),
                    output_closed: true,
                    stdout_sha256: hash(&output.stdout),
                    stderr_sha256: hash(&output.stderr),
                    stdout_bytes: output.stdout.len() as u64,
                    stderr_bytes: output.stderr.len() as u64,
                })
            },
        )
        .unwrap();
    assert_eq!(observed, facts());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    let report = Request::new(
        "process-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: observed,
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&report,true,6),Outcome::Tool(info) if info.phase==ToolPhase::Reported && info.facts.as_ref().unwrap().exit_code==Some(0))
    );
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 7,
                |_| panic!("must not start second process")
            )
            .is_err()
    );
}
