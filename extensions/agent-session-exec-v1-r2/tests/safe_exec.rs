//! Ordinary real Core Store qualification. No SDK-owned OS sandbox/backend.
use morrow_agent_session_exec_v1_r2::{
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
const FIXED_ARTIFACT: &[u8] = b"fixed synthetic artifact for logical execution callbacks";
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    artifact_path: PathBuf,
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
        retire: false,
    }
}
fn executor_capabilities() -> Capabilities {
    Capabilities {
        session_read: true,
        session_write: false,
        propose: false,
        execute: true,
        retire: false,
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
        // Logical callbacks inspect immutable intent data; they never launch this artifact.
        let artifact_path = temp.path().join("fixed-artifact.bin");
        assert!(artifact_path.is_absolute());
        std::fs::write(&artifact_path, FIXED_ARTIFACT).unwrap();
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
            artifact_path,
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
    #[cfg(unix)]
    fn real_printf() -> Self {
        let mut fixture = Self::new();
        fixture.artifact_path = PathBuf::from("/usr/bin/printf");
        fixture
    }
    fn intent(&self) -> Intent {
        Intent {
            operation_id: OP.into(),
            program: self.artifact_path.to_str().unwrap().into(),
            argv: vec!["fixed-output".into()],
            cwd: self._temp.path().to_str().unwrap().into(),
            env: vec![morrow_agent_session_exec_v1_r2::Environment {
                name: "MORROW_FIXED".into(),
                value: "approved".into(),
            }],
            input: b"immutable-input".to_vec(),
            execution_domain: "test-domain".into(),
            max_runtime_ms: 1000,
            artifact_sha256: hash(&std::fs::read(&self.artifact_path).unwrap()),
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
        // R2 host clocks are monotonic across admissions. Historical phase
        // review uses current native ownership rather than stale admissions.
        self.host
            .inspect_tool_record(&self.runtime, OP)
            .unwrap()
            .phase
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
    let expected_intent = f.intent();
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
                assert_eq!(input, &expected_intent);
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
        retire: false,
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
    let domain = hash(b"agent-session-exec-v1/tool/2");
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
fn reserved_terminal_capacity_survives_other_fixed_proposals_filling_the_store() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("full-single-core.sqlite");
    let budget = EventBudget {
        max_count: 1024,
        max_bytes: 256 * 1024,
    };
    let runtime = HostRuntime::new(Store::open(&path, budget).unwrap()).unwrap();
    let mut f = Fixture::from_runtime(temp, path, runtime);
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let mut input_bytes = morrow_agent_session_exec_v1_r2::MAX_BODY_BYTES;
    let mut saturated = false;
    let mut accepted = 0;
    for index in 0..64 {
        let mut intent = f.intent();
        intent.operation_id = format!("capacity-filler-{index}");
        intent.input = vec![0; input_bytes];
        let proposal = Request::new(
            format!("capacity-proposal-{index}"),
            Action::Propose {
                session_id: SESSION.into(),
                intent,
            },
        )
        .unwrap();
        match f.dispatch(&proposal, false, 4) {
            Outcome::Tool(info) if info.phase == ToolPhase::Proposed => accepted += 1,
            Outcome::Rejected(Error::Limit) if input_bytes == 0 => {
                saturated = true;
                break;
            }
            Outcome::Rejected(Error::Limit) => input_bytes /= 2,
            other => panic!("unexpected SDK capacity outcome: {other:?}"),
        }
    }
    assert!(accepted > 0 && saturated);
    let mut additional = f.intent();
    additional.operation_id = "after-retirement".into();
    additional.input.clear();
    let extra = Request::new(
        "after-retirement-proposal",
        Action::Propose {
            session_id: SESSION.into(),
            intent: additional,
        },
    )
    .unwrap();
    assert_eq!(
        f.dispatch(&extra, false, 4),
        Outcome::Rejected(Error::Limit)
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
    let reviewed = record_review(&f);
    let retirer = retirement_admission(&f, 7);
    f.host
        .retire_tool(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            &reviewed,
            false,
            || 8,
        )
        .unwrap();
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(&hash(b"agent-session-exec-v1/tool/2"), OP)
            .unwrap()
            .is_none()
    );
    assert!(
        matches!(f.dispatch(&extra,false,9),Outcome::Tool(info) if info.phase==ToolPhase::Proposed)
    );
}

#[cfg(unix)]
#[test]
fn trusted_callback_runs_one_fixed_real_process_and_reports_exit_and_output_facts() {
    let mut f = Fixture::real_printf();
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

fn retirement_admission(f: &Fixture, now: u64) -> Admission {
    let caps = Capabilities {
        session_read: true,
        retire: true,
        ..Capabilities::default()
    };
    f.host
        .admit(
            &f.runtime,
            &f.executor_connection,
            caps,
            caps,
            vec![SESSION.into()],
            "test-domain".into(),
            now + EXPIRES,
            now,
        )
        .unwrap()
}
fn record_review(f: &Fixture) -> morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation {
    f.host.inspect_tool_record(&f.runtime, OP).unwrap()
}
fn run_observed(f: &mut Fixture, observed: ExecutionFacts) -> [u8; 32] {
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
            |_| Ok(observed),
        )
        .unwrap();
    claim
}
fn assert_no_callback(f: &mut Fixture, claim: [u8; 32], now: u64) {
    let calls = AtomicUsize::new(0);
    assert!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || now,
                |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(facts())
                }
            )
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn trusted_partial_progress_preserves_report_receipt_and_wire_exposes_latest() {
    let mut f = Fixture::new();
    let mut partial = facts();
    partial.output_closed = false;
    let claim = run_observed(&mut f, partial.clone());
    let original = Request::new(
        "partial-report",
        Action::Report {
            operation_id: OP.into(),
            claim,
            facts: partial.clone(),
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&original,true,6),Outcome::Tool(info) if info.facts==Some(partial.clone()))
    );
    let before = record_review(&f);
    let after = f
        .host
        .reconcile_tool_observation(
            &mut f.runtime,
            &before.identity,
            before.record_revision,
            &facts(),
        )
        .unwrap();
    assert_eq!(after.phase, ToolPhase::Reported);
    assert_eq!(after.accepted_report, Some(partial.clone()));
    assert_eq!(after.latest, Some(facts()));
    assert_eq!(after.observation_revision, after.record_revision);
    let inspect = Request::new(
        "latest-inspect",
        Action::Inspect {
            operation_id: OP.into(),
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&inspect,true,7),Outcome::Tool(info) if info.facts==Some(partial.clone()) && info.observation==Some(facts()) && info.observation_revision==after.observation_revision)
    );
    assert!(
        matches!(f.dispatch(&original,true,8),Outcome::Tool(info) if info.facts==Some(partial) && info.observation==Some(facts()))
    );
    assert_no_callback(&mut f, claim, 9);
}

#[test]
fn exit_and_output_closure_can_arrive_in_either_order_without_invention() {
    for close_first in [false, true] {
        let mut f = Fixture::new();
        let mut partial = facts();
        partial.exit_code = None;
        partial.output_closed = false;
        run_observed(&mut f, partial.clone());
        let before = record_review(&f);
        if close_first {
            partial.output_closed = true;
        } else {
            partial.exit_code = Some(7);
        }
        let middle = f
            .host
            .reconcile_tool_observation(
                &mut f.runtime,
                &before.identity,
                before.record_revision,
                &partial,
            )
            .unwrap();
        assert_eq!(middle.latest, Some(partial.clone()));
        if close_first {
            assert_eq!(middle.latest.as_ref().unwrap().exit_code, None);
            partial.exit_code = Some(7);
        } else {
            assert!(!middle.latest.as_ref().unwrap().output_closed);
            partial.output_closed = true;
        }
        let after = f
            .host
            .reconcile_tool_observation(
                &mut f.runtime,
                &middle.identity,
                middle.record_revision,
                &partial,
            )
            .unwrap();
        assert_eq!(after.latest, Some(partial));
        assert_eq!(after.phase, ToolPhase::DispatchUnknown);
        assert_eq!(after.accepted_report, None);
    }
}

#[test]
fn monotonic_observations_reject_exit_eof_byte_and_digest_regressions() {
    let mut f = Fixture::new();
    let mut partial = facts();
    partial.output_closed = false;
    run_observed(&mut f, partial.clone());
    let before = record_review(&f);
    let mut growth = partial.clone();
    growth.stdout_bytes += 1;
    growth.stdout_sha256 = hash(b"fixed-output!");
    let current = f
        .host
        .reconcile_tool_observation(
            &mut f.runtime,
            &before.identity,
            before.record_revision,
            &growth,
        )
        .unwrap();
    let mut invalid = Vec::new();
    let mut changed = growth.clone();
    changed.exit_code = None;
    invalid.push(changed);
    let mut changed = growth.clone();
    changed.exit_code = Some(9);
    invalid.push(changed);
    let mut changed = growth.clone();
    changed.stdout_bytes -= 1;
    changed.stdout_sha256 = partial.stdout_sha256;
    invalid.push(changed);
    let mut changed = growth.clone();
    changed.stdout_sha256 = hash(b"different-count-same");
    invalid.push(changed);
    let mut changed = growth.clone();
    changed.stderr_sha256 = hash(b"not-empty");
    invalid.push(changed);
    for changed in invalid {
        assert!(
            f.host
                .reconcile_tool_observation(
                    &mut f.runtime,
                    &current.identity,
                    current.record_revision,
                    &changed
                )
                .is_err()
        );
        assert_eq!(record_review(&f), current);
    }
    let mut closed = growth.clone();
    closed.output_closed = true;
    let terminal = f
        .host
        .reconcile_tool_observation(
            &mut f.runtime,
            &current.identity,
            current.record_revision,
            &closed,
        )
        .unwrap();
    let mut reopened = closed.clone();
    reopened.output_closed = false;
    assert_eq!(
        f.host
            .reconcile_tool_observation(
                &mut f.runtime,
                &terminal.identity,
                terminal.record_revision,
                &reopened
            )
            .unwrap_err(),
        Error::Conflict
    );
    let mut extended = closed;
    extended.stdout_bytes += 1;
    extended.stdout_sha256 = hash(b"fixed-output!!");
    assert_eq!(
        f.host
            .reconcile_tool_observation(
                &mut f.runtime,
                &terminal.identity,
                terminal.record_revision,
                &extended
            )
            .unwrap_err(),
        Error::Conflict
    );
    assert_eq!(record_review(&f), terminal);
}

#[test]
fn trusted_reconciliation_binds_every_original_identity_field_and_revision() {
    let mut f = Fixture::new();
    let mut partial = facts();
    partial.output_closed = false;
    run_observed(&mut f, partial);
    let before = record_review(&f);
    let mut identities = Vec::new();
    let mut wrong = before.identity.clone();
    wrong.proposal_sha256[0] ^= 1;
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.intent_sha256[0] ^= 1;
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.execution_domain = "other-domain".into();
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.session_id = "other-session".into();
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.session_epoch += 1;
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.generation += 1;
    identities.push(wrong);
    let mut wrong = before.identity.clone();
    wrong.original_issuer_nonce[0] ^= 1;
    identities.push(wrong);
    for wrong in identities {
        assert_eq!(
            f.host
                .reconcile_tool_observation(
                    &mut f.runtime,
                    &wrong,
                    before.record_revision,
                    &facts()
                )
                .unwrap_err(),
            Error::Denied
        );
        assert_eq!(record_review(&f), before);
    }
    let after = f
        .host
        .reconcile_tool_observation(
            &mut f.runtime,
            &before.identity,
            before.record_revision,
            &facts(),
        )
        .unwrap();
    assert_eq!(
        f.host
            .reconcile_tool_observation(
                &mut f.runtime,
                &before.identity,
                before.record_revision,
                &facts()
            )
            .unwrap_err(),
        Error::Conflict
    );
    assert_eq!(record_review(&f), after);
}

#[test]
fn callback_error_late_facts_survive_new_owner_without_restoring_old_executor() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let marker = f._temp.path().join("real-effect");
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
                    std::fs::write(&marker, b"effect").unwrap();
                    Err(Error::Storage)
                }
            )
            .unwrap_err(),
        Error::CommitUnknown
    );
    assert_eq!(std::fs::read(marker).unwrap(), b"effect");
    let old = record_review(&f);
    assert!(old.invocation_started);
    assert_eq!(old.latest, None);
    f.host.revoke(&f.proposer).unwrap();
    f.host.revoke(&f.executor).unwrap();
    f.host = SessionExecHost::new(&mut f.runtime).unwrap();
    let after = f
        .host
        .reconcile_tool_observation(&mut f.runtime, &old.identity, old.record_revision, &facts())
        .unwrap();
    assert_eq!(after.identity, old.identity);
    assert_eq!(after.latest, Some(facts()));
    assert_eq!(after.phase, ToolPhase::DispatchUnknown);
    assert_eq!(after.accepted_report, None);
    f.executor = retirement_admission(&f, 6);
    let inspect = Request::new(
        "fresh-read-late",
        Action::Inspect {
            operation_id: OP.into(),
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&inspect,true,7),Outcome::Tool(info) if info.phase==ToolPhase::DispatchUnknown && info.facts.is_none() && info.observation==Some(facts()))
    );
    assert_no_callback(&mut f, claim, 8);
}

#[test]
fn foreign_runtime_cannot_reconcile_revoke_or_retire_an_operation() {
    let mut f = Fixture::new();
    run_observed(&mut f, facts());
    let before = record_review(&f);
    let mut foreign = Fixture::new();
    let retirer = retirement_admission(&foreign, 6);
    assert_eq!(
        f.host
            .reconcile_tool_observation(
                &mut foreign.runtime,
                &before.identity,
                before.record_revision,
                &facts()
            )
            .unwrap_err(),
        Error::Denied
    );
    assert_eq!(
        f.host.revoke_tool(&mut foreign.runtime, OP, 7).unwrap_err(),
        Error::Denied
    );
    assert_eq!(
        f.host
            .retire_tool(
                &mut foreign.runtime,
                &foreign.executor_connection,
                &retirer,
                &before,
                true,
                || 8
            )
            .unwrap_err(),
        Error::Denied
    );
    assert_eq!(record_review(&f), before);
}

#[test]
fn consumed_unstarted_claim_can_be_retired_by_fresh_owner_without_callback() {
    let mut f = Fixture::new();
    let original = f.propose();
    let permit = f.approve();
    let claim_request = Request::new(
        "lost-claim",
        Action::Claim {
            operation_id: OP.into(),
            permit,
        },
    )
    .unwrap();
    // Lose the winning reply without ever decoding its input or claim token.
    let lost = f
        .host
        .dispatch(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            claim_request.raw(),
            || 4,
        )
        .unwrap();
    drop(lost);
    let review = record_review(&f);
    assert_eq!(review.phase, ToolPhase::DispatchUnknown);
    assert!(!review.invocation_started);
    let new_host = SessionExecHost::new(&mut f.runtime).unwrap();
    let old_host = std::mem::replace(&mut f.host, new_host);
    let retirer = retirement_admission(&f, 5);
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &review,
                false,
                || 6
            )
            .unwrap_err(),
        Error::Denied
    );
    f.host
        .retire_tool(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            &review,
            true,
            || 7,
        )
        .unwrap();
    let tomb = f
        .host
        .inspect_retired_tool_record(&f.runtime, OP)
        .unwrap()
        .unwrap();
    assert_eq!(tomb.historical_phase, ToolPhase::DispatchUnknown);
    assert_eq!(tomb.record_sha256, review.record_sha256);
    let calls = AtomicUsize::new(0);
    assert!(
        old_host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                [9; 32],
                || 8,
                |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(facts())
                }
            )
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    // Fresh proposer approval still cannot reuse the retired operation identity.
    f.proposer = f
        .host
        .admit(
            &f.runtime,
            &f.proposer_connection,
            proposer_capabilities(),
            proposer_capabilities(),
            vec![SESSION.into()],
            "test-domain".into(),
            1000,
            9,
        )
        .unwrap();
    assert!(matches!(
        f.dispatch(&original, false, 10),
        Outcome::Rejected(Error::Conflict)
    ));
    assert!(
        f.runtime
            .store_local()
            .load_agent_ledger_local(&hash(b"agent-session-exec-v1/tool/2"), OP)
            .unwrap()
            .is_none()
    );
}

#[test]
fn callback_marker_wins_stale_retirement_and_started_unknown_requires_closed_facts() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    let unstarted = record_review(&f);
    let calls = AtomicUsize::new(0);
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
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err(Error::Storage)
                }
            )
            .unwrap_err(),
        Error::CommitUnknown
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let retirer = retirement_admission(&f, 6);
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &unstarted,
                true,
                || 7
            )
            .unwrap_err(),
        Error::Conflict
    );
    let started = record_review(&f);
    assert!(started.invocation_started);
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &started,
                true,
                || 8
            )
            .unwrap_err(),
        Error::Denied
    );
    let after = f
        .host
        .reconcile_tool_observation(
            &mut f.runtime,
            &started.identity,
            started.record_revision,
            &facts(),
        )
        .unwrap();
    f.host
        .retire_tool(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            &after,
            true,
            || 9,
        )
        .unwrap();
    assert_eq!(
        f.host
            .inspect_retired_tool_record(&f.runtime, OP)
            .unwrap()
            .unwrap()
            .historical_phase,
        ToolPhase::DispatchUnknown
    );
    assert_no_callback(&mut f, claim, 10);
}

#[test]
fn host_revocation_after_proposer_loss_and_independent_retirement_do_not_grant_execution() {
    for disconnect in [false, true] {
        let mut f = Fixture::new();
        f.propose();
        let permit = f.approve();
        if disconnect {
            f.runtime.disconnect(&f.proposer_connection).unwrap();
        } else {
            f.host.revoke(&f.proposer).unwrap();
        }
        f.host.revoke_tool(&mut f.runtime, OP, 5).unwrap();
        let review = record_review(&f);
        assert_eq!(review.phase, ToolPhase::Revoked);
        assert_eq!(
            f.host
                .retire_tool(
                    &mut f.runtime,
                    &f.executor_connection,
                    &f.executor,
                    &review,
                    false,
                    || 6
                )
                .unwrap_err(),
            Error::Denied
        );
        let retirer = retirement_admission(&f, 7);
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &review,
                false,
                || 8,
            )
            .unwrap();
        let stale = Request::new(
            "revoked-claim",
            Action::Claim {
                operation_id: OP.into(),
                permit,
            },
        )
        .unwrap();
        rejected(f.dispatch(&stale, true, 9));
        assert_eq!(
            f.host
                .inspect_retired_tool_record(&f.runtime, OP)
                .unwrap()
                .unwrap()
                .historical_phase,
            ToolPhase::Revoked
        );
    }
}

#[test]
fn tool_retirement_binds_review_sha_revision_and_complete_identity() {
    let mut f = Fixture::new();
    f.propose();
    f.host.revoke_tool(&mut f.runtime, OP, 3).unwrap();
    let review = record_review(&f);
    let retirer = retirement_admission(&f, 4);
    let mut forged = review.clone();
    forged.record_sha256[0] ^= 1;
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &forged,
                false,
                || 5
            )
            .unwrap_err(),
        Error::Conflict
    );
    let mut forged = review.clone();
    forged.record_revision += 1;
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &forged,
                false,
                || 6
            )
            .unwrap_err(),
        Error::Conflict
    );
    let mut forged = review.clone();
    forged.identity.proposal_sha256[0] ^= 1;
    assert_eq!(
        f.host
            .retire_tool(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                &forged,
                false,
                || 7
            )
            .unwrap_err(),
        Error::Denied
    );
    assert_eq!(record_review(&f), review);
}

#[test]
fn retained_unknown_blocks_session_deletion_until_verified_tool_retirement() {
    let mut f = Fixture::new();
    f.propose();
    let permit = f.approve();
    let claim = f.claim(permit);
    assert_eq!(
        f.host
            .execute_claimed(
                &mut f.runtime,
                &f.executor_connection,
                &f.executor,
                OP,
                claim,
                || 5,
                |_| Err(Error::Storage)
            )
            .unwrap_err(),
        Error::CommitUnknown
    );
    let retirer = retirement_admission(&f, 6);
    let session = f
        .host
        .review_session_retirement(&f.runtime, &f.executor_connection, &retirer, SESSION, 7)
        .unwrap();
    assert_eq!(
        f.host
            .retire_session(
                &mut f.runtime,
                &f.executor_connection,
                &retirer,
                SESSION,
                session.info.revision,
                session.record_sha256,
                || 8
            )
            .unwrap_err(),
        Error::Conflict
    );
    assert_eq!(
        f.host.rollover_generation(&mut f.runtime, 9).unwrap_err(),
        Error::Conflict
    );
    let old = record_review(&f);
    let closed = f
        .host
        .reconcile_tool_observation(&mut f.runtime, &old.identity, old.record_revision, &facts())
        .unwrap();
    f.host
        .retire_tool(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            &closed,
            true,
            || 10,
        )
        .unwrap();
    f.host
        .retire_session(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            SESSION,
            session.info.revision,
            session.record_sha256,
            || 11,
        )
        .unwrap();
    assert_eq!(f.host.rollover_generation(&mut f.runtime, 12).unwrap(), 2);
}

#[test]
fn generation_rollover_permanently_rejects_old_wire_and_requires_fresh_tool_approval() {
    let mut f = Fixture::new();
    let old_proposal = f.propose();
    let old_permit = f.approve();
    let old_claim = f.claim(old_permit);
    let review = record_review(&f);
    let retirer = retirement_admission(&f, 5);
    f.host
        .retire_tool(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            &review,
            true,
            || 6,
        )
        .unwrap();
    let session = f
        .host
        .review_session_retirement(&f.runtime, &f.executor_connection, &retirer, SESSION, 7)
        .unwrap();
    f.host
        .retire_session(
            &mut f.runtime,
            &f.executor_connection,
            &retirer,
            SESSION,
            session.info.revision,
            session.record_sha256,
            || 8,
        )
        .unwrap();
    assert_eq!(f.host.rollover_generation(&mut f.runtime, 9).unwrap(), 2);
    assert_no_callback(&mut f, old_claim, 10);
    f.host = SessionExecHost::new(&mut f.runtime).unwrap();
    assert_eq!(f.host.generation(&f.runtime).unwrap(), 2);
    f.proposer = f
        .host
        .admit(
            &f.runtime,
            &f.proposer_connection,
            proposer_capabilities(),
            proposer_capabilities(),
            vec![SESSION.into()],
            "test-domain".into(),
            1000,
            11,
        )
        .unwrap();
    f.executor = f
        .host
        .admit(
            &f.runtime,
            &f.executor_connection,
            executor_capabilities(),
            executor_capabilities(),
            vec![SESSION.into()],
            "test-domain".into(),
            1000,
            11,
        )
        .unwrap();
    assert!(matches!(
        f.dispatch(&old_proposal, false, 12),
        Outcome::Rejected(Error::Conflict)
    ));
    let create = Request::new_for_generation(
        "fresh-create",
        2,
        Action::Create {
            session_id: SESSION.into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    assert!(matches!(
        f.dispatch(&create, false, 13),
        Outcome::Session(_)
    ));
    let proposal = Request::new_for_generation(
        "fresh-propose",
        2,
        Action::Propose {
            session_id: SESSION.into(),
            intent: f.intent(),
        },
    )
    .unwrap();
    assert!(
        matches!(f.dispatch(&proposal,false,14),Outcome::Tool(info) if info.phase==ToolPhase::Proposed)
    );
    let reviewed = f.host.review_tool(&f.runtime, OP, 15).unwrap();
    let new_permit = f
        .host
        .approve(
            &mut f.runtime,
            &f.executor_connection,
            &f.executor,
            OP,
            reviewed.proposal_sha256,
            reviewed.intent_sha256,
            16,
        )
        .unwrap();
    assert_ne!(new_permit, old_permit);
    let stale = Request::new_for_generation(
        "old-permit-new-generation",
        2,
        Action::Claim {
            operation_id: OP.into(),
            permit: old_permit,
        },
    )
    .unwrap();
    rejected(f.dispatch(&stale, true, 17));
    assert_eq!(record_review(&f).phase, ToolPhase::Approved);
    assert!(
        f.host
            .inspect_retired_tool_record(&f.runtime, OP)
            .unwrap()
            .is_none()
    );
}
